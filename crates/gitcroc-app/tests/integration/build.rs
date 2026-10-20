//! Build configuration (M0-08): one version number for the whole app, the
//! updater and its key (SEC-7, M5-12), the app identifier (ADR 0002) and no
//! bundled git (D13).

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

/// The project's updater public key (README "Updater key"): the content of
/// the `.pub` file, base64 as Tauri takes it. The private key never enters
/// the repo.
const UPDATER_PUBLIC_KEY: &str = "dW50cnVzdGVkIGNvbW1lbnQ6IG1pbmlzaWduIHB1YmxpYyBrZXk6IDdDOUE5NTE3NjI5QTdDOTkKUldTWmZKcGlGNVdhZkFycG4veXhKemFOZW1zVlJubDFnTzNRZTY5YnNReEJTWnFPWVNpU0hwc1oK";

/// SEC-7: update packages are verified with the project's own key. The
/// updater has the public key and one https endpoint, the latest release's
/// manifest; the Windows installer runs passive. Updater artifacts are made
/// by the release config of M5-13 only, so a build without the private key
/// still works.
#[test]
fn sec_7_updates_are_verified_with_the_project_key() {
    let config = read_json(&app_dir().join("tauri.conf.json"));
    let updater = config
        .pointer("/plugins/updater")
        .expect("tauri.conf.json configures the updater");
    assert_eq!(updater["pubkey"].as_str(), Some(UPDATER_PUBLIC_KEY));
    let endpoints: Vec<&str> = updater["endpoints"]
        .as_array()
        .expect("endpoints is a list")
        .iter()
        .map(|endpoint| endpoint.as_str().expect("an endpoint is text"))
        .collect();
    assert_eq!(
        endpoints,
        ["https://github.com/ericlass/gitcroc/releases/latest/download/latest.json"]
    );
    assert!(
        endpoints
            .iter()
            .all(|endpoint| endpoint.starts_with("https://"))
    );
    assert_eq!(
        config.pointer("/plugins/updater/windows/installMode"),
        Some(&Value::from("passive"))
    );
    assert!(
        updater.get("dangerousInsecureTransportProtocol").is_none()
            && updater.get("dangerousAcceptInvalidCerts").is_none()
            && updater.get("dangerousAcceptInvalidHostnames").is_none(),
        "the updater must not loosen its transport"
    );
    assert!(
        config.pointer("/bundle/createUpdaterArtifacts").is_none(),
        "tauri.conf.json must not create updater artifacts; the release config does (M5-13)"
    );
    for path in overlay_configs() {
        let overlay = read_json(&path);
        assert!(
            overlay.pointer("/plugins/updater").is_none(),
            "{} must not change the updater",
            path.display()
        );
    }

    let manifest = read(&app_dir().join("Cargo.toml"));
    assert!(
        manifest.contains("tauri-plugin-updater"),
        "the updater plugin is a dependency of gitcroc-app"
    );
    // The public key decodes to a minisign public key.
    let key = String::from_utf8(
        base64::engine::Engine::decode(
            &base64::engine::general_purpose::STANDARD,
            UPDATER_PUBLIC_KEY,
        )
        .expect("the key is base64"),
    )
    .expect("the key is text");
    assert!(
        key.starts_with("untrusted comment: minisign public key: "),
        "{key}"
    );
}

/// D12: debug and test builds never check for updates, and neither do the
/// `e2e` and `perf-trace` builds of the smoke test and the benches, so no
/// test, smoke run or bench reaches the network. This test binary is a
/// debug build.
#[test]
fn d12_debug_and_test_builds_never_check() {
    const { assert!(cfg!(debug_assertions)) };
    const { assert!(!gitcroc_app::update::CHECKS) };
    let source = read(&app_dir().join("src/update.rs"));
    assert!(
        source.contains(
            "!(cfg!(debug_assertions) || cfg!(feature = \"e2e\") || cfg!(feature = \"perf-trace\"))"
        ),
        "CHECKS must be off in debug, e2e and perf-trace builds"
    );
}

/// PLAT-4: the packages that only notify open the releases page, fixed in
/// the app and never taken from the manifest.
#[test]
fn plat_4_the_download_page_is_fixed() {
    assert_eq!(
        gitcroc_app::update::RELEASES_PAGE,
        "https://github.com/ericlass/gitcroc/releases/latest"
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

/// D13: git is not bundled. No Tauri config of the app lists a git binary
/// anywhere in `bundle`, as a resource, a sidecar (`externalBin`) or a file
/// of a package. The version half is `d13_requires_git_2_40_or_newer`.
#[test]
fn d13_git_is_not_bundled() {
    let configs = std::iter::once(app_dir().join("tauri.conf.json")).chain(overlay_configs());
    for path in configs {
        let config = if path.extension().is_some_and(|ext| ext == "toml") {
            let table: toml::Table = toml::from_str(&read(&path))
                .unwrap_or_else(|e| panic!("parse {}: {e}", path.display()));
            serde_json::to_value(table).expect("a TOML table is JSON")
        } else {
            read_json(&path)
        };
        let mut named = Vec::new();
        if let Some(bundle) = config.get("bundle") {
            names_in(bundle, &mut named);
        }
        let git: Vec<&String> = named.iter().filter(|name| names_git(name)).collect();
        assert!(git.is_empty(), "{} bundles git: {git:?}", path.display());
    }
    assert!(names_git("binaries/git"));
    assert!(names_git("bin\\git.exe"));
    assert!(names_git("git-x86_64-pc-windows-msvc.exe"));
    assert!(names_git("vendor/MinGit/cmd"));
    assert!(!names_git("icons/icon.ico"));
    assert!(!names_git("gitcroc-askpass"));
}

/// Every string in `value`, and every key of its objects.
fn names_in(value: &Value, names: &mut Vec<String>) {
    match value {
        Value::String(text) => names.push(text.clone()),
        Value::Array(items) => items.iter().for_each(|item| names_in(item, names)),
        Value::Object(map) => {
            for (key, item) in map {
                names.push(key.clone());
                names_in(item, names);
            }
        }
        _ => {}
    }
}

/// Whether a bundle entry is git or a folder of it: a path component named
/// `git` with any extension, a sidecar `git-<target triple>`, or a MinGit
/// or PortableGit folder.
fn names_git(entry: &str) -> bool {
    entry.split(['/', '\\']).any(|component| {
        let lower = component.to_ascii_lowercase();
        let stem = lower.split('.').next().unwrap_or_default();
        stem == "git"
            || (stem.starts_with("git-") && stem.matches('-').count() >= 3)
            || stem == "mingit"
            || stem == "portablegit"
    })
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
