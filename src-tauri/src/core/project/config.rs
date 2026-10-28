//! `.iac-studio/project.toml` (DESIGN.md section 4.1): committable per-project settings.
//! M1a only reads it; writing (pins from the setup wizard) comes with the toolchain in M1b.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use super::ProjectError;
use super::fs::ProjectFs;

pub const CONFIG_PATH: &str = ".iac-studio/project.toml";
pub const SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "lowercase")]
#[ts(export)]
pub enum TerraformFlavor {
    Terraform,
    Opentofu,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(default, rename_all = "kebab-case")]
#[ts(export, optional_fields)]
pub struct TerraformConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub flavor: Option<TerraformFlavor>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(default, rename_all = "kebab-case")]
#[ts(export, optional_fields)]
pub struct AnsibleConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub core_version: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lint_version: Option<String>,
}

/// Unknown keys are ignored so that newer app versions can add settings.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(default, rename_all = "kebab-case")]
#[ts(export)]
pub struct ProjectConfig {
    pub schema: u32,
    pub terraform: TerraformConfig,
    pub ansible: AnsibleConfig,
}

impl Default for ProjectConfig {
    fn default() -> Self {
        Self {
            schema: SCHEMA_VERSION,
            terraform: TerraformConfig::default(),
            ansible: AnsibleConfig::default(),
        }
    }
}

/// Reads the project configuration; a missing file yields the defaults.
pub fn load(fs: &ProjectFs) -> Result<ProjectConfig, ProjectError> {
    match fs.read(CONFIG_PATH) {
        Ok(content) => parse(&content.text),
        Err(ProjectError::NotFound(_)) => Ok(ProjectConfig::default()),
        Err(e) => Err(e),
    }
}

pub fn parse(text: &str) -> Result<ProjectConfig, ProjectError> {
    let config: ProjectConfig =
        toml::from_str(text).map_err(|e| ProjectError::InvalidConfig(e.message().to_owned()))?;
    if config.schema > SCHEMA_VERSION {
        return Err(ProjectError::InvalidConfig(format!(
            "schema {} is newer than supported ({SCHEMA_VERSION}); update IaC Studio",
            config.schema
        )));
    }
    Ok(config)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_full_config() {
        let c = parse(
            r#"
schema = 1
[terraform]
flavor = "opentofu"
version = "1.13.2"
[ansible]
core-version = "2.21.5"
future-key = true
"#,
        )
        .unwrap();
        assert_eq!(c.terraform.flavor, Some(TerraformFlavor::Opentofu));
        assert_eq!(c.terraform.version.as_deref(), Some("1.13.2"));
        assert_eq!(c.ansible.core_version.as_deref(), Some("2.21.5"));
    }

    #[test]
    fn none_fields_are_omitted_in_json() {
        // TS type uses optional fields (`flavor?: …`), so `null` must not be sent.
        let json = serde_json::to_value(ProjectConfig::default()).unwrap();
        assert_eq!(json["terraform"], serde_json::json!({}));
        assert_eq!(json["ansible"], serde_json::json!({}));
    }

    #[test]
    fn empty_and_missing_give_defaults() {
        assert_eq!(parse("").unwrap(), ProjectConfig::default());
        let tmp = tempfile::tempdir().unwrap();
        let root = camino::Utf8PathBuf::from_path_buf(tmp.path().to_path_buf()).unwrap();
        let fs = ProjectFs::open(&root).unwrap();
        assert_eq!(load(&fs).unwrap(), ProjectConfig::default());
    }

    #[test]
    fn rejects_invalid_values_and_newer_schema() {
        assert!(matches!(
            parse("[terraform]\nflavor = \"pulumi\"\n"),
            Err(ProjectError::InvalidConfig(_))
        ));
        assert!(matches!(
            parse("schema = 99\n"),
            Err(ProjectError::InvalidConfig(_))
        ));
        assert!(matches!(
            parse("schema = "),
            Err(ProjectError::InvalidConfig(_))
        ));
    }
}
