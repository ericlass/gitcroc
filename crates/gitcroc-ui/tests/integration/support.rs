//! What the UI tests share: a window of their own, controls that record
//! what the title bar asks, an app in a temporary folder, and fake gits.

use std::cell::{Cell, RefCell};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use gitcroc_ai::{
    ApiKey, MockGate, MockProvider, Provider, ProviderFuture, ProviderReply, ProviderRequest,
    ProviderSpec,
};
use gitcroc_app::ai::{BoxFuture, DetectedCli, ModelsResult, ProviderCalls, SelfTest};
use gitcroc_app::app::{App as GitcrocApp, AppEvent, AppSetup};
use gitcroc_app::platform::{DesktopFonts, DialogFuture, Dialogs, FileFilter, MemoryKeychain};
use gitcroc_app::update::{NoUpdates, UpdateTest};
use gitcroc_core::{
    CacheDir, ConfigDir, GitStatus, LogDir, PaneLayout, ProviderEntry, ProviderKind, RepoId,
    UpdateSource, WORKSPACES_DIR, WorkspaceId,
};
use std::rc::Rc;

use gitcroc_ui::services::Services;
use gitcroc_ui::title_bar::{Platform, WindowControls};
use gitcroc_ui::window::GitcrocWindow;
use gpui_kit::test::TestWindowExt;
use gpui_kit::{
    AnyWindowHandle, App, AppContext, Bounds, Entity, Render, TestAppContext, VisualTestContext,
    Window, WindowBounds, WindowOptions, point, px, size,
};
use tokio::runtime::Runtime;

/// Opens a window showing the view `build` makes, and draws a frame.
pub fn open<V: Render>(
    cx: &mut TestAppContext,
    build: impl FnOnce(&mut Window, &mut App) -> Entity<V>,
) -> (VisualTestContext, Entity<V>) {
    // The app's calls end on its runtime's threads, which wake the
    // window's tasks from there.
    cx.executor().allow_parking();
    cx.update(gitcroc_ui::init);
    let (window, view): (AnyWindowHandle, Entity<V>) = cx
        .update(|cx| {
            let bounds = Bounds {
                origin: point(px(0.), px(0.)),
                size: size(px(1200.), px(800.)),
            };
            gpui_kit::open_window(
                WindowOptions {
                    window_bounds: Some(WindowBounds::Windowed(bounds)),
                    ..Default::default()
                },
                cx,
                build,
            )
        })
        .unwrap();
    let mut cx = VisualTestContext::from_window(window, cx);
    cx.run_until_parked();
    cx.update(|window, cx| window.render_frame(cx));
    (cx, view)
}

/// Opens the main window on the fixture's app, on Linux with gitcroc's
/// own title bar, and waits until it shows what git detection found: the
/// main window, the workspaces to open (UI-27), or the setup screen
/// (PLAT-3).
pub fn main_window(
    cx: &mut TestAppContext,
    fixture: &Fixture,
) -> (VisualTestContext, Entity<GitcrocWindow>) {
    let (mut cx, window) = opened_main_window(cx, fixture);
    settle(&mut cx, |window, _| {
        ["toolbar", "workspace-start", "git-setup"]
            .into_iter()
            .any(|id| window.try_find(id).is_some())
    });
    (cx, window)
}

/// The main window on the fixture's app, as it first draws.
pub fn opened_main_window(
    cx: &mut TestAppContext,
    fixture: &Fixture,
) -> (VisualTestContext, Entity<GitcrocWindow>) {
    opened_main_window_with(cx, fixture, Rc::new(FakeControls::default()))
}

/// The main window on the fixture's app, as it first draws, asking
/// `controls` what its title bar asks.
pub fn opened_main_window_with(
    cx: &mut TestAppContext,
    fixture: &Fixture,
    controls: Rc<FakeControls>,
) -> (VisualTestContext, Entity<GitcrocWindow>) {
    let services = fixture.services.clone();
    open(cx, move |window, cx| {
        cx.new(|cx| GitcrocWindow::new(services, controls, Platform::Linux, false, window, cx))
    })
}

