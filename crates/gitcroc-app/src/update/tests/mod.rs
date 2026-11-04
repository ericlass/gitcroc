//! The app's own updater (PLAT-4, SEC-7, M6-27): the manifest, the
//! signature, each platform's install step on every platform, and
//! `OwnSource` against a loopback server. Tests sign their packages with a
//! key of their own, as `tauri signer` signs: base64 of a minisign
//! signature whose trusted comment names the version.

mod server;

use std::ffi::OsString;
use std::fs;
use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use gitcroc_core::{Package, UpdateProgress, UpdateSource};
use serde_json::json;

use super::install::{self, Install};
use super::manifest::{self, Offer};
use super::source::{Endpoint, OwnSource, Target};
use super::verify;
use server::Server;

/// A key pair, and its public key as the app takes it: base64 of the key
/// file.
struct Keys {
    pair: minisign::KeyPair,
    public_key: String,
}

fn keys() -> Keys {
    let pair = minisign::KeyPair::generate_unencrypted_keypair().unwrap();
    let file = pair.pk.to_box().unwrap().into_string();
    Keys {
        public_key: STANDARD.encode(file),
        pair,
    }
}

/// A `.sig` as `tauri signer` writes it, with `trusted` as the trusted
/// comment.
fn sign_with_comment(keys: &Keys, package: &[u8], trusted: &str) -> String {
    let signature = minisign::sign(
        Some(&keys.pair.pk),
        &keys.pair.sk,
        package,
        Some(trusted),
        Some("signature from tauri secret key"),
    )
    .unwrap();
    STANDARD.encode(signature.into_string())
}

/// A `.sig` for `package` at `version`.
fn sign(keys: &Keys, package: &[u8], version: &str) -> String {
    sign_with_comment(
        keys,
        package,
        &format!("timestamp:1791649051\tfile:gitcroc.AppImage\tversion:{version}"),
    )
}

/// An AppImage's first bytes, then `rest`.
fn appimage(rest: &str) -> Vec<u8> {
    let mut bytes = b"\x7fELF".to_vec();
    bytes.extend_from_slice(rest.as_bytes());
    bytes
}

fn latest(version: &str, platforms: serde_json::Value) -> Vec<u8> {
    serde_json::to_vec(&json!({
        "version": version,
        "notes": "What changed",
        "pub_date": "2026-11-04T12:00:00Z",
        "platforms": platforms,
    }))
    .unwrap()
}

fn read(json: &[u8]) -> manifest::Manifest {
    manifest::read(json).unwrap()
}

#[test]
fn plat_4_manifest_platform_key_matches_release_manifest() {
    let names: Vec<String> = [
        "gitcroc_1.0.0_x64-setup.exe",
        "gitcroc-1.0.0-windows-x64-portable.zip",
        "gitcroc_1.0.0_universal.dmg",
        "gitcroc_universal.app.tar.gz",
        "gitcroc_1.0.0_amd64.AppImage",
        "gitcroc_1.0.0_amd64.deb",
        "gitcroc-1.0.0-1.x86_64.rpm",
    ]
    .map(str::to_owned)
    .to_vec();
    let packages = release_manifest::Packages::find(&names, "1.0.0").unwrap();
    let written = release_manifest::manifest(
        "1.0.0",
        "What changed",
        "2026-11-04T12:00:00Z",
        &packages,
        |file| Ok(format!("signature of {file}")),
    )
    .unwrap();
    let ours: Vec<&str> = [
        ("windows", "x86_64"),
        ("macos", "x86_64"),
        ("macos", "aarch64"),
        ("linux", "x86_64"),
    ]
    .iter()
    .map(|(os, arch)| manifest::platform_key(os, arch).unwrap())
    .collect();
    let mut theirs: Vec<&str> = written["platforms"]
        .as_object()
        .unwrap()
        .keys()
        .map(String::as_str)
        .collect();
    theirs.sort_unstable();
    let mut sorted = ours.clone();
    sorted.sort_unstable();
    assert_eq!(sorted, theirs);
    // No package updates itself on the other architectures.
    assert_eq!(manifest::platform_key("windows", "aarch64"), None);
    assert_eq!(manifest::platform_key("linux", "aarch64"), None);
    // The file the tool writes is read unchanged.
    let read = read(&serde_json::to_vec(&written).unwrap());
    assert_eq!(read.version, "1.0.0");
    assert_eq!(
        read.platforms["linux-x86_64"].url,
        "https://github.com/ericlass/gitcroc/releases/download/v1.0.0/gitcroc_1.0.0_amd64.AppImage"
    );
}

