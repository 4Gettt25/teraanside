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

    let state = platform::current()
        .app_dirs()
        .map_err(core::CoreError::from)
        .and_then(|dirs| {
            let ws = core::workspace::Workspace::init(&dirs)?;
            let tc = core::toolchain::service::Toolchain::new(dirs)?;
            Ok((std::sync::Arc::new(ws), std::sync::Arc::new(tc)))
        });
    let (workspace, toolchain) = match state {
        Ok(s) => s,
        Err(err) => {
            tracing::error!(error = %err, "failed to initialise application state");
            std::process::exit(1);
        }
    };

    // `iac-studio [PROJECT_DIR [FILE]]`
    let mut args = std::env::args().skip(1);
    if let Some(dir) = args.next() {
        let file = args.next();
        if let Err(err) = workspace.open_from_args(camino::Utf8Path::new(&dir), file) {
            tracing::warn!(error = %err, %dir, "could not open project from command line");
        }
    }

    if let Err(err) = tauri::Builder::default()
        // Used from Rust only (folder picker); no dialog permission is granted to the webview.
        .plugin(tauri_plugin_dialog::init())
        .manage(workspace)
        .manage(toolchain)
        .invoke_handler(tauri::generate_handler![
            commands::ping::ping,
            commands::counter::start_counter,
            commands::project::project_open_dialog,
            commands::project::project_initial,
            commands::project::project_open_recent,
            commands::project::project_recent,
            commands::project::project_close,
            commands::project::project_set_trusted,
            commands::project::project_tree,
            commands::project::project_watch,
            commands::project::file_read,
            commands::project::file_write,
            commands::frontend_log::frontend_log,
            commands::toolchain::toolchain_status,
            commands::toolchain::toolchain_choose_flavor,
            commands::toolchain::toolchain_setup,
            commands::toolchain::toolchain_cancel,
        ])
        .run(tauri::generate_context!())
    {
        tracing::error!(error = %err, "tauri application exited with an error");
        std::process::exit(1);
    }
}
