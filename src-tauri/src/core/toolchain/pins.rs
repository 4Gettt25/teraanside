//! Which tool versions a project needs: `.iac-studio/project.toml` first, then
//! `.tool-versions` (asdf/mise) and `.terraform-version`/`.opentofu-version`
//! (tfenv/tofuenv), then the app defaults. Version files are read, never written;
//! the setup wizard writes its choice into `project.toml` only.

use serde::Serialize;
use toml_edit::{DocumentMut, Item, Table, value};
use ts_rs::TS;

use super::catalog::{Tool, defaults};
use super::version::Version;
use crate::core::CoreError;
use crate::core::project::ProjectError;
use crate::core::project::config::{self, CONFIG_PATH, ProjectConfig, TerraformFlavor};
use crate::core::project::fs::ProjectFs;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum PinSource {
    ProjectToml,
    ToolVersions,
    VersionFile,
    Default,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Pin {
    pub version: Version,
    pub source: PinSource,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct VersionFiles {
    tool_versions: Option<String>,
    terraform_version: Option<String>,
    opentofu_version: Option<String>,
}

impl VersionFiles {
    pub fn read(fs: &ProjectFs) -> Self {
        let text = |p: &str| fs.read(p).ok().map(|c| c.text);
        Self {
            tool_versions: text(".tool-versions"),
            terraform_version: text(".terraform-version"),
            opentofu_version: text(".opentofu-version"),
        }
    }

    /// `name version [more versions…]` lines of `.tool-versions`.
    fn tool_versions(&self, name: &str) -> Option<String> {
        self.tool_versions.as_deref()?.lines().find_map(|line| {
            let line = line.split('#').next().unwrap_or("").trim();
            let mut parts = line.split_whitespace();
            (parts.next() == Some(name))
                .then(|| parts.next().map(str::to_owned))
                .flatten()
        })
    }

    fn first_line(text: Option<&str>) -> Option<String> {
        text?
            .lines()
            .map(str::trim)
            .find(|l| !l.is_empty() && !l.starts_with('#'))
            .map(str::to_owned)
    }

    /// Flavor implied by version files (if exactly one kind is present).
    pub fn implied_flavor(&self) -> Option<TerraformFlavor> {
        let tofu = self.tool_versions("opentofu").is_some() || self.opentofu_version.is_some();
        let tf = self.tool_versions("terraform").is_some() || self.terraform_version.is_some();
        match (tf, tofu) {
            (true, false) => Some(TerraformFlavor::Terraform),
            (false, true) => Some(TerraformFlavor::Opentofu),
            _ => None,
        }
    }

    fn terraform_pin(&self, flavor: TerraformFlavor) -> Option<(String, PinSource)> {
        let (asdf, file) = match flavor {
            TerraformFlavor::Terraform => ("terraform", self.terraform_version.as_deref()),
            TerraformFlavor::Opentofu => ("opentofu", self.opentofu_version.as_deref()),
        };
        self.tool_versions(asdf)
            .map(|v| (v, PinSource::ToolVersions))
            .or_else(|| Self::first_line(file).map(|v| (v, PinSource::VersionFile)))
    }
}

/// The resolved Terraform/OpenTofu requirement of a project.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TerraformRequirement {
    /// Neither `project.toml` nor version files decide: the wizard must ask.
    FlavorUnknown,
    Resolved {
        flavor: TerraformFlavor,
        pin: Result<Pin, String>,
    },
}

pub fn terraform_requirement(config: &ProjectConfig, files: &VersionFiles) -> TerraformRequirement {
    let Some(flavor) = config.terraform.flavor.or_else(|| files.implied_flavor()) else {
        return TerraformRequirement::FlavorUnknown;
    };
    let pin = match &config.terraform.version {
        Some(v) => parse_pin(v, PinSource::ProjectToml),
        None => match files.terraform_pin(flavor) {
            Some((v, source)) => parse_pin(&v, source),
            None => parse_pin(default_for(flavor), PinSource::Default),
        },
    };
    TerraformRequirement::Resolved { flavor, pin }
}

fn parse_pin(raw: &str, source: PinSource) -> Result<Pin, String> {
    Version::parse(raw)
        .map(|version| Pin { version, source })
        .map_err(|e| e.to_string())
}

pub fn flavor_tool(flavor: TerraformFlavor) -> Tool {
    match flavor {
        TerraformFlavor::Terraform => Tool::Terraform,
        TerraformFlavor::Opentofu => Tool::Opentofu,
    }
}

fn default_for(flavor: TerraformFlavor) -> &'static str {
    match flavor {
        TerraformFlavor::Terraform => defaults::TERRAFORM,
        TerraformFlavor::Opentofu => defaults::OPENTOFU,
    }
}

