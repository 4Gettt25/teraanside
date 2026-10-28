#![forbid(unsafe_code)]

pub mod commands;
pub mod core;
pub mod platform;
#[allow(clippy::unwrap_used, clippy::expect_used)]
pub mod spike_lsp;

use tracing_subscriber::EnvFilter;

fn init_tracing() {
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_writer(std::io::stderr)
        .init();
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    init_tracing();
    tracing::info!(version = env!("CARGO_PKG_VERSION"), "starting IaC Studio");

    let app = tauri::Builder::default()
        .manage(spike_lsp::LspSessions::default())
        .invoke_handler(tauri::generate_handler![
            spike_lsp::spike_lsp_start,
            spike_lsp::spike_lsp_send,
            spike_lsp::spike_lsp_stop,
            spike_lsp::spike_log,
            spike_lsp::spike_close_window,
            commands::ping::ping,
            commands::counter::start_counter
        ])
        .build(tauri::generate_context!());
    let app = match app {
        Ok(app) => app,
        Err(err) => {
            tracing::error!(error = %err, "failed to build tauri application");
            std::process::exit(1);
        }
    };
    app.run(|handle, event| {
        if let tauri::RunEvent::ExitRequested { .. } = event {
            use tauri::Manager;
            let state = handle.state::<spike_lsp::LspSessions>();
            tauri::async_runtime::block_on(spike_lsp::stop_all(&state));
        }
    });
}
