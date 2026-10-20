//! The updater's app half (PLAT-4, M5-12): Tauri's updater plugin as the
//! core's [`UpdateSource`], the system clock, and the events. The plugin
//! reads its one endpoint and the project's public key from
//! `tauri.conf.json` and refuses a package whose signature does not match
//! (SEC-7). The window has no updater permission: it reaches the plugin only
//! through the app's own commands (SEC-2).

use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use gitcroc_core::{
    AppLog, SettingsStore, UpdateClock, UpdateEvents, UpdateFound, UpdateFuture, UpdateProgress,
    UpdateSetup, UpdateSource, UpdateView, Updates,
};
use tauri::{AppHandle, Manager};
use tauri_plugin_updater::UpdaterExt;
use tauri_specta::Event;

use crate::ipc::UpdateStatusChangedEvent;
use crate::platform::{Installation, PlatformInstallation};

/// Whether this build checks for updates: release bundles only. Debug
/// builds, `just dev`, and the `e2e` and `perf-trace` builds of `just smoke`
/// and the benches never do, so no test, smoke run or bench reaches the
/// network (D12).
pub const CHECKS: bool =
    !(cfg!(debug_assertions) || cfg!(feature = "e2e") || cfg!(feature = "perf-trace"));

/// Where the packages that only notify send the user (PLAT-4). Fixed here,
/// never taken from the manifest.
pub const RELEASES_PAGE: &str = "https://github.com/ericlass/gitcroc/releases/latest";

/// The updater of the app, started with it. Needs the job engine managed.
pub fn updates(app: &AppHandle) -> Arc<Updates> {
    Updates::new(UpdateSetup {
        source: Arc::new(PluginSource {
            app: app.clone(),
            found: Mutex::new(None),
        }),
        clock: Arc::new(SystemClock),
        settings: app.state::<Arc<SettingsStore>>().inner().clone(),
        events: Arc::new(AppUpdateEvents { app: app.clone() }),
        package: PlatformInstallation::package(),
        checks: CHECKS,
        current_version: app.package_info().version.to_string(),
    })
}

/// Tauri's updater plugin.
struct PluginSource {
    app: AppHandle,
    /// What the last check found, for the install.
    found: Mutex<Option<tauri_plugin_updater::Update>>,
}

impl UpdateSource for PluginSource {
    fn check(&self) -> UpdateFuture<'_, Option<UpdateFound>> {
        Box::pin(async move {
            let updater = self.app.updater().map_err(|error| error.to_string())?;
            let update = updater.check().await.map_err(|error| error.to_string())?;
            let found = update.as_ref().map(|update| UpdateFound {
                version: update.version.clone(),
                // As the manifest gives it, RFC 3339.
                date: update
                    .raw_json
                    .get("pub_date")
                    .and_then(serde_json::Value::as_str)
                    .map(str::to_owned),
                notes: update.body.clone().unwrap_or_default(),
            });
            *lock(&self.found) = update;
            Ok(found)
        })
    }

    fn install(&self, progress: UpdateProgress) -> UpdateFuture<'_, ()> {
        Box::pin(async move {
            let update = lock(&self.found)
                .clone()
                .ok_or_else(|| "no update was found".to_owned())?;
            // Verified against the public key before anything is installed;
            // on Windows the passive installer takes over and restarts the
            // app.
            update
                .download_and_install(
                    |chunk, total| progress(u64::try_from(chunk).unwrap_or(u64::MAX), total),
                    || {},
                )
                .await
                .map_err(|error| error.to_string())
        })
    }
}

/// Locks `mutex`, also when a thread panicked while holding it; the value
/// is only ever replaced whole.
fn lock<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

struct SystemClock;

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

/// Sends each view as `update_status_changed`, and logs to `gitcroc.log`.
struct AppUpdateEvents {
    app: AppHandle,
}

impl UpdateEvents for AppUpdateEvents {
    fn changed(&self, view: &UpdateView) {
        // An emit fails only while the app shuts down.
        let _ = UpdateStatusChangedEvent(view.clone()).emit(&self.app);
    }

    fn log(&self, message: &str) {
        if let Some(log) = self.app.try_state::<Arc<AppLog>>() {
            log.warn(message);
        }
    }
}
