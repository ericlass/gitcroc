//! `gitcroc-next`: the GPUI app, until M6-30 makes it `gitcroc`.
//!
//!   gitcroc-next [--exit-after-first-frame]
//!   gitcroc-next --bench <script.json> <trace.json>     (`perf-trace` builds)
//!
//! It reads and writes the same settings, workspaces, cache and log as an
//! installed 1.0.0-rc.1. With `--exit-after-first-frame` it exits 0 once the
//! window has presented a frame, or 1 when none came within two minutes:
//! CI's check that a real window opens. With `--bench` it plays the script
//! `gitcroc-cli bench-ui` wrote, writes what it measured and exits 0, or 1
//! when the script failed (PERF-8). Without any font installed it exits 1
//! with a message, before GPUI would panic at the first text.

// No console window behind the app in Windows release builds.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::process::ExitCode;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use gitcroc_app::ai::ProviderCalls;
use gitcroc_app::app::{App, AppSetup, EVENT_CAPACITY};
use gitcroc_app::askpass;
use gitcroc_app::platform::{DesktopFontSource, OsKeychain, PlatformFonts, PlatformSockets};
use gitcroc_app::update::NoUpdates;
use gitcroc_core::{COMMIT_GRAPH_THRESHOLD, PlatformCacheDir, PlatformConfigDir, PlatformLogDir};
use gitcroc_ui::icons::Icons;
use gitcroc_ui::services::Services;
use gitcroc_ui::theme::{AppTheme, accent, fonts};

/// How long `--exit-after-first-frame` waits for the frame.
const FIRST_FRAME_TIMEOUT: Duration = Duration::from_secs(120);

static PRESENTED: AtomicBool = AtomicBool::new(false);
static NO_FONTS: AtomicBool = AtomicBool::new(false);
static BENCH_FAILED: AtomicBool = AtomicBool::new(false);

const USAGE: &str = if cfg!(feature = "perf-trace") {
    "usage: gitcroc-next [--exit-after-first-frame | --bench <script.json> <trace.json>]"
} else {
    "usage: gitcroc-next [--exit-after-first-frame]"
};

