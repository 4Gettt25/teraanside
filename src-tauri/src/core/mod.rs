//! Platform-neutral application core. All process execution and file writes
//! happen here (see DESIGN.md section 3).

mod error;

pub mod ansible;
pub mod diagnostics;
pub mod git;
pub mod lsp;
pub mod project;
pub mod runner;
pub mod schema;
pub mod secrets;
pub mod settings;
pub mod terraform;
pub mod toolchain;
pub mod workspace;

pub use error::CoreError;