/// Default version of a flavor (built-in defaults are validated by catalog tests).
pub fn default_version(flavor: TerraformFlavor) -> Result<Version, String> {
    parse_pin(default_for(flavor), PinSource::Default).map(|p| p.version)
}

/// Ansible environment pins (`project.toml` or app defaults).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AnsiblePins {
    pub python: Result<Version, String>,
    pub core: Result<Pin, String>,
    pub lint: Result<Pin, String>,
}

pub fn ansible_pins(config: &ProjectConfig) -> AnsiblePins {
    let pin = |configured: &Option<String>, default: &str| match configured {
        Some(v) => parse_pin(v, PinSource::ProjectToml),
        None => parse_pin(default, PinSource::Default),
    };
    AnsiblePins {
        python: Version::parse(defaults::PYTHON).map_err(|e| e.to_string()),
        core: pin(&config.ansible.core_version, defaults::ANSIBLE_CORE),
        lint: pin(&config.ansible.lint_version, defaults::ANSIBLE_LINT),
    }
}

/// Writes the flavor (and version, if not pinned yet) into `project.toml`,
/// preserving existing content, comments and formatting.
pub fn write_terraform_pin(
    fs: &ProjectFs,
    flavor: TerraformFlavor,
    version: &Version,
) -> Result<ProjectConfig, CoreError> {
    let (text, sha) = match fs.read(CONFIG_PATH) {
        Ok(c) => (c.text, Some(c.sha256)),
        Err(ProjectError::NotFound(_)) => (String::new(), None),
        Err(e) => return Err(e.into()),
    };
    let mut doc: DocumentMut = text
        .parse()
        .map_err(|e: toml_edit::TomlError| ProjectError::InvalidConfig(e.message().to_owned()))?;
    if !doc.contains_key("schema") {
        doc.insert("schema", value(i64::from(config::SCHEMA_VERSION)));
    }
    let tf = doc
        .entry("terraform")
        .or_insert_with(|| Item::Table(Table::new()))
        .as_table_mut()
        .ok_or_else(|| ProjectError::InvalidConfig("[terraform] is not a table".into()))?;
    tf.insert(
        "flavor",
        value(match flavor {
            TerraformFlavor::Terraform => "terraform",
            TerraformFlavor::Opentofu => "opentofu",
        }),
    );
    if !tf.contains_key("version") {
        tf.insert("version", value(version.as_str()));
    }
    let new_text = doc.to_string();
    let parsed = config::parse(&new_text)?;
    fs.create_dir_all(".iac-studio")?;
    // with the hash of what we read: fail instead of overwriting a concurrent edit
    fs.write(CONFIG_PATH, &new_text, sha.as_deref())?;
    Ok(parsed)
}

#[cfg(test)]
mod tests {
    use camino::Utf8PathBuf;

    use super::*;

    fn files(tool_versions: Option<&str>, tf: Option<&str>, tofu: Option<&str>) -> VersionFiles {
        VersionFiles {
            tool_versions: tool_versions.map(str::to_owned),
            terraform_version: tf.map(str::to_owned),
            opentofu_version: tofu.map(str::to_owned),
        }
    }

    fn cfg(text: &str) -> ProjectConfig {
        config::parse(text).unwrap()
    }

    #[test]
    fn project_toml_wins() {
        let r = terraform_requirement(
            &cfg("[terraform]\nflavor = \"opentofu\"\nversion = \"1.12.0\"\n"),
            &files(Some("terraform 1.5.7\n"), Some("1.4.0"), None),
        );
        assert_eq!(
            r,
            TerraformRequirement::Resolved {
                flavor: TerraformFlavor::Opentofu,
                pin: Ok(Pin {
                    version: Version::parse("1.12.0").unwrap(),
                    source: PinSource::ProjectToml
                })
            }
        );
    }

