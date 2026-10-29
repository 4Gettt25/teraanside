use std::os::unix::fs::PermissionsExt;
use std::sync::atomic::AtomicBool;

use camino::Utf8PathBuf;
use reqwest::Url;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

use super::super::catalog::{ArchiveKind, Artifact, Tool, Verification};
use super::super::download::{Downloader, HostPolicy};
use super::super::keys::PinnedKey;
use super::super::version::Version;
use super::*;

const TEST_KEYS: &[PinnedKey] = &[PinnedKey {
    owner: "IaC Studio TEST key - never trust",
    fingerprint: "982AE15F94174C0766A0CB8719171C8FAF48F65B",
    armored: include_str!("../testdata/fake-release/test-key.asc"),
}];

fn fixture(name: &str) -> Vec<u8> {
    std::fs::read(format!(
        "{}/src/core/toolchain/testdata/fake-release/{name}",
        env!("CARGO_MANIFEST_DIR")
    ))
    .unwrap()
}

async fn serve(server: &MockServer, at: &str, body: Vec<u8>) {
    Mock::given(method("GET"))
        .and(path(at))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(body))
        .mount(server)
        .await;
}

async fn release_server() -> MockServer {
    let server = MockServer::start().await;
    for f in [
        "faketool_1.0.0_linux_amd64.zip",
        "faketool-1.0.0.tar.gz",
        "symlink-1.0.0.tar.gz",
        "SHA256SUMS",
        "SHA256SUMS.sig",
    ] {
        serve(&server, &format!("/rel/{f}"), fixture(f)).await;
    }
    server
}

fn signed_zip(base: &str) -> Artifact {
    Artifact {
        tool: Tool::Terraform,
        version: Version::parse("1.0.0").unwrap(),
        url: format!("{base}/rel/faketool_1.0.0_linux_amd64.zip"),
        file_name: "faketool_1.0.0_linux_amd64.zip".into(),
        verification: Verification::SignedChecksums {
            sums_url: format!("{base}/rel/SHA256SUMS"),
            signature_url: format!("{base}/rel/SHA256SUMS.sig"),
            keys: TEST_KEYS,
        },
        archive: ArchiveKind::Zip,
        member: "faketool".into(),
        max_size: 1024 * 1024,
    }
}

fn pinned_tar(base: &str, file: &str, sha: &'static str) -> Artifact {
    Artifact {
        tool: Tool::Uv,
        version: Version::parse("1.0.0").unwrap(),
        url: format!("{base}/rel/{file}"),
        file_name: file.into(),
        verification: Verification::PinnedSha256(sha),
        archive: ArchiveKind::TarGz,
        member: "faketool-1.0.0/bin/faketool".into(),
        max_size: 1024 * 1024,
    }
}

const TAR_SHA: &str = "41f093d6ef2d0eb535187c1097f93e97cb78908ab9c1e5c7742769b77e1f6856";
const SYMLINK_TAR_SHA: &str = "aad64d284ca75bcdbcd32218f0b705dbc9100ec0db05a6842a669b18fcc6bd9b";

struct Env {
    _tmp: tempfile::TempDir,
    bin: Utf8PathBuf,
}

fn env() -> Env {
    let tmp = tempfile::tempdir().unwrap();
    let bin = Utf8PathBuf::from_path_buf(tmp.path().join("data/bin")).unwrap();
    Env { _tmp: tmp, bin }
}

fn downloader(server: &MockServer) -> Downloader {
    Downloader::new(HostPolicy::local_test_server(&server.uri(), &[])).unwrap()
}

async fn install(
    d: &Downloader,
    bin: &Utf8PathBuf,
    a: &Artifact,
) -> (Result<Utf8PathBuf, ToolchainError>, Vec<InstallProgress>) {
    let mut seen = Vec::new();
    let r = Installer {
        downloader: d,
        bin_root: bin,
    }
    .install(a, &AtomicBool::new(false), |p| seen.push(p))
    .await;
    (r, seen)
}

fn assert_nothing_installed(bin: &Utf8PathBuf) {
    let leftovers: Vec<_> = std::fs::read_dir(bin)
        .map(|rd| {
            rd.map(|e| e.unwrap().file_name().into_string().unwrap())
                .collect()
        })
        .unwrap_or_default();
    assert!(
        leftovers.iter().all(|n| !n.starts_with(".staging")),
        "staging left behind: {leftovers:?}"
    );
    assert!(installed(bin, Tool::Terraform, &Version::parse("1.0.0").unwrap()).is_none());
    assert!(installed(bin, Tool::Uv, &Version::parse("1.0.0").unwrap()).is_none());
}

