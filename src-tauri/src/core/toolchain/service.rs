//! Toolchain status of a project and the setup that installs what is missing.
//!
//! No project code is executed here (only official binaries are downloaded and
//! uv creates the venv outside the project), so setup does not require trust.
//! Versions from project files are strictly validated (see `version.rs`).

use std::sync::atomic::{AtomicBool, Ordering};

use serde::Serialize;
use ts_rs::TS;

use super::ToolchainError;
use super::ansible_env::{self, EnvPins};
use super::catalog::{self, Arch, Tool, defaults};
use super::download::{Downloader, HostPolicy};
use super::install::{self, InstallProgress, Installer};
use super::pins::{self, PinSource, TerraformRequirement, VersionFiles};
use super::version::Version;
use crate::core::CoreError;
use crate::core::project::OpenProject;
use crate::core::project::config::{ProjectConfig, TerraformFlavor};
use crate::core::project::tree::Detected;
use crate::platform::AppDirs;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, optional_fields)]
pub struct ToolStatus {
    pub tool: Tool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<PinSource>,
    pub installed: bool,
    /// Why this tool cannot be set up (e.g. invalid pin).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, optional_fields)]
pub struct AnsibleEnvStatus {
    pub python: String,
    pub ansible_core: String,
    pub ansible_lint: String,
    pub ready: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, optional_fields)]
pub struct ToolchainStatus {
    pub detected: Detected,
    /// Terraform project without a decided flavor: the wizard must ask.
    pub needs_flavor_choice: bool,
    pub tools: Vec<ToolStatus>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ansible: Option<AnsibleEnvStatus>,
    /// Everything required is installed.
    pub ready: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
#[ts(export)]
pub enum SetupStep {
    Tool {
        tool: Tool,
        version: String,
    },
    AnsibleEnv {
        python: String,
        ansible_core: String,
        ansible_lint: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
#[ts(export, optional_fields)]
pub enum SetupEvent {
    Planned {
        steps: Vec<SetupStep>,
    },
    StepStarted {
        index: u32,
    },
    Progress {
        index: u32,
        progress: InstallProgress,
    },
    Log {
        index: u32,
        line: String,
    },
    StepFinished {
        index: u32,
        #[serde(skip_serializing_if = "Option::is_none")]
        error: Option<String>,
    },
    Finished {
        ok: bool,
        cancelled: bool,
    },
}

pub struct Toolchain {
    dirs: AppDirs,
    downloader: Downloader,
    cancel: AtomicBool,
    running: AtomicBool,
}

/// Clears the "setup running" flag when the setup ends (also on error/panic).
struct RunningGuard<'a>(&'a AtomicBool);

impl Drop for RunningGuard<'_> {
    fn drop(&mut self) {
        self.0.store(false, Ordering::Release);
    }
}

impl Toolchain {
    pub fn new(dirs: AppDirs) -> Result<Self, ToolchainError> {
        Ok(Self {
            dirs,
            downloader: Downloader::new(HostPolicy::official())?,
            cancel: AtomicBool::new(false),
            running: AtomicBool::new(false),
        })
    }

    fn bin_root(&self) -> camino::Utf8PathBuf {
        self.dirs.data.join("bin")
    }

    pub fn installed_tool(&self, tool: Tool, version: &Version) -> Option<camino::Utf8PathBuf> {
        install::installed(&self.bin_root(), tool, version)
    }

    pub fn status(&self, project: &OpenProject, detected: Detected) -> ToolchainStatus {
        status_with(
            &self.bin_root(),
            &self.dirs,
            &project.config(),
            &VersionFiles::read(project.fs()),
            project,
            detected,
        )
    }

    /// Writes the wizard's Terraform/OpenTofu choice as pin into `project.toml`.
    pub fn choose_flavor(
        &self,
        project: &OpenProject,
        flavor: TerraformFlavor,
    ) -> Result<ProjectConfig, CoreError> {
        let version = pins::default_version(flavor).map_err(ToolchainError::Unsupported)?;
        pins::write_terraform_pin(project.fs(), flavor, &version)?;
        Ok(project.reload_config()?)
    }

