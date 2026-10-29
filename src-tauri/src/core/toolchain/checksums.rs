//! Parsing of `SHA256SUMS`-style files (`<64 hex>  <file name>` per line).

use super::ToolchainError;

/// Returns the SHA256 (lower-case hex) listed for `file_name`.
pub fn find_sha256(sums: &str, file_name: &str) -> Result<String, ToolchainError> {
    let mut found = None;
    for line in sums.lines() {
        let Some((hash, name)) = line.split_once(char::is_whitespace) else {
            continue;
        };
        // GNU format: two spaces, or " *" for binary mode
        let name = name.trim_start().trim_start_matches('*');
        if name != file_name {
            continue;
        }
        if hash.len() != 64 || !hash.bytes().all(|b| b.is_ascii_hexdigit()) {
            return Err(ToolchainError::Checksum(format!(
                "malformed entry for {file_name}"
            )));
        }
        if found.replace(hash.to_ascii_lowercase()).is_some() {
            return Err(ToolchainError::Checksum(format!(
                "{file_name} listed twice"
            )));
        }
    }
    found.ok_or_else(|| ToolchainError::Checksum(format!("{file_name} not listed")))
}

#[cfg(test)]
mod tests {
    use super::*;

    const SUMS: &str = include_str!("testdata/terraform_1.16.6_SHA256SUMS");

    #[test]
    fn finds_entry_in_real_file() {
        assert_eq!(
            find_sha256(SUMS, "terraform_1.16.6_linux_amd64.zip").unwrap(),
            "38034b38d03096399a7a1264a14561a8029b2b496fd1f169452656e53968f1b7"
        );
    }

    #[test]
    fn rejects_missing_duplicate_and_malformed() {
        assert!(find_sha256(SUMS, "terraform_1.16.6_linux_amd64").is_err());
        let a = "38034b38d03096399a7a1264a14561a8029b2b496fd1f169452656e53968f1b7";
        let dup = format!("{a}  f.zip\n{a}  f.zip\n");
        assert!(find_sha256(&dup, "f.zip").is_err());
        assert!(find_sha256("abc  f.zip\n", "f.zip").is_err());
        assert_eq!(find_sha256(&format!("{a} *f.zip\n"), "f.zip").unwrap(), a);
    }
}
