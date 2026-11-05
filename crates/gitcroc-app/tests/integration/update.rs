//! Which package the running app came from (PLAT-4, M5-12): what the
//! bundle kind and the environment say on each platform. The decisions take
//! what was seen as arguments, so all of them run on every platform.

use std::ffi::OsStr;

use gitcroc_app::platform::package::{
    BundleKind, bundle_kind, bundle_kind_named, linux_package, macos_package, windows_package,
};
use gitcroc_core::Package;

#[test]
fn plat_4_installer_app_and_appimage_install() {
    let appimage = Some(OsStr::new("/home/me/Apps/gitcroc.AppImage"));
    assert_eq!(
        linux_package(Some(BundleKind::AppImage), appimage),
        Package::AppImage
    );
    assert_eq!(
        windows_package(Some(BundleKind::Nsis), true),
        Package::WindowsInstaller
    );
    assert_eq!(macos_package(Some(BundleKind::App)), Package::MacApp);
    for package in [
        Package::AppImage,
        Package::WindowsInstaller,
        Package::MacApp,
    ] {
        assert!(package.installs(), "{package:?}");
    }
}

#[test]
fn plat_4_deb_rpm_and_portable_zip_only_notify() {
    let appimage = Some(OsStr::new("/home/me/Apps/gitcroc.AppImage"));
    assert_eq!(linux_package(Some(BundleKind::Deb), None), Package::Deb);
    assert_eq!(linux_package(Some(BundleKind::Rpm), None), Package::Rpm);
    // A `.deb` started from inside an AppImage's environment is still the
    // `.deb`.
    assert_eq!(linux_package(Some(BundleKind::Deb), appimage), Package::Deb);
    // The portable zip packs the binary the installer was built from, so it
    // carries the NSIS type, but has no uninstaller next to it.
    assert_eq!(
        windows_package(Some(BundleKind::Nsis), false),
        Package::WindowsPortable
    );
    assert_eq!(windows_package(None, false), Package::WindowsPortable);
    for package in [Package::Deb, Package::Rpm, Package::WindowsPortable] {
        assert!(!package.installs(), "{package:?}");
    }
}

#[test]
fn plat_4_an_unknown_build_only_notifies() {
    // An AppImage's binary copied out of it, without its runtime.
    assert_eq!(
        linux_package(Some(BundleKind::AppImage), None),
        Package::Unknown
    );
    assert_eq!(
        linux_package(Some(BundleKind::AppImage), Some(OsStr::new(""))),
        Package::Unknown
    );
    // A build run from the source tree.
    assert_eq!(linux_package(None, None), Package::Unknown);
    assert_eq!(macos_package(None), Package::Unknown);
    assert_eq!(
        windows_package(Some(BundleKind::Deb), true),
        Package::Unknown
    );
    assert!(!Package::Unknown.installs());
    // A plain `cargo build`, like this test's, is unmarked (M6-28).
    assert_eq!(bundle_kind(), None);
}

/// PLAT-4 (M6-28): the packaging step marks each package's binary with
/// `GITCROC_BUNDLE` at build time. The mapping is a function of the value,
/// so every platform's kinds are tested on every platform.
#[test]
fn plat_4_bundle_kind_comes_from_the_build() {
    for (value, kind) in [
        ("deb", BundleKind::Deb),
        ("rpm", BundleKind::Rpm),
        ("appimage", BundleKind::AppImage),
        ("nsis", BundleKind::Nsis),
        ("app", BundleKind::App),
    ] {
        assert_eq!(bundle_kind_named(Some(value)), Some(kind), "{value}");
    }
    for value in [
        "", "dmg", "Deb", "DEB", " deb", "deb\n", "zip", "portable", "unknown",
    ] {
        assert_eq!(bundle_kind_named(Some(value)), None, "{value:?}");
    }
    assert_eq!(bundle_kind_named(None), None);
}

