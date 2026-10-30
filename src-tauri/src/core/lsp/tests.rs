//! Language server tests with a fake server (Python script speaking LSP framing).

use std::os::unix::fs::PermissionsExt;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use camino::{Utf8Path, Utf8PathBuf};
use serde_json::{Value, json};

use super::*;
use crate::core::CoreError;
use crate::core::project::ProjectError;
use crate::core::runner::ChildEnv;
use crate::core::workspace::Workspace;
use crate::platform::AppDirs;

const FAKE_SERVER: &str = r#"#!/usr/bin/env python3
import json, os, sys, signal
mode = os.environ.get("FAKE_MODE", "normal")
out_dir = os.environ["FAKE_OUT"]
open(os.path.join(out_dir, "started"), "w").write(str(os.getpid()))
if mode == "stubborn":
    signal.signal(signal.SIGINT, signal.SIG_IGN)
inp, outp = sys.stdin.buffer, sys.stdout.buffer
def send(msg):
    body = json.dumps(msg).encode()
    outp.write(b"Content-Length: %d\r\n\r\n" % len(body) + body); outp.flush()
def read():
    length = None
    while True:
        line = inp.readline()
        if not line: return None
        line = line.strip()
        if not line: break
        k, v = line.decode().split(":", 1)
        if k.lower() == "content-length": length = int(v)
    return json.loads(inp.read(length))
if mode == "garbage":
    outp.write(b"this is not lsp\r\n\r\n"); outp.flush()
while True:
    msg = read()
    if msg is None:
        if mode == "stubborn":
            while True: signal.pause()
        sys.exit(0)
    if msg.get("method") == "initialize":
        send({"jsonrpc": "2.0", "id": msg["id"], "result": {"capabilities": {"hoverProvider": True}}})
        send({"jsonrpc": "2.0", "method": "textDocument/publishDiagnostics", "params": {"uri": "file:///x", "diagnostics": [{"message": "fake"}]}})
        send({"jsonrpc": "2.0", "id": "cfg-1", "method": "workspace/configuration", "params": {"items": [{"section": "ansible"}]}})
        if mode == "crash":
            sys.stderr.write("boom: fake crash\n"); sys.stderr.flush(); sys.exit(3)
    elif msg.get("id") == "cfg-1":
        open(os.path.join(out_dir, "config.json"), "w").write(json.dumps(msg))
"#;

struct Fx {
    _tmp: tempfile::TempDir,
    base: Utf8PathBuf,
    server: Utf8PathBuf,
    project: Utf8PathBuf,
}

fn fx() -> Fx {
    let tmp = tempfile::tempdir().unwrap();
    let base = Utf8PathBuf::from_path_buf(tmp.path().to_path_buf()).unwrap();
    let server = base.join("fake-ls");
    std::fs::write(&server, FAKE_SERVER).unwrap();
    std::fs::set_permissions(&server, std::fs::Permissions::from_mode(0o755)).unwrap();
    let project = base.join("project");
    std::fs::create_dir_all(&project).unwrap();
    Fx {
        _tmp: tmp,
        base,
        server,
        project,
    }
}

fn ctx(f: &Fx, mode: &str) -> LaunchContext {
    LaunchContext {
        project_root: f.project.canonicalize_utf8().unwrap(),
        terraform_ls: Some(f.server.clone()),
        // Ansible: "node" runs the fake script directly (bundle path = ignored arg)
        node: Some(f.server.clone()),
        als_bundle: Some(Utf8PathBuf::from("/nonexistent/bundle.cjs")),
        venv: Some(Utf8PathBuf::from("/data/venvs/abc")),
        env: ChildEnv::empty()
            .set("PATH", "/usr/local/bin:/usr/bin:/bin")
            .set("FAKE_OUT", f.base.as_str())
            .set("FAKE_MODE", mode),
    }
}

type Events = Arc<Mutex<Vec<LspEvent>>>;

fn sink() -> (Events, impl Fn(LspEvent) + Send + Sync + 'static) {
    let events: Events = Arc::default();
    let e = events.clone();
    (events, move |ev| e.lock().unwrap().push(ev))
}

async fn wait_for(events: &Events, pred: impl Fn(&LspEvent) -> bool) -> LspEvent {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        if let Some(e) = events.lock().unwrap().iter().find(|e| pred(e)) {
            return e.clone();
        }
        assert!(
            Instant::now() < deadline,
            "timed out; events: {:?}",
            events.lock().unwrap()
        );
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}

