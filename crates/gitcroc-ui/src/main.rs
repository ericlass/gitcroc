//! `gitcroc-next`: the GPUI app, until M6-30 makes it `gitcroc`.
//!
//!   gitcroc-next [--exit-after-first-frame]
//!
//! It reads and writes the same settings, workspaces, cache and log as an
//! installed 1.0.0-rc.1. With `--exit-after-first-frame` it exits 0 once the
//! window has presented a frame, or 1 when none came within two minutes:
//! CI's check that a real window opens.

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
use gitcroc_core::{PlatformCacheDir, PlatformConfigDir, PlatformLogDir};
use gitcroc_ui::icons::Icons;
use gitcroc_ui::services::Services;

/// How long `--exit-after-first-frame` waits for the frame.
const FIRST_FRAME_TIMEOUT: Duration = Duration::from_secs(120);

static PRESENTED: AtomicBool = AtomicBool::new(false);

fn main() -> ExitCode {
    // Started by git as its askpass helper (D10): no window.
    if let Some(code) = askpass::helper::<PlatformSockets>() {
        std::process::exit(code);
    }
    let first_frame = match std::env::args().skip(1).collect::<Vec<_>>().as_slice() {
        [] => false,
        [flag] if flag == "--exit-after-first-frame" => true,
        _ => {
            eprintln!("usage: gitcroc-next [--exit-after-first-frame]");
            return ExitCode::from(2);
        }
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
        event_capacity: EVENT_CAPACITY,
    });
    drop(entered);
    let system_title_bar = app.settings_get().settings.appearance.system_title_bar;
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
        dialog_requests.serve(cx);
        // Closing the last window quits, as on every platform but
        // macOS's custom; gitcroc has one window.
        cx.on_window_closed(|cx, _| {
            if cx.windows().is_empty() {
                cx.quit();
            }
        })
        .detach();
        let window = match gitcroc_ui::open_window(services, system_title_bar, cx) {
            Ok((window, _)) => window,
            Err(error) => {
                eprintln!("cannot open the window: {error:#}");
                cx.quit();
                return;
            }
        };
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
    // Jobs still running are abandoned, as when the Tauri app closed.
    runtime.shutdown_background();
    if first_frame && !PRESENTED.load(Ordering::SeqCst) {
        return ExitCode::FAILURE;
    }
    ExitCode::SUCCESS
}
