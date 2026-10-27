//! Which package the running app came from (PLAT-4, M5-12): what the
//! bundle kind and the environment say on each platform. The decisions take
//! what was seen as arguments, so all of them run on every platform.

use std::ffi::OsStr;

use gitcroc_app::platform::package::{
    BundleKind, bundle_kind, linux_package, macos_package, windows_package,
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
    assert_eq!(macos_package(Some(BundleKind::Dmg)), Package::MacApp);
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
    // No package marks its binary until M6-28.
    assert_eq!(bundle_kind(), None);
}

/// D12: debug and test builds never check for updates, and neither does the
/// `e2e` build, so no test or smoke run reaches the network. This test
/// binary is a debug build.
#[test]
fn d12_debug_and_test_builds_never_check() {
    const { assert!(cfg!(debug_assertions)) };
    const { assert!(!gitcroc_app::update::CHECKS) };
    let source = std::fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/update.rs"),
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
