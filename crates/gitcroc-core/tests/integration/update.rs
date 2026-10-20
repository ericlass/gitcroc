//! The update check and install (M5-12): when the app checks (D12), what
//! each package does with an update (PLAT-4), and that a package that fails
//! verification installs nothing (SEC-7). The source is a fake and the clock
//! a simulated one that moves only when the test says so: nothing here
//! reaches the network or waits for a time.

use std::path::PathBuf;
use std::pin::Pin;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use gitcroc_core::{
    CHECK_INTERVAL, ConfigDir, JobEvents, JobState, JobView, Jobs, Outcome, Package, SettingsStore,
    Summaries, SummaryCache, SummaryView, UpdateClock, UpdateError, UpdateEvents, UpdateFound,
    UpdateFuture, UpdateProgress, UpdateSetup, UpdateSource, UpdateStatus, UpdateView, Updates,
    WorkspaceId, Workspaces,
};
use tokio::sync::mpsc;

struct Dirs {
    root: tempfile::TempDir,
}

impl ConfigDir for Dirs {
    fn config_dir(&self) -> Option<PathBuf> {
        Some(self.root.path().join("config"))
    }
}

/// What the fake source answers, and how often it was asked.
struct FakeSource {
    found: Mutex<Result<Option<UpdateFound>, String>>,
    installed: Mutex<Result<(), String>>,
    checks: Mutex<u32>,
    installs: Mutex<u32>,
}

impl FakeSource {
    fn finding(version: &str) -> Arc<Self> {
        Arc::new(Self {
            found: Mutex::new(Ok(Some(found(version)))),
            installed: Mutex::new(Ok(())),
            checks: Mutex::new(0),
            installs: Mutex::new(0),
        })
    }

    fn set_found(&self, found: Result<Option<UpdateFound>, String>) {
        *self.found.lock().unwrap() = found;
    }

    fn checks(&self) -> u32 {
        *self.checks.lock().unwrap()
    }

    fn installs(&self) -> u32 {
        *self.installs.lock().unwrap()
    }
}

impl UpdateSource for FakeSource {
    fn check(&self) -> UpdateFuture<'_, Option<UpdateFound>> {
        *self.checks.lock().unwrap() += 1;
        let found = self.found.lock().unwrap().clone();
        Box::pin(async move { found })
    }

    fn install(&self, progress: UpdateProgress) -> UpdateFuture<'_, ()> {
        *self.installs.lock().unwrap() += 1;
        let installed = self.installed.lock().unwrap().clone();
        Box::pin(async move {
            // Two chunks of a download of 2 MiB.
            progress(1024 * 1024, Some(2 * 1024 * 1024));
            progress(1024 * 1024, Some(2 * 1024 * 1024));
            installed
        })
    }
}

fn found(version: &str) -> UpdateFound {
    UpdateFound {
        version: version.to_owned(),
        date: Some("2026-11-02T10:00:00Z".to_owned()),
        notes: "Fixes.\n<b>not markup</b>".to_owned(),
    }
}

/// A clock that tells the test each sleep it is asked for, and wakes the
/// sleeper only when the test advances it.
struct FakeClock {
    now: Mutex<u64>,
    asked: mpsc::UnboundedSender<Duration>,
    wake: tokio::sync::Mutex<mpsc::UnboundedReceiver<()>>,
}

struct ClockHandle {
    asked: mpsc::UnboundedReceiver<Duration>,
    wake: mpsc::UnboundedSender<()>,
}

impl ClockHandle {
    /// Waits until the schedule sleeps, and returns how long it asked for.
    async fn sleeping(&mut self) -> Duration {
        self.asked.recv().await.unwrap()
    }

    /// Ends the sleep the schedule is in.
    fn advance(&self) {
        self.wake.send(()).unwrap();
    }
}

fn clock() -> (Arc<FakeClock>, ClockHandle) {
    let (asked_tx, asked) = mpsc::unbounded_channel();
    let (wake, wake_rx) = mpsc::unbounded_channel();
    let clock = FakeClock {
        now: Mutex::new(1_700_000_000_000),
        asked: asked_tx,
        wake: tokio::sync::Mutex::new(wake_rx),
    };
    (Arc::new(clock), ClockHandle { asked, wake })
}

impl UpdateClock for FakeClock {
    fn now(&self) -> u64 {
        *self.now.lock().unwrap()
    }

    fn sleep(&self, duration: Duration) -> Pin<Box<dyn Future<Output = ()> + Send + '_>> {
        Box::pin(async move {
            self.asked.send(duration).unwrap();
            self.wake.lock().await.recv().await;
            *self.now.lock().unwrap() += u64::try_from(duration.as_millis()).unwrap();
        })
    }
}

