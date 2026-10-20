//! Which package the running app came from (PLAT-4, M5-12): what Tauri's
//! bundle type and the environment say on each platform. The decisions take
//! what was seen as arguments, so all of them run on every platform.

use std::ffi::OsStr;

use gitcroc_app::platform::package::{linux_package, macos_package, windows_package};
use gitcroc_core::Package;
use tauri::utils::config::BundleType;

#[test]
fn plat_4_installer_app_and_appimage_install() {
    let appimage = Some(OsStr::new("/home/me/Apps/gitcroc.AppImage"));
    assert_eq!(
        linux_package(Some(BundleType::AppImage), appimage),
        Package::AppImage
    );
    assert_eq!(
        windows_package(Some(BundleType::Nsis), true),
        Package::WindowsInstaller
    );
    assert_eq!(macos_package(Some(BundleType::App)), Package::MacApp);
    assert_eq!(macos_package(Some(BundleType::Dmg)), Package::MacApp);
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
    assert_eq!(linux_package(Some(BundleType::Deb), None), Package::Deb);
    assert_eq!(linux_package(Some(BundleType::Rpm), None), Package::Rpm);
    // A `.deb` started from inside an AppImage's environment is still the
    // `.deb`.
    assert_eq!(linux_package(Some(BundleType::Deb), appimage), Package::Deb);
    // The portable zip packs the binary the installer was built from, so it
    // carries the NSIS type, but has no uninstaller next to it.
    assert_eq!(
        windows_package(Some(BundleType::Nsis), false),
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
        linux_package(Some(BundleType::AppImage), None),
        Package::Unknown
    );
    assert_eq!(
        linux_package(Some(BundleType::AppImage), Some(OsStr::new(""))),
        Package::Unknown
    );
    // A build run from the source tree.
    assert_eq!(linux_package(None, None), Package::Unknown);
    assert_eq!(macos_package(None), Package::Unknown);
    assert_eq!(
        windows_package(Some(BundleType::Msi), true),
        Package::Unknown
    );
    assert!(!Package::Unknown.installs());
}
