//! `project` module (DESIGN.md section 4.1): open a directory, detect project types,
//! build the semantic tree and give confined file access.

mod error;
pub mod fs;

pub use error::ProjectError;
