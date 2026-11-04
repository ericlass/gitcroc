//! Which package the running app came from (PLAT-4), which decides whether
//! an update installs itself. Each package's binary carries its
//! [`BundleKind`]: the packaging step builds it with `GITCROC_BUNDLE` set
//! (M6-28), and [`bundle_kind`] reads it. That alone is not enough, so each
//! platform adds what it can see:
//!
//! - Linux: the type of the `.deb`, `.rpm` or AppImage. An AppImage also
//!   needs `APPIMAGE` set, which its runtime sets to the file the updater
//!   replaces; without it the binary was copied out of one.
//! - Windows: the binary is marked as the NSIS installer's, and the portable
//!   zip packs that same binary (M0-08). The installer leaves
//!   `uninstall.exe` next to the app; the portable zip has none.
//! - macOS: the `.app`, which the `.dmg` holds.
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
    /// The NSIS installer's; the portable zip packs the same binary.
    Nsis,
    /// The `.app`'s; the `.dmg` holds the same app.
    App,
}

/// The kind of package this binary was built for: `GITCROC_BUNDLE` when it
/// was built, which `.github/workflows/packages.yml` sets for each package.
/// A plain `cargo build` is unmarked, so it only notifies.
pub fn bundle_kind() -> Option<BundleKind> {
    bundle_kind_named(option_env!("GITCROC_BUNDLE"))
}

/// The kind a `GITCROC_BUNDLE` value names: `deb`, `rpm`, `appimage`,
/// `nsis` or `app`. Anything else, or no value, is none.
pub fn bundle_kind_named(value: Option<&str>) -> Option<BundleKind> {
    match value? {
        "deb" => Some(BundleKind::Deb),
        "rpm" => Some(BundleKind::Rpm),
        "appimage" => Some(BundleKind::AppImage),
        "nsis" => Some(BundleKind::Nsis),
        "app" => Some(BundleKind::App),
        _ => None,
    }
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
        Some(BundleKind::App) => Package::MacApp,
        _ => Package::Unknown,
    }
}
