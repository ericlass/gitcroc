//! The app's API (section 4, Crates): an [`App`] with one method per
//! operation, grouped by area, and one [`AppEvent`] channel. It owns the
//! stores the Tauri shell kept in its state, and does what its commands did,
//! with no UI toolkit. Methods are `async` or return at once; disk and git
//! work runs off the caller's thread, and anything slow is a job (ARCH-7).

mod ai;
mod askpass;
mod changes;
mod conflict;
mod error;
mod event;
mod history;
mod ops;
mod settings;
mod tree;
mod update;
mod workspace;

use std::path::PathBuf;
use std::sync::Arc;

use gitcroc_core::{
    AiJobs, AppInfo, AppLog, CacheDir, Changes, ConfigDir, GitRunner, GitSetup, GitStatus,
    GroupOps, Histories, JobEvents, JobView, Jobs, LogDir, Palettes, RepoChange, RepoId,
    SettingsStore, Summaries, SummaryCache, SummaryView, TabSwitch, Trees, UpdateSetup,
    UpdateSource, Updates, WatchWarning, Watchers, WorkspaceId, Workspaces,
};
use tokio::runtime::Handle;
use tokio::sync::broadcast;

pub use error::{AppError, AppErrorCode};
use event::Hub;
pub use event::{AppEvent, EVENT_CAPACITY, RepoRef};

use crate::ai::{AiBackend, AiProviders};
use crate::askpass::EventAskpassUi;
use crate::platform::{DesktopFonts, Dialogs, Installation, Keychain, PlatformInstallation};
use crate::update::{CHECKS, SystemClock};

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
    /// Where AI keys live (SEC-1, D11): the OS keychain, or memory in the
    /// `e2e` build and the tests.
    pub keychain: Arc<dyn Keychain>,
    /// What reaches the AI providers: `ProviderCalls`, or a fake.
    pub ai: Arc<dyn AiBackend>,
    /// Where updates come from (PLAT-4): `NoUpdates` until M6-27.
    pub updates: Arc<dyn UpdateSource>,
    /// The program git starts as its askpass helper (D10): the app's own
    /// executable, which runs `askpass::helper` first, or the tests'
    /// `askpass-helper`.
    pub askpass_helper: PathBuf,
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
    groups: Arc<GroupOps>,
    askpass: Arc<EventAskpassUi>,
    updates: Arc<Updates>,
    ai: Arc<AiProviders>,
    ai_jobs: Arc<AiJobs>,
    fonts: DesktopFonts,
    dialogs: Arc<dyn Dialogs>,
}

impl App {
    /// Loads the settings and workspaces, opens the askpass socket, starts
    /// the update schedule and the AI providers, starts watching the active
    /// workspace and detects git in the background, in the order the Tauri
    /// shell did. Detection reports as `GitStatus` events, then the active
    /// workspace's summaries are read again.
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
        let version = AppInfo::current().version;
        log.started(&version, PlatformInstallation::package());
        // Network writes ask through the UI (D10). Without the socket they
        // run without, and git fails as it would.
        let askpass = Arc::new(askpass_ui(&hub));
        match crate::askpass::serve(setup.askpass_helper, askpass.clone(), &setup.runtime) {
            Ok(server) => {
                jobs.use_askpass(server);
            }
            Err(error) => log.warn(&format!("git cannot ask for credentials: {error}")),
        }
        let preflights = hub.clone();
        let groups = GroupOps::new(
            hub.workspaces.clone(),
            hub.summaries.clone(),
            settings.clone(),
            jobs.clone(),
            move |ready| preflights.send(AppEvent::PreflightReady(ready.clone())),
        );
        // Checks only in release builds, after the startup refresh (D12).
        let updates = Updates::new(UpdateSetup {
            source: setup.updates,
            clock: Arc::new(SystemClock),
            settings: settings.clone(),
            events: Arc::new(AppUpdateEvents {
                hub: hub.clone(),
                log: log.clone(),
            }),
            package: PlatformInstallation::package(),
            checks: CHECKS,
            current_version: version,
        });
        setup.runtime.spawn(updates.clone().run_schedule());
        // With AI on, the CLI providers are detected and self-tested in the
        // background (AI-2, AI-16).
        let ai_events = Arc::new(AppAiEvents(hub.clone()));
        let ai = AiProviders::new(
            settings.clone(),
            setup.keychain,
            setup.ai,
            ai_events.clone(),
            log.clone(),
            setup.runtime.clone(),
        );
        // Proposal jobs (M4-08), which turning AI off cancels (AI-16).
        let ai_jobs = AiJobs::new(
            jobs.clone(),
            hub.workspaces.clone(),
            settings.clone(),
            ai_events,
        );
        ai.use_jobs(ai_jobs.clone());
        ai.start();
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
                groups,
                askpass,
                updates,
                ai,
                ai_jobs,
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
    /// (UI-30). When that refresh has ended, or there was none, the updater
    /// may check.
    fn spawn_git_detection(&self) {
        let app = self.clone();
        self.inner.runtime.spawn(async move {
            app.send(AppEvent::GitStatus(GitStatus::Checking));
            let chosen = app.inner.settings.git_path();
            let status = app.inner.git.detect_chosen(chosen.as_deref()).await;
            app.send(AppEvent::GitStatus(status));
            if let Ok(git) = app.usable_git().await {
                let workspaces = app.workspaces();
                let active = workspaces
                    .active()
                    .and_then(|active| workspaces.view(&active).ok());
                if let Some(workspace) = active {
                    let repos: Vec<RepoId> =
                        workspace.repos.into_iter().map(|repo| repo.id).collect();
                    app.inner.hub.refresh(workspace.id, repos, git).await;
                }
            }
            // The first update check waits for the startup refresh, so it
            // never delays the cold start (section 9, D12).
            app.inner.updates.startup_ended();
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

/// The askpass prompts (D10): sent as `AskpassPrompt`, withdrawn ones as
/// `AskpassClosed`.
fn askpass_ui(hub: &Hub) -> EventAskpassUi {
    let (shown, closed) = (hub.clone(), hub.clone());
    EventAskpassUi::new(
        move |prompt| shown.send(AppEvent::AskpassPrompt(prompt.clone())),
        move |prompt_id| closed.send(AppEvent::AskpassClosed { prompt_id }),
    )
}

/// The AI providers' views and replies, and the proposal jobs' results, sent
/// as `AppEvent`s.
struct AppAiEvents(Hub);

impl crate::ai::AiSink for AppAiEvents {
    fn changed(&self, view: &crate::ai::AiView) {
        self.0.send(AppEvent::AiChanged(view.clone()));
    }

    fn models(&self, reply: &crate::ai::AiModelsReply) {
        self.0.send(AppEvent::AiModels(reply.clone()));
    }

    fn self_test(&self, reply: &crate::ai::AiSelfTestReply) {
        self.0.send(AppEvent::AiSelfTest(reply.clone()));
    }
}

impl gitcroc_core::AiEvents for AppAiEvents {
    fn proposal_ready(&self, ready: &gitcroc_core::AiProposalReady) {
        self.0.send(AppEvent::AiProposalReady(ready.clone()));
    }
}

/// The updater's views, sent as `UpdateStatusChanged`, and its log lines.
struct AppUpdateEvents {
    hub: Hub,
    log: Arc<AppLog>,
}

impl gitcroc_core::UpdateEvents for AppUpdateEvents {
    fn changed(&self, view: &gitcroc_core::UpdateView) {
        self.hub.send(AppEvent::UpdateStatusChanged(view.clone()));
    }

    fn log(&self, message: &str) {
        self.log.warn(message);
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
