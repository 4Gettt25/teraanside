use camino::Utf8PathBuf;

use super::{AppDirs, Platform, PlatformError};

const APP_DIR_NAME: &str = "iac-studio";

pub struct LinuxPlatform;

impl Platform for LinuxPlatform {
    fn os(&self) -> &'static str {
        "linux"
    }

    fn app_dirs(&self) -> Result<AppDirs, PlatformError> {
        app_dirs_from_env(|key| std::env::var(key).ok())
    }

    fn new_file_permissions(&self) -> cap_std::fs::Permissions {
        use std::os::unix::fs::PermissionsExt;
        cap_std::fs::Permissions::from_std(std::fs::Permissions::from_mode(0o644))
    }

    fn write_private_file(&self, path: &camino::Utf8Path, bytes: &[u8]) -> std::io::Result<()> {
        use std::io::Write;
        use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt};
        let parent = path
            .parent()
            .ok_or_else(|| std::io::Error::other("path has no parent directory"))?;
        std::fs::DirBuilder::new()
            .recursive(true)
            .mode(0o700)
            .create(parent)?;
        let file_name = path.file_name().unwrap_or("file");
        let tmp = parent.join(format!(".{file_name}.tmp-{}", std::process::id()));
        let result = (|| {
            let mut f = std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .mode(0o600)
                .open(&tmp)?;
            f.write_all(bytes)?;
            f.sync_all()?;
            std::fs::rename(&tmp, path)
        })();
        if result.is_err() {
            let _ = std::fs::remove_file(&tmp);
        }
        result
    }

    fn env_allowlist(&self) -> &'static [&'static str] {
        &[
            "HOME",
            "USER",
            "LOGNAME",
            "PATH",
            "LANG",
            "LANGUAGE",
            "TZ",
            "TMPDIR",
            // ssh-agent for git/ansible connections
            "SSH_AUTH_SOCK",
            // corporate proxies (terraform init, uv, ansible-galaxy)
            "HTTP_PROXY",
            "HTTPS_PROXY",
            "NO_PROXY",
            "http_proxy",
            "https_proxy",
            "no_proxy",
        ]
    }

    fn env_allowlist_prefixes(&self) -> &'static [&'static str] {
        &["LC_"]
    }

    fn path_list_separator(&self) -> &'static str {
        ":"
    }

    fn prepare_command(&self, command: &mut tokio::process::Command) {
        command.process_group(0);
    }

    fn signal_process_tree(&self, pid: u32, signal: super::ProcessSignal) -> std::io::Result<()> {
        use rustix::process::{Pid, Signal, kill_process_group};
        let raw = i32::try_from(pid).map_err(|_| std::io::Error::other("pid out of range"))?;
        let pid = Pid::from_raw(raw).ok_or_else(|| std::io::Error::other("invalid pid 0"))?;
        let signal = match signal {
            super::ProcessSignal::Interrupt => Signal::INT,
            super::ProcessSignal::Terminate => Signal::TERM,
            super::ProcessSignal::Kill => Signal::KILL,
        };
        kill_process_group(pid, signal).map_err(std::io::Error::from)
    }

    fn termination_signal(&self, status: &std::process::ExitStatus) -> Option<i32> {
        use std::os::unix::process::ExitStatusExt;
        status.signal()
    }

    fn make_executable(&self, path: &camino::Utf8Path) -> std::io::Result<()> {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = std::fs::metadata(path)?.permissions();
        perms.set_mode(perms.mode() | 0o700);
        std::fs::set_permissions(path, perms)
    }
}

/// Resolves the XDG base directories (XDG Base Directory Specification):
/// a variable is only used when set to an absolute path, otherwise the
/// default relative to `$HOME` applies.
fn app_dirs_from_env(env: impl Fn(&str) -> Option<String>) -> Result<AppDirs, PlatformError> {
    let home = env("HOME")
        .map(Utf8PathBuf::from)
        .filter(|p| p.is_absolute())
        .ok_or(PlatformError::NoHomeDir)?;

    let base = |var: &str, default: &[&str]| -> Utf8PathBuf {
        let dir = env(var)
            .map(Utf8PathBuf::from)
            .filter(|p| p.is_absolute())
            .unwrap_or_else(|| default.iter().fold(home.clone(), |acc, c| acc.join(c)));
        dir.join(APP_DIR_NAME)
    };

    Ok(AppDirs {
        config: base("XDG_CONFIG_HOME", &[".config"]),
        data: base("XDG_DATA_HOME", &[".local", "share"]),
        cache: base("XDG_CACHE_HOME", &[".cache"]),
        state: base("XDG_STATE_HOME", &[".local", "state"]),
    })
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::*;

    fn env_of(pairs: &[(&str, &str)]) -> impl Fn(&str) -> Option<String> {
        let map: HashMap<String, String> = pairs
            .iter()
            .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
            .collect();
        move |key| map.get(key).cloned()
    }

    #[test]
    fn defaults_relative_to_home() {
        let dirs = app_dirs_from_env(env_of(&[("HOME", "/home/u")])).unwrap();
        assert_eq!(dirs.config, "/home/u/.config/iac-studio");
        assert_eq!(dirs.data, "/home/u/.local/share/iac-studio");
        assert_eq!(dirs.cache, "/home/u/.cache/iac-studio");
        assert_eq!(dirs.state, "/home/u/.local/state/iac-studio");
    }

    #[test]
    fn respects_absolute_xdg_variables() {
        let dirs = app_dirs_from_env(env_of(&[
            ("HOME", "/home/u"),
            ("XDG_CONFIG_HOME", "/cfg"),
            ("XDG_DATA_HOME", "/data"),
            ("XDG_CACHE_HOME", "/cache"),
            ("XDG_STATE_HOME", "/state"),
        ]))
        .unwrap();
        assert_eq!(dirs.config, "/cfg/iac-studio");
        assert_eq!(dirs.data, "/data/iac-studio");
        assert_eq!(dirs.cache, "/cache/iac-studio");
        assert_eq!(dirs.state, "/state/iac-studio");
    }

    #[test]
    fn ignores_relative_xdg_variables() {
        let dirs = app_dirs_from_env(env_of(&[
            ("HOME", "/home/u"),
            ("XDG_DATA_HOME", "rel/data"),
        ]))
        .unwrap();
        assert_eq!(dirs.data, "/home/u/.local/share/iac-studio");
    }

    #[test]
    fn missing_or_relative_home_is_an_error() {
        assert_eq!(
            app_dirs_from_env(env_of(&[])),
            Err(PlatformError::NoHomeDir)
        );
        assert_eq!(
            app_dirs_from_env(env_of(&[("HOME", "relative")])),
            Err(PlatformError::NoHomeDir)
        );
    }

    #[test]
    fn current_platform_is_linux() {
        assert_eq!(crate::platform::current().os(), "linux");
    }
}
