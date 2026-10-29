//! `toolchain` (DESIGN.md section 4.2): app-managed tool binaries per project.
//! Downloads come only from allowlisted official sources and are verified
//! (OpenPGP-signed checksum files, or a pinned SHA256 for uv) before use.

pub mod catalog;
pub mod checksums;
pub mod download;
pub mod install;
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
    #[error("download source not allowed: {0}")]
    NotAllowed(String),
    #[error("download failed: {0}")]
    Download(String),
    #[error("download is larger than the allowed {limit} bytes")]
    TooLarge { limit: u64 },
    #[error("cancelled")]
    Cancelled,
    #[error("archive problem: {0}")]
    Archive(String),
    #[error("i/o error ({context}): {source}")]
    Io {
        context: String,
        #[source]
        source: std::io::Error,
    },
}

impl ToolchainError {
    pub(crate) fn io(context: impl Into<String>) -> impl FnOnce(std::io::Error) -> Self {
        let context = context.into();
        move |source| Self::Io { context, source }
    }
}

impl ToolchainError {
    pub fn code(&self) -> &'static str {
        match self {
            Self::InvalidVersion(_) => "invalid_version",
            Self::Unsupported(_) => "unsupported",
            Self::Key(_) => "signing_key",
            Self::Signature(_) => "signature",
            Self::Checksum(_) => "checksum",
            Self::NotAllowed(_) => "download_not_allowed",
            Self::Download(_) => "download_failed",
            Self::TooLarge { .. } => "download_too_large",
            Self::Cancelled => "cancelled",
            Self::Archive(_) => "archive",
            Self::Io { .. } => "io",
        }
    }

    pub fn hint(&self) -> Option<&'static str> {
        match self {
            Self::Signature(_) | Self::Checksum(_) => Some(
                "The download did not match the vendor's signed checksums and was discarded. \
                 Retry later; if it persists, report it.",
            ),
            Self::Download(_) => Some(
                "Check the network connection or proxy settings (HTTPS_PROXY). Already \
                 installed versions keep working offline.",
            ),
            _ => None,
        }
    }
}