#[test]
fn plat_4_only_a_newer_version_is_an_update() {
    for newer in ["1.0.0-rc.2", "1.0.0", "1.0.1", "2.0.0-rc.1"] {
        assert!(manifest::is_newer("1.0.0-rc.1", newer).unwrap(), "{newer}");
    }
    for same_or_older in ["1.0.0-rc.1", "1.0.0-beta.3", "0.9.0"] {
        assert!(
            !manifest::is_newer("1.0.0-rc.1", same_or_older).unwrap(),
            "{same_or_older}"
        );
    }
    assert!(manifest::is_newer("1.0.0-rc.1", "latest").is_err());

    let json = latest(
        "1.0.0-rc.1",
        json!({ "linux-x86_64": { "url": "https://example.com/a", "signature": "s" } }),
    );
    let none = manifest::offer(read(&json), "1.0.0-rc.1", Some("linux-x86_64")).unwrap();
    assert_eq!(none, None);
    let json = latest(
        "1.0.0",
        json!({ "linux-x86_64": { "url": "https://example.com/a", "signature": "s" } }),
    );
    let found = manifest::offer(read(&json), "1.0.0-rc.1", Some("linux-x86_64")).unwrap();
    assert_eq!(
        found,
        Some(Offer {
            version: "1.0.0".to_owned(),
            notes: "What changed".to_owned(),
            date: "2026-11-04T12:00:00Z".to_owned(),
            url: "https://example.com/a".to_owned(),
            signature: "s".to_owned(),
        })
    );
}

#[test]
fn plat_4_manifest_without_this_platform_finds_no_update() {
    let json = latest(
        "1.0.0",
        json!({ "windows-x86_64": { "url": "https://example.com/a", "signature": "s" } }),
    );
    assert_eq!(
        manifest::offer(read(&json), "1.0.0-rc.1", Some("linux-x86_64")).unwrap(),
        None
    );
    // A platform where nothing updates itself.
    assert_eq!(
        manifest::offer(read(&json), "1.0.0-rc.1", None).unwrap(),
        None
    );
}