    #[test]
    fn version_files_imply_flavor_and_version() {
        let r = terraform_requirement(
            &cfg(""),
            &files(
                Some("# tools\nopentofu 1.11.2 1.10.0\nnodejs 24\n"),
                None,
                None,
            ),
        );
        let TerraformRequirement::Resolved { flavor, pin } = r else {
            panic!()
        };
        assert_eq!(flavor, TerraformFlavor::Opentofu);
        assert_eq!(pin.unwrap().source, PinSource::ToolVersions);

        let r = terraform_requirement(&cfg(""), &files(None, Some("\n1.5.7\n"), None));
        let TerraformRequirement::Resolved { flavor, pin } = r else {
            panic!()
        };
        assert_eq!(flavor, TerraformFlavor::Terraform);
        assert_eq!(
            pin.unwrap(),
            Pin {
                version: Version::parse("1.5.7").unwrap(),
                source: PinSource::VersionFile
            }
        );
    }

    #[test]
    fn unknown_or_ambiguous_flavor_asks_the_user() {
        assert_eq!(
            terraform_requirement(&cfg(""), &files(None, None, None)),
            TerraformRequirement::FlavorUnknown
        );
        assert_eq!(
            terraform_requirement(&cfg(""), &files(None, Some("1.5.7"), Some("1.11.0"))),
            TerraformRequirement::FlavorUnknown
        );
    }

    #[test]
    fn flavor_without_version_uses_default_and_bad_pins_are_reported() {
        let r = terraform_requirement(
            &cfg("[terraform]\nflavor = \"terraform\"\n"),
            &files(None, None, None),
        );
        let TerraformRequirement::Resolved { pin, .. } = r else {
            panic!()
        };
        assert_eq!(pin.unwrap().source, PinSource::Default);
        // tfenv syntax like "latest" is not supported (strict versions only)
        let r = terraform_requirement(&cfg(""), &files(None, Some("latest"), None));
        let TerraformRequirement::Resolved { pin, .. } = r else {
            panic!()
        };
        assert!(pin.is_err());
    }

    #[test]
    fn writes_pin_preserving_comments() {
        let tmp = tempfile::tempdir().unwrap();
        let root = Utf8PathBuf::from_path_buf(tmp.path().to_path_buf()).unwrap();
        std::fs::create_dir_all(root.join(".iac-studio")).unwrap();
        std::fs::write(
            root.join(".iac-studio/project.toml"),
            "# team settings\nschema = 1\n\n[ansible]\ncore-version = \"2.21.5\" # keep\n",
        )
        .unwrap();
        let fs = ProjectFs::open(&root).unwrap();
        let cfg = write_terraform_pin(
            &fs,
            TerraformFlavor::Opentofu,
            &Version::parse("1.13.1").unwrap(),
        )
        .unwrap();
        assert_eq!(cfg.terraform.flavor, Some(TerraformFlavor::Opentofu));
        let text = std::fs::read_to_string(root.join(".iac-studio/project.toml")).unwrap();
        assert!(text.starts_with("# team settings\n"), "{text}");
        assert!(text.contains("core-version = \"2.21.5\" # keep"), "{text}");
        assert!(
            text.contains("[terraform]\nflavor = \"opentofu\"\nversion = \"1.13.1\""),
            "{text}"
        );
        // an existing version pin is kept
        write_terraform_pin(
            &fs,
            TerraformFlavor::Terraform,
            &Version::parse("9.9.9").unwrap(),
        )
        .unwrap();
        let text = std::fs::read_to_string(root.join(".iac-studio/project.toml")).unwrap();
        assert!(text.contains("version = \"1.13.1\""), "{text}");
    }

    #[test]
    fn creates_config_file_when_missing() {
        let tmp = tempfile::tempdir().unwrap();
        let root = Utf8PathBuf::from_path_buf(tmp.path().to_path_buf()).unwrap();
        let fs = ProjectFs::open(&root).unwrap();
        write_terraform_pin(
            &fs,
            TerraformFlavor::Terraform,
            &Version::parse("1.16.6").unwrap(),
        )
        .unwrap();
        let reread = config::load(&fs).unwrap();
        assert_eq!(reread.schema, 1);
        assert_eq!(reread.terraform.version.as_deref(), Some("1.16.6"));
    }
}
