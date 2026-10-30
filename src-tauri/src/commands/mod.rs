//! Thin Tauri commands: validate input, delegate to `core`, map errors.

pub mod counter;
mod error;
pub mod frontend_log;
pub mod lsp;
pub mod ping;
pub mod project;
pub mod toolchain;

pub use error::CommandError;
