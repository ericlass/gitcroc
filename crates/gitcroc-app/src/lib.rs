//! Application shell: the Tauri 2 app that wires core and AI together.

pub mod ai;
pub mod askpass;
pub mod ipc;
pub mod platform;

use std::sync::Arc;

use gitcroc_core::{
    AppLog, Changes, GitSetup, Histories, PlatformCacheDir, PlatformConfigDir, PlatformLogDir,
    SettingsStore, Summaries, SummaryCache, Trees, Workspaces,
};
use platform::DesktopFontSource;
use tauri::Manager;

/// Errors returned by `gitcroc-app`.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("Tauri error: {0}")]
    Tauri(#[from] tauri::Error),
}

/// Opens the main window and runs the event loop until the app exits.
pub fn run() -> Result<(), Error> {
    #[cfg(any(debug_assertions, feature = "perf-trace"))]
    ipc::debug::mark_process_start();
    let ipc = ipc::builder();
    // Every git process the app runs goes to `gitcroc.log` (section 10).
    let log = Arc::new(AppLog::open(&PlatformLogDir));
    let command_log = log.clone();
    gitcroc_core::log_commands_to(move |record| command_log.command(record));
    // Small files, read once before the window shows, so the first paint
    // already has the saved theme and the workspaces of the saved tabs.
    let settings = Arc::new(SettingsStore::load(&PlatformConfigDir));
    let workspaces = Arc::new(Workspaces::load_with(&PlatformConfigDir, settings.clone()));
    // The cached summaries, read once so the first paint shows every repo
    // before git is read (PERF-3); the refresh starts after git detection.
    let summaries = Arc::new(Summaries::load(
        SummaryCache::open(&PlatformCacheDir),
        &workspaces,
    ));
    // The pool jobs and the refresh share has as many places as group
    // operations run sub-jobs at once (PERF-4).
    let parallelism = settings.view().settings.groups.group_parallelism;
    summaries
        .pool()
        .resize(usize::try_from(parallelism).unwrap_or(gitcroc_core::POOL_SIZE));
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
        .manage(log)
        .manage(settings)
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
            app.manage(platform::PlatformFonts.read());
            ipc.mount_events(app);
            // The watchers report through the app, so they start with it.
            app.manage(Arc::new(ipc::watchers(app.handle())));
            let jobs = ipc::jobs(app.handle());
            // Network writes ask through the window (D10). Without the
            // socket they run without, and git fails as it would.
            let askpass_ui = ipc::askpass_ui(app.handle());
            match askpass::serve(askpass_ui.clone()) {
                Ok(server) => {
                    jobs.use_askpass(server);
                }
                Err(error) => app
                    .state::<Arc<AppLog>>()
                    .warn(&format!("git cannot ask for credentials: {error}")),
            }
            app.manage(askpass_ui);
            app.manage(jobs);
            app.manage(ipc::group_ops(app.handle()));
            // AI providers (M4-05): their keys in the OS keychain, or in
            // memory for the `e2e` build. With AI on, the CLI providers are
            // detected and self-tested in the background (AI-2, AI-16).
            #[cfg(not(feature = "e2e"))]
            let keychain = Arc::new(platform::OsKeychain);
            #[cfg(feature = "e2e")]
            let keychain = Arc::new(platform::MemoryKeychain::default());
            let ai = ai::AiProviders::new(
                app.state::<Arc<SettingsStore>>().inner().clone(),
                keychain,
                Arc::new(ai::ProviderCalls),
                ipc::ai_events(app.handle()),
                app.state::<Arc<AppLog>>().inner().clone(),
                tauri::async_runtime::handle().inner().clone(),
            );
            ai.start();
            app.manage(ai);
            ipc::spawn_watching(app.handle());
            ipc::spawn_git_detection(app.handle().clone());
            ipc::perf_mark("setup done");
            Ok(())
        })
        .run(tauri::generate_context!())?;
    Ok(())
}