fn workspace(f: &Fx) -> (Workspace, u32) {
    let dirs = AppDirs {
        config: f.base.join("config"),
        data: f.base.join("data"),
        cache: f.base.join("cache"),
        state: f.base.join("state"),
    };
    let ws = Workspace::init(&dirs).unwrap();
    let id = ws.open(&f.project).unwrap().id;
    (ws, id)
}

fn initialize() -> Value {
    json!({"jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {"capabilities": {}}})
}

#[tokio::test]
async fn servers_never_start_for_untrusted_projects() {
    let f = fx();
    let (ws, id) = workspace(&f);
    let manager = LspManager::default();
    for kind in [ServerKind::Terraform, ServerKind::Ansible] {
        let (_events, emit) = sink();
        let r = ws.with_trusted(id, |t| {
            Ok(manager.start(t, kind, &ctx(&f, "normal"), emit)?)
        });
        assert!(
            matches!(r, Err(CoreError::Project(ProjectError::NotTrusted))),
            "{kind:?}: {:?}",
            r.err()
        );
    }
    tokio::time::sleep(Duration::from_millis(200)).await;
    assert!(
        !f.base.join("started").exists(),
        "a server process was started"
    );
    assert_eq!(manager.session_count(), 0);

    // after trusting, the same call starts the server
    ws.set_trusted(id, true).unwrap();
    let (events, emit) = sink();
    let sid = ws
        .with_trusted(id, |t| {
            Ok(manager.start(t, ServerKind::Terraform, &ctx(&f, "normal"), emit)?)
        })
        .unwrap();
    manager.send(sid, initialize()).await.unwrap();
    wait_for(
        &events,
        |e| matches!(e, LspEvent::Message { payload } if payload["id"] == 1),
    )
    .await;
    assert!(f.base.join("started").exists());
    manager.stop(sid).await.unwrap();
}

async fn trusted_start(
    f: &Fx,
    kind: ServerKind,
    mode: &str,
) -> (Workspace, LspManager, SessionId, Events) {
    let (ws, id) = workspace(f);
    ws.set_trusted(id, true).unwrap();
    let manager = LspManager::default();
    let (events, emit) = sink();
    let sid = ws
        .with_trusted(id, |t| Ok(manager.start(t, kind, &ctx(f, mode), emit)?))
        .unwrap();
    (ws, manager, sid, events)
}

#[tokio::test]
async fn initialize_round_trip_and_server_messages() {
    let f = fx();
    let (_ws, m, sid, events) = trusted_start(&f, ServerKind::Terraform, "normal").await;
    m.send(sid, initialize()).await.unwrap();
    let LspEvent::Message { payload } = wait_for(
        &events,
        |e| matches!(e, LspEvent::Message { payload } if payload["id"] == 1),
    )
    .await
    else {
        unreachable!()
    };
    assert_eq!(
        payload["result"]["capabilities"]["hoverProvider"],
        json!(true)
    );
    wait_for(&events, |e| {
        matches!(e, LspEvent::Message { payload } if payload["method"] == "textDocument/publishDiagnostics")
    })
    .await;
    m.stop(sid).await.unwrap();
    let exited = wait_for(&events, |e| matches!(e, LspEvent::Exited { .. })).await;
    assert!(
        matches!(
            exited,
            LspEvent::Exited {
                crashed: false,
                code: Some(0),
                ..
            }
        ),
        "{exited:?}"
    );
}

#[tokio::test]
async fn ansible_configuration_is_enforced_by_the_core() {
    let f = fx();
    let (_ws, m, sid, events) = trusted_start(&f, ServerKind::Ansible, "normal").await;
    m.send(sid, initialize()).await.unwrap();
    wait_for(&events, |e| {
        matches!(e, LspEvent::Message { payload } if payload["method"] == "workspace/configuration")
    })
    .await;
    // a (compromised) client tries to enable execution environments
    m.send(
        sid,
        json!({"jsonrpc": "2.0", "id": "cfg-1", "result": [
            {"executionEnvironment": {"enabled": true}, "python": {"interpreterPath": "/tmp/evil"}}
        ]}),
    )
    .await
    .unwrap();
    let path = f.base.join("config.json");
    let deadline = Instant::now() + Duration::from_secs(5);
    while !path.exists() {
        assert!(
            Instant::now() < deadline,
            "server never received the configuration"
        );
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    let seen: Value = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    assert_eq!(
        seen["result"][0]["executionEnvironment"]["enabled"],
        json!(false)
    );
    assert_eq!(
        seen["result"][0]["python"]["interpreterPath"],
        json!("/data/venvs/abc/bin/python")
    );
    m.stop(sid).await.unwrap();
}

#[tokio::test]
async fn crash_is_reported_with_stderr_tail() {
    let f = fx();
    let (_ws, m, sid, events) = trusted_start(&f, ServerKind::Terraform, "crash").await;
    m.send(sid, initialize()).await.unwrap();
    let exited = wait_for(&events, |e| matches!(e, LspEvent::Exited { .. })).await;
    let LspEvent::Exited {
        crashed,
        code,
        stderr_tail,
        ..
    } = exited
    else {
        unreachable!()
    };
    assert!(crashed);
    assert_eq!(code, Some(3));
    assert_eq!(stderr_tail, ["boom: fake crash"]);
    assert!(matches!(
        m.send(sid, initialize()).await,
        Err(LspError::Stopped)
    ));
    m.stop(sid).await.unwrap();
}

#[tokio::test]
async fn framing_garbage_stops_the_session() {
    let f = fx();
    let (_ws, m, sid, events) = trusted_start(&f, ServerKind::Terraform, "garbage").await;
    let exited = wait_for(&events, |e| matches!(e, LspEvent::Exited { .. })).await;
    let LspEvent::Exited { crashed, error, .. } = exited else {
        unreachable!()
    };
    assert!(crashed);
    assert!(error.unwrap().contains("framing"));
    m.stop(sid).await.unwrap();
}

#[tokio::test]
async fn stubborn_server_is_killed_after_the_grace_periods() {
    let f = fx();
    let (_ws, m, sid, events) = trusted_start(&f, ServerKind::Terraform, "stubborn").await;
    m.send(sid, initialize()).await.unwrap();
    wait_for(&events, |e| matches!(e, LspEvent::Message { .. })).await;
    let started = Instant::now();
    m.stop(sid).await.unwrap();
    // stdin EOF ignored -> 2 s grace -> SIGINT ignored (2 s) -> SIGTERM
    let took = started.elapsed();
    assert!(
        took >= Duration::from_secs(4) && took < Duration::from_secs(10),
        "{took:?}"
    );
    let pid: i32 = std::fs::read_to_string(f.base.join("started"))
        .unwrap()
        .parse()
        .unwrap();
    assert!(
        !Utf8Path::new(&format!("/proc/{pid}/cmdline")).exists() || {
            let stat = std::fs::read_to_string(format!("/proc/{pid}/stat")).unwrap_or_default();
            stat.contains(") Z ")
        }
    );
    let exited = wait_for(&events, |e| matches!(e, LspEvent::Exited { .. })).await;
    assert!(
        matches!(exited, LspEvent::Exited { crashed: false, .. }),
        "{exited:?}"
    );
}

#[tokio::test]
async fn restart_budget_stops_crash_loops() {
    let f = fx();
    let (ws, id) = workspace(&f);
    ws.set_trusted(id, true).unwrap();
    let m = LspManager::default();
    let mut ids = Vec::new();
    for _ in 0..MAX_STARTS {
        let (_e, emit) = sink();
        ids.push(
            ws.with_trusted(id, |t| {
                Ok(m.start(t, ServerKind::Terraform, &ctx(&f, "normal"), emit)?)
            })
            .unwrap(),
        );
    }
    let (_e, emit) = sink();
    let r = ws.with_trusted(id, |t| {
        Ok(m.start(t, ServerKind::Terraform, &ctx(&f, "normal"), emit)?)
    });
    assert!(
        matches!(r, Err(CoreError::Lsp(LspError::RestartLimit))),
        "{:?}",
        r.err()
    );
    // other server kinds have their own budget
    let (_e, emit) = sink();
    ids.push(
        ws.with_trusted(id, |t| {
            Ok(m.start(t, ServerKind::Ansible, &ctx(&f, "normal"), emit)?)
        })
        .unwrap(),
    );
    m.stop_all().await;
    assert_eq!(m.session_count(), 0);
}
