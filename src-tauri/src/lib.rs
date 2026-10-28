#![forbid(unsafe_code)]

pub mod commands;
pub mod core;
pub mod platform;

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

    if let Err(err) = tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![
            commands::ping::ping,
            commands::counter::start_counter
        ])
        .run(tauri::generate_context!())
    {
        tracing::error!(error = %err, "tauri application exited with an error");
        std::process::exit(1);
    }
}