/// Every view sent, and every line logged.
#[derive(Default)]
struct Heard {
    views: Mutex<Vec<UpdateView>>,
    logged: Mutex<Vec<String>>,
}

impl UpdateEvents for Heard {
    fn changed(&self, view: &UpdateView) {
        self.views.lock().unwrap().push(view.clone());
    }

    fn log(&self, message: &str) {
        self.logged.lock().unwrap().push(message.to_owned());
    }
}

/// Jobs as the engine reports them.
#[derive(Default)]
struct JobLog(Mutex<Vec<JobView>>);

impl JobEvents for JobLog {
    fn progress(&self, job: &JobView) {
        self.0.lock().unwrap().push(job.clone());
    }

    fn ended(&self, job: &JobView) {
        self.0.lock().unwrap().push(job.clone());
    }

    fn summary(&self, _view: SummaryView, _read: bool) {}
}

struct Setup {
    _dirs: Dirs,
    settings: Arc<SettingsStore>,
    source: Arc<FakeSource>,
    clock: ClockHandle,
    heard: Arc<Heard>,
    updates: Arc<Updates>,
    jobs: Arc<Jobs>,
    job_log: Arc<JobLog>,
    workspace: WorkspaceId,
}

fn setup(package: Package, checks: bool, source: Arc<FakeSource>) -> Setup {
    let dirs = Dirs {
        root: tempfile::tempdir().unwrap(),
    };
    let settings = Arc::new(SettingsStore::load(&dirs));
    let workspaces = Arc::new(Workspaces::load_with(&dirs, settings.clone()));
    let workspace = workspaces.views()[0].id.clone();
    let summaries = Arc::new(Summaries::load(SummaryCache::none(), &workspaces));
    let job_log = Arc::new(JobLog::default());
    let jobs = Jobs::new(workspaces, summaries, job_log.clone());
    let (fake_clock, clock) = clock();
    let heard = Arc::new(Heard::default());
    let updates = Updates::new(UpdateSetup {
        source: source.clone(),
        clock: fake_clock,
        settings: settings.clone(),
        events: heard.clone(),
        package,
        checks,
        current_version: "1.0.0".to_owned(),
    });
    Setup {
        _dirs: dirs,
        settings,
        source,
        clock,
        heard,
        updates,
        jobs,
        job_log,
        workspace,
    }
}

/// Lets every task that can run, run.
async fn settle() {
    for _ in 0..50 {
        tokio::task::yield_now().await;
    }
}

#[tokio::test]
async fn d12_first_check_after_the_startup_refresh_then_every_24_hours() {
    let mut s = setup(Package::AppImage, true, FakeSource::finding("1.1.0"));
    let schedule = tokio::spawn(s.updates.clone().run_schedule());

    // Nothing happens before the startup refresh has ended (section 9).
    settle().await;
    assert_eq!(s.source.checks(), 0);
    assert_eq!(s.updates.view().status, UpdateStatus::Idle);

    s.updates.startup_ended();
    assert_eq!(s.clock.sleeping().await, CHECK_INTERVAL);
    assert_eq!(CHECK_INTERVAL, Duration::from_secs(24 * 60 * 60));
    assert_eq!(s.source.checks(), 1);
    // Said again, the end of the startup starts no second check.
    s.updates.startup_ended();
    settle().await;
    assert_eq!(s.source.checks(), 1);

    s.clock.advance();
    assert_eq!(s.clock.sleeping().await, CHECK_INTERVAL);
    assert_eq!(s.source.checks(), 2);
    s.clock.advance();
    assert_eq!(s.clock.sleeping().await, CHECK_INTERVAL);
    assert_eq!(s.source.checks(), 3);
    schedule.abort();
}

#[tokio::test]
async fn d12_no_check_while_the_setting_is_off() {
    let mut s = setup(Package::AppImage, true, FakeSource::finding("1.1.0"));
    s.settings.set_update_check(false).unwrap();
    let schedule = tokio::spawn(s.updates.clone().run_schedule());
    s.updates.startup_ended();
    assert_eq!(s.clock.sleeping().await, CHECK_INTERVAL);
    s.clock.advance();
    assert_eq!(s.clock.sleeping().await, CHECK_INTERVAL);
    assert_eq!(s.source.checks(), 0);
    assert_eq!(s.updates.view().status, UpdateStatus::Idle);

    // Check now runs also while automatic checks are off.
    s.updates.check_now().await;
    assert_eq!(s.source.checks(), 1);
    assert!(matches!(
        s.updates.view().status,
        UpdateStatus::Available { ref version, .. } if version == "1.1.0"
    ));

    // Turned on again, the next scheduled check runs.
    s.settings.set_update_check(true).unwrap();
    s.clock.advance();
    assert_eq!(s.clock.sleeping().await, CHECK_INTERVAL);
    assert_eq!(s.source.checks(), 2);
    schedule.abort();
}

