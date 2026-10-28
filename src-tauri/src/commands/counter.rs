use tauri::ipc::Channel;

use crate::commands::CommandError;
use crate::core::CoreError;
use crate::core::diagnostics::{self, CounterEvent, CounterRequest};

/// Streams demo counter events to the frontend through `on_event`.
/// Resolves once the stream has finished.
#[tauri::command]
pub async fn start_counter(
    request: CounterRequest,
    on_event: Channel<CounterEvent>,
) -> Result<(), CommandError> {
    diagnostics::run_counter(request, |event| {
        on_event.send(event).map_err(|err| {
            tracing::warn!(error = %err, "counter channel closed");
            CoreError::StreamClosed
        })
    })
    .await?;
    Ok(())
}
