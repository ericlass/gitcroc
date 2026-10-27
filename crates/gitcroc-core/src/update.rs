//! The update check and auto-update (PLAT-4, D12, SEC-7), without Tauri
//! (ARCH-5): `gitcroc-app` implements [`UpdateSource`], verifying each
//! package with the project's public key (M6-27).
//!
//! Only release bundles check (D12). The first check runs once the active
//! workspace's startup refresh has ended, so it never delays the cold start
//! (section 9), then one every [`CHECK_INTERVAL`] while the app runs, while
//! the `update_check` setting is on. Nothing about checks is saved. A failed
//! check is no error: the status shows it with its time and the log has it,
//! and the next check runs on schedule.
//!
//! How an update arrives depends on the package (PLAT-4): the Windows
//! installer, the macOS app and the AppImage install it as a job and restart;
//! the `.deb`, the `.rpm` and the Windows portable zip only tell that it
//! exists.

use std::future::Future;
use std::pin::Pin;
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Duration;

use serde::{Deserialize, Serialize};
use tokio::sync::{Notify, oneshot};

use crate::lock;
use crate::{JobId, JobState, Jobs, Outcome, SettingsStore, WorkspaceId};

/// How long the app waits between two automatic checks.
pub const CHECK_INTERVAL: Duration = Duration::from_secs(24 * 60 * 60);

/// The package the running app came from (PLAT-4).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "snake_case")]
pub enum Package {
    /// The NSIS installer.
    WindowsInstaller,
    /// The portable zip.
    WindowsPortable,
    /// The `.app` of the `.dmg`.
    MacApp,
    AppImage,
    Deb,
    Rpm,
    /// None of the above, such as a build run from the source tree.
    Unknown,
}

impl Package {
    /// Whether an update installs itself; the other packages only tell that
    /// it exists.
    pub fn installs(self) -> bool {
        matches!(self, Self::WindowsInstaller | Self::MacApp | Self::AppImage)
    }
}

/// A newer version a check found.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UpdateFound {
    pub version: String,
    /// When it was published, as the manifest gives it.
    pub date: Option<String>,
    /// Its release notes, shown as plain text (SEC-3).
    pub notes: String,
}

/// What an [`UpdateSource`] call returns: the error as text.
pub type UpdateFuture<'a, T> = Pin<Box<dyn Future<Output = Result<T, String>> + Send + 'a>>;

/// Told the length of each downloaded chunk, and the download's whole
/// length when it is known.
pub type UpdateProgress = Box<dyn Fn(u64, Option<u64>) + Send + Sync>;

/// Where updates come from: the release manifest, read and verified by
/// `gitcroc-app` (M6-27), or a fake in tests.
pub trait UpdateSource: Send + Sync {
    /// Asks for a newer version than the running one; `None` when there is
    /// none.
    fn check(&self) -> UpdateFuture<'_, Option<UpdateFound>>;

    /// Downloads the update the last check found, verifies it with the
    /// project's public key (SEC-7) and installs it. A package that fails
    /// verification is not installed, and the error says why.
    fn install(&self, progress: UpdateProgress) -> UpdateFuture<'_, ()>;
}

/// The time, and the waits of the schedule. Tests drive the schedule with a
/// simulated one.
pub trait UpdateClock: Send + Sync {
    /// Now, in milliseconds since the Unix epoch.
    fn now(&self) -> u64;

    /// Waits `duration`.
    fn sleep(&self, duration: Duration) -> Pin<Box<dyn Future<Output = ()> + Send + '_>>;
}

/// What the updater tells the app.
pub trait UpdateEvents: Send + Sync {
    /// The view changed.
    fn changed(&self, view: &UpdateView);
    /// A line for the log, such as why a check failed.
    fn log(&self, message: &str);
}

/// Where the update check and install are. Times and byte counts are `u64`,
/// declared to specta as `u32` so TypeScript gets a plain `number`; specta
/// makes an `f64` `number | null`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum UpdateStatus {
    /// No check has run yet.
    Idle,
    Checking,
    /// The last check found no newer version; times are milliseconds since
    /// the Unix epoch.
    UpToDate {
        #[specta(type = u32)]
        checked_at: u64,
    },
    /// The last check found `version`.
    Available {
        version: String,
        date: Option<String>,
        notes: String,
        #[specta(type = u32)]
        checked_at: u64,
    },
    /// `done` of `total` bytes of `version` are downloaded; `total` is `None`
    /// while it is not known.
    Downloading {
        version: String,
        #[specta(type = u32)]
        done: u64,
        #[specta(type = Option<u32>)]
        total: Option<u64>,
    },
    /// `version` is installed; the app restarts into it.
    ReadyToRestart {
        version: String,
    },
    /// The last check failed, such as when offline.
    Failed {
        reason: String,
        #[specta(type = u32)]
        checked_at: u64,
    },
}

