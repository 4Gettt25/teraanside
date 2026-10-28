use crate::commands::CommandError;
use crate::core::diagnostics::{self, Pong};
use crate::platform;

/// Round-trip check between frontend and core.
#[tauri::command]
pub async fn ping(message: String) -> Result<Pong, CommandError> {
    tracing::debug!(len = message.len(), "ping");
    Ok(diagnostics::ping(&message, platform::current())?)
}
