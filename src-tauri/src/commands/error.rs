use serde::Serialize;

use crate::core::CoreError;

/// Structured error returned to the frontend by every command.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CommandError {
    /// Stable, machine-readable error code (e.g. `invalid_input`).
    pub code: String,
    /// Human-readable message, safe to display (never contains secrets).
    pub message: String,
    /// Optional suggestion how the user can resolve the error.
    pub hint: Option<String>,
}

impl From<CoreError> for CommandError {
    fn from(err: CoreError) -> Self {
        Self {
            code: err.code().to_owned(),
            message: err.to_string(),
            hint: err.hint().map(str::to_owned),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_core_error_to_structured_error() {
        let err = CommandError::from(CoreError::InvalidInput("count must be > 0".into()));
        assert_eq!(err.code, "invalid_input");
        assert_eq!(err.message, "invalid input: count must be > 0");
        assert!(err.hint.is_none());
    }
}
