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
