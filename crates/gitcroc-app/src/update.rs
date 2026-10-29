//! The updater's app half (PLAT-4): when this build checks, where the
//! packages that only notify send the user, the system clock, and the source
//! the app uses until M6-27 provides the real one. The service is `App`'s.

use std::time::{Duration, SystemTime, UNIX_EPOCH};

use gitcroc_core::{UpdateClock, UpdateFound, UpdateFuture, UpdateProgress, UpdateSource};

/// Whether this build checks for updates: release builds only. Debug builds
/// and the `e2e` build never do, so no test or smoke run reaches the network
/// (D12).
pub const CHECKS: bool = !(cfg!(debug_assertions) || cfg!(feature = "e2e"));

/// Where the packages that only notify send the user (PLAT-4). Fixed here,
/// never taken from the manifest.
pub const RELEASES_PAGE: &str = "https://github.com/ericlass/gitcroc/releases/latest";

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

/// A source that never finds an update, so nothing is ever installed. The
/// app's until M6-27 reads the release manifest without Tauri.
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
