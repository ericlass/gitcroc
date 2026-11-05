//! The `update-test` build's own update at start (M6-28c; PLAT-4, SEC-7,
//! D12), for the update test on the CI runners: the build checks at once,
//! without waiting for the startup or for the setting, installs what it
//! finds without asking, and logs what happened in 1.0.0-rc.1's lines, so
//! the test reads both apps' logs the same way:
//!
//! - `update: installed <version>`, then the app restarts into it;
//! - `update: refused (<reason>)`, such as a signature that does not match;
//! - `update: notify only (<version>)`, for the packages that do not install
//!   updates.
//!
//! The check, the verification and the install are [`OwnSource`]'s, as in
//! every build; only this start and the lines are the feature's. Release
//! builds never have it.
//!
//! The Windows installer replaces the app and starts the new version once
//! the old one has exited, so the old one cannot log the result. As rc.1
//! did, the version is written to [`PENDING`] in the config dir before the
//! install, and the version it names logs `update: installed` when it
//! starts. rc.1's update-test build writes the same file, so a GPUI build
//! that rc.1 installed logs it too.
//!
//! [`OwnSource`]: super::OwnSource

use std::path::Path;

use gitcroc_core::{AppLog, Package, UpdateSource};

/// The file in the config dir that names the version being installed.
pub const PENDING: &str = "update-test.pending";

/// What the update at start did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    /// No newer version was on offer.
    None,
    /// The version is installed; the app restarts into it.
    Installed(String),
    /// The version is on offer, but this package does not install updates.
    NotifyOnly(String),
    /// The check or the install failed, for this reason.
    Refused(String),
}

impl Outcome {
    /// The log line, 1.0.0-rc.1's; none when nothing was on offer.
    pub fn line(&self) -> Option<String> {
        match self {
            Self::None => None,
            Self::Installed(version) => Some(format!("update: installed {version}")),
            Self::NotifyOnly(version) => Some(format!("update: notify only ({version})")),
            Self::Refused(reason) => Some(format!("update: refused ({reason})")),
        }
    }
}

/// The update at start of the running app, version `version` from
/// `package`: first the install an earlier process left pending, then a
/// check of `source`, and the install of what it finds. Each outcome is
/// logged to `log`. `config` is the config dir, `None` when there is none.
pub async fn run(
    source: &dyn UpdateSource,
    package: Package,
    version: &str,
    config: Option<&Path>,
    log: &AppLog,
) -> Outcome {
    let pending = config.map(|dir| dir.join(PENDING));
    if let Some(resumed) = pending.as_deref().and_then(|file| resumed(file, version)) {
        log_outcome(log, &resumed);
    }
    let outcome = attempt(source, package, pending.as_deref()).await;
    log_outcome(log, &outcome);
    outcome
}

fn log_outcome(log: &AppLog, outcome: &Outcome) {
    if let Some(line) = outcome.line() {
        log.warn(&line);
    }
}

/// [`Outcome::Installed`] when `pending` names `version`, the running one:
/// an earlier process installed it and could not say so. The file is
/// removed.
fn resumed(pending: &Path, version: &str) -> Option<Outcome> {
    let named = std::fs::read_to_string(pending).ok()?;
    let _ = std::fs::remove_file(pending);
    (named.trim() == version).then(|| Outcome::Installed(version.to_owned()))
}

