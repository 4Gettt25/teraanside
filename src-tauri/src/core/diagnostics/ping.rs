use serde::Serialize;
use ts_rs::TS;

use crate::core::CoreError;
use crate::platform::Platform;

/// Maximum accepted length (in characters) of a ping message.
pub const MAX_MESSAGE_LEN: usize = 256;

/// Reply to a `ping` command.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct Pong {
    /// The message that was sent, echoed back.
    pub message: String,
    /// Version of the Rust core.
    pub core_version: String,
    /// OS family of the running core (platform capability).
    pub os: String,
}

pub fn ping(message: &str, platform: &dyn Platform) -> Result<Pong, CoreError> {
    if message.chars().count() > MAX_MESSAGE_LEN {
        return Err(CoreError::InvalidInput(format!(
            "message must be at most {MAX_MESSAGE_LEN} characters"
        )));
    }
    Ok(Pong {
        message: message.to_owned(),
        core_version: env!("CARGO_PKG_VERSION").to_owned(),
        os: platform.os().to_owned(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::platform;

    #[test]
    fn echoes_message_with_version_and_os() {
        let pong = ping("hello", platform::current()).unwrap();
        assert_eq!(pong.message, "hello");
        assert_eq!(pong.core_version, env!("CARGO_PKG_VERSION"));
        assert_eq!(pong.os, "linux");
    }

    #[test]
    fn rejects_too_long_messages() {
        let long = "x".repeat(MAX_MESSAGE_LEN + 1);
        let err = ping(&long, platform::current()).unwrap_err();
        assert_eq!(err.code(), "invalid_input");
    }

    #[test]
    fn serializes_camel_case() {
        let pong = ping("a", platform::current()).unwrap();
        let json = serde_json::to_value(&pong).unwrap();
        assert!(json.get("coreVersion").is_some());
    }
}
