//! Application shell: the Tauri 2 app that wires core and AI together.

pub mod ipc;

use std::sync::Arc;

use gitcroc_core::{
    Changes, GitSetup, Histories, JobIds, PlatformCacheDir, PlatformConfigDir, SettingsStore,
    Summaries, SummaryCache, Trees, Workspaces,
};
use tauri::Manager;

/// Errors returned by `gitcroc-app`.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Tauri error: {0}")]
    Tauri(#[from] tauri::Error),
}

/// Opens the main window and runs the event loop until the app exits.
pub fn run() -> Result<(), Error> {
    #[cfg(any(debug_assertions, feature = "perf-trace"))]
    ipc::debug::mark_process_start();
    let ipc = ipc::builder();
    // Small files, read once so the first paint has the workspace.
    let workspaces = Arc::new(Workspaces::load(&PlatformConfigDir));
    // The cached summaries, read once so the first paint shows every repo
    // before git is read (PERF-3); the refresh starts after git detection.
    let summaries = Arc::new(Summaries::load(
        SummaryCache::open(&PlatformCacheDir),
        &workspaces,
    ));
    ipc::perf_mark("state loaded");
    let invoke_handler = ipc.invoke_handler();
    // Debug and bench builds note each command's first call (M1-15).
    #[cfg(any(debug_assertions, feature = "perf-trace"))]
    let invoke_handler = move |invoke: tauri::ipc::Invoke| {
        ipc::perf_mark(&format!("ipc {}", invoke.message.command()));
        invoke_handler(invoke)
    };
    tauri::Builder::default()
        // The folder picker, used from Rust only; the window has no dialog
        // permission (SEC-2).
        .plugin(tauri_plugin_dialog::init())
        .invoke_handler(invoke_handler)
        .manage(GitSetup::new())
        .manage(JobIds::default())
        // A small file read once, before the window shows, so the first
        // paint already has the saved theme.
        .manage(SettingsStore::load(&PlatformConfigDir))
        .manage(workspaces)
        .manage(summaries)
        .manage(Arc::new(Trees::new()))
        .manage(Arc::new(Histories::default()))
        .manage(Arc::new(Changes::new()))
        // The window is created hidden and shown once its page starts
        // loading. Shown earlier, WebKitGTK's UI process waits for the web
        // process that is still starting, up to 500 ms, while the web process
        // waits for it (PERF-3, M1-15). Shown again when the page has loaded,
        // in case the start was missed; showing a shown window does nothing.
        .on_page_load(|webview, payload| {
            ipc::perf_mark(match payload.event() {
                tauri::webview::PageLoadEvent::Started => "page load started",
                tauri::webview::PageLoadEvent::Finished => "page load finished",
            });
            if let Err(error) = webview.window().show() {
                eprintln!("gitcroc: cannot show the window: {error}");
            }
        })
        .setup(move |app| {
            ipc::perf_mark("setup");
            #[cfg(feature = "perf-trace")]
            ipc::debug::probe_main_loop(app.handle());
            // Before the page loads, so its first `app_info` has it.
            app.manage(ipc::DesktopFonts::read());
            ipc.mount_events(app);
            // The watchers report through the app, so they start with it.
            app.manage(Arc::new(ipc::watchers(app.handle())));
            ipc::spawn_watching(app.handle());
            ipc::spawn_git_detection(app.handle().clone());
            ipc::perf_mark("setup done");
            Ok(())
        })
        .run(tauri::generate_context!())?;
    Ok(())
}
