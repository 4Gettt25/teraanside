//! Verified installation of a tool binary into
//! `$XDG_DATA_HOME/iac-studio/bin/<tool>/<version>/<tool>`.
//!
//! Order: verify the checksum file's signature (or take the pinned SHA256) →
//! download into a staging directory while hashing → compare → extract exactly
//! one regular file → mark executable → atomic rename into place. Nothing
//! unverified ever lands in the final location.

use std::io::Read;
use std::sync::atomic::AtomicBool;
use std::time::{SystemTime, UNIX_EPOCH};

use camino::{Utf8Path, Utf8PathBuf};
use serde::Serialize;
use ts_rs::TS;

use super::ToolchainError;
use super::catalog::{ArchiveKind, Artifact, Tool, Verification};
use super::checksums::find_sha256;
use super::download::Downloader;
use super::signature::Keyring;
use super::version::Version;
use crate::platform;

const MARKER: &str = ".iac-studio-verified";
const MAX_SUMS_BYTES: u64 = 1024 * 1024;
const MAX_SIGNATURE_BYTES: u64 = 64 * 1024;
/// Upper bound for the extracted executable.
const MAX_BINARY_BYTES: u64 = 512 * 1024 * 1024;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(
    tag = "phase",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
#[ts(export, optional_fields)]
pub enum InstallProgress {
    Verifying,
    Downloading {
        received: u64,
        #[serde(skip_serializing_if = "Option::is_none")]
        total: Option<u64>,
    },
    Extracting,
    Done,
}

/// Path of the installed executable if it was installed (and verified) before.
pub fn installed(bin_root: &Utf8Path, tool: Tool, version: &Version) -> Option<Utf8PathBuf> {
    let dir = version_dir(bin_root, tool, version);
    let exe = dir.join(tool.id());
    (dir.join(MARKER).is_file() && exe.is_file()).then_some(exe)
}

fn version_dir(bin_root: &Utf8Path, tool: Tool, version: &Version) -> Utf8PathBuf {
    bin_root.join(tool.id()).join(version.as_str())
}

pub struct Installer<'a> {
    pub downloader: &'a Downloader,
    pub bin_root: &'a Utf8Path,
}

impl Installer<'_> {
    pub async fn install(
        &self,
        artifact: &Artifact,
        cancel: &AtomicBool,
        mut progress: impl FnMut(InstallProgress),
    ) -> Result<Utf8PathBuf, ToolchainError> {
        if let Some(exe) = installed(self.bin_root, artifact.tool, &artifact.version) {
            progress(InstallProgress::Done);
            return Ok(exe);
        }
        progress(InstallProgress::Verifying);
        let expected = self.expected_sha256(artifact).await?;

        let staging = self.staging_dir()?;
        let result = async {
            let archive_path = staging.join(&artifact.file_name);
            let actual = self
                .downloader
                .fetch_to_file(
                    &artifact.url,
                    &archive_path,
                    artifact.max_size,
                    cancel,
                    |r, t| {
                        progress(InstallProgress::Downloading {
                            received: r,
                            total: t,
                        });
                    },
                )
                .await?;
            if actual != expected {
                return Err(ToolchainError::Checksum(format!(
                    "{}: expected {expected}, got {actual}",
                    artifact.file_name
                )));
            }
            progress(InstallProgress::Extracting);
            let target = staging.join("out");
            std::fs::create_dir(&target).map_err(ToolchainError::io(format!("create {target}")))?;
            let exe = target.join(artifact.tool.id());
            let (kind, member) = (artifact.archive, artifact.member.clone());
            let (archive_path2, exe2) = (archive_path.clone(), exe.clone());
            tokio::task::spawn_blocking(move || extract_one(kind, &archive_path2, &member, &exe2))
                .await
                .map_err(|e| ToolchainError::Archive(e.to_string()))??;
            platform::current()
                .make_executable(&exe)
                .map_err(ToolchainError::io(format!("chmod {exe}")))?;
            std::fs::write(
                target.join(MARKER),
                format!("sha256={actual}\nurl={}\n", artifact.url),
            )
            .map_err(ToolchainError::io("write marker"))?;
            self.commit(&target, artifact)
        }
        .await;
        let _ = std::fs::remove_dir_all(&staging);
        if result.is_ok() {
            progress(InstallProgress::Done);
        }
        result
    }

    async fn expected_sha256(&self, artifact: &Artifact) -> Result<String, ToolchainError> {
        match &artifact.verification {
            Verification::PinnedSha256(h) => Ok((*h).to_owned()),
            Verification::SignedChecksums {
                sums_url,
                signature_url,
                keys,
            } => {
                let sums = self
                    .downloader
                    .fetch_bytes(sums_url, MAX_SUMS_BYTES)
                    .await?;
                let sig = self
                    .downloader
                    .fetch_bytes(signature_url, MAX_SIGNATURE_BYTES)
                    .await?;
                let keyring = Keyring::load(keys)?;
                let now = SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .map_or(0, |d| d.as_secs());
                let signer = keyring.verify(&sums, &sig, now)?;
                tracing::info!(tool = artifact.tool.id(), version = %artifact.version, %signer, "checksums signature verified");
                let text = std::str::from_utf8(&sums)
                    .map_err(|_| ToolchainError::Checksum("checksum file is not UTF-8".into()))?;
                find_sha256(text, &artifact.file_name)
            }
        }
    }

    fn staging_dir(&self) -> Result<Utf8PathBuf, ToolchainError> {
        std::fs::create_dir_all(self.bin_root)
            .map_err(ToolchainError::io(format!("create {}", self.bin_root)))?;
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |d| d.as_nanos());
        let dir = self
            .bin_root
            .join(format!(".staging-{}-{nanos}", std::process::id()));
        std::fs::create_dir(&dir).map_err(ToolchainError::io(format!("create {dir}")))?;
        Ok(dir)
    }

    fn commit(&self, ready: &Utf8Path, artifact: &Artifact) -> Result<Utf8PathBuf, ToolchainError> {
        let dest = version_dir(self.bin_root, artifact.tool, &artifact.version);
        if let Some(parent) = dest.parent() {
            std::fs::create_dir_all(parent)
                .map_err(ToolchainError::io(format!("create {parent}")))?;
        }
        match std::fs::rename(ready, &dest) {
            Ok(()) => {}
            // installed concurrently by someone else: keep theirs if it is complete
            Err(_) if installed(self.bin_root, artifact.tool, &artifact.version).is_some() => {}
            Err(e) => return Err(ToolchainError::io(format!("install into {dest}"))(e)),
        }
        tracing::info!(tool = artifact.tool.id(), version = %artifact.version, path = %dest, "tool installed");
        Ok(dest.join(artifact.tool.id()))
    }
}