#[tokio::test]
async fn installs_signed_zip_and_reuses_it() {
    let server = release_server().await;
    let e = env();
    let d = downloader(&server);
    let a = signed_zip(&server.uri());
    let (r, seen) = install(&d, &e.bin, &a).await;
    let exe = r.unwrap();
    assert_eq!(exe, e.bin.join("terraform/1.0.0/terraform"));
    assert!(std::fs::read_to_string(&exe).unwrap().contains("fake-tool"));
    assert_ne!(
        std::fs::metadata(&exe).unwrap().permissions().mode() & 0o100,
        0
    );
    assert_eq!(seen.first(), Some(&InstallProgress::Verifying));
    assert!(
        seen.iter()
            .any(|p| matches!(p, InstallProgress::Downloading { .. }))
    );
    assert_eq!(seen.last(), Some(&InstallProgress::Done));
    // the traversal entry in the zip was never written anywhere
    assert!(!e.bin.join("terraform/escape.txt").exists());
    assert!(!e.bin.join("escape.txt").exists());

    // second call: already installed, no network needed
    drop(server);
    let (again, seen) = install(&d, &e.bin, &a).await;
    assert_eq!(again.unwrap(), exe);
    assert_eq!(seen, [InstallProgress::Done]);
}

#[tokio::test]
async fn installs_tar_gz_with_pinned_hash() {
    let server = release_server().await;
    let e = env();
    let a = pinned_tar(&server.uri(), "faketool-1.0.0.tar.gz", TAR_SHA);
    let (r, _) = install(&downloader(&server), &e.bin, &a).await;
    assert_eq!(r.unwrap(), e.bin.join("uv/1.0.0/uv"));
}

#[tokio::test]
async fn rejects_tampered_checksum_file() {
    let server = MockServer::start().await;
    let mut sums = fixture("SHA256SUMS");
    sums[0] = if sums[0] == b'0' { b'1' } else { b'0' };
    serve(&server, "/rel/SHA256SUMS", sums).await;
    serve(&server, "/rel/SHA256SUMS.sig", fixture("SHA256SUMS.sig")).await;
    serve(
        &server,
        "/rel/faketool_1.0.0_linux_amd64.zip",
        fixture("faketool_1.0.0_linux_amd64.zip"),
    )
    .await;
    let e = env();
    let (r, _) = install(&downloader(&server), &e.bin, &signed_zip(&server.uri())).await;
    assert!(matches!(r, Err(ToolchainError::Signature(_))), "{r:?}");
    assert_nothing_installed(&e.bin);
}

#[tokio::test]
async fn rejects_archive_with_wrong_hash() {
    let server = release_server().await;
    let e = env();
    let other = "0000000000000000000000000000000000000000000000000000000000000000";
    let a = pinned_tar(&server.uri(), "faketool-1.0.0.tar.gz", other);
    let (r, _) = install(&downloader(&server), &e.bin, &a).await;
    assert!(matches!(r, Err(ToolchainError::Checksum(_))), "{r:?}");
    assert_nothing_installed(&e.bin);
}

#[tokio::test]
async fn rejects_symlink_member() {
    let server = release_server().await;
    let e = env();
    let a = pinned_tar(&server.uri(), "symlink-1.0.0.tar.gz", SYMLINK_TAR_SHA);
    let (r, _) = install(&downloader(&server), &e.bin, &a).await;
    assert!(matches!(r, Err(ToolchainError::Archive(_))), "{r:?}");
    assert_nothing_installed(&e.bin);
}

#[tokio::test]
async fn refuses_sources_outside_the_allowlist() {
    let server = release_server().await;
    let e = env();
    let mut a = signed_zip(&server.uri());
    a.url = "https://evil.example/faketool.zip".into();
    let (r, _) = install(&downloader(&server), &e.bin, &a).await;
    assert!(matches!(r, Err(ToolchainError::NotAllowed(_))), "{r:?}");
}

#[tokio::test]
async fn redirects_only_to_allowed_origins() {
    let target = release_server().await;
    let front = MockServer::start().await;
    let location = format!("{}/rel/faketool-1.0.0.tar.gz", target.uri());
    Mock::given(method("GET"))
        .and(path("/rel/faketool-1.0.0.tar.gz"))
        .respond_with(ResponseTemplate::new(302).insert_header("Location", location.as_str()))
        .mount(&front)
        .await;
    let e = env();
    let a = pinned_tar(&front.uri(), "faketool-1.0.0.tar.gz", TAR_SHA);

    let strict = Downloader::new(HostPolicy::local_test_server(&front.uri(), &[])).unwrap();
    let (r, _) = install(&strict, &e.bin, &a).await;
    match r {
        Err(ToolchainError::Download(msg)) => assert!(msg.contains("redirect"), "{msg}"),
        other => panic!("expected refused redirect, got {other:?}"),
    }
    assert_nothing_installed(&e.bin);

    let target_uri = target.uri();
    let allowing = Downloader::new(HostPolicy::local_test_server(
        &front.uri(),
        &[target_uri.as_str()],
    ))
    .unwrap();
    let (r, _) = install(&allowing, &e.bin, &a).await;
    assert!(r.is_ok(), "{r:?}");
}

