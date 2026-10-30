//! Runner tests with fake binaries (small shell scripts written per test).

use std::os::unix::fs::PermissionsExt;
use std::time::{Duration, Instant};

use camino::{Utf8Path, Utf8PathBuf};
use tokio::sync::mpsc;

use super::*;

struct Fake {
    _dir: tempfile::TempDir,
    path: Utf8PathBuf,
}

fn fake(script: &str) -> Fake {
    let dir = tempfile::tempdir().unwrap();
    let path = Utf8PathBuf::from_path_buf(dir.path().join("fake-tool")).unwrap();
    std::fs::write(&path, format!("#!/bin/sh\n{script}\n")).unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
    Fake { _dir: dir, path }
}

fn spec(program: &Utf8Path) -> RunSpec {
    RunSpec {
        program: program.to_owned(),
        args: vec![],
        cwd: Utf8PathBuf::from("/"),
        env: ChildEnv::empty().set("PATH", "/usr/bin:/bin"),
        stop: StopTimeouts {
            after_interrupt: Duration::from_millis(500),
            after_terminate: Duration::from_millis(500),
        },
    }
}

async fn run(spec: RunSpec) -> (Vec<RunEvent>, RunOutcome) {
    let (tx, mut rx) = mpsc::channel(1024);
    let handle = spawn(spec, tx).unwrap();
    let outcome = handle.wait().await.unwrap();
    let mut events = Vec::new();
    while let Ok(e) = rx.try_recv() {
        events.push(e);
    }
    (events, outcome)
}

fn lines(events: &[RunEvent], wanted: OutputStream) -> Vec<String> {
    events
        .iter()
        .filter_map(|e| match e {
            RunEvent::Line { stream, text, .. } if *stream == wanted => Some(text.clone()),
            _ => None,
        })
        .collect()
}

#[tokio::test]
async fn streams_lines_and_exit_code() {
    let f = fake("echo one; echo err1 >&2; printf 'two\\r\\nthree'; exit 3");
    let mut s = spec(&f.path);
    s.args = vec!["ignored arg with spaces".into()];
    let (events, outcome) = run(s).await;
    assert!(matches!(events.first(), Some(RunEvent::Started { .. })));
    assert_eq!(
        lines(&events, OutputStream::Stdout),
        ["one", "two", "three"]
    );
    assert_eq!(lines(&events, OutputStream::Stderr), ["err1"]);
    assert_eq!(
        outcome,
        RunOutcome {
            code: Some(3),
            signal: None,
            cancelled: false
        }
    );
    assert!(matches!(events.last(), Some(RunEvent::Exited(o)) if *o == outcome));
}

#[tokio::test]
async fn arguments_are_passed_verbatim_without_shell() {
    let f = fake("for a in \"$@\"; do echo \"[$a]\"; done");
    let mut s = spec(&f.path);
    s.args = vec!["a b".into(), "$(id)".into(), ";rm -rf /".into(), "*".into()];
    let (events, _) = run(s).await;
    assert_eq!(
        lines(&events, OutputStream::Stdout),
        ["[a b]", "[$(id)]", "[;rm -rf /]", "[*]"]
    );
}

#[tokio::test]
async fn child_only_sees_the_allowlisted_environment() {
    let base = ChildEnv::from_allowlist_of([
        ("HOME".to_owned(), "/home/u".to_owned()),
        ("LC_TIME".to_owned(), "de_DE.UTF-8".to_owned()),
        ("AWS_SECRET_ACCESS_KEY".to_owned(), "s3cr3t".to_owned()),
        ("HCLOUD_TOKEN".to_owned(), "t0ken".to_owned()),
    ]);
    assert_eq!(base.get("HOME"), Some("/home/u"));
    assert_eq!(base.get("LC_TIME"), Some("de_DE.UTF-8"));
    assert_eq!(base.get("AWS_SECRET_ACCESS_KEY"), None);
    assert_eq!(base.get("HCLOUD_TOKEN"), None);

    let f = fake("echo \"HOME=$HOME SECRET=${AWS_SECRET_ACCESS_KEY:-unset} EXTRA=$EXTRA\"");
    let mut s = spec(&f.path);
    s.env = base.set("EXTRA", "1");
    let (events, _) = run(s).await;
    assert_eq!(
        lines(&events, OutputStream::Stdout),
        ["HOME=/home/u SECRET=unset EXTRA=1"]
    );
}

#[test]
fn prepend_path() {
    let env = ChildEnv::empty()
        .set("PATH", "/usr/bin")
        .prepend_path(Utf8Path::new("/venv/bin"));
    assert_eq!(env.get("PATH"), Some("/venv/bin:/usr/bin"));
    let env = ChildEnv::empty().prepend_path(Utf8Path::new("/x"));
    assert_eq!(env.get("PATH"), Some("/x"));
}

#[tokio::test]
async fn rejects_relative_and_missing_programs() {
    let (tx, _rx) = mpsc::channel(1);
    assert!(matches!(
        spawn(spec(Utf8Path::new("terraform")), tx.clone()),
        Err(RunnerError::RelativeProgram(_))
    ));
    assert!(matches!(
        spawn(spec(Utf8Path::new("/nonexistent/tool")), tx),
        Err(RunnerError::Spawn { .. })
    ));
}

#[tokio::test]
async fn cancel_sends_interrupt_first() {
    let f = fake("trap 'echo got-int; exit 130' INT; echo ready; while :; do sleep 0.05; done");
    let (tx, mut rx) = mpsc::channel(1024);
    let mut handle = spawn(spec(&f.path), tx).unwrap();
    wait_for_line(&mut rx, "ready").await;
    handle.cancel();
    let outcome = handle.wait().await.unwrap();
    assert!(outcome.cancelled);
    assert_eq!(outcome.code, Some(130));
    let mut rest = Vec::new();
    while let Ok(e) = rx.try_recv() {
        rest.push(e);
    }
    assert!(lines(&rest, OutputStream::Stdout).contains(&"got-int".to_owned()));
}

