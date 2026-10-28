use std::sync::Arc;

use serde::Serialize;
use tauri::State;
use tauri::ipc::Channel;
use tauri_plugin_dialog::DialogExt;
use ts_rs::TS;

use crate::commands::CommandError;
use crate::core::CoreError;
use crate::core::project::fs::FileContent;
use crate::core::project::tree::ProjectTree;
use crate::core::settings::RecentProject;
use crate::core::workspace::{ProjectId, ProjectInfo, Workspace};

/// Runs blocking core work off the async runtime.
async fn blocking<T: Send + 'static>(
    f: impl FnOnce() -> Result<T, CoreError> + Send + 'static,
) -> Result<T, CommandError> {
    tauri::async_runtime::spawn_blocking(f)
        .await
        .map_err(|e| CommandError {
            code: "internal".into(),
            message: format!("background task failed: {e}"),
            hint: None,
        })?
        .map_err(Into::into)
}

/// Shows the native folder dialog and opens the chosen folder. `None` if cancelled.
#[tauri::command]
pub async fn project_open_dialog(
    app: tauri::AppHandle,
    ws: State<'_, Arc<Workspace>>,
    title: String,
) -> Result<Option<ProjectInfo>, CommandError> {
    let (tx, rx) = tokio::sync::oneshot::channel();
    app.dialog().file().set_title(title).pick_folder(move |p| {
        let _ = tx.send(p);
    });
    let picked = rx.await.ok().flatten();
    let Some(path) = picked.and_then(|p| p.into_path().ok()) else {
        return Ok(None);
    };
    let Ok(path) = camino::Utf8PathBuf::from_path_buf(path) else {
        return Err(CommandError {
            code: "invalid_path".into(),
            message: "the selected folder path is not valid UTF-8".into(),
            hint: None,
        });
    };
    let ws = ws.inner().clone();
    blocking(move || ws.open(&path)).await.map(Some)
}

#[tauri::command]
pub async fn project_open_recent(
    ws: State<'_, Arc<Workspace>>,
    path: String,
) -> Result<ProjectInfo, CommandError> {
    let ws = ws.inner().clone();
    blocking(move || ws.open_recent(&path)).await
}

#[tauri::command]
pub async fn project_recent(
    ws: State<'_, Arc<Workspace>>,
) -> Result<Vec<RecentProject>, CommandError> {
    let ws = ws.inner().clone();
    blocking(move || Ok(ws.recent())).await
}

#[tauri::command]
pub fn project_close(ws: State<'_, Arc<Workspace>>, id: ProjectId) {
    ws.close(id);
}

#[tauri::command]
pub async fn project_set_trusted(
    ws: State<'_, Arc<Workspace>>,
    id: ProjectId,
    trusted: bool,
) -> Result<ProjectInfo, CommandError> {
    let ws = ws.inner().clone();
    blocking(move || ws.set_trusted(id, trusted)).await
}

#[tauri::command]
pub async fn project_tree(
    ws: State<'_, Arc<Workspace>>,
    id: ProjectId,
) -> Result<ProjectTree, CommandError> {
    let ws = ws.inner().clone();
    blocking(move || ws.tree(id)).await
}

#[tauri::command]
pub async fn file_read(
    ws: State<'_, Arc<Workspace>>,
    id: ProjectId,
    path: String,
) -> Result<FileContent, CommandError> {
    let ws = ws.inner().clone();
    blocking(move || ws.read_file(id, &path)).await
}

/// Returns the SHA256 of the written content.
#[tauri::command]
pub async fn file_write(
    ws: State<'_, Arc<Workspace>>,
    id: ProjectId,
    path: String,
    text: String,
    expected_sha256: Option<String>,
) -> Result<String, CommandError> {
    let ws = ws.inner().clone();
    blocking(move || ws.write_file(id, &path, &text, expected_sha256.as_deref())).await
}

#[derive(Debug, Clone, Serialize, TS)]
#[serde(tag = "type", rename_all = "camelCase")]
#[ts(export)]
pub enum ProjectEvent {
    Changed { paths: Vec<String> },
}

#[tauri::command]
pub async fn project_watch(
    ws: State<'_, Arc<Workspace>>,
    id: ProjectId,
    on_event: Channel<ProjectEvent>,
) -> Result<(), CommandError> {
    let ws = ws.inner().clone();
    blocking(move || {
        ws.watch(id, move |paths| {
            let _ = on_event.send(ProjectEvent::Changed { paths });
        })
    })
    .await
}