/// The updater as the UI shows it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
pub struct UpdateView {
    pub status: UpdateStatus,
    pub current_version: String,
    pub package: Package,
    /// An update installs itself; otherwise the download page is offered.
    pub installs: bool,
    /// This build checks at all: release bundles only (D12).
    pub checks: bool,
    /// The available version's notice was put off with Later.
    pub later: bool,
}

/// Why an update was not installed.
#[derive(Debug, thiserror::Error)]
pub enum UpdateError {
    #[error("no update is on offer")]
    NoUpdate,
    #[error("this package does not install updates; download the new version instead")]
    OnlyNotifies,
    #[error("wait for the running jobs to end before updating")]
    JobsRunning,
    #[error(transparent)]
    Job(#[from] crate::JobError),
}

/// What the updater is built from.
pub struct UpdateSetup {
    pub source: Arc<dyn UpdateSource>,
    pub clock: Arc<dyn UpdateClock>,
    pub settings: Arc<SettingsStore>,
    pub events: Arc<dyn UpdateEvents>,
    pub package: Package,
    /// Whether this build checks: false for debug, test and bench builds.
    pub checks: bool,
    pub current_version: String,
}

/// A started install: its job, and whether the update was installed, sent
/// when the job ends.
#[derive(Debug)]
pub struct UpdateInstall {
    pub job_id: JobId,
    pub installed: oneshot::Receiver<bool>,
}

struct State {
    status: UpdateStatus,
    /// The version Later put off.
    later: Option<String>,
    startup_ended: bool,
}

/// The app's updater.
pub struct Updates {
    source: Arc<dyn UpdateSource>,
    clock: Arc<dyn UpdateClock>,
    settings: Arc<SettingsStore>,
    events: Arc<dyn UpdateEvents>,
    package: Package,
    checks: bool,
    current_version: String,
    state: Mutex<State>,
    startup: Notify,
}

impl std::fmt::Debug for Updates {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Updates")
            .field("package", &self.package)
            .field("checks", &self.checks)
            .finish_non_exhaustive()
    }
}

impl Updates {
    pub fn new(setup: UpdateSetup) -> Arc<Self> {
        Arc::new(Self {
            source: setup.source,
            clock: setup.clock,
            settings: setup.settings,
            events: setup.events,
            package: setup.package,
            checks: setup.checks,
            current_version: setup.current_version,
            state: Mutex::new(State {
                status: UpdateStatus::Idle,
                later: None,
                startup_ended: false,
            }),
            startup: Notify::new(),
        })
    }

    pub fn view(&self) -> UpdateView {
        self.view_of(&self.lock())
    }

    fn view_of(&self, state: &State) -> UpdateView {
        let later = match &state.status {
            UpdateStatus::Available { version, .. } => state.later.as_ref() == Some(version),
            _ => false,
        };
        UpdateView {
            status: state.status.clone(),
            current_version: self.current_version.clone(),
            package: self.package,
            installs: self.package.installs(),
            checks: self.checks,
            later,
        }
    }

    /// The active workspace's startup refresh has ended, or there was none
    /// to run: the first automatic check may start. Said again, it does
    /// nothing.
    pub fn startup_ended(&self) {
        self.lock().startup_ended = true;
        self.startup.notify_one();
    }

    /// Runs the automatic checks until the app ends: the first after the
    /// startup refresh, then one every [`CHECK_INTERVAL`], each only while
    /// the setting is on. Returns at once in a build that does not check.
    pub async fn run_schedule(self: Arc<Self>) {
        if !self.checks {
            return;
        }
        while !self.lock().startup_ended {
            self.startup.notified().await;
        }
        loop {
            if self.settings.update_check() {
                self.check_now().await;
            }
            self.clock.sleep(CHECK_INTERVAL).await;
        }
    }

    /// Checks at once, also while automatic checks are off. Does nothing in
    /// a build that does not check, while a check runs, or once an install
    /// has begun.
    pub async fn check_now(&self) {
        if !self.checks {
            return;
        }
        let begun = self.change(|state| match state.status {
            UpdateStatus::Checking
            | UpdateStatus::Downloading { .. }
            | UpdateStatus::ReadyToRestart { .. } => false,
            _ => {
                state.status = UpdateStatus::Checking;
                true
            }
        });
        if !begun {
            return;
        }
        let result = self.source.check().await;
        let checked_at = self.clock.now();
        if let Err(reason) = &result {
            self.events
                .log(&format!("the update check failed: {reason}"));
        }
        self.change(|state| {
            state.status = match result {
                Ok(Some(found)) => UpdateStatus::Available {
                    version: found.version,
                    date: found.date,
                    notes: found.notes,
                    checked_at,
                },
                Ok(None) => UpdateStatus::UpToDate { checked_at },
                Err(reason) => UpdateStatus::Failed { reason, checked_at },
            };
            true
        });
    }

