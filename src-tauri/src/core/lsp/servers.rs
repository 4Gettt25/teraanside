//! How each language server is started and which settings the core enforces.

use camino::{Utf8Path, Utf8PathBuf};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use ts_rs::TS;

use super::LspError;
use crate::core::runner::{ChildEnv, RunSpec, StopTimeouts};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum ServerKind {
    Terraform,
    Ansible,
}

/// Resolved inputs for starting a server (paths come from the toolchain).
#[derive(Debug, Clone, Default)]
pub struct LaunchContext {
    pub project_root: Utf8PathBuf,
    pub terraform_ls: Option<Utf8PathBuf>,
    pub node: Option<Utf8PathBuf>,
    pub als_bundle: Option<Utf8PathBuf>,
    /// Ansible venv of the project (must contain `bin/python`).
    pub venv: Option<Utf8PathBuf>,
    pub env: ChildEnv,
}

/// Grace periods when stopping a server after closing its stdin.
const STOP: StopTimeouts = StopTimeouts {
    after_interrupt: std::time::Duration::from_secs(2),
    after_terminate: std::time::Duration::from_secs(2),
};

pub fn run_spec(kind: ServerKind, ctx: &LaunchContext) -> Result<RunSpec, LspError> {
    let missing = |what: &str| LspError::Unavailable(what.to_owned());
    match kind {
        ServerKind::Terraform => {
            let program = ctx
                .terraform_ls
                .clone()
                .ok_or_else(|| missing("terraform-ls"))?;
            Ok(RunSpec {
                program,
                args: vec!["serve".into()],
                cwd: ctx.project_root.clone(),
                env: ctx.env.clone(),
                stop: STOP,
            })
        }
        ServerKind::Ansible => {
            let node = ctx.node.clone().ok_or_else(|| missing("Node.js"))?;
            let bundle = ctx
                .als_bundle
                .clone()
                .ok_or_else(|| missing("ansible-language-server"))?;
            let venv = ctx
                .venv
                .clone()
                .ok_or_else(|| missing("Ansible environment"))?;
            Ok(RunSpec {
                program: node,
                args: vec![bundle.to_string(), "--stdio".into()],
                cwd: ctx.project_root.clone(),
                // ansible-lint and ansible-* from the project venv
                env: ctx.env.clone().prepend_path(&venv.join("bin")),
                stop: STOP,
            })
        }
    }
}

/// Settings the core forces for ansible-language-server, whatever the client sends:
/// execution environments stay off (that code path runs shell strings and pulls
/// container images) and Python/ansible come from the project venv.
pub fn enforce_ansible_settings(section: &mut Value, venv: &Utf8Path) {
    if !section.is_object() {
        *section = json!({});
    }
    let python = venv.join("bin").join("python");
    set(section, &["executionEnvironment", "enabled"], json!(false));
    set(
        section,
        &["python", "interpreterPath"],
        json!(python.as_str()),
    );
    set(section, &["python", "activationScript"], json!(""));
    set(
        section,
        &["ansible", "path"],
        json!(venv.join("bin").join("ansible").as_str()),
    );
    set(
        section,
        &["validation", "lint", "path"],
        json!(venv.join("bin").join("ansible-lint").as_str()),
    );
}

fn set(root: &mut Value, path: &[&str], value: Value) {
    let mut cur = root;
    for (i, key) in path.iter().enumerate() {
        let Some(obj) = cur.as_object_mut() else {
            return;
        };
        if i == path.len() - 1 {
            obj.insert((*key).to_owned(), value);
            return;
        }
        let entry = obj.entry((*key).to_owned()).or_insert_with(|| json!({}));
        if !entry.is_object() {
            *entry = json!({});
        }
        cur = entry;
    }
}