/// Runs the window's tasks until `done` holds. Work on the app's runtime
/// ends on other threads, so this waits for it between runs, for at most
/// ten seconds: long enough for any runner, and only reached on a failure.
pub fn settle(cx: &mut VisualTestContext, mut done: impl FnMut(&mut Window, &mut App) -> bool) {
    for _ in 0..1000 {
        cx.run_until_parked();
        if cx.update(|window, cx| {
            window.render_frame(cx);
            done(window, cx)
        }) {
            return;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    panic!("the window never got there");
}

/// What the title bar asked of the window.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Asked {
    Minimize,
    ToggleMaximize,
    Close,
    Move,
}

/// Window controls that record what they are asked.
#[derive(Default)]
pub struct FakeControls {
    pub asked: RefCell<Vec<Asked>>,
    pub maximized: Cell<bool>,
    /// The frame a Linux window manager draws instead of the app.
    pub server_decorated: Cell<bool>,
}

impl WindowControls for FakeControls {
    fn minimize(&self, _: &mut Window) {
        self.asked.borrow_mut().push(Asked::Minimize);
    }

    fn toggle_maximize(&self, _: &mut Window) {
        self.asked.borrow_mut().push(Asked::ToggleMaximize);
    }

    fn close(&self, _: &mut Window, _: &mut App) {
        self.asked.borrow_mut().push(Asked::Close);
    }

    fn start_move(&self, _: &mut Window) {
        self.asked.borrow_mut().push(Asked::Move);
    }

    fn is_maximized(&self, _: &Window) -> bool {
        self.maximized.get()
    }

    fn client_decorated(&self, _: &Window) -> bool {
        !self.server_decorated.get()
    }
}

struct Dirs(PathBuf);

impl ConfigDir for Dirs {
    fn config_dir(&self) -> Option<PathBuf> {
        Some(self.0.join("config"))
    }
}

impl CacheDir for Dirs {
    fn cache_dir(&self) -> Option<PathBuf> {
        Some(self.0.join("cache"))
    }
}

impl LogDir for Dirs {
    fn log_dir(&self) -> Option<PathBuf> {
        Some(self.0.join("logs"))
    }
}

/// The answers queued for the next file dialogs, oldest first; `None`
/// cancels.
pub type Answers = Arc<std::sync::Mutex<std::collections::VecDeque<Option<PathBuf>>>>;

/// File dialogs that answer with the next queued answer, or else with one
/// path, or cancel.
struct Answering {
    queued: Answers,
    otherwise: Option<PathBuf>,
}

impl Answering {
    fn answer(&self) -> DialogFuture {
        let next = self.queued.lock().unwrap().pop_front();
        Box::pin(std::future::ready(
            next.unwrap_or_else(|| self.otherwise.clone()),
        ))
    }
}

impl Dialogs for Answering {
    fn pick_folder(&self, _: &str) -> DialogFuture {
        self.answer()
    }

    fn pick_file(&self, _: &str, _: Option<&FileFilter>) -> DialogFuture {
        self.answer()
    }

    fn save_file(&self, _: &str, _: &str, _: Option<&FileFilter>) -> DialogFuture {
        self.answer()
    }
}

/// How a [`Fixture`] starts its app.
pub struct Options {
    /// The event channel's capacity.
    pub capacity: usize,
    /// The settings file's text, if there is one.
    pub settings: Option<String>,
    /// Whether to wait for the first git detection.
    pub wait_for_git: bool,
    /// What the file dialogs answer; `None` cancels.
    pub picked: Option<PathBuf>,
    /// How slow a first history page must be to earn the commit-graph
    /// offer (PERF-2): never by default, so no test sees it by chance.
    pub commit_graph_threshold: Duration,
    /// How many files a repo must track to earn the fsmonitor offer
    /// (PERF-7): never by default, so no test sees it by chance.
    pub fsmonitor_threshold: usize,
    /// Where updates come from, with the package and whether the app
    /// checks; `None` never finds one, as a debug build never checks (D12).
    pub updates: Option<(Arc<dyn UpdateSource>, UpdateTest)>,
    /// What the AI providers are: the real ones by default.
    pub ai: AiBackend,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            capacity: gitcroc_app::app::EVENT_CAPACITY,
            settings: None,
            wait_for_git: true,
            picked: None,
            commit_graph_threshold: Duration::MAX,
            fsmonitor_threshold: usize::MAX,
            updates: None,
            ai: AiBackend::ProviderCalls,
        }
    }
}

/// What reaches the AI providers in a test.
pub enum AiBackend {
    /// The real providers of `gitcroc-ai`.
    ProviderCalls,
    /// A fake that answers from a script, reaching no network and no CLI.
    Script(AiScript),
}

