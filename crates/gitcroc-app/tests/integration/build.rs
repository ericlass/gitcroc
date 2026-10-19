//! Build configuration (M0-08): one version number for the whole app, no
//! updater until auto-update arrives in M5, and the app identifier (ADR 0002).

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

/// ADR 0002: the app identifier is `dev.gitcroc.desktop`. Tauri merges an
/// overlay config over `tauri.conf.json` (`--config`, or a platform file such
/// as `tauri.linux.conf.json`), so no overlay may set another identifier.
#[test]
fn the_identifier_is_dev_gitcroc_desktop() {
    let config = read_json(&app_dir().join("tauri.conf.json"));
    assert_eq!(
        config["identifier"].as_str(),
        Some("dev.gitcroc.desktop"),
        "tauri.conf.json identifier"
    );

    for path in overlay_configs() {
        let identifier = if path.extension().is_some_and(|ext| ext == "toml") {
            let table: toml::Table = toml::from_str(&read(&path))
                .unwrap_or_else(|e| panic!("parse {}: {e}", path.display()));
            table.get("identifier").map(ToString::to_string)
        } else {
            read_json(&path).get("identifier").map(ToString::to_string)
        };
        assert_eq!(
            identifier,
            None,
            "{} must not set an identifier",
            path.display()
        );
    }
}

/// The Tauri configs in the app crate other than `tauri.conf.json`:
/// `tauri.<name>.conf.json` and `Tauri.<name>.toml`.
fn overlay_configs() -> Vec<PathBuf> {
    let entries =
        fs::read_dir(app_dir()).unwrap_or_else(|e| panic!("read {}: {e}", app_dir().display()));
    let mut overlays: Vec<PathBuf> = entries
        .map(|entry| entry.expect("directory entry").path())
        .filter(|path| {
            let name = path
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or_default();
            let lower = name.to_ascii_lowercase();
            name != "tauri.conf.json"
                && lower.starts_with("tauri.")
                && (lower.ends_with(".conf.json") || lower.ends_with(".toml"))
        })
        .collect();
    overlays.sort();
    overlays
}
