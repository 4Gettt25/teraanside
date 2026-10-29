//! HTTPS downloads restricted to an allowlist of official release locations.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use camino::Utf8Path;
use reqwest::Url;
use sha2::{Digest, Sha256};
use tokio::io::AsyncWriteExt;

use super::ToolchainError;

/// Where downloads may start, and where redirects may lead.
#[derive(Debug, Clone)]
pub struct HostPolicy {
    /// (origin, path prefix) pairs a download URL must match.
    sources: Vec<(String, String)>,
    /// Origins a redirect may point to (in addition to `sources`).
    redirect_origins: Vec<String>,
    allow_plain_http: bool,
}

impl HostPolicy {
    /// The official release locations (DESIGN.md §10 "Downloads").
    pub fn official() -> Self {
        let s = |o: &str, p: &str| (o.to_owned(), p.to_owned());
        Self {
            sources: vec![
                s("https://releases.hashicorp.com", "/terraform/"),
                s("https://releases.hashicorp.com", "/terraform-ls/"),
                s(
                    "https://github.com",
                    "/opentofu/opentofu/releases/download/",
                ),
                s("https://github.com", "/astral-sh/uv/releases/download/"),
                s("https://nodejs.org", "/dist/"),
            ],
            // GitHub serves release assets from here after a redirect.
            redirect_origins: vec!["https://release-assets.githubusercontent.com".to_owned()],
            allow_plain_http: false,
        }
    }

    /// Tests only: a local mock server.
    #[cfg(test)]
    pub fn local_test_server(origin: &str, redirect_origins: &[&str]) -> Self {
        Self {
            sources: vec![(origin.to_owned(), "/".to_owned())],
            redirect_origins: redirect_origins.iter().map(|o| (*o).to_owned()).collect(),
            allow_plain_http: true,
        }
    }

    fn origin(url: &Url) -> String {
        url.origin().ascii_serialization()
    }

    fn scheme_ok(&self, url: &Url) -> bool {
        url.scheme() == "https" || (self.allow_plain_http && url.scheme() == "http")
    }

    pub fn allows_source(&self, url: &Url) -> bool {
        let origin = Self::origin(url);
        self.scheme_ok(url)
            && url.username().is_empty()
            && url.password().is_none()
            && self
                .sources
                .iter()
                .any(|(o, prefix)| *o == origin && url.path().starts_with(prefix.as_str()))
    }

    pub fn allows_redirect(&self, url: &Url) -> bool {
        self.allows_source(url)
            || (self.scheme_ok(url) && self.redirect_origins.contains(&Self::origin(url)))
    }
}

pub struct Downloader {
    client: reqwest::Client,
    policy: Arc<HostPolicy>,
}

const MAX_REDIRECTS: usize = 5;

impl Downloader {
    pub fn new(policy: HostPolicy) -> Result<Self, ToolchainError> {
        let policy = Arc::new(policy);
        let redirect_policy = {
            let policy = policy.clone();
            reqwest::redirect::Policy::custom(move |attempt| {
                if attempt.previous().len() >= MAX_REDIRECTS {
                    attempt.error("too many redirects")
                } else if policy.allows_redirect(attempt.url()) {
                    attempt.follow()
                } else {
                    let target = HostPolicy::origin(attempt.url());
                    attempt.error(format!("redirect to {target} is not allowed"))
                }
            })
        };
        let client = reqwest::Client::builder()
            .redirect(redirect_policy)
            .https_only(!policy.allow_plain_http)
            .connect_timeout(Duration::from_secs(20))
            .read_timeout(Duration::from_secs(60))
            .user_agent(concat!("iac-studio/", env!("CARGO_PKG_VERSION")))
            .build()
            .map_err(|e| ToolchainError::Download(format!("http client: {e}")))?;
        Ok(Self { client, policy })
    }

    fn check(&self, url: &str) -> Result<Url, ToolchainError> {
        let parsed = Url::parse(url).map_err(|_| ToolchainError::NotAllowed(url.to_owned()))?;
        if self.policy.allows_source(&parsed) {
            Ok(parsed)
        } else {
            Err(ToolchainError::NotAllowed(url.to_owned()))
        }
    }

    async fn get(&self, url: &str) -> Result<reqwest::Response, ToolchainError> {
        let url = self.check(url)?;
        let response = self
            .client
            .get(url.clone())
            .send()
            .await
            .map_err(|e| ToolchainError::Download(describe(&url, &e)))?;
        response
            .error_for_status()
            .map_err(|e| ToolchainError::Download(describe(&url, &e)))
    }

    /// Small files (checksums, signatures) into memory.
    pub async fn fetch_bytes(&self, url: &str, limit: u64) -> Result<Vec<u8>, ToolchainError> {
        let mut response = self.get(url).await?;
        if response.content_length().is_some_and(|n| n > limit) {
            return Err(ToolchainError::TooLarge { limit });
        }
        let mut out = Vec::new();
        while let Some(chunk) = response
            .chunk()
            .await
            .map_err(|e| ToolchainError::Download(e.to_string()))?
        {
            if out.len() as u64 + chunk.len() as u64 > limit {
                return Err(ToolchainError::TooLarge { limit });
            }
            out.extend_from_slice(&chunk);
        }
        Ok(out)
    }

    /// Streams `url` into `dest` (created new), returns its SHA256 (lower-case hex).
    pub async fn fetch_to_file(
        &self,
        url: &str,
        dest: &Utf8Path,
        limit: u64,
        cancel: &AtomicBool,
        mut progress: impl FnMut(u64, Option<u64>),
    ) -> Result<String, ToolchainError> {
        let mut response = self.get(url).await?;
        let total = response.content_length();
        if total.is_some_and(|n| n > limit) {
            return Err(ToolchainError::TooLarge { limit });
        }
        let mut file = tokio::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(dest)
            .await
            .map_err(ToolchainError::io(format!("create {dest}")))?;
        let mut hasher = Sha256::new();
        let mut received = 0u64;
        progress(0, total);
        while let Some(chunk) = response
            .chunk()
            .await
            .map_err(|e| ToolchainError::Download(e.to_string()))?
        {
            if cancel.load(Ordering::Relaxed) {
                return Err(ToolchainError::Cancelled);
            }
            received += chunk.len() as u64;
            if received > limit {
                return Err(ToolchainError::TooLarge { limit });
            }
            hasher.update(&chunk);
            file.write_all(&chunk)
                .await
                .map_err(ToolchainError::io(format!("write {dest}")))?;
            progress(received, total);
        }
        file.sync_all()
            .await
            .map_err(ToolchainError::io(format!("sync {dest}")))?;
        Ok(super::super::project::fs::hex_sha256_digest(hasher))
    }
}

fn describe(url: &Url, e: &reqwest::Error) -> String {
    let what = if e.is_redirect() {
        "redirect refused"
    } else if e.is_timeout() {
        "timed out"
    } else if e.is_connect() {
        "could not connect"
    } else if e.is_status() {
        "server returned an error"
    } else {
        "request failed"
    };
    let status = e
        .status()
        .map(|s| format!(" (HTTP {})", s.as_u16()))
        .unwrap_or_default();
    let source = std::error::Error::source(e)
        .map(|s| format!(": {s}"))
        .unwrap_or_default();
    format!("{what}{status} for {}{source}", url.as_str())
}
