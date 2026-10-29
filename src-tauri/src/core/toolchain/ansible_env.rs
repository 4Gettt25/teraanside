//! Per-project Python environment with pinned ansible-core and ansible-lint,
//! created with the managed uv (DESIGN.md §4.2):
//! `$XDG_DATA_HOME/iac-studio/venvs/<project-hash>/`.
//!
//! - Python is a uv-managed build; uv checks its SHA256 against the metadata
//!   compiled into uv (`uv-python-managed/download-metadata.json`, `HashMismatch`).
//! - Packages: wheels only (`--only-binary :all:`, no build scripts run) and
//!   `--exclude-newer P14D` (the 14-day rule also for transitive PyPI packages).

use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use camino::{Utf8Path, Utf8PathBuf};
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;

use super::ToolchainError;
use super::version::Version;
use crate::core::project::fs::hex_sha256;
use crate::core::runner::{self, ChildEnv, RunEvent, RunSpec, StopTimeouts};
use crate::platform::AppDirs;

const MARKER: &str = "iac-studio-env.toml";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EnvPins {
    pub python: String,
    pub ansible_core: String,
    pub ansible_lint: String,
}

impl EnvPins {
    pub fn new(python: &Version, core: &Version, lint: &Version) -> Self {
        Self {
            python: python.to_string(),
            ansible_core: core.to_string(),
            ansible_lint: lint.to_string(),
        }
    }
}

pub fn venv_dir(data_dir: &Utf8Path, project_root: &Utf8Path) -> Utf8PathBuf {
    let hash = hex_sha256(project_root.as_str().as_bytes());
    data_dir.join("venvs").join(&hash[..16])
}

/// The venv exists and was created with exactly these pins.
pub fn is_ready(venv: &Utf8Path, pins: &EnvPins) -> bool {
    std::fs::read_to_string(venv.join(MARKER))
        .ok()
        .and_then(|t| toml::from_str::<EnvPins>(&t).ok())
        .is_some_and(|p| p == *pins)
        && venv.join("bin").join("ansible-playbook").is_file()
}

pub fn uv_env(dirs: &AppDirs) -> ChildEnv {
    ChildEnv::from_allowlist()
        .set("UV_CACHE_DIR", dirs.cache.join("uv").as_str())
        .set("UV_PYTHON_INSTALL_DIR", dirs.data.join("python").as_str())
}

/// (Re)creates the environment. Output lines of uv are passed to `on_line`.
pub async fn create(
    uv: &Utf8Path,
    dirs: &AppDirs,
    venv: &Utf8Path,
    pins: &EnvPins,
    cancel: &AtomicBool,
    mut on_line: impl FnMut(String),
) -> Result<(), ToolchainError> {
    std::fs::create_dir_all(&dirs.data)
        .map_err(ToolchainError::io(format!("create {}", dirs.data)))?;
    let env = uv_env(dirs);
    let common = ["--no-config", "--no-progress", "--color", "never"];
    let mut venv_args: Vec<String> = vec!["venv".into()];
    venv_args.extend(common.iter().map(|s| (*s).to_owned()));
    venv_args.extend([
        "--no-project".into(),
        "--managed-python".into(),
        "--clear".into(),
        "--python".into(),
        pins.python.clone(),
        venv.to_string(),
    ]);
    run_uv(uv, &dirs.data, &env, venv_args, cancel, &mut on_line).await?;

    let python = venv.join("bin").join("python");
    let mut pip_args: Vec<String> = vec!["pip".into(), "install".into()];
    pip_args.extend(common.iter().map(|s| (*s).to_owned()));
    pip_args.extend([
        "--python".into(),
        python.to_string(),
        "--exclude-newer".into(),
        "P14D".into(),
        "--only-binary".into(),
        ":all:".into(),
        format!("ansible-core=={}", pins.ansible_core),
        format!("ansible-lint=={}", pins.ansible_lint),
    ]);
    run_uv(uv, &dirs.data, &env, pip_args, cancel, &mut on_line).await?;

    let marker = toml::to_string(pins).map_err(|e| ToolchainError::Archive(e.to_string()))?;
    std::fs::write(venv.join(MARKER), marker).map_err(ToolchainError::io("write venv marker"))?;
    Ok(())
}