#[test]
fn plat_4_a_manifest_without_a_field_is_not_read() {
    for field in ["version", "notes", "pub_date", "platforms"] {
        let mut json: serde_json::Value =
            serde_json::from_slice(&latest("1.0.0", json!({}))).unwrap();
        json.as_object_mut().unwrap().remove(field);
        let error = manifest::read(&serde_json::to_vec(&json).unwrap()).unwrap_err();
        assert!(error.contains(field), "{error}");
    }
    let error = manifest::read(br#"{"version":"1.0.0","notes":"","pub_date":"","platforms":{"linux-x86_64":{"url":"https://a"}}}"#)
        .unwrap_err();
    assert!(error.contains("signature"), "{error}");
}

#[test]
fn sec_7_refuses_a_package_with_a_bad_signature() {
    let keys = keys();
    let package = appimage("version 1.0.0");
    let signature = sign(&keys, &package, "1.0.0");
    verify::verify(&package, &signature, &keys.public_key, "1.0.0").unwrap();

    let tampered = appimage("version 1.0.0, changed");
    let error = verify::verify(&tampered, &signature, &keys.public_key, "1.0.0").unwrap_err();
    assert!(error.contains("signature does not match"), "{error}");
    // Signed with another key.
    let other = keys_signature(&package);
    assert!(verify::verify(&package, &other, &keys.public_key, "1.0.0").is_err());
    // Not a signature at all.
    assert!(verify::verify(&package, "not base64!", &keys.public_key, "1.0.0").is_err());
    assert!(
        verify::verify(
            &package,
            &STANDARD.encode("text"),
            &keys.public_key,
            "1.0.0"
        )
        .is_err()
    );
}

fn keys_signature(package: &[u8]) -> String {
    sign(&keys(), package, "1.0.0")
}

#[test]
fn sec_7_refuses_a_signature_for_another_version() {
    let keys = keys();
    let package = appimage("version 1.0.0-rc.2");
    // An old package, validly signed, offered as a newer version.
    let signature = sign(&keys, &package, "1.0.0-rc.2");
    let error = verify::verify(&package, &signature, &keys.public_key, "1.0.0").unwrap_err();
    assert!(error.contains("1.0.0-rc.2"), "{error}");
    // A signature that names no version.
    let unversioned = sign_with_comment(
        &keys,
        &package,
        "timestamp:1791649051\tfile:gitcroc.AppImage",
    );
    assert!(verify::verify(&package, &unversioned, &keys.public_key, "1.0.0-rc.2").is_err());
    // Spelled with a `v`, it is the same version.
    let spelled = sign(&keys, &package, "v1.0.0-rc.2");
    verify::verify(&package, &spelled, &keys.public_key, "1.0.0-rc.2").unwrap();
}

#[test]
fn sec_7_refuses_a_download_url_that_is_not_https() {
    assert!(manifest::checked_url("https://github.com/a.AppImage", false).is_ok());
    for url in [
        "http://github.com/a.AppImage",
        "http://127.0.0.1:8080/a.AppImage",
        "ftp://github.com/a.AppImage",
        "file:///tmp/a.AppImage",
        "github.com/a.AppImage",
    ] {
        assert!(manifest::checked_url(url, false).is_err(), "{url}");
    }
    // M6-28's update test serves plain http from a loopback address only.
    for url in [
        "http://127.0.0.1:8080/a",
        "http://localhost:8080/a",
        "http://[::1]:8080/a",
    ] {
        assert!(manifest::checked_url(url, true).is_ok(), "{url}");
    }
    assert!(manifest::checked_url("http://github.com/a", true).is_err());
    assert!(manifest::checked_url("http://127.0.0.1.example.com/a", true).is_err());
}

#[tokio::test(flavor = "multi_thread")]
async fn sec_7_own_source_refuses_a_package_url_that_is_not_https() {
    let keys = keys();
    let package = appimage("new");
    let server = Server::start().await;
    server.serve(
        "/latest.json",
        latest(
            "1.0.0",
            json!({ "linux-x86_64": {
                "url": "http://example.com/gitcroc.AppImage",
                "signature": sign(&keys, &package, "1.0.0"),
            } }),
        ),
    );
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("gitcroc.AppImage");
    fs::write(&file, appimage("old")).unwrap();
    let source = loopback_source(&server, &keys, Install::AppImage { file: file.clone() });
    assert!(source.check().await.unwrap().is_some());
    let error = source.install(no_progress()).await.unwrap_err();
    assert!(error.contains("not https"), "{error}");
    assert_eq!(fs::read(&file).unwrap(), appimage("old"));
}

#[test]
fn sec_7_public_key_is_the_rc1_key() {
    // The `plugins.updater.pubkey` of crates/gitcroc-app/tauri.conf.json at
    // the tag v1.0.0-rc.1.
    const RC1: &str = "dW50cnVzdGVkIGNvbW1lbnQ6IG1pbmlzaWduIHB1YmxpYyBrZXk6IDdDOUE5NTE3NjI5QTdDOTkKUldTWmZKcGlGNVdhZkFycG4veXhKemFOZW1zVlJubDFnTzNRZTY5YnNReEJTWnFPWVNpU0hwc1oK";
    assert_eq!(verify::PUBLIC_KEY, RC1);
    verify::public_key(verify::PUBLIC_KEY).unwrap();
    let file = String::from_utf8(STANDARD.decode(verify::PUBLIC_KEY).unwrap()).unwrap();
    assert!(file.contains("7C9A9517629A7C99"), "{file}");
}

#[cfg(not(feature = "update-test"))]
#[test]
fn sec_7_release_build_endpoint_and_key_are_fixed() {
    assert_eq!(
        manifest::ENDPOINT,
        "https://github.com/ericlass/gitcroc/releases/latest/download/latest.json"
    );
    let release = Endpoint::release();
    assert_eq!(release.url, manifest::ENDPOINT);
    assert_eq!(release.public_key, verify::PUBLIC_KEY);
    assert!(!release.loopback_http);
    assert_eq!(Endpoint::built_in(), release);
    assert_eq!(OwnSource::new().endpoint(), &release);
}

#[test]
fn plat_4_nsis_update_arguments_match_tauri() {
    // What tauri-plugin-updater 2.14.0's `updater_parameters` builds for an
    // NSIS installer in passive mode with restart after install: the mode,
    // `/UPDATE`, the restart flag, then `/ARGS` and the app's arguments,
    // each escaped by its `escape_nsis_current_exe_arg`.
    assert_eq!(nsis(&[]), "/P /UPDATE /R /ARGS");
    assert_eq!(
        nsis(&[
            "--open",
            r"C:\Users\Me\My Repos",
            "a/b",
            "",
            r#"say "hi""#,
            r"trailing\ x\",
        ]),
        r#"/P /UPDATE /R /ARGS --open "C:\Users\Me\My Repos" "a/b" "" "say \"hi\"" "trailing\ x\\""#
    );
}

fn nsis(args: &[&str]) -> String {
    let args: Vec<OsString> = args.iter().map(OsString::from).collect();
    install::nsis_parameters(&args).into_string().unwrap()
}

#[test]
fn plat_4_appimage_is_replaced_in_place_with_its_mode() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("gitcroc.AppImage");
    fs::write(&file, appimage("old")).unwrap();
    // The temporary file starts private (0o600 on Unix), so keeping the
    // old file's mode (0o644 there, as written) shows it was copied.
    let mode = fs::metadata(&file).unwrap().permissions();
    install::replace_appimage(&appimage("new"), &file).unwrap();
    assert_eq!(fs::read(&file).unwrap(), appimage("new"));
    assert_eq!(fs::metadata(&file).unwrap().permissions(), mode);
    assert_eq!(entries(dir.path()), ["gitcroc.AppImage"]);
}