/// What the fake AI backend answers.
#[derive(Clone)]
pub struct AiScript {
    /// The CLIs detection finds (AI-2).
    pub detected: Vec<DetectedCli>,
    /// Every self-test's result.
    pub self_test: SelfTest,
    /// Every model list.
    pub models: ModelsResult,
    /// What the mock provider of a proposal job replies, in order; each
    /// job's provider starts from the first.
    pub replies: Vec<ProviderReply>,
    /// Offers the mock kind, as the `e2e` build does.
    pub mock: bool,
    /// Holds each proposal request until the test releases it through
    /// [`Fixture::ai_gates`].
    pub held: bool,
    /// Holds each model list and self-test until
    /// [`Fixture::release_ai_answer`], so a test sees one being asked for.
    pub hold_answers: bool,
}

impl Default for AiScript {
    fn default() -> Self {
        Self {
            detected: Vec::new(),
            self_test: SelfTest::Passed {
                version: Some("1.0.0".to_owned()),
            },
            models: ModelsResult::NotOffered,
            replies: Vec::new(),
            mock: false,
            held: false,
            hold_answers: false,
        }
    }
}

/// The fake behind [`AiBackend::Script`].
struct ScriptedAi {
    script: AiScript,
    /// The gates of held mock providers, one per proposal job.
    gates: std::sync::Mutex<Vec<MockGate>>,
    /// Every mock provider made, one per proposal job, for its requests.
    mocks: std::sync::Mutex<Vec<Arc<MockProvider>>>,
    /// Lets one held model list or self-test answer per permit.
    answers: tokio::sync::Semaphore,
}

impl ScriptedAi {
    async fn answer<T>(&self, result: T) -> T {
        if self.script.hold_answers
            && let Ok(permit) = self.answers.acquire().await
        {
            permit.forget();
        }
        result
    }

    fn mock(&self) -> Box<dyn Provider> {
        let mut provider = MockProvider::new(self.script.replies.iter().cloned().map(Ok));
        if self.script.held {
            let (held, gate) = provider.held();
            self.gates.lock().unwrap().push(gate);
            provider = held;
        }
        let provider = Arc::new(provider);
        self.mocks.lock().unwrap().push(provider.clone());
        Box::new(Shared(provider))
    }
}

/// A mock the fake keeps too, so a test can read its requests.
struct Shared(Arc<MockProvider>);

impl Provider for Shared {
    fn complete<'a>(&'a self, request: &'a ProviderRequest) -> ProviderFuture<'a> {
        self.0.complete(request)
    }
}

impl gitcroc_app::ai::AiBackend for ScriptedAi {
    fn detect(&self) -> Vec<DetectedCli> {
        self.script.detected.clone()
    }

    fn self_test<'a>(&'a self, _: ProviderKind, _: &'a Path) -> BoxFuture<'a, SelfTest> {
        Box::pin(self.answer(self.script.self_test.clone()))
    }

    fn list_models<'a>(
        &'a self,
        _: &'a ProviderEntry,
        _: Option<ApiKey>,
    ) -> BoxFuture<'a, ModelsResult> {
        Box::pin(self.answer(self.script.models.clone()))
    }

    fn provider(&self, _: ProviderSpec) -> Result<Box<dyn Provider>, String> {
        Ok(self.mock())
    }

    fn kinds(&self) -> Vec<ProviderKind> {
        let mut kinds = vec![
            ProviderKind::Anthropic,
            ProviderKind::Openai,
            ProviderKind::ClaudeCode,
            ProviderKind::Codex,
        ];
        if self.script.mock {
            kinds.push(ProviderKind::Mock);
        }
        kinds
    }

    fn mock(&self) -> Result<Box<dyn Provider>, String> {
        Ok(ScriptedAi::mock(self))
    }
}

/// A settings file that chooses the git at `path` (PLAT-3), with `schema`.
pub fn settings_with_git(path: &Path, schema: u32) -> String {
    let git = toml::Value::String(path.to_str().unwrap().to_owned());
    format!("schema = {schema}\ngit_path = {git}\n")
}

/// An app on its own runtime, with its files in a temporary folder.
pub struct Fixture {
    pub runtime: Runtime,
    pub services: Services,
    answers: Answers,
    keychain: Arc<MemoryKeychain>,
    ai: Option<Arc<ScriptedAi>>,
    dir: tempfile::TempDir,
}

/// The app goes as `gitcroc-next`'s does when it quits: git's prompts get
/// no answer first (D10). Dropping the runtime then waits for the threads
/// that read git's output, so a git process left running hangs the test.
impl Drop for Fixture {
    fn drop(&mut self) {
        self.runtime.block_on(self.app().shutdown());
    }
}

impl Fixture {
    /// Starts the app with an event channel of `capacity`, and waits for its
    /// git detection.
    pub fn start(capacity: usize) -> Self {
        Self::start_with(Options {
            capacity,
            ..Options::default()
        })
    }

