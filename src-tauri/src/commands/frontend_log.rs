/// Forwards uncaught frontend errors into the core log (DESIGN.md section 9).
/// Messages are truncated; the frontend must not send file contents or secrets.
#[tauri::command]
pub fn frontend_log(message: String) {
    const MAX: usize = 2000;
    let shown: String = message.chars().take(MAX).collect();
    tracing::error!(target: "frontend", "{shown}");
}
