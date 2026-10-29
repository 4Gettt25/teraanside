//! What the app can install, from where, and how each download is verified.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use super::ToolchainError;
use super::keys::{self, PinnedKey};
use super::version::Version;

/// Default versions offered when a project does not pin one. Each was published
/// at least 14 days before it was entered here (CLAUDE.md "Versionsregel").
/// Updating: bump the version, add/verify the uv hash, run the release tests.
pub mod defaults {
    pub const TERRAFORM: &str = "1.16.6";
    pub const OPENTOFU: &str = "1.13.1";
    pub const TERRAFORM_LS: &str = "0.39.0";
    pub const UV: &str = "0.13.1";
    pub const NODE: &str = "24.22.0";
    pub const PYTHON: &str = "3.13.16";
    pub const ANSIBLE_CORE: &str = "2.21.5";
    pub const ANSIBLE_LINT: &str = "26.10.1";
}

/// uv publishes no OpenPGP signature, so its archives are pinned by SHA256 per
/// version and target (cross-checked: release `.sha256` file and GitHub API digest).
const UV_SHA256: &[(&str, &str, &str)] = &[(
    "0.13.1",
    "x86_64-unknown-linux-gnu",
    "eadd9d27c134ab5ac1ffd4473b97a741407f3ea238288f03121fc4deddf90220",
)];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum Tool {
    Terraform,
    Opentofu,
    TerraformLs,
    Uv,
    Node,
}

impl Tool {
    pub const ALL: [Tool; 5] = [
        Tool::Terraform,
        Tool::Opentofu,
        Tool::TerraformLs,
        Tool::Uv,
        Tool::Node,
    ];

    /// Directory name below `bin/` and executable name.
    pub fn id(self) -> &'static str {
        match self {
            Tool::Terraform => "terraform",
            Tool::Opentofu => "tofu",
            Tool::TerraformLs => "terraform-ls",
            Tool::Uv => "uv",
            Tool::Node => "node",
        }
    }
}

/// CPU architectures with known artifact names.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Arch {
    X86_64,
    Aarch64,
}