#[test]
fn plat_4_failed_appimage_install_leaves_the_old_file() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("gitcroc.AppImage");
    fs::write(&file, appimage("old")).unwrap();
    // Not ELF.
    let error = install::replace_appimage(b"<html>not found</html>", &file).unwrap_err();
    assert!(error.contains("not an AppImage"), "{error}");
    assert_eq!(fs::read(&file).unwrap(), appimage("old"));
    // The old file is gone from under it.
    let missing = dir.path().join("moved.AppImage");
    assert!(install::replace_appimage(&appimage("new"), &missing).is_err());
    assert!(!missing.exists());
    assert_eq!(entries(dir.path()), ["gitcroc.AppImage"]);
}

#[test]
fn plat_4_mac_app_swap_restores_the_backup_on_failure() {
    let dir = tempfile::tempdir().unwrap();
    let app = dir.path().join("gitcroc.app");
    let staging = dir.path().join(".gitcroc-update");
    fs::create_dir_all(app.join("Contents/MacOS")).unwrap();
    fs::write(app.join("Contents/MacOS/gitcroc"), "old").unwrap();
    fs::create_dir_all(&staging).unwrap();
    // The new app is not there to move.
    let error = install::swap_app(
        &app,
        &staging.join("new/gitcroc.app"),
        &staging.join("old.app"),
    )
    .unwrap_err();
    assert!(error.contains("could not be moved into place"), "{error}");
    assert_eq!(
        fs::read_to_string(app.join("Contents/MacOS/gitcroc")).unwrap(),
        "old"
    );
    assert!(!staging.join("old.app").exists());

    // And with it there, the swap succeeds and keeps the old one aside.
    let new = staging.join("gitcroc.app");
    fs::create_dir_all(new.join("Contents/MacOS")).unwrap();
    fs::write(new.join("Contents/MacOS/gitcroc"), "new").unwrap();
    install::swap_app(&app, &new, &staging.join("old.app")).unwrap();
    assert_eq!(
        fs::read_to_string(app.join("Contents/MacOS/gitcroc")).unwrap(),
        "new"
    );
    assert_eq!(
        fs::read_to_string(staging.join("old.app/Contents/MacOS/gitcroc")).unwrap(),
        "old"
    );
}

