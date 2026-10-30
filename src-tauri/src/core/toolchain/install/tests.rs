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

// ---- Archive and download hardening (zip-slip, links, ordering, limits) ----

mod hardening {
    use std::io::Write;

    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    use super::*;

    const MEMBER: &str = "faketool-1.0.0/bin/faketool";
    const SCRIPT: &[u8] = b"#!/bin/sh\necho ok\n";

    enum Entry<'a> {
        File(&'a str, &'a [u8]),
        /// Raw name written byte by byte (bypasses the tar crate's path checks).
        RawName(&'a str, &'a [u8]),
        Symlink(&'a str, &'a str),
        Hardlink(&'a str, &'a str),
    }

    fn tar_gz(entries: &[Entry]) -> Vec<u8> {
        let gz = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::fast());
        let mut b = tar::Builder::new(gz);
        for e in entries {
            let mut h = tar::Header::new_gnu();
            h.set_mode(0o755);
            h.set_mtime(0);
            match e {
                Entry::File(name, data) => {
                    h.set_size(data.len() as u64);
                    h.set_path_absolute(name).unwrap();
                    h.set_cksum();
                    b.append(&h, *data).unwrap();
                }
                Entry::RawName(name, data) => {
                    h.set_size(data.len() as u64);
                    let raw = &mut h.as_old_mut().name;
                    raw[..name.len()].copy_from_slice(name.as_bytes());
                    h.set_cksum();
                    b.append(&h, *data).unwrap();
                }
                Entry::Symlink(name, target) | Entry::Hardlink(name, target) => {
                    h.set_size(0);
                    h.set_entry_type(if matches!(e, Entry::Symlink(..)) {
                        tar::EntryType::Symlink
                    } else {
                        tar::EntryType::Link
                    });
                    h.set_path(name).unwrap();
                    h.set_link_name(target).unwrap();
                    h.set_cksum();
                    b.append(&h, std::io::empty()).unwrap();
                }
            }
        }
        b.into_inner().unwrap().finish().unwrap()
    }

    fn zip(entries: &[Entry]) -> Vec<u8> {
        let mut w = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
        let opts = zip::write::SimpleFileOptions::default().unix_permissions(0o755);
        for e in entries {
            match e {
                Entry::File(name, data) | Entry::RawName(name, data) => {
                    w.start_file(*name, opts).unwrap();
                    w.write_all(data).unwrap();
                }
                Entry::Symlink(name, target) | Entry::Hardlink(name, target) => {
                    w.add_symlink(*name, *target, opts).unwrap();
                }
            }
        }
        w.finish().unwrap().into_inner()
    }

    fn leak_sha(bytes: &[u8]) -> &'static str {
        Box::leak(crate::core::project::fs::hex_sha256(bytes).into_boxed_str())
    }

    fn artifact(base: &str, kind: ArchiveKind, member: &str, sha: &'static str) -> Artifact {
        Artifact {
            tool: Tool::Uv,
            version: Version::parse("1.0.0").unwrap(),
            url: format!("{base}/rel/archive"),
            file_name: "archive".into(),
            verification: Verification::PinnedSha256(sha),
            archive: kind,
            member: member.into(),
            max_size: 1024 * 1024,
        }
    }

    /// Every file below `root` (to prove nothing was written outside `bin/uv/1.0.0`).
    fn all_files(root: &std::path::Path) -> Vec<String> {
        let mut out = Vec::new();
        let mut stack = vec![root.to_path_buf()];
        while let Some(dir) = stack.pop() {
            for e in std::fs::read_dir(&dir).into_iter().flatten().flatten() {
                let p = e.path();
                if e.file_type().unwrap().is_dir() {
                    stack.push(p);
                } else {
                    out.push(p.strip_prefix(root).unwrap().to_string_lossy().into_owned());
                }
            }
        }
        out.sort();
        out
    }

    async fn install_archive(
        bytes: Vec<u8>,
        kind: ArchiveKind,
        member: &str,
        sha: &'static str,
    ) -> (Result<Utf8PathBuf, ToolchainError>, Vec<String>) {
        let server = MockServer::start().await;
        serve(&server, "/rel/archive", bytes).await;
        let tmp = tempfile::tempdir().unwrap();
        let bin = Utf8PathBuf::from_path_buf(tmp.path().join("data/bin")).unwrap();
        let (r, _) = install(
            &downloader(&server),
            &bin,
            &artifact(&server.uri(), kind, member, sha),
        )
        .await;
        (r, all_files(tmp.path()))
    }

    #[tokio::test]
    async fn traversal_and_absolute_entries_are_never_written() {
        for kind in [ArchiveKind::TarGz, ArchiveKind::Zip] {
            let entries = [
                Entry::RawName("../../../escape-rel", b"pwned"),
                Entry::File("/tmp/iac-studio-escape-abs", b"pwned"),
                Entry::RawName("faketool-1.0.0/../../escape-mid", b"pwned"),
                Entry::File(MEMBER, SCRIPT),
            ];
            let bytes = match kind {
                ArchiveKind::TarGz => tar_gz(&entries),
                ArchiveKind::Zip => zip(&entries),
            };
            let sha = leak_sha(&bytes);
            let (r, files) = install_archive(bytes, kind, MEMBER, sha).await;
            r.unwrap();
            assert_eq!(
                files,
                [
                    "data/bin/uv/1.0.0/.iac-studio-verified",
                    "data/bin/uv/1.0.0/uv"
                ],
                "{kind:?}"
            );
            assert!(!std::path::Path::new("/tmp/iac-studio-escape-abs").exists());
        }
    }

    #[tokio::test]
    async fn member_that_is_a_link_is_rejected() {
        let cases: Vec<(ArchiveKind, Vec<u8>)> = vec![
            (
                ArchiveKind::TarGz,
                tar_gz(&[Entry::Symlink(MEMBER, "/etc/passwd")]),
            ),
            (
                ArchiveKind::TarGz,
                tar_gz(&[Entry::Symlink(MEMBER, "../../../../outside")]),
            ),
            (
                ArchiveKind::TarGz,
                tar_gz(&[Entry::Hardlink(MEMBER, "/etc/passwd")]),
            ),
            (
                ArchiveKind::Zip,
                zip(&[Entry::Symlink(MEMBER, "/etc/passwd")]),
            ),
        ];
        for (kind, bytes) in cases {
            let sha = leak_sha(&bytes);
            let (r, files) = install_archive(bytes, kind, MEMBER, sha).await;
            assert!(
                matches!(r, Err(ToolchainError::Archive(_))),
                "{kind:?}: {r:?}"
            );
            assert!(files.is_empty(), "{kind:?}: {files:?}");
        }
    }

    #[tokio::test]
    async fn entries_with_lookalike_names_do_not_match_the_member() {
        // "./x", "x/", absolute and traversal spellings of the member name are not it
        let bytes = tar_gz(&[
            Entry::File(&format!("/{MEMBER}"), b"abs"),
            Entry::RawName("faketool-1.0.0/bin/../bin/faketool", b"dotdot"),
        ]);
        let sha = leak_sha(&bytes);
        let (r, files) = install_archive(bytes, ArchiveKind::TarGz, MEMBER, sha).await;
        assert!(matches!(r, Err(ToolchainError::Archive(_))), "{r:?}");
        assert!(files.is_empty(), "{files:?}");
    }

    #[tokio::test]
    async fn checksum_is_verified_before_anything_is_extracted() {
        let bytes = tar_gz(&[Entry::File(MEMBER, SCRIPT)]);
        let wrong = "1111111111111111111111111111111111111111111111111111111111111111";
        let (r, files) = install_archive(bytes, ArchiveKind::TarGz, MEMBER, wrong).await;
        assert!(matches!(r, Err(ToolchainError::Checksum(_))), "{r:?}");
        assert!(
            files.is_empty(),
            "nothing extracted or left behind: {files:?}"
        );
    }

    #[tokio::test]
    async fn signature_is_verified_before_the_archive_is_downloaded() {
        let server = MockServer::start().await;
        let mut sums = fixture("SHA256SUMS");
        sums.extend_from_slice(b"\n");
        serve(&server, "/rel/SHA256SUMS", sums).await;
        serve(&server, "/rel/SHA256SUMS.sig", fixture("SHA256SUMS.sig")).await;
        Mock::given(method("GET"))
            .and(path("/rel/faketool_1.0.0_linux_amd64.zip"))
            .respond_with(ResponseTemplate::new(200))
            .expect(0) // verified when the server is dropped
            .mount(&server)
            .await;
        let e = env();
        let (r, _) = install(&downloader(&server), &e.bin, &signed_zip(&server.uri())).await;
        assert!(matches!(r, Err(ToolchainError::Signature(_))), "{r:?}");
    }

    #[tokio::test]
    async fn redirect_chains_are_checked_at_every_hop() {
        let evil = MockServer::start().await;
        serve(&evil, "/x", fixture("faketool-1.0.0.tar.gz")).await;
        let mid = MockServer::start().await;
        let front = MockServer::start().await;
        let redirect =
            |to: String| ResponseTemplate::new(302).insert_header("Location", to.as_str());
        Mock::given(path("/rel/faketool-1.0.0.tar.gz"))
            .respond_with(redirect(format!("{}/hop", mid.uri())))
            .mount(&front)
            .await;
        Mock::given(path("/hop"))
            .respond_with(redirect(format!("{}/x", evil.uri())))
            .mount(&mid)
            .await;
        let mid_uri = mid.uri();
        let d = Downloader::new(HostPolicy::local_test_server(
            &front.uri(),
            &[mid_uri.as_str()],
        ))
        .unwrap();
        let e = env();
        let a = pinned_tar(&front.uri(), "faketool-1.0.0.tar.gz", TAR_SHA);
        let (r, _) = install(&d, &e.bin, &a).await;
        assert!(
            matches!(&r, Err(ToolchainError::Download(m)) if m.contains("redirect")),
            "{r:?}"
        );
        assert_nothing_installed(&e.bin);
    }

    #[tokio::test]
    async fn redirect_loops_are_cut_off() {
        let server = MockServer::start().await;
        let me = format!("{}/rel/faketool-1.0.0.tar.gz", server.uri());
        Mock::given(path("/rel/faketool-1.0.0.tar.gz"))
            .respond_with(ResponseTemplate::new(302).insert_header("Location", me.as_str()))
            .mount(&server)
            .await;
        let e = env();
        let (r, _) = install(
            &downloader(&server),
            &e.bin,
            &pinned_tar(&server.uri(), "faketool-1.0.0.tar.gz", TAR_SHA),
        )
        .await;
        assert!(
            matches!(&r, Err(ToolchainError::Download(m)) if m.contains("redirect")),
            "{r:?}"
        );
    }

    /// A server that streams `total` bytes with chunked encoding (no Content-Length).
    async fn chunked_server(total: usize) -> String {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            while let Ok((mut sock, _)) = listener.accept().await {
                tokio::spawn(async move {
                    let mut req = Vec::new();
                    let mut buf = [0u8; 1024];
                    while !req.windows(4).any(|w| w == b"\r\n\r\n") {
                        let n = sock.read(&mut buf).await.unwrap_or(0);
                        if n == 0 {
                            return;
                        }
                        req.extend_from_slice(&buf[..n]);
                    }
                    let head = b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\nConnection: close\r\n\r\n";
                    if sock.write_all(head).await.is_err() {
                        return;
                    }
                    let chunk = vec![b'x'; 1024];
                    for _ in 0..total / 1024 {
                        let frame = [b"400\r\n".as_slice(), &chunk, b"\r\n"].concat();
                        if sock.write_all(&frame).await.is_err() {
                            return; // client gave up (expected once the limit is hit)
                        }
                    }
                    let _ = sock.write_all(b"0\r\n\r\n").await;
                });
            }
        });
        format!("http://{addr}")
    }

    #[tokio::test]
    async fn size_limit_applies_while_streaming_without_content_length() {
        let base = chunked_server(10 * 1024 * 1024).await;
        let d = Downloader::new(HostPolicy::local_test_server(&base, &[])).unwrap();
        let e = env();
        let mut a = artifact(&base, ArchiveKind::TarGz, MEMBER, TAR_SHA);
        a.max_size = 64 * 1024;
        let mut largest = 0u64;
        let r = Installer {
            downloader: &d,
            bin_root: &e.bin,
        }
        .install(&a, &AtomicBool::new(false), |p| {
            if let InstallProgress::Downloading { received, total } = p {
                assert!(total.is_none(), "server must not announce a length");
                largest = largest.max(received);
            }
        })
        .await;
        assert!(matches!(r, Err(ToolchainError::TooLarge { .. })), "{r:?}");
        assert!(
            largest <= 64 * 1024,
            "never accepted more than the limit: {largest}"
        );
        assert_nothing_installed(&e.bin);
    }
}
