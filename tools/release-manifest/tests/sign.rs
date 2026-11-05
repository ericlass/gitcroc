//! `release-manifest sign` and `verify` as the release workflow runs them
//! (SEC-7, M6-28b): the key from the environment, a `.sig` beside each
//! file, and exit 1 on an error, 2 on a usage error.

// Integration test crate: helpers outside #[test] fns are test code too (QA-4).
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::fs;
use std::path::Path;
use std::process::{Command, Output};

use base64::Engine;
use base64::engine::general_purpose::STANDARD;

/// A key pair as `tauri signer generate` writes it: base64 of each box.
fn tauri_keys(password: &str) -> (String, String) {
    let pair = minisign::KeyPair::generate_encrypted_keypair(Some(password.to_owned())).unwrap();
    (
        STANDARD.encode(pair.sk.to_box(None).unwrap().into_string()),
        STANDARD.encode(pair.pk.to_box().unwrap().into_string()),
    )
}

fn run(dir: &Path, key: Option<(&str, &str)>, args: &[&str]) -> Output {
    let mut command = Command::new(env!("CARGO_BIN_EXE_release-manifest"));
    command
        .args(args)
        .current_dir(dir)
        .env_remove("TAURI_SIGNING_PRIVATE_KEY")
        .env_remove("TAURI_SIGNING_PRIVATE_KEY_PASSWORD");
    if let Some((key, password)) = key {
        command
            .env("TAURI_SIGNING_PRIVATE_KEY", key)
            .env("TAURI_SIGNING_PRIVATE_KEY_PASSWORD", password);
    }
    command.output().unwrap()
}

fn code(output: &Output) -> i32 {
    output.status.code().unwrap()
}

fn stderr(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}

#[test]
fn sec_7_sign_and_verify_from_the_command_line() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("gitcroc_1.0.0_x64-setup.exe"), b"MZ").unwrap();
    fs::write(dir.path().join("gitcroc.app.tar.gz"), b"\x1f\x8b").unwrap();
    // The empty password, as `tauri signer generate --ci` writes it; a
    // password costs seconds of scrypt in a debug build.
    let (secret, public) = tauri_keys("");
    let files = ["gitcroc_1.0.0_x64-setup.exe", "gitcroc.app.tar.gz"];

    let mut args = vec!["sign", "--version", "1.0.0"];
    args.extend(files);
    let signed = run(dir.path(), Some((&secret, "")), &args);
    assert_eq!(code(&signed), 0, "{}", stderr(&signed));
    for file in files {
        assert!(
            dir.path().join(format!("{file}.sig")).is_file(),
            "{file}.sig"
        );
    }

    let mut args = vec!["verify", "--version", "1.0.0", "--public-key", &public];
    args.extend(files);
    let verified = run(dir.path(), None, &args);
    assert_eq!(code(&verified), 0, "{}", stderr(&verified));

    // Another version, or the project's key, which did not sign them.
    let mut args = vec!["verify", "--version", "1.0.1", "--public-key", &public];
    args.extend(files);
    assert_eq!(code(&run(dir.path(), None, &args)), 1);
    let mut args = vec!["verify", "--version", "1.0.0"];
    args.extend(files);
    let project = run(dir.path(), None, &args);
    assert_eq!(code(&project), 1);
    assert!(
        stderr(&project).contains("gitcroc_1.0.0_x64-setup.exe"),
        "{}",
        stderr(&project)
    );
}

#[test]
fn sec_7_sign_fails_without_the_key_or_with_a_wrong_password() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("gitcroc.AppImage"), b"\x7fELF").unwrap();
    let (secret, _) = tauri_keys("pw");
    let args = ["sign", "--version", "1.0.0", "gitcroc.AppImage"];

    let no_key = run(dir.path(), None, &args);
    assert_eq!(code(&no_key), 1);
    assert!(
        stderr(&no_key).contains("TAURI_SIGNING_PRIVATE_KEY"),
        "{}",
        stderr(&no_key)
    );

    let wrong = run(dir.path(), Some((&secret, "nope")), &args);
    assert_eq!(code(&wrong), 1, "{}", stderr(&wrong));

    let missing = run(
        dir.path(),
        Some((&secret, "pw")),
        &["sign", "--version", "1.0.0", "missing.AppImage"],
    );
    assert_eq!(code(&missing), 1, "{}", stderr(&missing));
    assert!(!dir.path().join("gitcroc.AppImage.sig").exists());
}

#[test]
fn sec_7_sign_and_verify_usage_errors() {
    let dir = tempfile::tempdir().unwrap();
    for args in [
        &["sign", "--version", "1.0.0"][..],
        &["sign", "gitcroc.AppImage"],
        &[
            "sign",
            "--version",
            "1.0.0",
            "--public-key",
            "x",
            "gitcroc.AppImage",
        ],
        &[
            "sign",
            "--version",
            "1.0.0",
            "--dry-run",
            "gitcroc.AppImage",
        ],
        &["verify", "--version", "1.0.0"],
        &["verify", "--version", "1.0.0", "--public-key"],
        &["section", "--version", "1.0.0", "gitcroc.AppImage"],
        &["keygen"],
        &["keygen", "a.key", "b.key"],
        &["keygen", "--version", "1.0.0", "a.key"],
    ] {
        assert_eq!(code(&run(dir.path(), None, args)), 2, "{args:?}");
    }
}

/// The update test's throwaway key (M6-28c): `keygen` writes the secret
/// half as `TAURI_SIGNING_PRIVATE_KEY` takes it, with the empty password,
/// and prints the public half, which verifies what the secret one signs.
#[test]
fn sec_7_keygen_makes_a_throwaway_key() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("gitcroc.AppImage"), b"\x7fELF").unwrap();
    let made = run(dir.path(), None, &["keygen", "update-test.key"]);
    assert_eq!(code(&made), 0, "{}", stderr(&made));
    let public = String::from_utf8(made.stdout).unwrap().trim().to_owned();
    assert_ne!(public, release_manifest::sign::PUBLIC_KEY);
    let secret = fs::read_to_string(dir.path().join("update-test.key")).unwrap();

    let signed = run(
        dir.path(),
        Some((&secret, "")),
        &["sign", "--version", "1.0.1", "gitcroc.AppImage"],
    );
    assert_eq!(code(&signed), 0, "{}", stderr(&signed));
    let args = [
        "verify",
        "--version",
        "1.0.1",
        "--public-key",
        &public,
        "gitcroc.AppImage",
    ];
    assert_eq!(code(&run(dir.path(), None, &args)), 0);
    let project = ["verify", "--version", "1.0.1", "gitcroc.AppImage"];
    assert_eq!(code(&run(dir.path(), None, &project)), 1);

    // An existing file is never overwritten.
    let again = run(dir.path(), None, &["keygen", "update-test.key"]);
    assert_eq!(code(&again), 1, "{}", stderr(&again));
    assert_eq!(
        fs::read_to_string(dir.path().join("update-test.key")).unwrap(),
        secret
    );
}