impl Arch {
    pub fn current() -> Result<Self, ToolchainError> {
        match std::env::consts::ARCH {
            "x86_64" => Ok(Arch::X86_64),
            "aarch64" => Ok(Arch::Aarch64),
            other => Err(ToolchainError::Unsupported(format!(
                "CPU architecture {other}"
            ))),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArchiveKind {
    Zip,
    TarGz,
}

#[derive(Debug, Clone)]
pub enum Verification {
    /// Checksum file with a detached OpenPGP signature from one of `keys`.
    SignedChecksums {
        sums_url: String,
        signature_url: String,
        keys: &'static [PinnedKey],
    },
    /// SHA256 compiled into the app.
    PinnedSha256(&'static str),
}

#[derive(Debug, Clone)]
pub struct Artifact {
    pub tool: Tool,
    pub version: Version,
    pub url: String,
    /// File name as listed in the checksum file.
    pub file_name: String,
    pub verification: Verification,
    pub archive: ArchiveKind,
    /// Path of the executable inside the archive.
    pub member: String,
    /// Upper bound for the download size (bytes).
    pub max_size: u64,
}

const MIB: u64 = 1024 * 1024;

pub fn artifact(tool: Tool, version: &Version, arch: Arch) -> Result<Artifact, ToolchainError> {
    let v = version.as_str();
    let hashicorp = |product: &str, max_mib: u64| {
        let a = match arch {
            Arch::X86_64 => "amd64",
            Arch::Aarch64 => "arm64",
        };
        let base = format!("https://releases.hashicorp.com/{product}/{v}");
        let file_name = format!("{product}_{v}_linux_{a}.zip");
        Artifact {
            tool,
            version: version.clone(),
            url: format!("{base}/{file_name}"),
            file_name,
            verification: Verification::SignedChecksums {
                sums_url: format!("{base}/{product}_{v}_SHA256SUMS"),
                signature_url: format!("{base}/{product}_{v}_SHA256SUMS.72D7468F.sig"),
                keys: keys::HASHICORP,
            },
            archive: ArchiveKind::Zip,
            member: product.to_owned(),
            max_size: max_mib * MIB,
        }
    };
    Ok(match tool {
        Tool::Terraform => hashicorp("terraform", 200),
        Tool::TerraformLs => hashicorp("terraform-ls", 200),
        Tool::Opentofu => {
            let a = match arch {
                Arch::X86_64 => "amd64",
                Arch::Aarch64 => "arm64",
            };
            let base = format!("https://github.com/opentofu/opentofu/releases/download/v{v}");
            let file_name = format!("tofu_{v}_linux_{a}.tar.gz");
            Artifact {
                tool,
                version: version.clone(),
                url: format!("{base}/{file_name}"),
                file_name,
                verification: Verification::SignedChecksums {
                    sums_url: format!("{base}/tofu_{v}_SHA256SUMS"),
                    signature_url: format!("{base}/tofu_{v}_SHA256SUMS.gpgsig"),
                    keys: keys::OPENTOFU,
                },
                archive: ArchiveKind::TarGz,
                member: "tofu".into(),
                max_size: 200 * MIB,
            }
        }
        Tool::Node => {
            let a = match arch {
                Arch::X86_64 => "x64",
                Arch::Aarch64 => "arm64",
            };
            let base = format!("https://nodejs.org/dist/v{v}");
            let dir = format!("node-v{v}-linux-{a}");
            let file_name = format!("{dir}.tar.gz");
            Artifact {
                tool,
                version: version.clone(),
                url: format!("{base}/{file_name}"),
                file_name,
                verification: Verification::SignedChecksums {
                    sums_url: format!("{base}/SHASUMS256.txt"),
                    signature_url: format!("{base}/SHASUMS256.txt.sig"),
                    keys: keys::NODE,
                },
                archive: ArchiveKind::TarGz,
                member: format!("{dir}/bin/node"),
                max_size: 150 * MIB,
            }
        }
        Tool::Uv => {
            let target = match arch {
                Arch::X86_64 => "x86_64-unknown-linux-gnu",
                Arch::Aarch64 => "aarch64-unknown-linux-gnu",
            };
            let sha = UV_SHA256
                .iter()
                .find(|(ver, t, _)| *ver == v && *t == target)
                .map(|(_, _, h)| *h)
                .ok_or_else(|| {
                    ToolchainError::Unsupported(format!(
                        "uv {v} for {target} has no pinned checksum in this app version"
                    ))
                })?;
            let file_name = format!("uv-{target}.tar.gz");
            Artifact {
                tool,
                version: version.clone(),
                url: format!("https://github.com/astral-sh/uv/releases/download/{v}/{file_name}"),
                file_name,
                verification: Verification::PinnedSha256(sha),
                archive: ArchiveKind::TarGz,
                member: format!("uv-{target}/uv"),
                max_size: 100 * MIB,
            }
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn v(s: &str) -> Version {
        Version::parse(s).unwrap()
    }

    #[test]
    fn defaults_are_valid_versions() {
        for d in [
            defaults::TERRAFORM,
            defaults::OPENTOFU,
            defaults::TERRAFORM_LS,
            defaults::UV,
            defaults::NODE,
            defaults::PYTHON,
            defaults::ANSIBLE_CORE,
            defaults::ANSIBLE_LINT,
        ] {
            Version::parse(d).unwrap();
        }
        // the default uv version must have a pinned hash
        artifact(Tool::Uv, &v(defaults::UV), Arch::X86_64).unwrap();
    }

    #[test]
    fn urls_match_the_official_release_layout() {
        let tf = artifact(Tool::Terraform, &v("1.16.6"), Arch::X86_64).unwrap();
        assert_eq!(
            tf.url,
            "https://releases.hashicorp.com/terraform/1.16.6/terraform_1.16.6_linux_amd64.zip"
        );
        let Verification::SignedChecksums {
            sums_url,
            signature_url,
            ..
        } = &tf.verification
        else {
            panic!("terraform must use signed checksums");
        };
        assert_eq!(
            sums_url,
            "https://releases.hashicorp.com/terraform/1.16.6/terraform_1.16.6_SHA256SUMS"
        );
        assert!(signature_url.ends_with("terraform_1.16.6_SHA256SUMS.72D7468F.sig"));

        let tofu = artifact(Tool::Opentofu, &v("1.13.1"), Arch::X86_64).unwrap();
        assert_eq!(
            tofu.url,
            "https://github.com/opentofu/opentofu/releases/download/v1.13.1/tofu_1.13.1_linux_amd64.tar.gz"
        );
        let node = artifact(Tool::Node, &v("24.22.0"), Arch::X86_64).unwrap();
        assert_eq!(
            node.url,
            "https://nodejs.org/dist/v24.22.0/node-v24.22.0-linux-x64.tar.gz"
        );
        assert_eq!(node.member, "node-v24.22.0-linux-x64/bin/node");
        let uv = artifact(Tool::Uv, &v("0.13.1"), Arch::X86_64).unwrap();
        assert_eq!(
            uv.url,
            "https://github.com/astral-sh/uv/releases/download/0.13.1/uv-x86_64-unknown-linux-gnu.tar.gz"
        );
        assert!(matches!(uv.verification, Verification::PinnedSha256(_)));
    }

    #[test]
    fn uv_without_pinned_hash_is_refused() {
        assert!(matches!(
            artifact(Tool::Uv, &v("0.13.6"), Arch::X86_64),
            Err(ToolchainError::Unsupported(_))
        ));
        assert!(artifact(Tool::Uv, &v("0.13.1"), Arch::Aarch64).is_err());
    }

    #[test]
    fn checksum_file_names_match_real_sums_files() {
        let tf = artifact(Tool::Terraform, &v("1.16.6"), Arch::X86_64).unwrap();
        super::super::checksums::find_sha256(
            include_str!("testdata/terraform_1.16.6_SHA256SUMS"),
            &tf.file_name,
        )
        .unwrap();
        let tofu = artifact(Tool::Opentofu, &v("1.13.1"), Arch::X86_64).unwrap();
        super::super::checksums::find_sha256(
            include_str!("testdata/tofu_1.13.1_SHA256SUMS"),
            &tofu.file_name,
        )
        .unwrap();
        let node = artifact(Tool::Node, &v("24.22.0"), Arch::X86_64).unwrap();
        super::super::checksums::find_sha256(
            include_str!("testdata/node-v24.22.0-SHASUMS256.txt"),
            &node.file_name,
        )
        .unwrap();
    }
}
