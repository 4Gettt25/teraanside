use std::sync::Arc;

use tauri::State;
use tauri::ipc::Channel;

use crate::commands::CommandError;
use crate::core::CoreError;
use crate::core::project::config::TerraformFlavor;
use crate::core::project::tree;
use crate::core::toolchain::service::{SetupEvent, Toolchain, ToolchainStatus};
use crate::core::workspace::{ProjectId, Workspace};

fn status_of(ws: &Workspace, tc: &Toolchain, id: ProjectId) -> Result<ToolchainStatus, CoreError> {
    let entry = ws.entry(id)?;
    let project = entry.project();
    project.reload_config()?;
    let detected = tree::build_tree(project.fs(), project.name())?.detected;
    let status = tc.status(project, detected);
    tracing::debug!(
        id,
        ready = status.ready,
        needs_flavor_choice = status.needs_flavor_choice,
        tools = status.tools.len(),
        "toolchain status"
    );
    Ok(status)
}

#[tauri::command]
pub async fn toolchain_status(
    ws: State<'_, Arc<Workspace>>,
    tc: State<'_, Arc<Toolchain>>,
    id: ProjectId,
) -> Result<ToolchainStatus, CommandError> {
    let (ws, tc) = (ws.inner().clone(), tc.inner().clone());
    tauri::async_runtime::spawn_blocking(move || status_of(&ws, &tc, id))
        .await
        .map_err(internal)?
        .map_err(Into::into)
}

/// Writes the chosen flavor as pin into `project.toml` and returns the new status.
#[tauri::command]
pub async fn toolchain_choose_flavor(
    ws: State<'_, Arc<Workspace>>,
    tc: State<'_, Arc<Toolchain>>,
    id: ProjectId,
    flavor: TerraformFlavor,
) -> Result<ToolchainStatus, CommandError> {
    let (ws, tc) = (ws.inner().clone(), tc.inner().clone());
    tauri::async_runtime::spawn_blocking(move || {
        let entry = ws.entry(id)?;
        tc.choose_flavor(entry.project(), flavor)?;
        status_of(&ws, &tc, id)
    })
    .await
    .map_err(internal)?
    .map_err(Into::into)
}

/// Installs everything missing; streams progress, returns the final status.
#[tauri::command]
pub async fn toolchain_setup(
    ws: State<'_, Arc<Workspace>>,
    tc: State<'_, Arc<Toolchain>>,
    id: ProjectId,
    on_event: Channel<SetupEvent>,
) -> Result<ToolchainStatus, CommandError> {
    let (ws, tc) = (ws.inner().clone(), tc.inner().clone());
    let status = {
        let (ws, tc) = (ws.clone(), tc.clone());
        tauri::async_runtime::spawn_blocking(move || status_of(&ws, &tc, id))
            .await
            .map_err(internal)??
    };
    let entry = ws.entry(id)?;
    tc.setup(entry.project(), &status, |event| {
        let _ = on_event.send(event);
    })
    .await?;
    let (ws2, tc2) = (ws.clone(), tc.clone());
    tauri::async_runtime::spawn_blocking(move || status_of(&ws2, &tc2, id))
        .await
        .map_err(internal)?
        .map_err(Into::into)
}

#[tauri::command]
pub fn toolchain_cancel(tc: State<'_, Arc<Toolchain>>) {
    tc.cancel();
}

fn internal(e: impl std::fmt::Display) -> CommandError {
    CommandError {
        code: "internal".into(),
        message: format!("background task failed: {e}"),
        hint: None,
    }
}
