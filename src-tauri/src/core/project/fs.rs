//! Confined file access inside a project root.
//!
//! Every access is resolved relative to an open directory handle (cap-std; on Linux
//! `openat2` with `RESOLVE_BENEATH`), so the path check happens *at* access time.
//! Swapping a path component for a symlink between a check and the access cannot
//! escape the project.

use std::io::{Read, Write};

use camino::{Utf8Component, Utf8Path, Utf8PathBuf};
use cap_std::ambient_authority;
use cap_std::fs::Dir;
use cap_tempfile::TempFile;
use serde::Serialize;
use sha2::{Digest, Sha256};
use ts_rs::TS;

use super::ProjectError;
use crate::platform;

/// Files larger than this are not opened in the editor.
pub const MAX_FILE_SIZE: u64 = 10 * 1024 * 1024;

/// Text content of a project file plus the hash used for conflict detection.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct FileContent {
    pub path: String,
    pub text: String,
    pub sha256: String,
}

pub struct ProjectFs {
    root: Dir,
    root_path: Utf8PathBuf,
}

impl ProjectFs {
    /// Opens the project root. This is the only place using ambient authority.
    pub fn open(root_path: &Utf8Path) -> Result<Self, ProjectError> {
        let shown = root_path.as_str();
        let root = Dir::open_ambient_dir(root_path, ambient_authority())
            .map_err(|e| ProjectError::from_io(shown, e))?;
        Ok(Self {
            root,
            root_path: root_path.to_owned(),
        })
    }

    pub fn root_path(&self) -> &Utf8Path {
        &self.root_path
    }

    /// A new handle to the root directory (for confined walks).
    pub(crate) fn dir_handle(&self) -> std::io::Result<Dir> {
        self.root.try_clone()
    }

    pub fn read(&self, rel: &str) -> Result<FileContent, ProjectError> {
        let path = validate_relative(rel)?;
        let mut file = self
            .root
            .open(path.as_std_path())
            .map_err(|e| ProjectError::from_io(rel, e))?;
        let meta = file.metadata().map_err(|e| ProjectError::from_io(rel, e))?;
        if !meta.is_file() {
            return Err(ProjectError::NotAFile(rel.to_owned()));
        }
        if meta.len() > MAX_FILE_SIZE {
            return Err(ProjectError::TooLarge {
                path: rel.to_owned(),
                limit: MAX_FILE_SIZE,
            });
        }
        let mut bytes = Vec::with_capacity(usize::try_from(meta.len()).unwrap_or(0));
        Read::by_ref(&mut file)
            .take(MAX_FILE_SIZE + 1)
            .read_to_end(&mut bytes)
            .map_err(|e| ProjectError::from_io(rel, e))?;
        if bytes.len() as u64 > MAX_FILE_SIZE {
            return Err(ProjectError::TooLarge {
                path: rel.to_owned(),
                limit: MAX_FILE_SIZE,
            });
        }
        let sha256 = hex_sha256(&bytes);
        let text = String::from_utf8(bytes).map_err(|_| ProjectError::NotText(rel.to_owned()))?;
        Ok(FileContent {
            path: path.into_string(),
            text,
            sha256,
        })
    }

    /// Writes `text` atomically: temporary file in the target directory, fsync, rename.
    /// With `expected_sha256` the write fails with `Conflict` if the file changed on disk
    /// (or appeared/disappeared) since it was read.
    pub fn write(
        &self,
        rel: &str,
        text: &str,
        expected_sha256: Option<&str>,
    ) -> Result<String, ProjectError> {
        let path = validate_relative(rel)?;
        let name = path
            .file_name()
            .ok_or_else(|| ProjectError::InvalidPath(rel.to_owned()))?;
        let parent = match path.parent() {
            Some(p) if !p.as_str().is_empty() => self
                .root
                .open_dir(p.as_std_path())
                .map_err(|e| ProjectError::from_io(rel, e))?,
            _ => self
                .root
                .try_clone()
                .map_err(|e| ProjectError::from_io(rel, e))?,
        };

        let existing = match parent.symlink_metadata(name) {
            Ok(meta) => Some(meta),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
            Err(e) => return Err(ProjectError::from_io(rel, e)),
        };
        if let Some(meta) = &existing {
            if meta.is_symlink() {
                return Err(ProjectError::SymlinkWrite(rel.to_owned()));
            }
            if !meta.is_file() {
                return Err(ProjectError::NotAFile(rel.to_owned()));
            }
        }
        if let Some(expected) = expected_sha256 {
            let current = match &existing {
                Some(_) => Some(hex_sha256(
                    &parent
                        .read(name)
                        .map_err(|e| ProjectError::from_io(rel, e))?,
                )),
                None => None,
            };
            if current.as_deref() != Some(expected) {
                return Err(ProjectError::Conflict(rel.to_owned()));
            }
        }

        let permissions = match &existing {
            Some(meta) => meta.permissions(),
            None => platform::current().new_file_permissions(),
        };
        let mut tmp = TempFile::new(&parent).map_err(|e| ProjectError::from_io(rel, e))?;
        tmp.write_all(text.as_bytes())
            .and_then(|()| tmp.as_file().sync_all())
            .and_then(|()| tmp.as_file().set_permissions(permissions))
            .map_err(|e| ProjectError::from_io(rel, e))?;
        tmp.replace(name)
            .map_err(|e| ProjectError::from_io(rel, e))?;
        Ok(hex_sha256(text.as_bytes()))
    }
}

