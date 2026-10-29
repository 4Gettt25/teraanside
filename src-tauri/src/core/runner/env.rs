use std::collections::BTreeMap;

use camino::Utf8Path;

use crate::platform;

/// Environment of a child process, built from an allowlist instead of inheriting
/// everything (secrets in the app's environment must not leak to tools).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ChildEnv {
    vars: BTreeMap<String, String>,
}

impl ChildEnv {
    /// Allowlisted variables from the app's own environment.
    pub fn from_allowlist() -> Self {
        Self::from_allowlist_of(std::env::vars())
    }

    pub(crate) fn from_allowlist_of(vars: impl IntoIterator<Item = (String, String)>) -> Self {
        let p = platform::current();
        let names = p.env_allowlist();
        let prefixes = p.env_allowlist_prefixes();
        Self {
            vars: vars
                .into_iter()
                .filter(|(k, _)| {
                    names.contains(&k.as_str()) || prefixes.iter().any(|pre| k.starts_with(pre))
                })
                .collect(),
        }
    }

    /// An empty environment (tests, fully controlled tools).
    pub fn empty() -> Self {
        Self::default()
    }

    pub fn set(mut self, key: &str, value: &str) -> Self {
        self.vars.insert(key.to_owned(), value.to_owned());
        self
    }

    pub fn remove(mut self, key: &str) -> Self {
        self.vars.remove(key);
        self
    }

    /// Puts `dir` in front of `PATH` (e.g. a venv's bin directory).
    pub fn prepend_path(mut self, dir: &Utf8Path) -> Self {
        let sep = platform::current().path_list_separator();
        let value = match self.vars.get("PATH") {
            Some(old) if !old.is_empty() => format!("{dir}{sep}{old}"),
            _ => dir.to_string(),
        };
        self.vars.insert("PATH".into(), value);
        self
    }

    pub fn get(&self, key: &str) -> Option<&str> {
        self.vars.get(key).map(String::as_str)
    }

    pub fn vars(&self) -> impl Iterator<Item = (&String, &String)> {
        self.vars.iter()
    }
}
