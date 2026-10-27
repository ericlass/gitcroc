//! The app's API (section 4, Crates): an [`App`] with one method per
//! operation, grouped by area, and one [`AppEvent`] channel. It owns the
//! stores the Tauri shell kept in its state, and does what its commands did,
//! with no UI toolkit. Methods are `async` or return at once; disk and git
//! work runs off the caller's thread, and anything slow is a job (ARCH-7).

mod changes;
mod conflict;
mod error;
mod event;
mod history;
mod ops;
mod settings;
mod tree;
mod workspace;

use std::sync::Arc;

use gitcroc_core::{
    AppInfo, AppLog, CacheDir, Changes, ConfigDir, GitRunner, GitSetup, GitStatus, Histories,
    JobEvents, JobView, Jobs, LogDir, Palettes, RepoChange, RepoId, SettingsStore, Summaries,
    SummaryCache, SummaryView, TabSwitch, Trees, WatchWarning, Watchers, WorkspaceId, Workspaces,
};
use tokio::runtime::Handle;
use tokio::sync::broadcast;

pub use error::{AppError, AppErrorCode};
use event::Hub;
pub use event::{AppEvent, EVENT_CAPACITY, RepoRef};

use crate::platform::{DesktopFonts, Dialogs, Installation, PlatformInstallation};

/// What [`App::start`] needs from the binary that runs it.
pub struct AppSetup<'a> {
    pub config: &'a dyn ConfigDir,
    pub cache: &'a dyn CacheDir,
    pub logs: &'a dyn LogDir,
    /// The runtime the app's work runs on: a multi-thread runtime, so no
    /// core work runs on the UI's thread (ARCH-7).
    pub runtime: Handle,
    /// The desktop's fonts, read by the UI before the start (UI-26).
    pub fonts: DesktopFonts,
    /// The file dialogs the calls that ask for a path open (SEC-2).
    pub dialogs: Arc<dyn Dialogs>,
    /// [`EVENT_CAPACITY`], or less in tests of lag.
    pub event_capacity: usize,
}

/// The running app's state and operations. Cheap to clone; every clone is
/// the same app.
#[derive(Clone)]
pub struct App {
    inner: Arc<Inner>,
}

struct Inner {
    hub: Hub,
    runtime: Handle,
    git: Arc<GitSetup>,
    log: Arc<AppLog>,
    settings: Arc<SettingsStore>,
    palettes: Arc<Palettes>,
    histories: Arc<Histories>,
    changes: Arc<Changes>,
    watchers: Arc<Watchers>,
    jobs: Arc<Jobs>,
    fonts: DesktopFonts,
    dialogs: Arc<dyn Dialogs>,
}

impl App {
    /// Loads the settings and workspaces, starts watching the active one and
    /// detects git in the background, in the order the Tauri shell did.
    /// Detection reports as `GitStatus` events, then the active workspace's
    /// summaries are read again.
    pub fn start(setup: AppSetup<'_>) -> Self {
        // Every git process the app runs goes to `gitcroc.log` (section 10).
        let log = Arc::new(AppLog::open(setup.logs));
        let command_log = log.clone();
        gitcroc_core::log_commands_to(move |record| command_log.command(record));
        // Small files, read once before the first view, so it already has
        // the saved theme and the workspaces of the saved tabs.
        let settings = Arc::new(SettingsStore::load(setup.config));
        let workspaces = Arc::new(Workspaces::load_with(setup.config, settings.clone()));
        // The cached summaries, so the first view shows every repo before
        // git is read (PERF-3); the refresh starts after git detection.
        let summaries = Arc::new(Summaries::load(
            SummaryCache::open(setup.cache),
            &workspaces,
        ));
        // The pool jobs and the refresh share has as many places as group
        // operations run sub-jobs at once (PERF-4).
        let parallelism = settings.view().settings.groups.group_parallelism;
        summaries
            .pool()
            .resize(usize::try_from(parallelism).unwrap_or(gitcroc_core::POOL_SIZE));
        let (events, _) = broadcast::channel(setup.event_capacity.max(1));
        let hub = Hub {
            events,
            workspaces,
            summaries,
            trees: Arc::new(Trees::new()),
        };
        let git = Arc::new(GitSetup::new());
        let watchers = Arc::new(watchers(&hub, &git, &log, &setup.runtime));
        let jobs = Jobs::new(
            hub.workspaces.clone(),
            hub.summaries.clone(),
            Arc::new(AppJobEvents {
                hub: hub.clone(),
                watchers: watchers.clone(),
            }),
        );
        log.started(&AppInfo::current().version, PlatformInstallation::package());
        let app = Self {
            inner: Arc::new(Inner {
                hub,
                runtime: setup.runtime,
                git,
                log,
                settings,
                palettes: Arc::new(Palettes::new()),
                histories: Arc::new(Histories::default()),
                changes: Arc::new(Changes::new()),
                watchers,
                jobs,
                fonts: setup.fonts,
                dialogs: setup.dialogs,
            }),
        };
        if let Some(active) = app.inner.hub.workspaces.active() {
            app.spawn_watch_sync(active);
        }
        app.spawn_git_detection();
        app
    }

