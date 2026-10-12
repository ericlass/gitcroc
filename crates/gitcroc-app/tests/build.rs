//! Build configuration (M0-08): one version number for the whole app, and no
//! updater until auto-update arrives in M5.

// Integration test crate: helpers outside #[test] fns are test code too (QA-4).
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::fs;
use std::path::{Path, PathBuf};

use serde_json::Value;

fn app_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn repo_dir() -> PathBuf {
    app_dir().join("../..")
}

fn read(path: &Path) -> String {
    fs::read_to_string(path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()))
}

fn read_json(path: &Path) -> Value {
    serde_json::from_str(&read(path)).unwrap_or_else(|e| panic!("parse {}: {e}", path.display()))
}

fn workspace_version() -> String {
    let path = repo_dir().join("Cargo.toml");
    let manifest: toml::Table =
        toml::from_str(&read(&path)).unwrap_or_else(|e| panic!("parse {}: {e}", path.display()));
    manifest["workspace"]["package"]["version"]
        .as_str()
        .expect("workspace.package.version")
        .to_owned()
}

/// The version lives in the workspace `Cargo.toml` only. Tauri reads it from
/// the app crate when `tauri.conf.json` has no `version`; `ui/package.json`
/// may repeat it but must not disagree.
#[test]
fn one_version_number() {
    let version = workspace_version();
    assert_eq!(
        env!("CARGO_PKG_VERSION"),
        version,
        "the app crate inherits the workspace version"
    );

    let config = read_json(&app_dir().join("tauri.conf.json"));
    assert!(
        config.get("version").is_none(),
        "tauri.conf.json must not set a version; Tauri reads it from Cargo.toml"
    );

    let package = read_json(&repo_dir().join("ui/package.json"));
    if let Some(ui_version) = package.get("version") {
        assert_eq!(
            ui_version.as_str(),
            Some(version.as_str()),
            "ui/package.json version"
        );
    }
}

/// Auto-update arrives in M5; until then the build has no update endpoint.
#[test]
fn no_updater_until_m5() {
    let config = read_json(&app_dir().join("tauri.conf.json"));
    assert!(
        config.pointer("/plugins/updater").is_none(),
        "tauri.conf.json must not configure the updater"
    );
    assert!(
        config.pointer("/bundle/createUpdaterArtifacts").is_none(),
        "tauri.conf.json must not create updater artifacts"
    );

    let manifest = read(&app_dir().join("Cargo.toml"));
    assert!(
        !manifest.contains("tauri-plugin-updater"),
        "the updater plugin must not be a dependency"
    );
}
