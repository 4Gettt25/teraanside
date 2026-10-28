//! Workspace trust (DESIGN.md section 10): nothing that can execute project code
//! (terraform init/plan, language servers, ansible-lint, …) runs before the user
//! trusted the project.
//!
//! Enforcement is by type: those functions take a [`TrustedProject`], which can
//! only be obtained from [`TrustStore::check`].

use std::collections::BTreeMap;

use camino::{Utf8Path, Utf8PathBuf};
use serde::{Deserialize, Serialize};

use super::ProjectError;
use super::open::OpenProject;
use crate::platform;

const STORE_VERSION: u32 = 1;

/// Proof that a project is trusted. Cannot be constructed outside this module:
///
/// ```compile_fail,E0451
/// use iac_studio_lib::core::project::trust::TrustedProject;
/// fn forge(p: &iac_studio_lib::core::project::OpenProject) -> TrustedProject<'_> {
///     TrustedProject { project: p }
/// }
/// ```
#[derive(Clone, Copy)]
pub struct TrustedProject<'a> {
    project: &'a OpenProject,
}

impl<'a> TrustedProject<'a> {
    pub fn project(&self) -> &'a OpenProject {
        self.project
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct TrustEntry {
    /// Seconds since the Unix epoch.
    trusted_at: u64,
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct StoreFile {
    version: u32,
    #[serde(default)]
    trusted: BTreeMap<String, TrustEntry>,
}

/// Persistent set of trusted project roots (canonical paths, exact match).
pub struct TrustStore {
    file: Utf8PathBuf,
    data: StoreFile,
}

impl TrustStore {
    /// Loads the store; a missing file means "nothing trusted". A corrupt file is an
    /// error (we never silently trust or untrust).
    pub fn load(file: Utf8PathBuf) -> Result<Self, ProjectError> {
        let data = match std::fs::read_to_string(&file) {
            Ok(text) => toml::from_str::<StoreFile>(&text)
                .map_err(|e| ProjectError::InvalidConfig(format!("{file}: {}", e.message())))?,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => StoreFile {
                version: STORE_VERSION,
                trusted: BTreeMap::new(),
            },
            Err(e) => return Err(ProjectError::from_io(file.as_str(), e)),
        };
        Ok(Self { file, data })
    }

    pub fn is_trusted(&self, root: &Utf8Path) -> bool {
        self.data.trusted.contains_key(root.as_str())
    }

    pub fn check<'a>(&self, project: &'a OpenProject) -> Result<TrustedProject<'a>, ProjectError> {
        if self.is_trusted(project.root()) {
            Ok(TrustedProject { project })
        } else {
            Err(ProjectError::NotTrusted)
        }
    }

    pub fn grant(&mut self, project: &OpenProject, now_unix: u64) -> Result<(), ProjectError> {
        self.data.trusted.insert(
            project.root().to_string(),
            TrustEntry {
                trusted_at: now_unix,
            },
        );
        self.save()
    }

    pub fn revoke(&mut self, project: &OpenProject) -> Result<(), ProjectError> {
        if self.data.trusted.remove(project.root().as_str()).is_some() {
            self.save()?;
        }
        Ok(())
    }

    fn save(&self) -> Result<(), ProjectError> {
        let data = StoreFile {
            version: STORE_VERSION,
            trusted: self.data.trusted.clone(),
        };
        let text = toml::to_string(&data)
            .map_err(|e| ProjectError::InvalidConfig(format!("serialize trust store: {e}")))?;
        platform::current()
            .write_private_file(&self.file, text.as_bytes())
            .map_err(|e| ProjectError::from_io(self.file.as_str(), e))
    }
}

#[cfg(test)]
mod tests {
    use std::os::unix::fs::PermissionsExt;

    use super::*;

    fn setup() -> (tempfile::TempDir, OpenProject, Utf8PathBuf) {
        let tmp = tempfile::tempdir().unwrap();
        let base = Utf8PathBuf::from_path_buf(tmp.path().to_path_buf()).unwrap();
        std::fs::create_dir_all(base.join("proj")).unwrap();
        let project = OpenProject::open(&base.join("proj")).unwrap();
        (tmp, project, base.join("config/iac-studio/trust.toml"))
    }

    #[test]
    fn untrusted_until_granted_and_persisted() {
        let (_tmp, project, file) = setup();
        let mut store = TrustStore::load(file.clone()).unwrap();
        assert!(matches!(
            store.check(&project),
            Err(ProjectError::NotTrusted)
        ));
        store.grant(&project, 1_700_000_000).unwrap();
        assert!(store.check(&project).is_ok());

        let reloaded = TrustStore::load(file.clone()).unwrap();
        assert!(reloaded.check(&project).is_ok());
        let mode = std::fs::metadata(&file).unwrap().permissions().mode();
        assert_eq!(mode & 0o777, 0o600);
    }

    #[test]
    fn revoke_removes_trust() {
        let (_tmp, project, file) = setup();
        let mut store = TrustStore::load(file.clone()).unwrap();
        store.grant(&project, 1).unwrap();
        store.revoke(&project).unwrap();
        assert!(store.check(&project).is_err());
        assert!(TrustStore::load(file).unwrap().check(&project).is_err());
    }

    #[test]
    fn trust_is_exact_path_not_prefix() {
        let (tmp, project, file) = setup();
        let base = Utf8PathBuf::from_path_buf(tmp.path().to_path_buf()).unwrap();
        std::fs::create_dir_all(base.join("proj/child")).unwrap();
        std::fs::create_dir_all(base.join("proj-other")).unwrap();
        let mut store = TrustStore::load(file).unwrap();
        store.grant(&project, 1).unwrap();
        let child = OpenProject::open(&base.join("proj/child")).unwrap();
        let sibling = OpenProject::open(&base.join("proj-other")).unwrap();
        assert!(store.check(&child).is_err());
        assert!(store.check(&sibling).is_err());
    }

    #[test]
    fn symlinked_path_resolves_to_same_trust() {
        let (tmp, project, file) = setup();
        let base = Utf8PathBuf::from_path_buf(tmp.path().to_path_buf()).unwrap();
        std::os::unix::fs::symlink(base.join("proj"), base.join("alias")).unwrap();
        let mut store = TrustStore::load(file).unwrap();
        store.grant(&project, 1).unwrap();
        let via_alias = OpenProject::open(&base.join("alias")).unwrap();
        assert!(store.check(&via_alias).is_ok());
    }

    #[test]
    fn corrupt_store_is_an_error() {
        let (_tmp, _project, file) = setup();
        std::fs::create_dir_all(file.parent().unwrap()).unwrap();
        std::fs::write(&file, "trusted = 5").unwrap();
        assert!(matches!(
            TrustStore::load(file),
            Err(ProjectError::InvalidConfig(_))
        ));
    }
}