    /// A receiver of every event sent from now on, in the order they were
    /// sent. One that falls behind by [`AppSetup::event_capacity`] gets
    /// `Lagged`, and has to read the state it shows again (ARCH-6).
    pub fn subscribe(&self) -> broadcast::Receiver<AppEvent> {
        self.inner.hub.events.subscribe()
    }

    fn workspaces(&self) -> &Arc<Workspaces> {
        &self.inner.hub.workspaces
    }

    fn summaries(&self) -> &Arc<Summaries> {
        &self.inner.hub.summaries
    }

    fn trees(&self) -> &Arc<Trees> {
        &self.inner.hub.trees
    }

    fn send(&self, event: AppEvent) {
        self.inner.hub.send(event);
    }

    /// Runs `work` on the runtime's blocking threads: it touches the disk
    /// (ARCH-7).
    async fn blocking<T: Send + 'static>(
        &self,
        work: impl FnOnce() -> T + Send + 'static,
    ) -> Result<T, AppError> {
        Ok(self.inner.runtime.spawn_blocking(work).await?)
    }

    /// A runner for the detected git, or why there is none.
    async fn usable_git(&self) -> Result<GitRunner, AppError> {
        match self.inner.git.git().await {
            Some(git) => Ok(GitRunner::new(git.path)),
            None => Err(AppError {
                code: AppErrorCode::GitUnavailable,
                message: "no usable git was found".to_owned(),
                git_stderr: None,
            }),
        }
    }

    /// The per-workspace state a tab switch acts on.
    fn tab_switch(&self) -> TabSwitch {
        TabSwitch {
            workspaces: self.workspaces().clone(),
            summaries: self.summaries().clone(),
            watchers: self.inner.watchers.clone(),
            histories: self.inner.histories.clone(),
            changes: self.inner.changes.clone(),
        }
    }

    /// Brings the workspace's watches in line with its repos, in the
    /// background, when it is the active one. The other tabs are not
    /// watched (UI-30).
    fn spawn_watch_sync(&self, workspace_id: WorkspaceId) {
        let watchers = self.inner.watchers.clone();
        let workspaces = self.workspaces().clone();
        self.inner.runtime.spawn_blocking(move || {
            if workspaces.active().as_ref() == Some(&workspace_id) {
                watchers.sync(&workspaces, &workspace_id);
            }
        });
    }

    /// Detects git in the background (ARCH-7), sending `GitStatus` when it
    /// starts and when it is done. With a usable git, the active
    /// workspace's summaries are then read again (PERF-3); until then the
    /// cached ones stand. The other tabs are read when they become active
    /// (UI-30).
    fn spawn_git_detection(&self) {
        let app = self.clone();
        self.inner.runtime.spawn(async move {
            app.send(AppEvent::GitStatus(GitStatus::Checking));
            let chosen = app.inner.settings.git_path();
            let status = app.inner.git.detect_chosen(chosen.as_deref()).await;
            app.send(AppEvent::GitStatus(status));
            let Ok(git) = app.usable_git().await else {
                return;
            };
            let workspaces = app.workspaces();
            let active = workspaces
                .active()
                .and_then(|active| workspaces.view(&active).ok());
            if let Some(workspace) = active {
                let repos: Vec<RepoId> = workspace.repos.into_iter().map(|repo| repo.id).collect();
                app.inner.hub.refresh(workspace.id, repos, git).await;
            }
        });
    }
}

/// The job engine's events, sent as `AppEvent`s (ARCH-6, ARCH-7).
struct AppJobEvents {
    hub: Hub,
    watchers: Arc<Watchers>,
}

impl JobEvents for AppJobEvents {
    fn progress(&self, job: &JobView) {
        self.hub.send(AppEvent::JobProgress(job.clone()));
    }

    fn ended(&self, job: &JobView) {
        self.hub.send(AppEvent::JobEnded(job.clone()));
    }

    fn summary(&self, view: SummaryView, read: bool) {
        self.hub.summary(view, read);
    }

    fn hold_watch(&self, workspace: &WorkspaceId, repo: &RepoId, held: bool) {
        self.watchers.hold(workspace, repo, held);
    }
}

/// The file watchers (PERF-6). A repo that changed on disk is read again
/// (OPS-7); a ref or `HEAD` that changed also raises its `refs_version`, so
/// the history reads it again. A failure is logged and sent once as
/// `WatchWarning`.
fn watchers(hub: &Hub, git: &Arc<GitSetup>, log: &Arc<AppLog>, runtime: &Handle) -> Watchers {
    let (changed, warned) = (hub.clone(), hub.clone());
    let (git, log, runtime) = (git.clone(), log.clone(), runtime.clone());
    Watchers::new(
        move |change: RepoChange| {
            let (hub, git) = (changed.clone(), git.clone());
            runtime.spawn(async move {
                // Without a usable git there is nothing to read with yet;
                // the refresh after detection reads every repo.
                let Some(git) = git.git().await else {
                    return;
                };
                if change.refs {
                    hub.summaries
                        .refs_changed(&change.workspace_id, &change.repo_id);
                }
                let git = GitRunner::new(git.path);
                hub.refresh(change.workspace_id, vec![change.repo_id], git)
                    .await;
            });
        },
        move |warning: WatchWarning| {
            log.warn(&format!("file watching: {}", warning.message));
            warned.send(AppEvent::WatchWarning(warning));
        },
    )
}