/// D12: debug and test builds never check for updates, and neither does the
/// `e2e` build, so no test or smoke run reaches the network. This test
/// binary is a debug build.
#[test]
fn d12_debug_and_test_builds_never_check() {
    const { assert!(cfg!(debug_assertions)) };
    const { assert!(!gitcroc_app::update::CHECKS) };
    let source = std::fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/update/mod.rs"),
    )
    .unwrap();
    assert!(
        source.contains("!(cfg!(debug_assertions) || cfg!(feature = \"e2e\"))"),
        "CHECKS must be off in debug and e2e builds"
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

/// SEC-7 (M6-28b): what `release-manifest sign` writes is what the app
/// accepts, for the version it was signed for and no other.
#[test]
fn sec_7_signature_carries_the_version() {
    use base64::Engine;
    use base64::engine::general_purpose::STANDARD;
    use gitcroc_app::update::verify;
    use release_manifest::sign;

    // As `tauri signer generate --ci` makes it: the empty password.
    let pair = minisign::KeyPair::generate_encrypted_keypair(Some(String::new())).unwrap();
    let secret = STANDARD.encode(pair.sk.to_box(None).unwrap().into_string());
    let public = STANDARD.encode(pair.pk.to_box().unwrap().into_string());
    let key = sign::secret_key(&secret, "").unwrap();
    let package = b"\x7fELF an AppImage".as_slice();
    let signature = sign::signature(
        &key,
        package,
        "gitcroc_1.0.0-rc.2_amd64.AppImage",
        "1.0.0-rc.2",
    )
    .unwrap();

    verify::verify(package, &signature, &public, "1.0.0-rc.2").unwrap();
    let older = verify::verify(package, &signature, &public, "1.0.0-rc.1").unwrap_err();
    assert!(older.contains("1.0.0-rc.2"), "{older}");
    assert!(verify::verify(package, &signature, verify::PUBLIC_KEY, "1.0.0-rc.2").is_err());
}

/// SEC-7 (M6-28b): the release workflow checks its signatures with the key
/// the app checks updates with.
#[test]
fn sec_7_release_manifest_verifies_with_the_apps_key() {
    assert_eq!(
        release_manifest::sign::PUBLIC_KEY,
        gitcroc_app::update::verify::PUBLIC_KEY
    );
}

/// D12: only the update test's builds check at start and trust a test key.
/// `update-test` is no default, no crate turns it on, and the release
/// workflow never passes it (M6-28c).
#[test]
fn d12_update_test_is_not_a_default_feature_and_never_released() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let read = |path: &str| {
        std::fs::read_to_string(root.join(path)).unwrap_or_else(|e| panic!("read {path}: {e}"))
    };
    let manifest: toml::Table = toml::from_str(&read("crates/gitcroc-app/Cargo.toml")).unwrap();
    let features = manifest["features"].as_table().unwrap();
    assert!(features.contains_key("update-test"));
    let default = features.get("default").and_then(toml::Value::as_array);
    assert!(
        default.is_none_or(|default| default.iter().all(|f| f.as_str() != Some("update-test"))),
        "{default:?}"
    );
    for (name, enables) in features {
        let enables = enables.as_array().unwrap();
        assert!(
            enables.iter().all(|f| f.as_str() != Some("update-test")),
            "the feature {name} turns update-test on"
        );
    }
    // No other crate turns it on.
    for dir in ["crates", "tools"] {
        for entry in std::fs::read_dir(root.join(dir)).unwrap() {
            let cargo = entry.unwrap().path().join("Cargo.toml");
            if cargo.ends_with("gitcroc-app/Cargo.toml") || !cargo.is_file() {
                continue;
            }
            let text = std::fs::read_to_string(&cargo).unwrap();
            assert!(!text.contains("update-test"), "{}", cargo.display());
        }
    }
    assert!(!read("Cargo.toml").contains("update-test"));

    let release = read(".github/workflows/release.yml");
    assert!(
        !release.contains("update-test"),
        "release.yml passes update-test"
    );
    assert!(!release.contains("GITCROC_UPDATE_TEST"));
}
