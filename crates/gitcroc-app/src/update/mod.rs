//! The updater's app half (PLAT-4): when this build checks, where the
//! packages that only notify send the user, the system clock, and
//! [`OwnSource`], which reads 1.0.0-rc.1's `latest.json`, verifies each
//! package with the same key (SEC-7) and installs it without Tauri. The
//! service is `App`'s.

#[cfg(feature = "update-test")]
pub mod at_start;
pub mod install;
pub mod manifest;
mod source;
pub mod verify;

pub use source::OwnSource;

use std::ffi::OsString;
use std::path::PathBuf;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use gitcroc_core::{Package, UpdateClock, UpdateFound, UpdateFuture, UpdateProgress, UpdateSource};

use crate::platform::{Installation, PlatformInstallation};

/// Whether this build checks for updates: release builds only. Debug builds
/// and the `e2e` build never do, so no test or smoke run reaches the network
/// (D12).
pub const CHECKS: bool = !(cfg!(debug_assertions) || cfg!(feature = "e2e"));

/// Where the packages that only notify send the user (PLAT-4). Fixed here,
/// never taken from the manifest.
pub const RELEASES_PAGE: &str = "https://github.com/ericlass/gitcroc/releases/latest";

/// The program the restart into an installed update starts, when GPUI's
/// own choice, the running executable, is wrong: an AppImage's executable
/// is inside its mount, which is gone once the app exits, so the AppImage
/// file, `$APPIMAGE`, is started instead. `None` everywhere else.
pub fn restart_path() -> Option<PathBuf> {
    restart_path_for(
        PlatformInstallation::package(),
        std::env::var_os("APPIMAGE"),
    )
}

/// [`restart_path`] for `package`, with `appimage` as `$APPIMAGE`.
pub fn restart_path_for(package: Package, appimage: Option<OsString>) -> Option<PathBuf> {
    match package {
        Package::AppImage => appimage.filter(|file| !file.is_empty()).map(PathBuf::from),
        _ => None,
    }
}

/// The system's clock, for the update schedule.
#[derive(Debug, Clone, Copy, Default)]
pub struct SystemClock;

impl UpdateClock for SystemClock {
    fn now(&self) -> u64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |since| {
                u64::try_from(since.as_millis()).unwrap_or(u64::MAX)
            })
    }

    fn sleep(&self, duration: Duration) -> std::pin::Pin<Box<dyn Future<Output = ()> + Send + '_>> {
        Box::pin(tokio::time::sleep(duration))
    }
}

/// What a test of the update dialog sets in place of the running app's
/// package and of D12's rule that only release builds check: with a fake
/// [`UpdateSource`], no check reaches the network. The binary leaves it
/// unset.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UpdateTest {
    pub package: gitcroc_core::Package,
    pub checks: bool,
}

/// A source that never finds an update, so nothing is ever installed: the
/// tests' that reach no network.
#[derive(Debug, Clone, Copy, Default)]
pub struct NoUpdates;

impl UpdateSource for NoUpdates {
    fn check(&self) -> UpdateFuture<'_, Option<UpdateFound>> {
        Box::pin(async { Ok(None) })
    }

    fn install(&self, _progress: UpdateProgress) -> UpdateFuture<'_, ()> {
        Box::pin(async { Err("no update was found".to_owned()) })
    }
}

#[cfg(test)]
mod tests;
