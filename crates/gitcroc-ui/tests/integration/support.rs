//! What the UI tests share: a window of their own, controls that record
//! what the title bar asks, an app in a temporary folder, and fake gits.

use std::cell::{Cell, RefCell};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use gitcroc_app::ai::ProviderCalls;
use gitcroc_app::app::{App as GitcrocApp, AppEvent, AppSetup};
use gitcroc_app::platform::{DesktopFonts, DialogFuture, Dialogs, FileFilter, MemoryKeychain};
use gitcroc_app::update::{NoUpdates, UpdateTest};
use gitcroc_core::{
    CacheDir, ConfigDir, GitStatus, LogDir, PaneLayout, RepoId, UpdateSource, WORKSPACES_DIR,
    WorkspaceId,
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

/// File dialogs that answer every request with one path, or cancel.
struct Answering(Option<PathBuf>);

impl Answering {
    fn answer(&self) -> DialogFuture {
        Box::pin(std::future::ready(self.0.clone()))
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
        }
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
                dialogs: Arc::new(Answering(options.picked)),
                keychain: Arc::new(MemoryKeychain::default()),
                ai: Arc::new(ProviderCalls),
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
            dir,
        }
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