#[tokio::test]
async fn cancel_escalates_to_kill_for_stubborn_processes() {
    let f = fake("trap '' INT TERM; echo ready; while :; do sleep 0.05; done");
    let (tx, mut rx) = mpsc::channel(1024);
    let mut handle = spawn(spec(&f.path), tx).unwrap();
    wait_for_line(&mut rx, "ready").await;
    let started = Instant::now();
    handle.cancel();
    let outcome = handle.wait().await.unwrap();
    assert!(outcome.cancelled);
    assert_eq!(outcome.signal, Some(9), "{outcome:?}");
    // 500 ms after SIGINT + 500 ms after SIGTERM, then SIGKILL
    assert!(started.elapsed() >= Duration::from_millis(900));
    assert!(started.elapsed() < Duration::from_secs(5));
}

#[tokio::test]
async fn cancel_stops_descendants_too() {
    let f = fake("sleep 30 & echo \"child=$!\"; wait");
    let (tx, mut rx) = mpsc::channel(1024);
    let mut handle = spawn(spec(&f.path), tx).unwrap();
    let line = wait_for_prefix(&mut rx, "child=").await;
    let grandchild: u32 = line.trim_start_matches("child=").parse().unwrap();
    handle.cancel();
    handle.wait().await.unwrap();
    assert_gone(grandchild).await;
}

#[tokio::test]
async fn descendants_holding_pipes_do_not_block_completion() {
    // The main process exits, a background child keeps stdout open.
    let f = fake("sleep 30 & echo \"child=$!\"; exit 0");
    let started = Instant::now();
    let (events, outcome) = run(spec(&f.path)).await;
    assert_eq!(outcome.code, Some(0));
    assert!(started.elapsed() < Duration::from_secs(10));
    let line = lines(&events, OutputStream::Stdout).remove(0);
    assert_gone(line.trim_start_matches("child=").parse().unwrap()).await;
}

#[tokio::test]
async fn long_lines_are_truncated_and_invalid_utf8_is_replaced() {
    let f = fake(&format!(
        "head -c {} /dev/zero | tr '\\0' 'x'; echo; printf '\\377ok\\n'",
        lines::MAX_LINE_BYTES + 10
    ));
    let (events, _) = run(spec(&f.path)).await;
    let outs: Vec<_> = events
        .iter()
        .filter_map(|e| match e {
            RunEvent::Line {
                text, truncated, ..
            } => Some((text.len(), *truncated, text.clone())),
            _ => None,
        })
        .collect();
    assert_eq!(outs[0].0, lines::MAX_LINE_BYTES);
    assert!(outs[0].1);
    assert_eq!(outs[1].2, "\u{fffd}ok");
    assert!(!outs[1].1);
}

async fn wait_for_line(rx: &mut mpsc::Receiver<RunEvent>, wanted: &str) {
    wait_for_prefix(rx, wanted).await;
}

async fn wait_for_prefix(rx: &mut mpsc::Receiver<RunEvent>, prefix: &str) -> String {
    let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
    loop {
        let event = tokio::time::timeout_at(deadline, rx.recv())
            .await
            .expect("timed out waiting for output")
            .expect("channel closed");
        if let RunEvent::Line { text, .. } = event
            && text.starts_with(prefix)
        {
            return text;
        }
    }
}

async fn assert_gone(pid: u32) {
    let deadline = Instant::now() + Duration::from_secs(3);
    loop {
        let stat = std::fs::read_to_string(format!("/proc/{pid}/stat")).unwrap_or_default();
        // gone, or a zombie waiting for init to reap it
        let state = stat
            .rsplit(')')
            .next()
            .and_then(|r| r.split_whitespace().next());
        if stat.is_empty() || state == Some("Z") {
            return;
        }
        assert!(
            Instant::now() < deadline,
            "process {pid} still running: {stat}"
        );
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

#[tokio::test]
async fn stdio_mode_gives_raw_streams_and_still_reports_stderr_and_exit() {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    // echo stdin back, one note on stderr
    let f = fake("echo note >&2; cat");
    let (tx, mut rx) = mpsc::channel(64);
    let mut p = spawn_stdio(spec(&f.path), tx).unwrap();
    p.stdin
        .write_all(b"Content-Length: 2\r\n\r\n{}")
        .await
        .unwrap();
    drop(p.stdin); // EOF -> cat exits
    let mut out = Vec::new();
    p.stdout.read_to_end(&mut out).await.unwrap();
    assert_eq!(out, b"Content-Length: 2\r\n\r\n{}");
    let outcome = p.handle.wait().await.unwrap();
    assert_eq!(outcome.code, Some(0));
    let mut events = Vec::new();
    while let Ok(e) = rx.try_recv() {
        events.push(e);
    }
    assert_eq!(lines(&events, OutputStream::Stderr), ["note"]);
    assert!(
        lines(&events, OutputStream::Stdout).is_empty(),
        "stdout is not consumed by the runner"
    );
    assert!(matches!(events.last(), Some(RunEvent::Exited(_))));
}

#[tokio::test]
async fn stdio_mode_cancel_stops_the_process() {
    let f = fake("trap '' INT; cat");
    let (tx, _rx) = mpsc::channel(64);
    let mut p = spawn_stdio(spec(&f.path), tx).unwrap();
    p.handle.cancel();
    let outcome = p.handle.wait().await.unwrap();
    assert!(outcome.cancelled);
}
