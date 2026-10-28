//! User settings in `$XDG_CONFIG_HOME/iac-studio/settings.toml` (DESIGN.md section 8).

use camino::Utf8PathBuf;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::core::CoreError;
use crate::core::project::ProjectError;
use crate::platform;

const MAX_RECENT: usize = 10;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct RecentProject {
    pub path: String,
    pub name: String,
    /// Whether the directory still exists (computed when listing).
    pub exists: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct StoredRecent {
    path: String,
    name: String,
}

#[derive(Debug, Default, Serialize, Deserialize)]
#[serde(default, rename_all = "kebab-case")]
struct SettingsFile {
    recent_projects: Vec<StoredRecent>,
}

pub struct Settings {
    file: Utf8PathBuf,
    data: SettingsFile,
}

impl Settings {
    /// Loads settings; a missing or unreadable file falls back to defaults (settings
    /// are convenience only, unlike the trust store).
    pub fn load(file: Utf8PathBuf) -> Self {
        let data = std::fs::read_to_string(&file)
            .ok()
            .and_then(|t| match toml::from_str(&t) {
                Ok(d) => Some(d),
                Err(e) => {
                    tracing::warn!(error = %e.message(), "ignoring invalid settings file");
                    None
                }
            })
            .unwrap_or_default();
        Self { file, data }
    }

    pub fn recent_projects(&self) -> Vec<RecentProject> {
        self.data
            .recent_projects
            .iter()
            .map(|r| RecentProject {
                path: r.path.clone(),
                name: r.name.clone(),
                exists: std::path::Path::new(&r.path).is_dir(),
            })
            .collect()
    }

    pub fn add_recent(&mut self, path: &str, name: &str) -> Result<(), CoreError> {
        self.data.recent_projects.retain(|r| r.path != path);
        self.data.recent_projects.insert(
            0,
            StoredRecent {
                path: path.to_owned(),
                name: name.to_owned(),
            },
        );
        self.data.recent_projects.truncate(MAX_RECENT);
        self.save()
    }

    fn save(&self) -> Result<(), CoreError> {
        let text = toml::to_string(&self.data)
            .map_err(|e| ProjectError::InvalidConfig(format!("serialize settings: {e}")))?;
        platform::current()
            .write_private_file(&self.file, text.as_bytes())
            .map_err(|e| ProjectError::from_io(self.file.as_str(), e).into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keeps_most_recent_first_without_duplicates() {
        let tmp = tempfile::tempdir().unwrap();
        let file = Utf8PathBuf::from_path_buf(tmp.path().join("settings.toml")).unwrap();
        let mut s = Settings::load(file.clone());
        for i in 0..12 {
            s.add_recent(&format!("/p/{i}"), &format!("p{i}")).unwrap();
        }
        s.add_recent("/p/5", "p5").unwrap();
        let reloaded = Settings::load(file);
        let paths: Vec<_> = reloaded
            .recent_projects()
            .into_iter()
            .map(|r| r.path)
            .collect();
        assert_eq!(paths.len(), MAX_RECENT);
        assert_eq!(paths[0], "/p/5");
        assert_eq!(paths[1], "/p/11");
        assert_eq!(paths.iter().filter(|p| *p == "/p/5").count(), 1);
    }

    #[test]
    fn invalid_file_falls_back_to_defaults() {
        let tmp = tempfile::tempdir().unwrap();
        let file = Utf8PathBuf::from_path_buf(tmp.path().join("settings.toml")).unwrap();
        std::fs::write(&file, "recent-projects = 3").unwrap();
        assert!(Settings::load(file).recent_projects().is_empty());
    }
}