fn main() -> ExitCode {
    // Started by git as its askpass helper (D10): no window.
    if let Some(code) = askpass::helper::<PlatformSockets>() {
        std::process::exit(code);
    }
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    let first_frame = match args.as_slice() {
        [] => false,
        [flag] if flag == "--exit-after-first-frame" => true,
        #[cfg(feature = "perf-trace")]
        [flag, _, _] if flag == "--bench" => false,
        _ => {
            eprintln!("{USAGE}");
            return ExitCode::from(2);
        }
    };
    #[cfg(feature = "perf-trace")]
    let bench = match args.as_slice() {
        [flag, script, trace] if flag == "--bench" => match bench::read(script.as_ref()) {
            Ok(script) => Some((script, std::path::PathBuf::from(trace))),
            Err(error) => {
                eprintln!("{error}");
                return ExitCode::from(2);
            }
        },
        _ => None,
    };
    // The app's work runs here, never on GPUI's main thread (ARCH-7).
    let runtime = match tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .thread_name("gitcroc")
        .build()
    {
        Ok(runtime) => runtime,
        Err(error) => {
            eprintln!("cannot start the runtime: {error}");
            return ExitCode::FAILURE;
        }
    };
    let (dialogs, dialog_requests) = gitcroc_ui::dialogs::dialogs();
    // The askpass socket is opened in the runtime's context.
    let entered = runtime.enter();
    let app = App::start(AppSetup {
        config: &PlatformConfigDir,
        cache: &PlatformCacheDir,
        logs: &PlatformLogDir,
        runtime: runtime.handle().clone(),
        fonts: PlatformFonts.read(),
        dialogs: Arc::new(dialogs),
        keychain: Arc::new(OsKeychain),
        ai: Arc::new(ProviderCalls),
        updates: Arc::new(NoUpdates),
        // Git starts this binary as its askpass helper; `main` answers first.
        askpass_helper: std::env::current_exe().unwrap_or_default(),
        update_test: None,
        event_capacity: EVENT_CAPACITY,
        commit_graph_threshold: COMMIT_GRAPH_THRESHOLD,
    });
    drop(entered);
    let system_title_bar = app.settings_get().settings.appearance.system_title_bar;
    let quitting = app.clone();
    let services = Services::new(app, runtime.handle().clone());
    if first_frame {
        std::thread::spawn(|| {
            std::thread::sleep(FIRST_FRAME_TIMEOUT);
            eprintln!("no frame was presented within {FIRST_FRAME_TIMEOUT:?}");
            std::process::exit(1);
        });
    }
    gpui_kit::application().with_assets(Icons).run(move |cx| {
        gitcroc_ui::init(cx);
        if let Err(error) = fonts::check_installed(AppTheme::global(cx).installed_fonts()) {
            eprintln!("{error}");
            services.app().log_warning(&error.to_string());
            NO_FONTS.store(true, Ordering::SeqCst);
            // A quit asked for before the event loop runs is lost on
            // Linux: a task quits from inside it.
            cx.spawn(async |cx| cx.update(|cx| cx.quit())).detach();
            return;
        }
        accent::follow(cx);
        dialog_requests.serve(cx);
        // Closing the last window quits, as on every platform but
        // macOS's custom; gitcroc has one window.
        cx.on_window_closed(|cx, _| {
            if cx.windows().is_empty() {
                cx.quit();
            }
        })
        .detach();
        let (window, _view) = match gitcroc_ui::open_window(services, system_title_bar, cx) {
            Ok(opened) => opened,
            Err(error) => {
                eprintln!("cannot open the window: {error:#}");
                cx.quit();
                return;
            }
        };
        #[cfg(feature = "perf-trace")]
        if let Some((script, trace)) = bench {
            bench::start(window, _view, script, trace, cx);
        }
        if first_frame {
            // The callback of the frame after the first one, so the
            // first has been presented.
            let _ = window.update(cx, |_, window, _| {
                window.on_next_frame(|window, _| {
                    window.on_next_frame(|_, cx| {
                        PRESENTED.store(true, Ordering::SeqCst);
                        cx.quit();
                    });
                });
            });
        }
    });
    // Git's prompts get no answer, so nothing waits on the app (D10). Jobs
    // still running are abandoned, as when the Tauri app closed.
    runtime.block_on(quitting.shutdown());
    runtime.shutdown_background();
    if NO_FONTS.load(Ordering::SeqCst) {
        return ExitCode::FAILURE;
    }
    if first_frame && !PRESENTED.load(Ordering::SeqCst) {
        return ExitCode::FAILURE;
    }
    if BENCH_FAILED.load(Ordering::SeqCst) {
        return ExitCode::FAILURE;
    }
    ExitCode::SUCCESS
}

/// The benchmark runner in the window (PERF-8).
#[cfg(feature = "perf-trace")]
mod bench {
    use std::path::{Path, PathBuf};
    use std::sync::atomic::Ordering;

    use gitcroc_ui::perf::{Runner, Script, Tick};
    use gitcroc_ui::window::GitcrocWindow;
    use gpui_kit::{AnyWindowHandle, App, Entity, Window};

    use super::BENCH_FAILED;

    pub fn read(path: &Path) -> Result<Script, String> {
        let text = std::fs::read_to_string(path)
            .map_err(|error| format!("cannot read {}: {error}", path.display()))?;
        serde_json::from_str(&text)
            .map_err(|error| format!("cannot parse {}: {error}", path.display()))
    }

    /// Plays `script` in the window, one tick each frame, then writes the
    /// trace to `trace` and quits.
    pub fn start(
        window: AnyWindowHandle,
        view: Entity<GitcrocWindow>,
        script: Script,
        trace: PathBuf,
        cx: &mut App,
    ) {
        let runner = Runner::new(view, script);
        let _ = window.update(cx, |_, window, _| {
            // Frames of an inactive window are slowed down.
            window.activate_window();
            next(window, runner, trace);
        });
    }

    fn next(window: &Window, mut runner: Runner, trace: PathBuf) {
        window.on_next_frame(move |window, cx| match runner.tick(window, cx) {
            Tick::Running => next(window, runner, trace),
            Tick::Done(done) => {
                let written = serde_json::to_string(&done)
                    .map_err(|error| error.to_string())
                    .and_then(|json| {
                        std::fs::write(&trace, json).map_err(|error| error.to_string())
                    });
                if let Err(error) = written {
                    eprintln!("cannot write {}: {error}", trace.display());
                    BENCH_FAILED.store(true, Ordering::SeqCst);
                }
                cx.quit();
            }
            Tick::Failed(error) => {
                eprintln!("the bench failed: {error}");
                BENCH_FAILED.store(true, Ordering::SeqCst);
                cx.quit();
            }
        });
    }
}