    pub fn cancel(&self) {
        self.cancel.store(true, Ordering::Relaxed);
    }

    /// Installs everything missing; reports each step through `emit`.
    pub async fn setup(
        &self,
        project: &OpenProject,
        status: &ToolchainStatus,
        emit: impl Fn(SetupEvent),
    ) -> Result<(), CoreError> {
        if self.running.swap(true, Ordering::AcqRel) {
            return Err(ToolchainError::Unsupported("a setup is already running".into()).into());
        }
        let _guard = RunningGuard(&self.running);
        self.cancel.store(false, Ordering::Relaxed);
        if status.needs_flavor_choice {
            return Err(
                ToolchainError::Unsupported("choose Terraform or OpenTofu first".into()).into(),
            );
        }
        let steps = plan(status);
        emit(SetupEvent::Planned {
            steps: steps.clone(),
        });
        let arch = Arch::current()?;
        let mut ok = true;
        for (i, step) in steps.iter().enumerate() {
            let index = u32::try_from(i).unwrap_or(u32::MAX);
            if self.cancel.load(Ordering::Relaxed) {
                break;
            }
            emit(SetupEvent::StepStarted { index });
            let result = match step {
                SetupStep::Tool { tool, version } => {
                    async {
                        let version = Version::parse(version)?;
                        let artifact = catalog::artifact(*tool, &version, arch)?;
                        Installer {
                            downloader: &self.downloader,
                            bin_root: &self.bin_root(),
                        }
                        .install(&artifact, &self.cancel, |progress| {
                            emit(SetupEvent::Progress { index, progress });
                        })
                        .await
                        .map(|_| ())
                    }
                    .await
                }
                SetupStep::AnsibleEnv {
                    python,
                    ansible_core,
                    ansible_lint,
                } => {
                    async {
                        let uv_version = Version::parse(defaults::UV)?;
                        let uv = self.installed_tool(Tool::Uv, &uv_version).ok_or_else(|| {
                            ToolchainError::Unsupported("uv is not installed".into())
                        })?;
                        let pins = EnvPins {
                            python: python.clone(),
                            ansible_core: ansible_core.clone(),
                            ansible_lint: ansible_lint.clone(),
                        };
                        let venv = ansible_env::venv_dir(&self.dirs.data, project.root());
                        ansible_env::create(&uv, &self.dirs, &venv, &pins, &self.cancel, |line| {
                            emit(SetupEvent::Log { index, line });
                        })
                        .await
                    }
                    .await
                }
            };
            let error = result.err().map(|e| e.to_string());
            ok &= error.is_none();
            emit(SetupEvent::StepFinished { index, error });
            if !ok {
                break;
            }
        }
        let cancelled = self.cancel.load(Ordering::Relaxed);
        emit(SetupEvent::Finished {
            ok: ok && !cancelled,
            cancelled,
        });
        Ok(())
    }
}

/// Status computation without network access (testable).
fn status_with(
    bin_root: &camino::Utf8Path,
    dirs: &AppDirs,
    config: &ProjectConfig,
    files: &VersionFiles,
    project: &OpenProject,
    detected: Detected,
) -> ToolchainStatus {
    let mut tools = Vec::new();
    let mut needs_flavor_choice = false;
    let tool_status = |tool: Tool, pin: Result<pins::Pin, String>| match pin {
        Ok(p) => ToolStatus {
            tool,
            installed: install::installed(bin_root, tool, &p.version).is_some(),
            version: Some(p.version.to_string()),
            source: Some(p.source),
            error: None,
        },
        Err(e) => ToolStatus {
            tool,
            version: None,
            source: None,
            installed: false,
            error: Some(e),
        },
    };
    let default_pin = |raw: &str| {
        Version::parse(raw)
            .map(|version| pins::Pin {
                version,
                source: PinSource::Default,
            })
            .map_err(|e| e.to_string())
    };

    if detected.terraform {
        match pins::terraform_requirement(config, files) {
            TerraformRequirement::FlavorUnknown => needs_flavor_choice = true,
            TerraformRequirement::Resolved { flavor, pin } => {
                tools.push(tool_status(pins::flavor_tool(flavor), pin));
            }
        }
        tools.push(tool_status(
            Tool::TerraformLs,
            default_pin(defaults::TERRAFORM_LS),
        ));
    }
    let mut ansible = None;
    if detected.ansible {
        tools.push(tool_status(Tool::Uv, default_pin(defaults::UV)));
        tools.push(tool_status(Tool::Node, default_pin(defaults::NODE)));
        let p = pins::ansible_pins(config);
        ansible = Some(match (p.python, p.core, p.lint) {
            (Ok(py), Ok(core), Ok(lint)) => {
                let pins = EnvPins::new(&py, &core.version, &lint.version);
                let venv = ansible_env::venv_dir(&dirs.data, project.root());
                AnsibleEnvStatus {
                    ready: ansible_env::is_ready(&venv, &pins),
                    python: pins.python,
                    ansible_core: pins.ansible_core,
                    ansible_lint: pins.ansible_lint,
                    error: None,
                }
            }
            (py, core, lint) => AnsibleEnvStatus {
                python: py.as_ref().map(ToString::to_string).unwrap_or_default(),
                ansible_core: core
                    .as_ref()
                    .map(|p| p.version.to_string())
                    .unwrap_or_default(),
                ansible_lint: lint
                    .as_ref()
                    .map(|p| p.version.to_string())
                    .unwrap_or_default(),
                ready: false,
                error: [py.err(), core.err(), lint.err()]
                    .into_iter()
                    .flatten()
                    .next(),
            },
        });
    }
    let ready = !needs_flavor_choice
        && tools.iter().all(|t| t.installed)
        && ansible.as_ref().is_none_or(|a| a.ready);
    ToolchainStatus {
        detected,
        needs_flavor_choice,
        tools,
        ansible,
        ready,
    }
}

/// Steps needed to reach `ready` (tools with errors are skipped and reported).
pub fn plan(status: &ToolchainStatus) -> Vec<SetupStep> {
    let mut steps: Vec<SetupStep> = status
        .tools
        .iter()
        .filter(|t| !t.installed && t.error.is_none())
        .filter_map(|t| {
            t.version.clone().map(|version| SetupStep::Tool {
                tool: t.tool,
                version,
            })
        })
        .collect();
    if let Some(a) = &status.ansible
        && !a.ready
        && a.error.is_none()
    {
        steps.push(SetupStep::AnsibleEnv {
            python: a.python.clone(),
            ansible_core: a.ansible_core.clone(),
            ansible_lint: a.ansible_lint.clone(),
        });
    }
    steps
}

#[cfg(test)]
mod tests {
    use camino::Utf8PathBuf;