#[tokio::test]
async fn enforces_size_limit_and_cancel() {
    let server = release_server().await;
    let e = env();
    let d = downloader(&server);
    let mut a = pinned_tar(&server.uri(), "faketool-1.0.0.tar.gz", TAR_SHA);
    a.max_size = 10;
    let (r, _) = install(&d, &e.bin, &a).await;
    assert!(matches!(r, Err(ToolchainError::TooLarge { .. })), "{r:?}");

    let a = pinned_tar(&server.uri(), "faketool-1.0.0.tar.gz", TAR_SHA);
    let r = Installer {
        downloader: &d,
        bin_root: &e.bin,
    }
    .install(&a, &AtomicBool::new(true), |_| {})
    .await;
    assert!(matches!(r, Err(ToolchainError::Cancelled)), "{r:?}");
    assert_nothing_installed(&e.bin);
}

#[test]
fn official_policy() {
    let p = HostPolicy::official();
    let ok = |u: &str| p.allows_source(&Url::parse(u).unwrap());
    assert!(ok(
        "https://releases.hashicorp.com/terraform/1.16.6/terraform_1.16.6_linux_amd64.zip"
    ));
    assert!(ok(
        "https://github.com/opentofu/opentofu/releases/download/v1.13.1/x.tar.gz"
    ));
    assert!(ok("https://nodejs.org/dist/v24.22.0/SHASUMS256.txt"));
    assert!(!ok("http://releases.hashicorp.com/terraform/1.16.6/x.zip"));
    assert!(!ok("https://releases.hashicorp.com/vault/1.0.0/x.zip"));
    assert!(!ok(
        "https://releases.hashicorp.com.evil.example/terraform/x.zip"
    ));
    assert!(!ok(
        "https://user:pw@releases.hashicorp.com/terraform/x.zip"
    ));
    assert!(!ok(
        "https://github.com/evil/opentofu/releases/download/v1/x"
    ));
    assert!(!ok("https://release-assets.githubusercontent.com/x"));
    let redirect = |u: &str| p.allows_redirect(&Url::parse(u).unwrap());
    assert!(redirect(
        "https://release-assets.githubusercontent.com/github-production-release-asset/1/2"
    ));
    assert!(!redirect("https://objects.example/x"));
    assert!(!redirect("http://release-assets.githubusercontent.com/x"));
}

/// Real downloads from the official sources with the real keys (network).
/// Run manually: `cargo test real_downloads -- --ignored --nocapture`
#[tokio::test]
#[ignore = "network: downloads ~150 MB from the official release servers"]
#[allow(clippy::print_stderr)] // manual run with --nocapture: report sizes and timings
async fn real_downloads_of_all_default_tools() {
    use super::super::catalog::{self, Arch, defaults};
    let e = env();
    let d = Downloader::new(HostPolicy::official()).unwrap();
    for (tool, version) in [
        (Tool::Terraform, defaults::TERRAFORM),
        (Tool::Opentofu, defaults::OPENTOFU),
        (Tool::TerraformLs, defaults::TERRAFORM_LS),
        (Tool::Uv, defaults::UV),
        (Tool::Node, defaults::NODE),
    ] {
        let a = catalog::artifact(
            tool,
            &Version::parse(version).unwrap(),
            Arch::current().unwrap(),
        )
        .unwrap();
        let started = std::time::Instant::now();
        let (r, _) = install(&d, &e.bin, &a).await;
        let exe = r.unwrap_or_else(|err| panic!("{tool:?}: {err}"));
        let size = std::fs::metadata(&exe).unwrap().len();
        let out = std::process::Command::new(exe.as_std_path())
            .arg(if tool == Tool::Node {
                "--version"
            } else {
                "version"
            })
            .output()
            .unwrap();
        let first = String::from_utf8_lossy(&out.stdout)
            .lines()
            .next()
            .unwrap_or("")
            .to_owned();
        let first = if first.is_empty() && tool == Tool::Uv {
            let out = std::process::Command::new(exe.as_std_path())
                .arg("--version")
                .output()
                .unwrap();
            String::from_utf8_lossy(&out.stdout).trim().to_owned()
        } else {
            first
        };
        eprintln!(
            "{tool:?} {version}: {} MB in {:.1}s -> {first}",
            size / 1024 / 1024,
            started.elapsed().as_secs_f32()
        );
        assert!(first.contains(version), "{tool:?}: {first}");
    }
}
