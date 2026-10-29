//! Application-wide state: open projects, trust store and settings.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::{SystemTime, UNIX_EPOCH};

use camino::Utf8Path;
use serde::Serialize;
use ts_rs::TS;

use crate::core::CoreError;
use crate::core::project::config::ProjectConfig;
use crate::core::project::fs::FileContent;
use crate::core::project::tree::{self, ProjectTree};
use crate::core::project::trust::{TrustStore, TrustedProject};
use crate::core::project::watch::{self, ProjectWatcher};
use crate::core::project::{OpenProject, ProjectError};
use crate::core::settings::{RecentProject, Settings};
use crate::platform::AppDirs;

pub type ProjectId = u32;

/// Paths below these prefixes are never written from the editor (e.g. git hooks
/// would turn a file write into code execution).
const WRITE_PROTECTED: &[&str] = &[".git"];

#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ProjectInfo {
    pub id: ProjectId,
    pub name: String,
    pub root: String,
    pub trusted: bool,
    pub config: ProjectConfig,
}

pub struct Entry {
    project: OpenProject,
    watcher: Mutex<Option<ProjectWatcher>>,
}

impl Entry {
    pub fn project(&self) -> &OpenProject {
        &self.project
    }
}

/// Project (and optional file) given on the command line: `iac-studio [DIR [FILE]]`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, optional_fields)]
pub struct InitialOpen {
    pub project: ProjectInfo,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file: Option<String>,
}

pub struct Workspace {
    initial: Mutex<Option<InitialOpen>>,
    next_id: AtomicU32,
    projects: Mutex<HashMap<ProjectId, Arc<Entry>>>,
    trust: Mutex<TrustStore>,
    settings: Mutex<Settings>,
}

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    // A panic while holding the lock cannot leave these maps half-updated in a
    // dangerous way, so recover instead of propagating the poison.
    m.lock().unwrap_or_else(PoisonError::into_inner)
}

fn now_unix() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

impl Workspace {
    pub fn init(dirs: &AppDirs) -> Result<Self, CoreError> {
        Ok(Self {
            initial: Mutex::new(None),
            next_id: AtomicU32::new(1),
            projects: Mutex::new(HashMap::new()),
            trust: Mutex::new(TrustStore::load(dirs.config.join("trust.toml"))?),
            settings: Mutex::new(Settings::load(dirs.config.join("settings.toml"))),
        })
    }

    /// Opens a project (or returns the already open one with the same root).
    pub fn open(&self, path: &Utf8Path) -> Result<ProjectInfo, CoreError> {
        let project = OpenProject::open(path)?;
        let existing = lock(&self.projects)
            .iter()
            .find(|(_, e)| e.project.root() == project.root())
            .map(|(id, _)| *id);
        let id = match existing {
            Some(id) => id,
            None => {
                let id = self.next_id.fetch_add(1, Ordering::Relaxed);
                lock(&self.projects).insert(
                    id,
                    Arc::new(Entry {
                        project,
                        watcher: Mutex::new(None),
                    }),
                );
                id
            }
        };
        let info = self.info(id)?;
        lock(&self.settings).add_recent(&info.root, &info.name)?;
        Ok(info)
    }

    /// Opens a project from the recent list only (the frontend cannot open
    /// arbitrary paths; new projects come from the native folder dialog).
    pub fn open_recent(&self, path: &str) -> Result<ProjectInfo, CoreError> {
        let known = lock(&self.settings)
            .recent_projects()
            .iter()
            .any(|r| r.path == path);
        if !known {
            return Err(ProjectError::UnknownProject.into());
        }
        self.open(Utf8Path::new(path))
    }

    /// Opens the project given on the command line. The path comes from the user's
    /// shell, not from the webview.
    pub fn open_from_args(&self, dir: &Utf8Path, file: Option<String>) -> Result<(), CoreError> {
        let project = self.open(dir)?;
        *lock(&self.initial) = Some(InitialOpen { project, file });
        Ok(())
    }

    /// Returns the command-line project once (the frontend asks on start-up).
    pub fn take_initial(&self) -> Option<InitialOpen> {
        lock(&self.initial).take()
    }

    pub fn close(&self, id: ProjectId) {
        lock(&self.projects).remove(&id);
    }

    pub fn recent(&self) -> Vec<RecentProject> {
        lock(&self.settings).recent_projects()
    }

    /// Shared handle to an open project (kept alive by long-running operations).
    pub fn entry(&self, id: ProjectId) -> Result<Arc<Entry>, CoreError> {
        lock(&self.projects)
            .get(&id)
            .cloned()
            .ok_or_else(|| ProjectError::UnknownProject.into())
    }

    pub fn info(&self, id: ProjectId) -> Result<ProjectInfo, CoreError> {
        let entry = self.entry(id)?;
        let p = &entry.project;
        // pick up external edits of project.toml
        let config = p.reload_config()?;
        Ok(ProjectInfo {
            id,
            name: p.name().to_owned(),
            root: p.root().to_string(),
            trusted: lock(&self.trust).is_trusted(p.root()),
            config,
        })
    }

