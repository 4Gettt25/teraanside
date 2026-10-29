use std::sync::{PoisonError, RwLock};

use camino::{Utf8Path, Utf8PathBuf};

use super::ProjectError;
use super::config::{self, ProjectConfig};
use super::fs::ProjectFs;

/// An opened project: canonical root, confined file access and its configuration.
pub struct OpenProject {
    root: Utf8PathBuf,
    name: String,
    fs: ProjectFs,
    config: RwLock<ProjectConfig>,
}

impl OpenProject {
    /// Opens `path` as a project. The path is canonicalized once here; all later
    /// accesses go through the confined `ProjectFs`.
    pub fn open(path: &Utf8Path) -> Result<Self, ProjectError> {
        let shown = path.as_str();
        let canonical = path
            .canonicalize_utf8()
            .map_err(|e| ProjectError::from_io(shown, e))?;
        if !canonical.is_dir() {
            return Err(ProjectError::NotADirectory(shown.to_owned()));
        }
        let fs = ProjectFs::open(&canonical)?;
        let config = config::load(&fs)?;
        let name = canonical
            .file_name()
            .map_or_else(|| canonical.to_string(), str::to_owned);
        Ok(Self {
            root: canonical,
            name,
            fs,
            config: RwLock::new(config),
        })
    }

    pub fn root(&self) -> &Utf8Path {
        &self.root
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn fs(&self) -> &ProjectFs {
        &self.fs
    }

    /// The configuration as last loaded.
    pub fn config(&self) -> ProjectConfig {
        self.config
            .read()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }

    /// Re-reads `project.toml` (after the wizard wrote a pin or an external edit).
    pub fn reload_config(&self) -> Result<ProjectConfig, ProjectError> {
        let fresh = config::load(&self.fs)?;
        *self.config.write().unwrap_or_else(PoisonError::into_inner) = fresh.clone();
        Ok(fresh)
    }
}