/// Extracts exactly the archive entry `member` (a regular file) to `dest`.
/// Other entries — including traversal or link entries — are never written.
fn extract_one(
    kind: ArchiveKind,
    archive: &Utf8Path,
    member: &str,
    dest: &Utf8Path,
) -> Result<(), ToolchainError> {
    let file =
        std::fs::File::open(archive).map_err(ToolchainError::io(format!("open {archive}")))?;
    let mut out = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(dest)
        .map_err(ToolchainError::io(format!("create {dest}")))?;
    let missing = || ToolchainError::Archive(format!("{member} not found in archive"));
    match kind {
        ArchiveKind::Zip => {
            let mut zip =
                zip::ZipArchive::new(file).map_err(|e| ToolchainError::Archive(e.to_string()))?;
            let mut entry = zip.by_name(member).map_err(|_| missing())?;
            if !entry.is_file() || entry.is_symlink() {
                return Err(ToolchainError::Archive(format!(
                    "{member} is not a regular file"
                )));
            }
            copy_limited(&mut entry, &mut out)?;
        }
        ArchiveKind::TarGz => {
            let mut tar = tar::Archive::new(flate2::read::GzDecoder::new(file));
            let entries = tar
                .entries()
                .map_err(|e| ToolchainError::Archive(e.to_string()))?;
            let mut found = false;
            for entry in entries {
                let mut entry = entry.map_err(|e| ToolchainError::Archive(e.to_string()))?;
                let path = entry
                    .path()
                    .map_err(|e| ToolchainError::Archive(e.to_string()))?;
                if path.to_str() != Some(member) {
                    continue;
                }
                if !entry.header().entry_type().is_file() {
                    return Err(ToolchainError::Archive(format!(
                        "{member} is not a regular file"
                    )));
                }
                copy_limited(&mut entry, &mut out)?;
                found = true;
                break;
            }
            if !found {
                return Err(missing());
            }
        }
    }
    out.sync_all()
        .map_err(ToolchainError::io(format!("sync {dest}")))
}

fn copy_limited(reader: &mut impl Read, out: &mut std::fs::File) -> Result<(), ToolchainError> {
    let copied = std::io::copy(&mut reader.take(MAX_BINARY_BYTES + 1), out)
        .map_err(|e| ToolchainError::Archive(e.to_string()))?;
    if copied > MAX_BINARY_BYTES {
        return Err(ToolchainError::TooLarge {
            limit: MAX_BINARY_BYTES,
        });
    }
    Ok(())
}

#[cfg(test)]
mod tests;
