//! `toolchain` (DESIGN.md section 4.2): app-managed tool binaries per project.
//! Downloads come only from allowlisted official sources and are verified
//! (OpenPGP-signed checksum files, or a pinned SHA256 for uv) before use.

pub mod catalog;
pub mod checksums;
pub mod keys;
pub mod signature;
pub mod version;

use thiserror::Error;

#[derive(Debug, Error)]
pub enum ToolchainError {
    #[error("invalid version '{0}': expected MAJOR.MINOR.PATCH")]
    InvalidVersion(String),
    #[error("{0}")]
    Unsupported(String),
    #[error("signing key problem: {0}")]
    Key(String),
    #[error("signature verification failed: {0}")]
    Signature(String),
    #[error("checksum problem: {0}")]
    Checksum(String),
}

impl ToolchainError {
    pub fn code(&self) -> &'static str {
        match self {
            Self::InvalidVersion(_) => "invalid_version",
            Self::Unsupported(_) => "unsupported",
            Self::Key(_) => "signing_key",
            Self::Signature(_) => "signature",
            Self::Checksum(_) => "checksum",
        }
    }

    pub fn hint(&self) -> Option<&'static str> {
        match self {
            Self::Signature(_) | Self::Checksum(_) => Some(
                "The download did not match the vendor's signed checksums and was discarded. \
                 Retry later; if it persists, report it.",
            ),
            _ => None,
        }
    }
}
