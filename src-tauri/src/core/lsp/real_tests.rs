//! End-to-end with the real servers (network: installs the toolchain).
//! Run: `cargo test real_language_servers -- --ignored --nocapture`

use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use camino::Utf8PathBuf;
use serde_json::{Value, json};

use super::*;
use crate::core::project::config::TerraformFlavor;
use crate::core::project::tree;
use crate::core::runner::ChildEnv;
use crate::core::toolchain::service::Toolchain;
use crate::core::workspace::Workspace;
use crate::platform::AppDirs;

type Inbox = Arc<Mutex<Vec<Value>>>;

struct Client {
    m: Arc<LspManager>,
    sid: SessionId,
    inbox: Inbox,
    next: i64,
}

impl Client {
    async fn request(&mut self, method: &str, params: Value) -> Value {
        self.next += 1;
        let id = self.next;
        self.m
            .send(
                self.sid,
                json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params}),
            )
            .await
            .unwrap();
        self.wait(
            |m| m.get("id") == Some(&json!(id)) && m.get("method").is_none(),
            60,
        )
        .await
    }

    async fn notify(&self, method: &str, params: Value) {
        self.m
            .send(
                self.sid,
                json!({"jsonrpc": "2.0", "method": method, "params": params}),
            )
            .await
            .unwrap();
    }

    /// Waits for a server message; answers server requests on the way.
    async fn wait(&self, pred: impl Fn(&Value) -> bool, secs: u64) -> Value {
        let deadline = Instant::now() + Duration::from_secs(secs);
        let mut answered = std::collections::HashSet::new();
        loop {
            let msgs: Vec<Value> = self.inbox.lock().unwrap().clone();
            for m in &msgs {
                if let (Some(id), Some(method)) =
                    (m.get("id"), m.get("method").and_then(Value::as_str))
                    && answered.insert(id.to_string())
                {
                    let result = match method {
                        "workspace/configuration" => {
                            let n = m
                                .pointer("/params/items")
                                .and_then(Value::as_array)
                                .map_or(1, Vec::len);
                            json!(vec![json!({}); n])
                        }
                        _ => Value::Null,
                    };
                    self.m
                        .send(
                            self.sid,
                            json!({"jsonrpc": "2.0", "id": id, "result": result}),
                        )
                        .await
                        .unwrap();
                }
            }
            if let Some(found) = msgs.iter().find(|m| pred(m)) {
                return found.clone();
            }
            assert!(
                Instant::now() < deadline,
                "timed out; got {} messages",
                msgs.len()
            );
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    }
}

