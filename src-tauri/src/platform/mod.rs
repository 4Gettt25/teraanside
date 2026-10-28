//! Everything OS-specific lives behind this module (DESIGN.md sections 4.10 and 13).
//! The MVP only ships the Linux implementation.

use camino::Utf8PathBuf;
use thiserror::Error;

#[cfg(target_os = "linux")]
mod linux;

#[cfg(not(target_os = "linux"))]
compile_error!("IaC Studio MVP only supports Linux (see DESIGN.md section 13)");

#[derive(Debug, Error, PartialEq, Eq)]
pub enum PlatformError {
    #[error("could not determine the home directory")]
    NoHomeDir,
}

/// Per-user application directories.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppDirs {
    pub config: Utf8PathBuf,
    pub data: Utf8PathBuf,
    pub cache: Utf8PathBuf,
    pub state: Utf8PathBuf,
}

/// OS abstraction. A Windows implementation can be added later without touching other modules.
pub trait Platform: Send + Sync {
    /// Stable identifier of the OS family, exposed to the frontend as a capability.
    fn os(&self) -> &'static str;
    /// Application directories (config, data, cache, state).
    fn app_dirs(&self) -> Result<AppDirs, PlatformError>;
}

/// The platform implementation for the current target.
pub fn current() -> &'static dyn Platform {
    #[cfg(target_os = "linux")]
    {
        &linux::LinuxPlatform
    }
}