    use super::*;
    use crate::core::project::config;

    struct Fx {
        _tmp: tempfile::TempDir,
        dirs: AppDirs,
        project: OpenProject,
    }

    fn fx(files: &[(&str, &str)]) -> Fx {
        let tmp = tempfile::tempdir().unwrap();
        let base = Utf8PathBuf::from_path_buf(tmp.path().to_path_buf()).unwrap();
        let root = base.join("repo");
        std::fs::create_dir_all(&root).unwrap();
        for (p, c) in files {
            let path = root.join(p);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, c).unwrap();
        }
        let dirs = AppDirs {
            config: base.join("config"),
            data: base.join("data"),
            cache: base.join("cache"),
            state: base.join("state"),
        };
        Fx {
            project: OpenProject::open(&root).unwrap(),
            _tmp: tmp,
            dirs,
        }
    }

    fn status(f: &Fx, detected: Detected) -> ToolchainStatus {
        status_with(
            &f.dirs.data.join("bin"),
            &f.dirs,
            &f.project.config(),
            &VersionFiles::read(f.project.fs()),
            &f.project,
            detected,
        )
    }

    const TF: Detected = Detected {
        terraform: true,
        ansible: false,
    };
    const BOTH: Detected = Detected {
        terraform: true,
        ansible: true,
    };

