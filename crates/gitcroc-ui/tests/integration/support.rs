//! What the UI tests share: a window of their own, controls that record
//! what the title bar asks, and an app in a temporary folder.

use std::cell::{Cell, RefCell};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use gitcroc_app::ai::ProviderCalls;
use gitcroc_app::app::{App as GitcrocApp, AppSetup};
use gitcroc_app::platform::{DesktopFonts, MemoryKeychain};
use gitcroc_app::update::NoUpdates;
use gitcroc_core::{CacheDir, ConfigDir, GitStatus, LogDir};
use gitcroc_ui::services::Services;
use gitcroc_ui::title_bar::WindowControls;
use gpui_kit::test::TestWindowExt;
use gpui_kit::{
    AnyWindowHandle, App, Bounds, Entity, Render, TestAppContext, VisualTestContext, Window,
    WindowBounds, WindowOptions, point, px, size,
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

/// An app on its own runtime, with its files in a temporary folder.
pub struct Fixture {
    pub runtime: Runtime,
    pub services: Services,
    _dir: tempfile::TempDir,
}

impl Fixture {
    /// Starts the app with an event channel of `capacity`, and waits for its
    /// git detection.
    pub fn start(capacity: usize) -> Self {
        let dir = tempfile::tempdir().unwrap();
        let dirs = Dirs(dir.path().to_owned());
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .unwrap();
        let (dialogs, _) = gitcroc_ui::dialogs::dialogs();
        let app = {
            let _entered = runtime.enter();
            GitcrocApp::start(AppSetup {
                config: &dirs,
                cache: &dirs,
                logs: &dirs,
                runtime: runtime.handle().clone(),
                fonts: DesktopFonts::default(),
                dialogs: Arc::new(dialogs),
                keychain: Arc::new(MemoryKeychain::default()),
                ai: Arc::new(ProviderCalls),
                updates: Arc::new(NoUpdates),
                askpass_helper: PathBuf::new(),
                event_capacity: capacity,
            })
        };
        runtime.block_on(async {
            while matches!(app.git_status().await, GitStatus::Checking) {
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        });
        let services = Services::new(app, runtime.handle().clone());
        Self {
            runtime,
            services,
            _dir: dir,
        }
    }

    pub fn app(&self) -> &GitcrocApp {
        self.services.app()
    }
}