/// Accepts only relative paths made of normal components (no `..`, no root, no `.`
/// tricks, no NUL). Escapes via symlinks are prevented separately at access time.
pub fn validate_relative(rel: &str) -> Result<Utf8PathBuf, ProjectError> {
    let invalid = || ProjectError::InvalidPath(rel.to_owned());
    if rel.is_empty() || rel.contains('\0') {
        return Err(invalid());
    }
    let mut out = Utf8PathBuf::new();
    for component in Utf8Path::new(rel).components() {
        match component {
            Utf8Component::Normal(c) => out.push(c),
            Utf8Component::CurDir => {}
            _ => return Err(invalid()),
        }
    }
    if out.as_str().is_empty() {
        return Err(invalid());
    }
    Ok(out)
}

pub fn hex_sha256(bytes: &[u8]) -> String {
    hex_sha256_digest(Sha256::new_with_prefix(bytes))
}

/// Finalizes an incremental SHA256 as lower-case hex.
pub fn hex_sha256_digest(hasher: Sha256) -> String {
    hasher
        .finalize()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

#[cfg(test)]
mod tests {
    use std::os::unix::fs::{PermissionsExt, symlink};
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, Ordering};

    use super::*;

    struct Fixture {
        _tmp: tempfile::TempDir,
        root: Utf8PathBuf,
        outside: Utf8PathBuf,
    }

    fn fixture() -> Fixture {
        let tmp = tempfile::tempdir().unwrap();
        let base = Utf8PathBuf::from_path_buf(tmp.path().to_path_buf()).unwrap();
        let root = base.join("project");
        let outside = base.join("outside");
        std::fs::create_dir_all(root.join("sub")).unwrap();
        std::fs::create_dir_all(&outside).unwrap();
        std::fs::write(root.join("main.tf"), "resource {}\n").unwrap();
        std::fs::write(root.join("sub/file.yml"), "inside\n").unwrap();
        std::fs::write(outside.join("file.yml"), "SECRET-OUTSIDE\n").unwrap();
        std::fs::write(outside.join("secret"), "SECRET-OUTSIDE\n").unwrap();
        Fixture {
            _tmp: tmp,
            root,
            outside,
        }
    }

    #[test]
    fn validates_relative_paths() {
        for ok in ["main.tf", "a/b.yml", "./a/b"] {
            assert!(validate_relative(ok).is_ok(), "{ok}");
        }
        for bad in ["", "/etc/passwd", "../x", "a/../../x", "a/..", ".", "a\0b"] {
            assert!(
                matches!(validate_relative(bad), Err(ProjectError::InvalidPath(_))),
                "{bad:?}"
            );
        }
    }

    #[test]
    fn reads_file_with_hash() {
        let f = fixture();
        let fs = ProjectFs::open(&f.root).unwrap();
        let c = fs.read("main.tf").unwrap();
        assert_eq!(c.text, "resource {}\n");
        assert_eq!(c.sha256, hex_sha256(b"resource {}\n"));
    }

    #[test]
    fn rejects_symlink_pointing_outside() {
        let f = fixture();
        symlink(f.outside.join("secret"), f.root.join("link")).unwrap();
        symlink(&f.outside, f.root.join("linkdir")).unwrap();
        let fs = ProjectFs::open(&f.root).unwrap();
        assert!(matches!(
            fs.read("link"),
            Err(ProjectError::OutsideProject(_))
        ));
        assert!(matches!(
            fs.read("linkdir/secret"),
            Err(ProjectError::OutsideProject(_))
        ));
        assert!(fs.write("linkdir/new", "x", None).is_err());
        assert!(!f.outside.join("new").exists());
    }

    #[test]
    fn follows_symlinks_that_stay_inside() {
        let f = fixture();
        symlink("sub/file.yml", f.root.join("inner-link")).unwrap();
        let fs = ProjectFs::open(&f.root).unwrap();
        assert_eq!(fs.read("inner-link").unwrap().text, "inside\n");
    }

    #[test]
    fn symlink_swapped_after_open_is_checked_again_on_access() {
        let f = fixture();
        let fs = ProjectFs::open(&f.root).unwrap();
        assert_eq!(fs.read("sub/file.yml").unwrap().text, "inside\n");
        // Replace the directory with a symlink to the outside after the first access.
        std::fs::rename(f.root.join("sub"), f.root.join("sub-old")).unwrap();
        symlink(&f.outside, f.root.join("sub")).unwrap();
        assert!(matches!(
            fs.read("sub/file.yml"),
            Err(ProjectError::OutsideProject(_))
        ));
        assert!(fs.write("sub/file.yml", "pwned", None).is_err());
        assert_eq!(
            std::fs::read_to_string(f.outside.join("file.yml")).unwrap(),
            "SECRET-OUTSIDE\n"
        );
    }

    #[test]
    fn concurrent_symlink_swaps_never_leak_outside_content() {
        let f = fixture();
        let fs = ProjectFs::open(&f.root).unwrap();
        let stop = Arc::new(AtomicBool::new(false));
        let swapper = {
            let (root, outside, stop) = (f.root.clone(), f.outside.clone(), stop.clone());
            std::thread::spawn(move || {
                std::fs::rename(root.join("sub"), root.join("real")).unwrap();
                while !stop.load(Ordering::Relaxed) {
                    let _ = std::fs::remove_file(root.join("sub"));
                    let _ = symlink(&outside, root.join("sub"));
                    let _ = std::fs::remove_file(root.join("sub"));
                    let _ = symlink("real", root.join("sub"));
                }
            })
        };
        let mut inside_reads = 0;
        for _ in 0..5_000 {
            if let Ok(c) = fs.read("sub/file.yml") {
                assert_eq!(c.text, "inside\n", "read content from outside the project");
                inside_reads += 1;
            }
            let _ = fs.write("sub/file.yml", "inside\n", None);
        }
        stop.store(true, Ordering::Relaxed);
        swapper.join().unwrap();
        assert_eq!(
            std::fs::read_to_string(f.outside.join("file.yml")).unwrap(),
            "SECRET-OUTSIDE\n"
        );
        assert!(inside_reads > 0);
    }

    #[test]
    fn writes_atomically_and_keeps_permissions() {
        let f = fixture();
        std::fs::set_permissions(
            f.root.join("main.tf"),
            std::fs::Permissions::from_mode(0o640),
        )
        .unwrap();
        let fs = ProjectFs::open(&f.root).unwrap();
        let hash = fs.write("main.tf", "changed\n", None).unwrap();
        assert_eq!(hash, hex_sha256(b"changed\n"));
        assert_eq!(
            std::fs::read_to_string(f.root.join("main.tf")).unwrap(),
            "changed\n"
        );
        let mode = std::fs::metadata(f.root.join("main.tf"))
            .unwrap()
            .permissions()
            .mode();
        assert_eq!(mode & 0o777, 0o640);
        // no temporary files left behind
        let names: Vec<_> = std::fs::read_dir(&f.root)
            .unwrap()
            .map(|e| e.unwrap().file_name().into_string().unwrap())
            .collect();
        assert!(names.iter().all(|n| !n.starts_with('.')), "{names:?}");
    }

    #[test]
    fn new_files_get_default_permissions() {
        let f = fixture();
        let fs = ProjectFs::open(&f.root).unwrap();
        fs.write("sub/new.yml", "x", None).unwrap();
        let mode = std::fs::metadata(f.root.join("sub/new.yml"))
            .unwrap()
            .permissions()
            .mode();
        assert_eq!(mode & 0o777, 0o644);
    }

    #[test]
    fn detects_conflicts() {
        let f = fixture();
        let fs = ProjectFs::open(&f.root).unwrap();
        let loaded = fs.read("main.tf").unwrap();
        std::fs::write(f.root.join("main.tf"), "external edit\n").unwrap();
        assert!(matches!(
            fs.write("main.tf", "mine", Some(&loaded.sha256)),
            Err(ProjectError::Conflict(_))
        ));
        assert_eq!(
            std::fs::read_to_string(f.root.join("main.tf")).unwrap(),
            "external edit\n"
        );
        let current = fs.read("main.tf").unwrap();
        assert!(fs.write("main.tf", "mine", Some(&current.sha256)).is_ok());
    }

    #[test]
    fn refuses_to_write_through_symlinks() {
        let f = fixture();
        symlink("main.tf", f.root.join("alias.tf")).unwrap();
        let fs = ProjectFs::open(&f.root).unwrap();
        assert!(matches!(
            fs.write("alias.tf", "x", None),
            Err(ProjectError::SymlinkWrite(_))
        ));
    }

    #[test]
    fn rejects_non_text_and_directories() {
        let f = fixture();
        std::fs::write(f.root.join("bin"), [0xff, 0xfe, 0x00]).unwrap();
        let fs = ProjectFs::open(&f.root).unwrap();
        assert!(matches!(fs.read("bin"), Err(ProjectError::NotText(_))));
        assert!(matches!(fs.read("sub"), Err(ProjectError::NotAFile(_))));
        assert!(matches!(fs.read("missing"), Err(ProjectError::NotFound(_))));
    }
}