    pub fn start_with(options: Options) -> Self {
        let dir = tempfile::tempdir().unwrap();
        let dirs = Dirs(dir.path().to_owned());
        if let Some(settings) = &options.settings {
            std::fs::create_dir_all(dir.path().join("config")).unwrap();
            std::fs::write(
                dir.path().join("config").join(gitcroc_core::SETTINGS_FILE),
                settings,
            )
            .unwrap();
        }
        let answers = Answers::default();
        let keychain = Arc::new(MemoryKeychain::default());
        let scripted = match options.ai {
            AiBackend::ProviderCalls => None,
            AiBackend::Script(script) => Some(Arc::new(ScriptedAi {
                script,
                gates: Default::default(),
                mocks: Default::default(),
                answers: tokio::sync::Semaphore::new(0),
            })),
        };
        let ai: Arc<dyn gitcroc_app::ai::AiBackend> = match &scripted {
            Some(scripted) => scripted.clone(),
            None => Arc::new(ProviderCalls),
        };
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .unwrap();
        let app = {
            let _entered = runtime.enter();
            GitcrocApp::start(AppSetup {
                config: &dirs,
                cache: &dirs,
                logs: &dirs,
                runtime: runtime.handle().clone(),
                fonts: DesktopFonts::default(),
                dialogs: Arc::new(Answering {
                    queued: answers.clone(),
                    otherwise: options.picked,
                }),
                keychain: keychain.clone(),
                ai,
                updates: options.updates.as_ref().map_or_else(
                    || Arc::new(NoUpdates) as Arc<dyn UpdateSource>,
                    |(source, _)| source.clone(),
                ),
                update_test: options.updates.as_ref().map(|(_, test)| *test),
                // Git starts the app's binary as its askpass helper (D10).
                askpass_helper: PathBuf::from(env!("CARGO_BIN_EXE_gitcroc-next")),
                event_capacity: options.capacity,
                commit_graph_threshold: options.commit_graph_threshold,
                fsmonitor_threshold: options.fsmonitor_threshold,
            })
        };
        if options.wait_for_git {
            runtime.block_on(async {
                while matches!(app.git_status().await, GitStatus::Checking) {
                    tokio::time::sleep(Duration::from_millis(10)).await;
                }
            });
        }
        let services = Services::new(app, runtime.handle().clone());
        Self {
            runtime,
            services,
            answers,
            keychain,
            ai: scripted,
            dir,
        }
    }

    /// Every secret in the keychain (SEC-1).
    pub fn keychain_secrets(&self) -> Vec<String> {
        use gitcroc_app::platform::Keychain;
        let app = self.app().clone();
        let view = self
            .runtime
            .block_on(async move { app.ai_providers_get().await })
            .unwrap();
        view.providers
            .iter()
            .filter_map(|provider| {
                let account = gitcroc_app::platform::keychain_account(&provider.id.0);
                self.keychain.read(&account).unwrap()
            })
            .collect()
    }

    /// The gates of the held mock providers, one per proposal job so far.
    pub fn ai_gates(&self) -> Vec<MockGate> {
        let ai = self.ai.as_ref().expect("no scripted AI backend");
        ai.gates.lock().unwrap().clone()
    }

    /// Every request the mock providers got, in order.
    pub fn ai_requests(&self) -> Vec<ProviderRequest> {
        let ai = self.ai.as_ref().expect("no scripted AI backend");
        let mocks = ai.mocks.lock().unwrap();
        mocks.iter().flat_map(|mock| mock.requests()).collect()
    }

    /// Lets one held model list or self-test answer.
    pub fn release_ai_answer(&self) {
        let ai = self.ai.as_ref().expect("no scripted AI backend");
        ai.answers.add_permits(1);
    }

    /// Makes the next file dialog answer `picked`; `None` cancels.
    pub fn answer(&self, picked: Option<&Path>) {
        self.answers
            .lock()
            .unwrap()
            .push_back(picked.map(Path::to_owned));
    }

    pub fn app(&self) -> &GitcrocApp {
        self.services.app()
    }

    /// The active workspace.
    pub fn active(&self) -> WorkspaceId {
        self.app().workspace_get().active.unwrap()
    }