/// Patches a client→server message for ansible-language-server:
/// responses to `workspace/configuration` (sections given by the request) and
/// `workspace/didChangeConfiguration` notifications.
pub fn enforce_ansible_message(
    msg: &mut Value,
    config_sections: Option<&[Option<String>]>,
    venv: &Utf8Path,
) {
    if let Some(sections) = config_sections
        && let Some(items) = msg.get_mut("result").and_then(Value::as_array_mut)
    {
        for (item, section) in items.iter_mut().zip(sections) {
            match section.as_deref() {
                Some("ansible") => enforce_ansible_settings(item, venv),
                // whole configuration or unknown section: enforce below "ansible"
                None | Some("") => {
                    if !item.is_object() {
                        *item = json!({});
                    }
                    let ansible = item
                        .as_object_mut()
                        .map(|o| o.entry("ansible").or_insert_with(|| json!({})));
                    if let Some(a) = ansible {
                        enforce_ansible_settings(a, venv);
                    }
                }
                Some(other) if other.starts_with("ansible.") => {
                    // sub-section requested: enforce on a temporary root and copy back
                    let mut root = json!({});
                    let key = other.trim_start_matches("ansible.");
                    if let Some(o) = root.as_object_mut() {
                        o.insert(key.to_owned(), item.take());
                    }
                    enforce_ansible_settings(&mut root, venv);
                    *item = root.get(key).cloned().unwrap_or(Value::Null);
                }
                Some(_) => {}
            }
        }
    }
    if msg.get("method").and_then(Value::as_str) == Some("workspace/didChangeConfiguration")
        && let Some(settings) = msg.pointer_mut("/params/settings")
    {
        if !settings.is_object() {
            *settings = json!({});
        }
        if let Some(o) = settings.as_object_mut() {
            enforce_ansible_settings(o.entry("ansible").or_insert_with(|| json!({})), venv);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const VENV: &str = "/data/venvs/abc";

    #[test]
    fn configuration_response_is_forced_safe() {
        let mut msg = json!({"jsonrpc": "2.0", "id": 4, "result": [
            {"executionEnvironment": {"enabled": true, "image": "evil"}, "python": {"interpreterPath": "/tmp/evil"}},
            null,
            {"unrelated": 1}
        ]});
        let sections = [Some("ansible".to_owned()), None, Some("editor".to_owned())];
        enforce_ansible_message(&mut msg, Some(&sections), Utf8Path::new(VENV));
        let r = &msg["result"];
        assert_eq!(r[0]["executionEnvironment"]["enabled"], json!(false));
        assert_eq!(r[0]["executionEnvironment"]["image"], json!("evil")); // untouched, inert
        assert_eq!(
            r[0]["python"]["interpreterPath"],
            json!("/data/venvs/abc/bin/python")
        );
        assert_eq!(
            r[0]["validation"]["lint"]["path"],
            json!("/data/venvs/abc/bin/ansible-lint")
        );
        assert_eq!(
            r[1]["ansible"]["executionEnvironment"]["enabled"],
            json!(false)
        );
        assert_eq!(r[2], json!({"unrelated": 1}));
    }

    #[test]
    fn sub_section_and_change_notification_are_enforced() {
        let mut msg = json!({"id": 1, "result": [{"enabled": true}]});
        let sections = [Some("ansible.executionEnvironment".to_owned())];
        enforce_ansible_message(&mut msg, Some(&sections), Utf8Path::new(VENV));
        assert_eq!(msg["result"][0]["enabled"], json!(false));

        let mut note = json!({"jsonrpc": "2.0", "method": "workspace/didChangeConfiguration",
            "params": {"settings": {"ansible": {"executionEnvironment": {"enabled": true}}}}});
        enforce_ansible_message(&mut note, None, Utf8Path::new(VENV));
        assert_eq!(
            note["params"]["settings"]["ansible"]["executionEnvironment"]["enabled"],
            json!(false)
        );
    }

    #[test]
    fn other_messages_are_untouched() {
        let original = json!({"jsonrpc": "2.0", "id": 9, "result": {"items": []}});
        let mut msg = original.clone();
        enforce_ansible_message(&mut msg, None, Utf8Path::new(VENV));
        assert_eq!(msg, original);
    }

    #[test]
    fn launch_specs() {
        let ctx = LaunchContext {
            project_root: "/p".into(),
            terraform_ls: Some("/d/bin/terraform-ls/0.39.0/terraform-ls".into()),
            node: Some("/d/bin/node/24.22.0/node".into()),
            als_bundle: Some("/app/als/ansible-language-server.cjs".into()),
            venv: Some(VENV.into()),
            env: ChildEnv::empty().set("PATH", "/usr/bin"),
        };
        let tf = run_spec(ServerKind::Terraform, &ctx).unwrap();
        assert_eq!(tf.args, ["serve"]);
        let als = run_spec(ServerKind::Ansible, &ctx).unwrap();
        assert_eq!(als.program, "/d/bin/node/24.22.0/node");
        assert_eq!(
            als.args,
            ["/app/als/ansible-language-server.cjs", "--stdio"]
        );
        assert_eq!(als.env.get("PATH"), Some("/data/venvs/abc/bin:/usr/bin"));
        let none = LaunchContext::default();
        assert!(matches!(
            run_spec(ServerKind::Ansible, &none),
            Err(LspError::Unavailable(_))
        ));
    }
}
