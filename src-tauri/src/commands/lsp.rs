use std::sync::Arc;

use serde_json::Value;
use tauri::ipc::Channel;
use tauri::{Manager, State};

use crate::commands::CommandError;
use crate::core::lsp::{LaunchContext, LspEvent, LspManager, ServerKind, SessionId};
use crate::core::runner::ChildEnv;
use crate::core::toolchain::service::Toolchain;
use crate::core::workspace::{ProjectId, Workspace};

/// Relative path of the bundled ansible-language-server inside the app resources.
const ALS_RESOURCE: &str = "resources/als/ansible-language-server.cjs";

/// Starts a language server for a trusted project. Fails with `not_trusted`
/// for untrusted projects and `lsp_unavailable` if tools are missing.
#[tauri::command]
pub async fn lsp_start(
    app: tauri::AppHandle,
    ws: State<'_, Arc<Workspace>>,
    tc: State<'_, Arc<Toolchain>>,
    lsp: State<'_, Arc<LspManager>>,
    id: ProjectId,
    kind: ServerKind,
    on_event: Channel<LspEvent>,
) -> Result<SessionId, CommandError> {
    let entry = ws.entry(id)?;
    let project = entry.project();
    let paths = {
        let (tc, entry) = (tc.inner().clone(), entry.clone());
        tauri::async_runtime::spawn_blocking(move || tc.lsp_paths(entry.project()))
            .await
            .map_err(|e| CommandError {
                code: "internal".into(),
                message: e.to_string(),
                hint: None,
            })?
    };
    let als_bundle = app
        .path()
        .resource_dir()
        .ok()
        .map(|d| d.join(ALS_RESOURCE))
        .filter(|p| p.is_file())
        .and_then(|p| camino::Utf8PathBuf::from_path_buf(p).ok());
    let ctx = LaunchContext {
        project_root: project.root().to_owned(),
        terraform_ls: paths.terraform_ls,
        node: paths.node,
        als_bundle,
        venv: paths.venv,
        env: ChildEnv::from_allowlist(),
    };
    let session = ws.with_trusted(id, |trusted| {
        Ok(lsp.start(trusted, kind, &ctx, move |event| {
            // a closed channel (window gone) is handled by stop_all on exit
            let _ = on_event.send(event);
        })?)
    })?;
    Ok(session)
}

#[tauri::command]
pub async fn lsp_send(
    lsp: State<'_, Arc<LspManager>>,
    session: SessionId,
    message: Value,
) -> Result<(), CommandError> {
    Ok(lsp.send(session, message).await?)
}

#[tauri::command]
pub async fn lsp_stop(
    lsp: State<'_, Arc<LspManager>>,
    session: SessionId,
) -> Result<(), CommandError> {
    Ok(lsp.stop(session).await?)
}