async fn run_uv(
    uv: &Utf8Path,
    cwd: &Utf8Path,
    env: &ChildEnv,
    args: Vec<String>,
    cancel: &AtomicBool,
    on_line: &mut impl FnMut(String),
) -> Result<(), ToolchainError> {
    let (tx, mut rx) = mpsc::channel(256);
    let spec = RunSpec {
        program: uv.to_owned(),
        args,
        cwd: cwd.to_owned(),
        env: env.clone(),
        stop: StopTimeouts::default(),
    };
    let mut handle =
        runner::spawn(spec, tx).map_err(|e| ToolchainError::Download(format!("uv: {e}")))?;
    let mut last_lines: Vec<String> = Vec::new();
    let mut outcome = None;
    let mut cancelled = false;
    loop {
        tokio::select! {
            event = rx.recv() => match event {
                Some(RunEvent::Line { text, .. }) => {
                    last_lines.push(text.clone());
                    if last_lines.len() > 5 { last_lines.remove(0); }
                    on_line(text);
                }
                Some(RunEvent::Exited(o)) => { outcome = Some(o); break; }
                Some(RunEvent::Started { .. }) => {}
                None => break,
            },
            () = tokio::time::sleep(Duration::from_millis(200)), if !cancelled => {
                if cancel.load(Ordering::Relaxed) {
                    cancelled = true;
                    handle.cancel();
                }
            }
        }
    }
    let outcome = match outcome {
        Some(o) => o,
        None => handle
            .wait()
            .await
            .map_err(|e| ToolchainError::Download(format!("uv: {e}")))?,
    };
    if outcome.cancelled || cancelled {
        return Err(ToolchainError::Cancelled);
    }
    if !outcome.success() {
        return Err(ToolchainError::Download(format!(
            "uv failed (exit {:?}): {}",
            outcome.code,
            last_lines.join(" | ")
        )));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn venv_dir_is_stable_per_project() {
        let a = venv_dir(Utf8Path::new("/data"), Utf8Path::new("/home/u/repo"));
        let b = venv_dir(Utf8Path::new("/data"), Utf8Path::new("/home/u/repo"));
        let c = venv_dir(Utf8Path::new("/data"), Utf8Path::new("/home/u/other"));
        assert_eq!(a, b);
        assert_ne!(a, c);
        assert!(a.starts_with("/data/venvs"));
        assert_eq!(a.file_name().unwrap().len(), 16);
    }

    #[test]
    fn readiness_requires_matching_marker() {
        let tmp = tempfile::tempdir().unwrap();
        let venv = Utf8PathBuf::from_path_buf(tmp.path().to_path_buf()).unwrap();
        let v = |s| Version::parse(s).unwrap();
        let pins = EnvPins::new(&v("3.13.16"), &v("2.21.5"), &v("26.10.1"));
        assert!(!is_ready(&venv, &pins));
        std::fs::create_dir_all(venv.join("bin")).unwrap();
        std::fs::write(venv.join("bin/ansible-playbook"), "").unwrap();
        std::fs::write(venv.join(MARKER), toml::to_string(&pins).unwrap()).unwrap();
        assert!(is_ready(&venv, &pins));
        let other = EnvPins::new(&v("3.13.16"), &v("2.20.0"), &v("26.10.1"));
        assert!(!is_ready(&venv, &other));
    }

    /// Fake uv: records its arguments, creates the venv layout.
    #[tokio::test]
    async fn runs_uv_with_pinned_wheel_only_arguments() {
        use std::os::unix::fs::PermissionsExt;
        let tmp = tempfile::tempdir().unwrap();
        let base = Utf8PathBuf::from_path_buf(tmp.path().to_path_buf()).unwrap();
        let uv = base.join("uv");
        let log = base.join("args.log");
        std::fs::write(
            &uv,
            format!(
                "#!/bin/sh\necho \"$@\" >> {log}\necho \"UV_CACHE_DIR=$UV_CACHE_DIR\" >> {log}\n\
                 if [ \"$1\" = venv ]; then for a; do last=$a; done; mkdir -p \"$last/bin\"; \
                 : > \"$last/bin/ansible-playbook\"; fi\necho done\n"
            ),
        )
        .unwrap();
        std::fs::set_permissions(&uv, std::fs::Permissions::from_mode(0o755)).unwrap();
        let dirs = AppDirs {
            config: base.join("config"),
            data: base.join("data"),
            cache: base.join("cache"),
            state: base.join("state"),
        };
        let venv = base.join("data/venvs/x");
        let v = |s| Version::parse(s).unwrap();
        let pins = EnvPins::new(&v("3.13.16"), &v("2.21.5"), &v("26.10.1"));
        let mut lines = Vec::new();
        create(&uv, &dirs, &venv, &pins, &AtomicBool::new(false), |l| {
            lines.push(l)
        })
        .await
        .unwrap();
        assert!(is_ready(&venv, &pins));
        let log = std::fs::read_to_string(&log).unwrap();
        assert!(log.contains(&format!(
            "venv --no-config --no-progress --color never --no-project --managed-python --clear --python 3.13.16 {venv}"
        )), "{log}");
        assert!(log.contains("--exclude-newer P14D --only-binary :all: ansible-core==2.21.5 ansible-lint==26.10.1"), "{log}");
        assert!(
            log.contains(&format!("UV_CACHE_DIR={}/cache/uv", base)),
            "{log}"
        );
        assert_eq!(lines, ["done", "done"]);
    }
}
