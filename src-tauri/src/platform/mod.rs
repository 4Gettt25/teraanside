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

/// Signals used to stop a child process (tree) step by step.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProcessSignal {
    /// Polite stop (Ctrl+C semantics); tools like terraform finish cleanly.
    Interrupt,
    Terminate,
    Kill,
}

/// OS abstraction. A Windows implementation can be added later without touching other modules.
pub trait Platform: Send + Sync {
    /// Stable identifier of the OS family, exposed to the frontend as a capability.
    fn os(&self) -> &'static str;
    /// Application directories (config, data, cache, state).
    fn app_dirs(&self) -> Result<AppDirs, PlatformError>;
    /// Permissions for newly created user files (e.g. a new file saved from the editor).
    fn new_file_permissions(&self) -> cap_std::fs::Permissions;
    /// Atomically writes an app-private file (only the user can read it), creating
    /// parent directories. Used for app state such as the trust store.
    fn write_private_file(&self, path: &camino::Utf8Path, bytes: &[u8]) -> std::io::Result<()>;

    /// Environment variable names passed from the app's environment to child
    /// processes (allowlist; everything else is dropped).
    fn env_allowlist(&self) -> &'static [&'static str];
    /// Name prefixes that are passed through as well (e.g. locale `LC_*`).
    fn env_allowlist_prefixes(&self) -> &'static [&'static str];
    /// Separator of `PATH`-like variables.
    fn path_list_separator(&self) -> &'static str;

    /// Prepares a command so that the child and its descendants can be signalled
    /// together (Linux: own process group).
    fn prepare_command(&self, command: &mut tokio::process::Command);
    /// Sends `signal` to the child started with `prepare_command` and its descendants.
    fn signal_process_tree(&self, pid: u32, signal: ProcessSignal) -> std::io::Result<()>;
    /// The signal that terminated the process, if any.
    fn termination_signal(&self, status: &std::process::ExitStatus) -> Option<i32>;
    /// Marks a file as executable for the current user (downloaded tool binaries).
    fn make_executable(&self, path: &camino::Utf8Path) -> std::io::Result<()>;
}

/// The platform implementation for the current target.
pub fn current() -> &'static dyn Platform {
    #[cfg(target_os = "linux")]
    {
        &linux::LinuxPlatform
    }
}
