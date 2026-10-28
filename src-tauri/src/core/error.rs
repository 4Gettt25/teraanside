use thiserror::Error;

use crate::platform::PlatformError;

/// Errors produced by the core. Converted to `CommandError` at the command boundary.
#[derive(Debug, Error)]
pub enum CoreError {
    #[error("invalid input: {0}")]
    InvalidInput(String),
    #[error(transparent)]
    Platform(#[from] PlatformError),
}

impl CoreError {
    pub fn code(&self) -> &'static str {
        match self {
            Self::InvalidInput(_) => "invalid_input",
            Self::Platform(_) => "platform",
        }
    }

    pub fn hint(&self) -> Option<&'static str> {
        match self {
            Self::InvalidInput(_) => None,
            Self::Platform(PlatformError::NoHomeDir) => {
                Some("Set the HOME environment variable or the XDG base directory variables.")
            }
        }
    }
}
