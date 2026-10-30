use thiserror::Error;

use crate::core::lsp::LspError;
use crate::core::project::ProjectError;
use crate::core::toolchain::ToolchainError;
use crate::platform::PlatformError;

/// Errors produced by the core. Converted to `CommandError` at the command boundary.
#[derive(Debug, Error)]
pub enum CoreError {
    #[error("invalid input: {0}")]
    InvalidInput(String),
    #[error("the event stream to the frontend was closed")]
    StreamClosed,
    #[error(transparent)]
    Platform(#[from] PlatformError),
    #[error(transparent)]
    Project(#[from] ProjectError),
    #[error(transparent)]
    Toolchain(#[from] ToolchainError),
    #[error(transparent)]
    Lsp(#[from] LspError),
}

impl CoreError {
    pub fn code(&self) -> &'static str {
        match self {
            Self::InvalidInput(_) => "invalid_input",
            Self::StreamClosed => "stream_closed",
            Self::Platform(_) => "platform",
            Self::Project(e) => e.code(),
            Self::Toolchain(e) => e.code(),
            Self::Lsp(e) => e.code(),
        }
    }

    pub fn hint(&self) -> Option<&'static str> {
        match self {
            Self::InvalidInput(_) | Self::StreamClosed => None,
            Self::Platform(PlatformError::NoHomeDir) => {
                Some("Set the HOME environment variable or the XDG base directory variables.")
            }
            Self::Project(e) => e.hint(),
            Self::Toolchain(e) => e.hint(),
            Self::Lsp(e) => e.hint(),
        }
    }
}