    pub fn set_trusted(&self, id: ProjectId, trusted: bool) -> Result<ProjectInfo, CoreError> {
        let entry = self.entry(id)?;
        {
            let mut store = lock(&self.trust);
            if trusted {
                store.grant(&entry.project, now_unix())?;
                tracing::info!(root = %entry.project.root(), "project trusted");
            } else {
                store.revoke(&entry.project)?;
                tracing::info!(root = %entry.project.root(), "project trust revoked");
            }
        }
        self.info(id)
    }

    /// Runs `f` with proof of trust; fails with `NotTrusted` otherwise.
    pub fn with_trusted<T>(
        &self,
        id: ProjectId,
        f: impl FnOnce(TrustedProject<'_>) -> Result<T, CoreError>,
    ) -> Result<T, CoreError> {
        let entry = self.entry(id)?;
        let trusted = lock(&self.trust).check(&entry.project)?;
        f(trusted)
    }

    pub fn tree(&self, id: ProjectId) -> Result<ProjectTree, CoreError> {
        let entry = self.entry(id)?;
        Ok(tree::build_tree(entry.project.fs(), entry.project.name())?)
    }

    pub fn read_file(&self, id: ProjectId, rel: &str) -> Result<FileContent, CoreError> {
        Ok(self.entry(id)?.project.fs().read(rel)?)
    }

    pub fn write_file(
        &self,
        id: ProjectId,
        rel: &str,
        text: &str,
        expected_sha256: Option<&str>,
    ) -> Result<String, CoreError> {
        let normalized = crate::core::project::fs::validate_relative(rel)?;
        if WRITE_PROTECTED.iter().any(|p| normalized.starts_with(p)) {
            return Err(ProjectError::InvalidPath(rel.to_owned()).into());
        }
        Ok(self
            .entry(id)?
            .project
            .fs()
            .write(normalized.as_str(), text, expected_sha256)?)
    }

    /// Starts (or replaces) the change watcher of a project.
    pub fn watch(
        &self,
        id: ProjectId,
        on_change: impl Fn(Vec<String>) + Send + 'static,
    ) -> Result<(), CoreError> {
        let entry = self.entry(id)?;
        let watcher = watch::watch(entry.project.root(), on_change)?;
        *lock(&entry.watcher) = Some(watcher);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use camino::Utf8PathBuf;

    use super::*;

    fn setup() -> (tempfile::TempDir, Workspace, Utf8PathBuf) {
        let tmp = tempfile::tempdir().unwrap();
        let base = Utf8PathBuf::from_path_buf(tmp.path().to_path_buf()).unwrap();
        let dirs = AppDirs {
            config: base.join("config"),
            data: base.join("data"),
            cache: base.join("cache"),
            state: base.join("state"),
        };
        let proj = base.join("proj");
        std::fs::create_dir_all(proj.join(".git/hooks")).unwrap();
        std::fs::write(proj.join("main.tf"), "x").unwrap();
        (tmp, Workspace::init(&dirs).unwrap(), proj)
    }

    #[test]
    fn open_is_idempotent_and_recorded_as_recent() {
        let (_tmp, ws, proj) = setup();
        let a = ws.open(&proj).unwrap();
        let b = ws.open(&proj).unwrap();
        assert_eq!(a.id, b.id);
        assert!(!a.trusted);
        assert_eq!(ws.recent()[0].path, a.root);
    }

    #[test]
    fn open_recent_only_accepts_known_paths() {
        let (_tmp, ws, proj) = setup();
        assert!(ws.open_recent(proj.as_str()).is_err());
        let info = ws.open(&proj).unwrap();
        ws.close(info.id);
        assert!(ws.open_recent(&info.root).is_ok());
        assert!(ws.open_recent("/").is_err());
    }

    #[test]
    fn trust_gate() {
        let (_tmp, ws, proj) = setup();
        let id = ws.open(&proj).unwrap().id;
        let ran = ws.with_trusted(id, |_| Ok(()));
        assert!(matches!(
            ran,
            Err(CoreError::Project(ProjectError::NotTrusted))
        ));
        assert!(ws.set_trusted(id, true).unwrap().trusted);
        assert!(ws.with_trusted(id, |_| Ok(())).is_ok());
        assert!(!ws.set_trusted(id, false).unwrap().trusted);
        assert!(ws.with_trusted(id, |_| Ok(())).is_err());
    }

    #[test]
    fn refuses_writes_into_git_dir() {
        let (_tmp, ws, proj) = setup();
        let id = ws.open(&proj).unwrap().id;
        for p in [".git/hooks/pre-commit", "./.git/config", ".git"] {
            assert!(ws.write_file(id, p, "#!/bin/sh\n", None).is_err(), "{p}");
        }
        assert!(!proj.join(".git/hooks/pre-commit").exists());
        assert!(ws.write_file(id, ".gitignore", "x\n", None).is_ok());
    }

    #[test]
    fn initial_open_is_taken_once() {
        let (_tmp, ws, proj) = setup();
        ws.open_from_args(&proj, Some("main.tf".into())).unwrap();
        let first = ws.take_initial().unwrap();
        assert_eq!(first.file.as_deref(), Some("main.tf"));
        assert!(ws.take_initial().is_none());
    }

    #[test]
    fn unknown_project_id() {
        let (_tmp, ws, _) = setup();
        assert!(matches!(
            ws.tree(99),
            Err(CoreError::Project(ProjectError::UnknownProject))
        ));
    }
}