    #[test]
    fn terraform_project_without_pin_asks_for_flavor() {
        let f = fx(&[("main.tf", "")]);
        let s = status(&f, TF);
        assert!(s.needs_flavor_choice);
        assert!(!s.ready);
        assert_eq!(
            s.tools.iter().map(|t| t.tool).collect::<Vec<_>>(),
            [Tool::TerraformLs]
        );
    }

    #[test]
    fn mixed_project_plans_all_missing_steps() {
        let f = fx(&[(
            ".iac-studio/project.toml",
            "[terraform]\nflavor = \"opentofu\"\n",
        )]);
        let s = status(&f, BOTH);
        assert!(!s.needs_flavor_choice);
        let steps = plan(&s);
        assert_eq!(
            steps,
            [
                SetupStep::Tool {
                    tool: Tool::Opentofu,
                    version: defaults::OPENTOFU.into()
                },
                SetupStep::Tool {
                    tool: Tool::TerraformLs,
                    version: defaults::TERRAFORM_LS.into()
                },
                SetupStep::Tool {
                    tool: Tool::Uv,
                    version: defaults::UV.into()
                },
                SetupStep::Tool {
                    tool: Tool::Node,
                    version: defaults::NODE.into()
                },
                SetupStep::AnsibleEnv {
                    python: defaults::PYTHON.into(),
                    ansible_core: defaults::ANSIBLE_CORE.into(),
                    ansible_lint: defaults::ANSIBLE_LINT.into()
                },
            ]
        );
    }

    #[test]
    fn invalid_pins_are_reported_and_not_planned() {
        let f = fx(&[(
            ".iac-studio/project.toml",
            "[terraform]\nflavor = \"terraform\"\nversion = \"1.6.0-beta\"\n[ansible]\ncore-version = \"latest\"\n",
        )]);
        let s = status(&f, BOTH);
        let tf = s.tools.iter().find(|t| t.tool == Tool::Terraform).unwrap();
        assert!(tf.error.is_some());
        assert!(s.ansible.as_ref().unwrap().error.is_some());
        let steps = plan(&s);
        assert!(steps.iter().all(|st| !matches!(
            st,
            SetupStep::Tool {
                tool: Tool::Terraform,
                ..
            } | SetupStep::AnsibleEnv { .. }
        )));
    }

    #[test]
    fn installed_tools_count_as_ready() {
        let f = fx(&[(
            ".iac-studio/project.toml",
            "[terraform]\nflavor = \"terraform\"\nversion = \"1.16.6\"\n",
        )]);
        for (tool, v) in [
            ("terraform", "1.16.6"),
            ("terraform-ls", defaults::TERRAFORM_LS),
        ] {
            let dir = f.dirs.data.join("bin").join(tool).join(v);
            std::fs::create_dir_all(&dir).unwrap();
            std::fs::write(dir.join(tool), "").unwrap();
            std::fs::write(dir.join(".iac-studio-verified"), "").unwrap();
        }
        let s = status(&f, TF);
        assert!(s.ready, "{s:?}");
        assert!(plan(&s).is_empty());
    }

    #[test]
    fn choosing_a_flavor_writes_the_pin() {
        let f = fx(&[("main.tf", "")]);
        let tc = Toolchain::new(f.dirs.clone()).unwrap();
        let cfg = tc
            .choose_flavor(&f.project, TerraformFlavor::Opentofu)
            .unwrap();
        assert_eq!(cfg.terraform.flavor, Some(TerraformFlavor::Opentofu));
        assert_eq!(cfg.terraform.version.as_deref(), Some(defaults::OPENTOFU));
        let reread = config::load(f.project.fs()).unwrap();
        assert_eq!(reread, cfg);
        assert_eq!(f.project.config(), cfg, "in-memory config is refreshed");
    }
}
