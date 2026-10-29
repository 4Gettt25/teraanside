use std::fmt;

use serde::Serialize;
use ts_rs::TS;

use super::ToolchainError;

/// A strict release version `MAJOR.MINOR.PATCH` (digits only).
///
/// Versions can come from untrusted project files (`project.toml`,
/// `.terraform-version`); the strict format guarantees they cannot inject
/// anything into download URLs, paths or arguments.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, TS)]
#[ts(export, type = "string")]
pub struct Version(String);

impl Version {
    pub fn parse(s: &str) -> Result<Self, ToolchainError> {
        let s = s.trim();
        let s = s.strip_prefix('v').unwrap_or(s);
        let parts: Vec<&str> = s.split('.').collect();
        let ok = parts.len() == 3
            && parts
                .iter()
                .all(|p| !p.is_empty() && p.len() <= 5 && p.bytes().all(|b| b.is_ascii_digit()));
        if ok {
            Ok(Self(s.to_owned()))
        } else {
            Err(ToolchainError::InvalidVersion(s.chars().take(40).collect()))
        }
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn major(&self) -> &str {
        self.0.split('.').next().unwrap_or("")
    }
}

impl fmt::Display for Version {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_strict_versions() {
        assert_eq!(Version::parse("1.16.6").unwrap().as_str(), "1.16.6");
        assert_eq!(Version::parse(" v1.13.1\n").unwrap().as_str(), "1.13.1");
        assert_eq!(Version::parse("24.22.0").unwrap().major(), "24");
    }

    #[test]
    fn rejects_everything_else() {
        for bad in [
            "",
            "1.2",
            "1.2.3.4",
            "1.2.x",
            "latest",
            "~> 1.5",
            ">=1.0.0",
            "1.6.0-beta1",
            "1.2.3/../../x",
            "1.2.3?a=b",
            "1..3",
            "1.2.3 ",
            "١.٢.٣",
            "1.2.999999",
        ] {
            let trimmed_ok = bad == "1.2.3 "; // surrounding whitespace is trimmed
            assert_eq!(Version::parse(bad).is_ok(), trimmed_ok, "{bad:?}");
        }
    }
}