    /// Puts off the available version's notice until the next start or a
    /// newer version.
    pub fn later(&self) {
        self.change(|state| match &state.status {
            UpdateStatus::Available { version, .. } => {
                state.later = Some(version.clone());
                true
            }
            _ => false,
        });
    }

    /// Installs the available update as job "Update to <version>" of
    /// `workspace`, refused while another job runs. Its download progress
    /// goes to the job and to the status. Returns at once; needs a tokio
    /// runtime.
    pub fn install(
        self: &Arc<Self>,
        jobs: &Arc<Jobs>,
        workspace: &WorkspaceId,
    ) -> Result<UpdateInstall, UpdateError> {
        if !self.package.installs() {
            return Err(UpdateError::OnlyNotifies);
        }
        if jobs
            .all_views()
            .iter()
            .any(|job| job.state != JobState::Ended)
        {
            return Err(UpdateError::JobsRunning);
        }
        // Taken off offer at once, so a second install finds nothing.
        let (version, previous) = {
            let mut state = self.lock();
            let UpdateStatus::Available { version, .. } = &state.status else {
                return Err(UpdateError::NoUpdate);
            };
            let version = version.clone();
            let previous = std::mem::replace(
                &mut state.status,
                UpdateStatus::Downloading {
                    version: version.clone(),
                    done: 0,
                    total: None,
                },
            );
            (version, previous)
        };
        let job = match jobs.track(workspace, &format!("Update to {version}")) {
            Ok(job) => job,
            Err(error) => {
                self.lock().status = previous;
                return Err(error.into());
            }
        };
        self.events.changed(&self.view());
        let (sent, installed) = oneshot::channel();
        let job_id = job.job_id();
        let updates = self.clone();
        let report = job.step_reporter("Downloading (KiB)");
        tokio::spawn(async move {
            let progress = updates.clone();
            let downloading = version.clone();
            let done = Mutex::new(0_u64);
            let on_chunk: UpdateProgress = Box::new(move |chunk, total| {
                let so_far = {
                    let mut done = lock(&done);
                    *done = done.saturating_add(chunk);
                    *done
                };
                report(kib(so_far), total.map_or(0, kib));
                progress.downloaded(&downloading, so_far, total);
            });
            match updates.source.install(on_chunk).await {
                Ok(()) => {
                    updates.change(|state| {
                        state.status = UpdateStatus::ReadyToRestart {
                            version: version.clone(),
                        };
                        true
                    });
                    job.end(Outcome::Succeeded, String::new(), None);
                    let _ = sent.send(true);
                }
                Err(reason) => {
                    updates
                        .events
                        .log(&format!("the update to {version} failed: {reason}"));
                    updates.change(|state| {
                        state.status = previous;
                        true
                    });
                    job.end(
                        Outcome::Failed,
                        String::new(),
                        Some(format!("the update was not installed: {reason}")),
                    );
                    let _ = sent.send(false);
                }
            }
        });
        Ok(UpdateInstall { job_id, installed })
    }

    /// Records `so_far` downloaded bytes, sending the view when the percent
    /// changed, or each MiB while the length is not known.
    fn downloaded(&self, version: &str, so_far: u64, total: Option<u64>) {
        self.change(|state| {
            let before = match &state.status {
                UpdateStatus::Downloading { done, .. } => *done,
                _ => return false,
            };
            state.status = UpdateStatus::Downloading {
                version: version.to_owned(),
                done: so_far,
                total,
            };
            let step = |bytes: u64| match total {
                Some(total) if total > 0 => bytes.saturating_mul(100) / total,
                _ => bytes / (1024 * 1024),
            };
            step(before) != step(so_far)
        });
    }

    /// Changes the state and sends the view when `change` says it changed.
    fn change(&self, change: impl FnOnce(&mut State) -> bool) -> bool {
        let view = {
            let mut state = self.lock();
            if !change(&mut state) {
                return false;
            }
            self.view_of(&state)
        };
        self.events.changed(&view);
        true
    }

    fn lock(&self) -> MutexGuard<'_, State> {
        // Every change sets whole fields, so a panic leaves none half-set.
        lock(&self.state)
    }
}

/// Bytes as whole KiB, for the job's progress.
fn kib(bytes: u64) -> u32 {
    u32::try_from(bytes / 1024).unwrap_or(u32::MAX)
}
