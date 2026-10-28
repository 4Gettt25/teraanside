//! Thin Tauri commands: validate input, delegate to `core`, map errors.

mod error;
pub mod ping;

pub use error::CommandError;
