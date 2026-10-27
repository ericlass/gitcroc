//! Which package the running app came from (PLAT-4), which decides whether
//! an update installs itself. Each package's binary carries its
//! [`BundleKind`], which [`bundle_kind`] reads. That alone is not enough, so
//! each platform adds what it can see:
//!
//! - Linux: the type of the `.deb`, `.rpm` or AppImage. An AppImage also
//!   needs `APPIMAGE` set, which its runtime sets to the file the updater
//!   replaces; without it the binary was copied out of one.
//! - Windows: the binary is marked as the NSIS installer's, and the portable
//!   zip packs that same binary (M0-08). The installer leaves
//!   `uninstall.exe` next to the app; the portable zip has none.
//! - macOS: the `.app` of the `.dmg`.
//!
//! These decisions take what was seen as arguments, so every platform's
//! tests run on every platform; M5-14 checks them on the built packages.

use std::ffi::OsStr;

use gitcroc_core::Package;

/// The kind of package a binary was built for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BundleKind {
    Deb,
    Rpm,
    AppImage,
    Nsis,
    App,
    Dmg,
}

/// The kind of package this binary was built for. Tauri's bundler marked
/// it; until the packaging without Tauri marks it too (M6-28), no binary
/// claims one, so every build only notifies.
pub fn bundle_kind() -> Option<BundleKind> {
    None
}

/// The uninstaller the NSIS installer puts next to the app.
pub const WINDOWS_UNINSTALLER: &str = "uninstall.exe";

/// The package on Linux, from the bundle type and `APPIMAGE`.
pub fn linux_package(bundle: Option<BundleKind>, appimage: Option<&OsStr>) -> Package {
    match bundle {
        Some(BundleKind::Deb) => Package::Deb,
        Some(BundleKind::Rpm) => Package::Rpm,
        Some(BundleKind::AppImage) if appimage.is_some_and(|path| !path.is_empty()) => {
            Package::AppImage
        }
        _ => Package::Unknown,
    }
}

/// The package on Windows, from the bundle type and whether the installer's
/// uninstaller is next to the app.
pub fn windows_package(bundle: Option<BundleKind>, uninstaller: bool) -> Package {
    match bundle {
        Some(BundleKind::Nsis) if uninstaller => Package::WindowsInstaller,
        Some(BundleKind::Nsis) | None => Package::WindowsPortable,
        _ => Package::Unknown,
    }
}

/// The package on macOS, from the bundle type.
pub fn macos_package(bundle: Option<BundleKind>) -> Package {
    match bundle {
        Some(BundleKind::App | BundleKind::Dmg) => Package::MacApp,
        _ => Package::Unknown,
    }
}