/// Checks `source`, and installs what it finds when `package` installs
/// updates, with no one asked. `pending` names the version while it is
/// installed.
async fn attempt(source: &dyn UpdateSource, package: Package, pending: Option<&Path>) -> Outcome {
    let found = match source.check().await {
        Ok(Some(found)) => found,
        Ok(None) => return Outcome::None,
        Err(reason) => return Outcome::Refused(reason),
    };
    if !package.installs() {
        return Outcome::NotifyOnly(found.version);
    }
    if let Some(file) = pending
        && let Err(error) = std::fs::write(file, &found.version)
    {
        return Outcome::Refused(format!("{} could not be written: {error}", file.display()));
    }
    let installed = source.install(Box::new(|_, _| {})).await;
    if let Some(file) = pending {
        let _ = std::fs::remove_file(file);
    }
    match installed {
        Ok(()) => Outcome::Installed(found.version),
        Err(reason) => Outcome::Refused(reason),
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicUsize, Ordering};

    use gitcroc_core::{LogDir, UpdateFound, UpdateFuture, UpdateProgress};

    use super::*;

    /// A source offering `offer`, whose install fails with `fails`.
    struct Fake {
        offer: Result<Option<&'static str>, &'static str>,
        fails: Option<&'static str>,
        installs: AtomicUsize,
        /// The pending file while the install ran.
        pending: Option<PathBuf>,
        pending_seen: std::sync::Mutex<Option<String>>,
    }

    impl Fake {
        fn new(offer: Result<Option<&'static str>, &'static str>) -> Self {
            Self {
                offer,
                fails: None,
                installs: AtomicUsize::new(0),
                pending: None,
                pending_seen: std::sync::Mutex::new(None),
            }
        }
    }

    impl UpdateSource for Fake {
        fn check(&self) -> UpdateFuture<'_, Option<UpdateFound>> {
            let offer = self.offer.map(|version| {
                version.map(|version| UpdateFound {
                    version: version.to_owned(),
                    date: None,
                    notes: String::new(),
                })
            });
            Box::pin(async move { offer.map_err(str::to_owned) })
        }

        fn install(&self, _progress: UpdateProgress) -> UpdateFuture<'_, ()> {
            self.installs.fetch_add(1, Ordering::SeqCst);
            if let Some(file) = &self.pending {
                *self.pending_seen.lock().unwrap() = std::fs::read_to_string(file).ok();
            }
            let fails = self.fails;
            Box::pin(async move { fails.map_or(Ok(()), |reason| Err(reason.to_owned())) })
        }
    }

    struct Dirs(PathBuf);

    impl LogDir for Dirs {
        fn log_dir(&self) -> Option<PathBuf> {
            Some(self.0.join("logs"))
        }
    }

    fn log_lines(dir: &Path) -> Vec<String> {
        std::fs::read_to_string(dir.join("logs").join(gitcroc_core::LOG_FILE))
            .unwrap_or_default()
            .lines()
            .filter_map(|line| {
                line.split_once(" warning: ")
                    .map(|(_, text)| text.to_owned())
            })
            .collect()
    }

    fn block_on<F: Future>(future: F) -> F::Output {
        tokio::runtime::Builder::new_current_thread()
            .build()
            .unwrap()
            .block_on(future)
    }

    #[test]
    fn plat_4_update_test_build_installs_at_start() {
        for package in [
            Package::WindowsInstaller,
            Package::MacApp,
            Package::AppImage,
        ] {
            let dir = tempfile::tempdir().unwrap();
            let log = AppLog::open(&Dirs(dir.path().to_owned()));
            let mut source = Fake::new(Ok(Some("1.0.1")));
            source.pending = Some(dir.path().join(PENDING));
            let outcome = block_on(run(&source, package, "1.0.0", Some(dir.path()), &log));
            assert_eq!(outcome, Outcome::Installed("1.0.1".into()), "{package:?}");
            // Installed once, with no one asked, and the version pending
            // while it was.
            assert_eq!(source.installs.load(Ordering::SeqCst), 1);
            assert_eq!(
                source.pending_seen.lock().unwrap().as_deref(),
                Some("1.0.1")
            );
            assert!(!dir.path().join(PENDING).exists());
            assert_eq!(log_lines(dir.path()), ["update: installed 1.0.1"]);
        }
    }

    /// The Windows installer's new version says what the old one installed.
    #[test]
    fn plat_4_update_test_build_logs_the_install_it_was_started_by() {
        let dir = tempfile::tempdir().unwrap();
        let log = AppLog::open(&Dirs(dir.path().to_owned()));
        std::fs::write(dir.path().join(PENDING), "1.0.1").unwrap();
        let source = Fake::new(Ok(None));
        let outcome = block_on(run(
            &source,
            Package::WindowsInstaller,
            "1.0.1",
            Some(dir.path()),
            &log,
        ));
        assert_eq!(outcome, Outcome::None);
        assert_eq!(log_lines(dir.path()), ["update: installed 1.0.1"]);
        assert!(!dir.path().join(PENDING).exists());

        // A version that did not start is not reported, and the file goes.
        std::fs::write(dir.path().join(PENDING), "1.0.2").unwrap();
        block_on(run(
            &source,
            Package::WindowsInstaller,
            "1.0.1",
            Some(dir.path()),
            &log,
        ));
        assert_eq!(log_lines(dir.path()), ["update: installed 1.0.1"]);
        assert!(!dir.path().join(PENDING).exists());
    }

    #[test]
    fn plat_4_update_test_logs_refused_and_notify_only() {
        // A package whose signature does not match.
        let dir = tempfile::tempdir().unwrap();
        let log = AppLog::open(&Dirs(dir.path().to_owned()));
        let mut bad = Fake::new(Ok(Some("1.0.2")));
        bad.fails = Some("the update's signature does not match: bad");
        let outcome = block_on(run(
            &bad,
            Package::AppImage,
            "1.0.1",
            Some(dir.path()),
            &log,
        ));
        assert!(matches!(outcome, Outcome::Refused(_)), "{outcome:?}");
        assert!(!dir.path().join(PENDING).exists());

        // A check that fails.
        let failed = Fake::new(Err("connection refused"));
        block_on(run(&failed, Package::MacApp, "1.0.1", None, &log));

        // The packages that only notify never install.
        for package in [Package::Deb, Package::Rpm, Package::WindowsPortable] {
            let source = Fake::new(Ok(Some("1.0.1")));
            let outcome = block_on(run(&source, package, "1.0.0", Some(dir.path()), &log));
            assert_eq!(outcome, Outcome::NotifyOnly("1.0.1".into()));
            assert_eq!(source.installs.load(Ordering::SeqCst), 0);
        }
        assert!(!dir.path().join(PENDING).exists());

        // Nothing on offer: no line.
        block_on(run(
            &Fake::new(Ok(None)),
            Package::AppImage,
            "1.0.1",
            None,
            &log,
        ));
        assert_eq!(
            log_lines(dir.path()),
            [
                "update: refused (the update's signature does not match: bad)",
                "update: refused (connection refused)",
                "update: notify only (1.0.1)",
                "update: notify only (1.0.1)",
                "update: notify only (1.0.1)",
            ]
        );
    }
}