#[tokio::test]
async fn d12_debug_and_test_builds_never_check() {
    let s = setup(Package::AppImage, false, FakeSource::finding("1.1.0"));
    // The schedule of a build that does not check ends at once.
    let schedule = tokio::spawn(s.updates.clone().run_schedule());
    s.updates.startup_ended();
    schedule.await.unwrap();
    s.updates.check_now().await;
    assert_eq!(s.source.checks(), 0);
    let view = s.updates.view();
    assert!(!view.checks);
    assert_eq!(view.status, UpdateStatus::Idle);
}

#[tokio::test]
async fn d12_a_failed_check_is_logged_and_the_schedule_goes_on() {
    let mut s = setup(Package::AppImage, true, FakeSource::finding("1.1.0"));
    s.source.set_found(Err("error sending request".to_owned()));
    let schedule = tokio::spawn(s.updates.clone().run_schedule());
    s.updates.startup_ended();
    assert_eq!(s.clock.sleeping().await, CHECK_INTERVAL);
    // Shown with its time, and logged; no error is raised.
    assert_eq!(
        s.updates.view().status,
        UpdateStatus::Failed {
            reason: "error sending request".to_owned(),
            checked_at: 1_700_000_000_000,
        }
    );
    let logged = s.heard.logged.lock().unwrap().clone();
    assert_eq!(logged.len(), 1);
    assert!(logged[0].contains("error sending request"), "{logged:?}");

    s.source.set_found(Ok(None));
    s.clock.advance();
    assert_eq!(s.clock.sleeping().await, CHECK_INTERVAL);
    assert_eq!(
        s.updates.view().status,
        UpdateStatus::UpToDate {
            checked_at: 1_700_000_000_000 + 24 * 60 * 60 * 1000,
        }
    );
    schedule.abort();
}

#[tokio::test]
async fn sec_7_update_check_can_be_turned_off() {
    let s = setup(Package::MacApp, true, FakeSource::finding("1.1.0"));
    // On by default, and not written until it changes (DATA-1).
    assert!(s.settings.update_check());
    assert!(s.settings.view().update_check);
    let path = s.settings.path().unwrap().to_owned();
    s.settings.set_update_check(false).unwrap();
    assert!(!s.settings.view().update_check);
    let text = std::fs::read_to_string(&path).unwrap();
    assert!(text.contains("update_check = false"), "{text}");
    assert!(text.starts_with("schema = 2\n"), "{text}");
    let dirs = Dirs {
        root: tempfile::tempdir().unwrap(),
    };
    std::fs::create_dir_all(dirs.config_dir().unwrap()).unwrap();
    std::fs::copy(&path, dirs.config_dir().unwrap().join("settings.toml")).unwrap();
    assert!(!SettingsStore::load(&dirs).update_check());
}

#[tokio::test]
async fn plat_4_installer_app_and_appimage_install() {
    for package in [
        Package::WindowsInstaller,
        Package::MacApp,
        Package::AppImage,
    ] {
        assert!(package.installs(), "{package:?}");
        let s = setup(package, true, FakeSource::finding("1.1.0"));
        s.updates.check_now().await;
        let view = s.updates.view();
        assert!(view.installs, "{package:?}");
        assert_eq!(view.current_version, "1.0.0");
        assert_eq!(
            view.status,
            UpdateStatus::Available {
                version: "1.1.0".to_owned(),
                date: Some("2026-11-02T10:00:00Z".to_owned()),
                notes: "Fixes.\n<b>not markup</b>".to_owned(),
                checked_at: 1_700_000_000_000,
            }
        );

        let install = s.updates.install(&s.jobs, &s.workspace).unwrap();
        assert!(install.installed.await.unwrap(), "{package:?}");
        assert_eq!(s.source.installs(), 1);
        assert_eq!(
            s.updates.view().status,
            UpdateStatus::ReadyToRestart {
                version: "1.1.0".to_owned()
            }
        );
        // The download's progress went to the job and to the status.
        let jobs = s.job_log.0.lock().unwrap().clone();
        assert!(jobs.iter().all(|job| job.label == "Update to 1.1.0"));
        assert!(
            jobs.iter()
                .any(|job| job.state == JobState::Running && job.done == 1024 && job.total == 2048),
            "{jobs:?}"
        );
        let last = jobs.last().unwrap();
        assert_eq!(last.job_id, install.job_id);
        assert_eq!(last.outcome, Some(Outcome::Succeeded));
        let views = s.heard.views.lock().unwrap().clone();
        assert!(views.iter().any(|view| matches!(
            view.status,
            UpdateStatus::Downloading {
                done: 2_097_152,
                total: Some(2_097_152),
                ..
            }
        )));
    }
}

