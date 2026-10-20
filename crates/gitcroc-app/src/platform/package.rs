//! Which package the running app came from (PLAT-4), which decides whether
//! an update installs itself. Tauri's bundler writes the bundle type into
//! the binary of each package it builds, and `bundle_type()` reads it. That
//! alone is not enough, so each platform adds what it can see:
//!
//! - Linux: the type of the `.deb`, `.rpm` or AppImage. An AppImage also
//!   needs `APPIMAGE` set, which its runtime sets to the file the updater
//!   replaces; without it the binary was copied out of one. The bundler
//!   leaves the last type it built in `target/release`'s binary, so a build
//!   run from there may claim one; it only ever notifies.
//! - Windows: the bundler marks the binary as the NSIS installer's, and the
//!   portable zip packs that same binary (M0-08). The installer leaves
//!   `uninstall.exe` next to the app; the portable zip has none.
//! - macOS: the `.app` of the `.dmg`, which `bundle_type()` reports for every
//!   macOS binary.
//!
//! These decisions take what was seen as arguments, so every platform's
//! tests run on every platform; M5-14 checks them on the built packages.

use std::ffi::OsStr;

use gitcroc_core::Package;
use tauri::utils::config::BundleType;

/// The uninstaller the NSIS installer puts next to the app.
pub const WINDOWS_UNINSTALLER: &str = "uninstall.exe";

/// The package on Linux, from the bundle type and `APPIMAGE`.
pub fn linux_package(bundle: Option<BundleType>, appimage: Option<&OsStr>) -> Package {
    match bundle {
        Some(BundleType::Deb) => Package::Deb,
        Some(BundleType::Rpm) => Package::Rpm,
        Some(BundleType::AppImage) if appimage.is_some_and(|path| !path.is_empty()) => {
            Package::AppImage
        }
        _ => Package::Unknown,
    }
}

/// The package on Windows, from the bundle type and whether the installer's
/// uninstaller is next to the app.
pub fn windows_package(bundle: Option<BundleType>, uninstaller: bool) -> Package {
    match bundle {
        Some(BundleType::Nsis) if uninstaller => Package::WindowsInstaller,
        Some(BundleType::Nsis) | None => Package::WindowsPortable,
        _ => Package::Unknown,
    }
}

/// The package on macOS, from the bundle type.
pub fn macos_package(bundle: Option<BundleType>) -> Package {
    match bundle {
        Some(BundleType::App | BundleType::Dmg) => Package::MacApp,
        _ => Package::Unknown,
    }
}