    /// Adds the repos below `path` to workspace `id` as the user would, and
    /// returns their ids by name once the add has reported.
    pub fn add_repos(&self, id: &WorkspaceId, path: &Path) -> Vec<(String, RepoId)> {
        let app = self.app().clone();
        self.runtime.block_on(async {
            let mut events = app.subscribe();
            let job = app
                .workspace_add_path(id.clone(), path.to_owned())
                .await
                .unwrap();
            loop {
                if let AppEvent::WorkspaceAddReport(report) = events.recv().await.unwrap()
                    && report.job_id == job
                {
                    assert!(report.refused.is_empty(), "{report:?}");
                    return report
                        .added
                        .into_iter()
                        .map(|repo| (repo.name, repo.id))
                        .collect();
                }
            }
        })
    }

    /// Reads every repo of workspace `id` again, as when the window regains
    /// focus (OPS-7); each summary arrives as an event.
    pub fn refresh(&self, id: &WorkspaceId) {
        let app = self.app().clone();
        let id = id.clone();
        self.runtime
            .block_on(async move { app.workspace_refresh(id).await })
            .unwrap();
    }

    /// The settings file's text.
    pub fn settings_file(&self) -> String {
        std::fs::read_to_string(
            self.dir
                .path()
                .join("config")
                .join(gitcroc_core::SETTINGS_FILE),
        )
        .unwrap()
    }

    /// The workspace's saved pane layout.
    pub fn layout(&self, id: &WorkspaceId) -> PaneLayout {
        self.app()
            .workspace_get()
            .open
            .into_iter()
            .find(|view| &view.id == id)
            .map(|view| view.layout)
            .unwrap_or_default()
    }

    /// Saves `layout` as the workspace's pane layout, as the window does.
    pub fn set_layout(&self, id: &WorkspaceId, layout: PaneLayout) {
        let app = self.app().clone();
        let id = id.clone();
        self.runtime
            .block_on(async move { app.workspace_set_layout(id, layout).await })
            .unwrap();
    }

    /// The workspace file's text.
    pub fn workspace_file(&self, id: &WorkspaceId) -> String {
        std::fs::read_to_string(
            self.dir
                .path()
                .join("config")
                .join(WORKSPACES_DIR)
                .join(format!("{}.toml", id.0)),
        )
        .unwrap()
    }
}

/// A script that answers `git --version` like git would (QA-2), as the
/// core's detection tests do, with the version in a file beside it so a
/// test can change it. A blocked one answers only once released, so a
/// test sees detection running without depending on timing.
pub struct FakeGit {
    dir: tempfile::TempDir,
    path: PathBuf,
}

impl FakeGit {
    pub fn new(version: &str) -> Self {
        let git = Self::blocked(version);
        git.release();
        git
    }

    pub fn blocked(version: &str) -> Self {
        let dir = tempfile::tempdir().unwrap();
        #[cfg(windows)]
        let path = {
            let path = dir.path().join("git.cmd");
            std::fs::write(
                &path,
                "@echo off\r\n:wait\r\nif not exist \"%~dp0release\" goto wait\r\n\
                 set /p version=<\"%~dp0version\"\r\necho git version %version%\r\n",
            )
            .unwrap();
            path
        };
        #[cfg(not(windows))]
        let path = {
            use std::os::unix::fs::PermissionsExt;
            let path = dir.path().join("git");
            std::fs::write(
                &path,
                "#!/bin/sh\ndir=$(dirname \"$0\")\n\
                 if [ \"$1\" = --version ]; then\n\
                 while [ ! -e \"$dir/release\" ]; do sleep 0.01; done\n\
                 fi\n\
                 echo \"git version $(cat \"$dir/version\")\"\n",
            )
            .unwrap();
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
            path
        };
        let git = Self { dir, path };
        git.set_version(version);
        #[cfg(not(windows))]
        wait_until_runnable(&git.path);
        git
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn set_version(&self, version: &str) {
        std::fs::write(self.dir.path().join("version"), version).unwrap();
    }

    pub fn release(&self) {
        std::fs::write(self.dir.path().join("release"), "").unwrap();
    }

    /// Makes the next `--version` wait until [`Self::release`].
    pub fn block(&self) {
        std::fs::remove_file(self.dir.path().join("release")).unwrap();
    }
}

/// A child forked by another test thread while the script was open for
/// writing holds that handle until it execs, and running the script fails
/// with "Text file busy" meanwhile; one successful run, without
/// `--version` so it never waits, means no such handle is left.
#[cfg(not(windows))]
fn wait_until_runnable(path: &Path) {
    for _ in 0..1000 {
        match std::process::Command::new(path).output() {
            Err(error) if error.raw_os_error() == Some(26) => std::thread::yield_now(),
            result => {
                result.unwrap();
                return;
            }
        }
    }
    panic!("{} stays busy", path.display());
}