#[tokio::test]
#[ignore = "network: installs tools, runs terraform-ls and ansible-language-server"]
#[allow(clippy::print_stderr)]
async fn real_language_servers() {
    let tmp = tempfile::tempdir().unwrap();
    let base = Utf8PathBuf::from_path_buf(tmp.path().to_path_buf()).unwrap();
    let root = base.join("repo");
    std::fs::create_dir_all(root.join("roles/web/tasks")).unwrap();
    let main_tf = "resource \"null_resource\" \"x\" {\n  unknown_attribute = true\n  \n}\n";
    std::fs::write(root.join("main.tf"), main_tf).unwrap();
    std::fs::write(root.join("ansible.cfg"), "[defaults]\n").unwrap();
    let playbook = "---\n- name: Demo\n  hosts: localhost\n  tasks:\n    - shell: echo hi\n";
    std::fs::write(root.join("site.yml"), playbook).unwrap();

    let dirs = AppDirs {
        config: base.join("config"),
        data: base.join("data"),
        cache: base.join("cache"),
        state: base.join("state"),
    };
    let ws = Workspace::init(&dirs).unwrap();
    let id = ws.open(&root).unwrap().id;
    ws.set_trusted(id, true).unwrap();
    let tc = Toolchain::new(dirs.clone()).unwrap();
    let entry = ws.entry(id).unwrap();
    let project = entry.project();
    let detected = tree::build_tree(project.fs(), project.name())
        .unwrap()
        .detected;
    tc.choose_flavor(project, TerraformFlavor::Opentofu)
        .unwrap();
    let status = tc.status(project, detected);
    let t0 = Instant::now();
    tc.setup(project, &status, |_| {}).await.unwrap();
    eprintln!("toolchain set up in {:.1}s", t0.elapsed().as_secs_f32());
    let paths = tc.lsp_paths(project);
    let ctx = LaunchContext {
        project_root: project.root().to_owned(),
        terraform_ls: paths.terraform_ls,
        node: paths.node,
        als_bundle: Some(
            Utf8PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("resources/als/ansible-language-server.cjs"),
        ),
        venv: paths.venv,
        env: ChildEnv::from_allowlist(),
    };
    let m = Arc::new(LspManager::default());
    let root_uri = format!("file://{}", project.root());

    // --- terraform-ls ---
    let inbox: Inbox = Arc::default();
    let sink = inbox.clone();
    let sid = ws
        .with_trusted(id, |t| {
            Ok(m.start(t, ServerKind::Terraform, &ctx, move |e| {
                if let LspEvent::Message { payload } = e {
                    sink.lock().unwrap().push(payload);
                }
            })?)
        })
        .unwrap();
    let mut c = Client {
        m: m.clone(),
        sid,
        inbox,
        next: 0,
    };
    let t = Instant::now();
    let init = c
        .request("initialize", json!({"processId": null, "rootUri": root_uri, "capabilities": {"textDocument": {"completion": {"completionItem": {"snippetSupport": true}}}}}))
        .await;
    eprintln!("terraform-ls initialize: {} ms", t.elapsed().as_millis());
    assert!(init["result"]["capabilities"]["completionProvider"].is_object());
    c.notify("initialized", json!({})).await;
    let uri = format!("{root_uri}/main.tf");
    c.notify("textDocument/didOpen", json!({"textDocument": {"uri": uri, "languageId": "terraform", "version": 1, "text": main_tf}})).await;
    let diag = c
        .wait(
            |m| {
                m["method"] == "textDocument/publishDiagnostics"
                    && m["params"]["uri"] == uri
                    && !m["params"]["diagnostics"]
                        .as_array()
                        .is_none_or(Vec::is_empty)
            },
            30,
        )
        .await;
    eprintln!(
        "terraform diagnostics: {}",
        diag["params"]["diagnostics"][0]["message"]
    );
    let t = Instant::now();
    let comp = c
        .request(
            "textDocument/completion",
            json!({"textDocument": {"uri": uri}, "position": {"line": 2, "character": 2}}),
        )
        .await;
    let items = comp["result"]["items"]
        .as_array()
        .or_else(|| comp["result"].as_array())
        .cloned()
        .unwrap_or_default();
    eprintln!(
        "terraform completion: {} items in {} ms",
        items.len(),
        t.elapsed().as_millis()
    );
    assert!(items.iter().any(|i| i["label"] == "count"), "{items:?}");
    m.stop(sid).await.unwrap();

    // --- ansible-language-server ---
    let inbox: Inbox = Arc::default();
    let sink = inbox.clone();
    let sid = ws
        .with_trusted(id, |t| {
            Ok(m.start(t, ServerKind::Ansible, &ctx, move |e| {
                if let LspEvent::Message { payload } = e {
                    sink.lock().unwrap().push(payload);
                }
            })?)
        })
        .unwrap();
    let mut c = Client {
        m: m.clone(),
        sid,
        inbox,
        next: 0,
    };
    let t = Instant::now();
    let init = c
        .request("initialize", json!({"processId": null, "rootUri": root_uri, "workspaceFolders": [{"uri": root_uri, "name": "repo"}], "capabilities": {"workspace": {"configuration": true}, "textDocument": {"hover": {"contentFormat": ["markdown"]}}}}))
        .await;
    eprintln!(
        "ansible-language-server initialize: {} ms",
        t.elapsed().as_millis()
    );
    assert!(
        init["result"]["capabilities"]["hoverProvider"]
            .as_bool()
            .unwrap_or(false)
    );
    c.notify("initialized", json!({})).await;
    let uri = format!("{root_uri}/site.yml");
    c.notify("textDocument/didOpen", json!({"textDocument": {"uri": uri, "languageId": "ansible", "version": 1, "text": playbook}})).await;
    let t = Instant::now();
    let diag = c
        .wait(
            |m| {
                m["method"] == "textDocument/publishDiagnostics"
                    && m["params"]["uri"] == uri
                    && !m["params"]["diagnostics"]
                        .as_array()
                        .is_none_or(Vec::is_empty)
            },
            90,
        )
        .await;
    let msgs: Vec<String> = diag["params"]["diagnostics"]
        .as_array()
        .unwrap()
        .iter()
        .map(|d| format!("{} {}", d["source"], d["code"]))
        .collect();
    eprintln!(
        "ansible diagnostics after {} ms: {msgs:?}",
        t.elapsed().as_millis()
    );
    assert!(msgs.iter().any(|m| m.contains("ansible-lint")), "{msgs:?}");
    let hover = c
        .request(
            "textDocument/hover",
            json!({"textDocument": {"uri": uri}, "position": {"line": 4, "character": 8}}),
        )
        .await;
    let text = hover["result"]["contents"].to_string();
    eprintln!(
        "ansible hover: {}",
        text.chars().take(80).collect::<String>()
    );
    assert!(
        text.to_lowercase().contains("shell") || text.contains("command"),
        "{text}"
    );
    m.stop(sid).await.unwrap();
}
