//! The updater's app half (PLAT-4): when this build checks, where the
//! packages that only notify send the user, and the system clock. The update
//! source comes with M6-27, and the service with `App` (M6-05).

use std::time::{Duration, SystemTime, UNIX_EPOCH};

use gitcroc_core::UpdateClock;

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
