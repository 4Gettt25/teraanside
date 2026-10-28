//! Thin Tauri commands: validate input, delegate to `core`, map errors.

pub mod counter;
mod error;
pub mod ping;

pub use error::CommandError;