#[test]
fn plat_4_the_running_app_and_what_updates_it() {
    assert_eq!(
        install::running_app(Path::new(
            "/Applications/gitcroc.app/Contents/MacOS/gitcroc"
        )),
        Some(Path::new("/Applications/gitcroc.app").to_path_buf())
    );
    assert_eq!(install::running_app(Path::new("/usr/bin/gitcroc")), None);
    assert_eq!(
        Install::running(Package::WindowsInstaller),
        Ok(Install::WindowsInstaller)
    );
    for package in [
        Package::Deb,
        Package::Rpm,
        Package::WindowsPortable,
        Package::Unknown,
    ] {
        assert!(Install::running(package).is_err(), "{package:?}");
    }
    // The unpacked archive holds one app.
    let dir = tempfile::tempdir().unwrap();
    assert!(install::unpacked_app(dir.path()).is_err());
    fs::create_dir(dir.path().join("gitcroc.app")).unwrap();
    assert_eq!(
        install::unpacked_app(dir.path()).unwrap(),
        dir.path().join("gitcroc.app")
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn plat_4_nothing_is_written_before_verification() {
    let keys = keys();
    let package = appimage("new");
    let server = Server::start().await;
    server.serve("/gitcroc.AppImage", appimage("new, tampered with"));
    server.serve(
        "/latest.json",
        latest(
            "1.0.0",
            json!({ "linux-x86_64": {
                "url": server.url("/gitcroc.AppImage"),
                "signature": sign(&keys, &package, "1.0.0"),
            } }),
        ),
    );
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("gitcroc.AppImage");
    fs::write(&file, appimage("old")).unwrap();
    let source = loopback_source(&server, &keys, Install::AppImage { file: file.clone() });
    assert!(source.check().await.unwrap().is_some());
    let error = source.install(no_progress()).await.unwrap_err();
    assert!(error.contains("signature does not match"), "{error}");
    assert_eq!(fs::read(&file).unwrap(), appimage("old"));
    assert_eq!(entries(dir.path()), ["gitcroc.AppImage"]);
}

#[tokio::test(flavor = "multi_thread")]
async fn plat_4_own_source_finds_downloads_verifies_and_installs_over_loopback() {
    let keys = keys();
    let package = appimage(&"new version ".repeat(10_000));
    let server = Server::start().await;
    server.serve("/gitcroc.AppImage", package.clone());
    server.serve(
        "/latest.json",
        latest(
            "1.0.0",
            json!({
                "linux-x86_64": {
                    "url": server.url("/gitcroc.AppImage"),
                    "signature": sign(&keys, &package, "1.0.0"),
                },
                "windows-x86_64": {
                    "url": server.url("/gitcroc-setup.exe"),
                    "signature": "not this one",
                },
            }),
        ),
    );
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("gitcroc.AppImage");
    fs::write(&file, appimage("old")).unwrap();
    let source = loopback_source(&server, &keys, Install::AppImage { file: file.clone() });

    let found = source.check().await.unwrap().unwrap();
    assert_eq!(found.version, "1.0.0");
    assert_eq!(found.notes, "What changed");
    assert_eq!(found.date.as_deref(), Some("2026-11-04T12:00:00Z"));

    let downloaded = Arc::new(AtomicU64::new(0));
    let total = Arc::new(AtomicU64::new(0));
    let progress: UpdateProgress = {
        let downloaded = downloaded.clone();
        let total = total.clone();
        Box::new(move |chunk, length| {
            downloaded.fetch_add(chunk, Ordering::SeqCst);
            total.store(length.unwrap_or(0), Ordering::SeqCst);
        })
    };
    source.install(progress).await.unwrap();
    assert_eq!(fs::read(&file).unwrap(), package);
    let length = u64::try_from(package.len()).unwrap();
    assert_eq!(downloaded.load(Ordering::SeqCst), length);
    assert_eq!(total.load(Ordering::SeqCst), length);

    // The same version again is no update.
    let same = OwnSource::with(
        source.endpoint().clone(),
        "1.0.0".to_owned(),
        Some("linux-x86_64"),
        Target::Given(Install::AppImage { file }),
    );
    assert!(same.check().await.unwrap().is_none());
    assert_eq!(
        same.install(no_progress()).await.unwrap_err(),
        "no update was found"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn plat_4_a_failed_check_is_an_error_and_offers_nothing() {
    let keys = keys();
    let server = Server::start().await;
    // No manifest is served.
    let dir = tempfile::tempdir().unwrap();
    let source = loopback_source(
        &server,
        &keys,
        Install::AppImage {
            file: dir.path().join("gitcroc.AppImage"),
        },
    );
    let error = source.check().await.unwrap_err();
    assert!(error.contains("404"), "{error}");
    server.serve("/latest.json", b"{}".to_vec());
    assert!(source.check().await.is_err());
    assert_eq!(
        source.install(no_progress()).await.unwrap_err(),
        "no update was found"
    );
}

/// A source at 1.0.0-rc.1 on Linux reading the server's `latest.json`,
/// verifying with `keys`, and installing to `install`.
fn loopback_source(server: &Server, keys: &Keys, install: Install) -> OwnSource {
    OwnSource::with(
        Endpoint {
            url: server.url("/latest.json"),
            public_key: keys.public_key.clone(),
            loopback_http: true,
        },
        "1.0.0-rc.1".to_owned(),
        Some("linux-x86_64"),
        Target::Given(install),
    )
}

fn no_progress() -> UpdateProgress {
    Box::new(|_, _| {})
}

/// The names in `dir`, sorted.
fn entries(dir: &Path) -> Vec<String> {
    let mut names: Vec<String> = fs::read_dir(dir)
        .unwrap()
        .map(|entry| entry.unwrap().file_name().into_string().unwrap())
        .collect();
    names.sort();
    names
}