#[tokio::test]
async fn plat_4_deb_rpm_and_portable_zip_only_notify() {
    for package in [
        Package::Deb,
        Package::Rpm,
        Package::WindowsPortable,
        Package::Unknown,
    ] {
        assert!(!package.installs(), "{package:?}");
        let s = setup(package, true, FakeSource::finding("1.1.0"));
        s.updates.check_now().await;
        let view = s.updates.view();
        assert!(!view.installs, "{package:?}");
        assert!(matches!(view.status, UpdateStatus::Available { .. }));
        assert!(matches!(
            s.updates.install(&s.jobs, &s.workspace),
            Err(UpdateError::OnlyNotifies)
        ));
        assert_eq!(s.source.installs(), 0);
        assert!(s.jobs.all_views().is_empty());
    }
}

#[tokio::test]
async fn plat_4_a_bad_signature_installs_nothing() {
    let s = setup(Package::AppImage, true, FakeSource::finding("1.1.0"));
    // What the plugin reports when the package's signature does not match
    // the public key (SEC-7).
    *s.source.installed.lock().unwrap() =
        Err("the signature verification failed: invalid signature".to_owned());
    s.updates.check_now().await;
    let install = s.updates.install(&s.jobs, &s.workspace).unwrap();
    assert!(!install.installed.await.unwrap());
    let job = s.jobs.all_views().remove(0);
    assert_eq!(job.outcome, Some(Outcome::Failed));
    assert!(
        job.message
            .as_deref()
            .is_some_and(|message| message.contains("invalid signature")),
        "{job:?}"
    );
    // The update is still on offer; nothing is ready to restart into.
    assert!(matches!(
        s.updates.view().status,
        UpdateStatus::Available { ref version, .. } if version == "1.1.0"
    ));
    let logged = s.heard.logged.lock().unwrap().clone();
    assert!(logged.iter().any(|line| line.contains("invalid signature")));
}

#[tokio::test]
async fn plat_4_install_is_refused_while_another_job_runs() {
    let s = setup(
        Package::WindowsInstaller,
        true,
        FakeSource::finding("1.1.0"),
    );
    s.updates.check_now().await;
    let running = s.jobs.track(&s.workspace, "Fetch all remotes").unwrap();
    assert!(matches!(
        s.updates.install(&s.jobs, &s.workspace),
        Err(UpdateError::JobsRunning)
    ));
    assert_eq!(s.source.installs(), 0);
    running.end(Outcome::Succeeded, String::new(), None);
    let install = s.updates.install(&s.jobs, &s.workspace).unwrap();
    assert!(install.installed.await.unwrap());
}

#[tokio::test]
async fn plat_4_install_needs_an_update_on_offer() {
    let s = setup(Package::AppImage, true, FakeSource::finding("1.1.0"));
    assert!(matches!(
        s.updates.install(&s.jobs, &s.workspace),
        Err(UpdateError::NoUpdate)
    ));
    s.source.set_found(Ok(None));
    s.updates.check_now().await;
    assert!(matches!(
        s.updates.view().status,
        UpdateStatus::UpToDate { .. }
    ));
    assert!(matches!(
        s.updates.install(&s.jobs, &s.workspace),
        Err(UpdateError::NoUpdate)
    ));
}

#[tokio::test]
async fn plat_4_later_hides_the_notice_until_a_newer_version() {
    let s = setup(Package::Deb, true, FakeSource::finding("1.1.0"));
    s.updates.check_now().await;
    assert!(!s.updates.view().later);
    s.updates.later();
    assert!(s.updates.view().later);
    // The same version found again stays put off.
    s.updates.check_now().await;
    assert!(s.updates.view().later);
    // A newer one shows again.
    s.source.set_found(Ok(Some(found("1.2.0"))));
    s.updates.check_now().await;
    let view = s.updates.view();
    assert!(!view.later);
    assert!(matches!(
        view.status,
        UpdateStatus::Available { ref version, .. } if version == "1.2.0"
    ));
    // Each change was sent.
    let views = s.heard.views.lock().unwrap().clone();
    assert!(
        views
            .iter()
            .any(|view| view.status == UpdateStatus::Checking)
    );
    assert!(views.iter().any(|view| view.later));
}
