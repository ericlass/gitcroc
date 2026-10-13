//! Application shell: the Tauri 2 app that wires core and AI together.

pub mod ipc;

use std::sync::Arc;

use gitcroc_core::{
    Changes, GitSetup, Histories, JobIds, PlatformCacheDir, PlatformConfigDir, SettingsStore,
    Summaries, SummaryCache, Trees, Workspaces,
};

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
    let ipc = ipc::builder();
    // Small files, read once so the first paint has the workspace.
    let workspaces = Arc::new(Workspaces::load(&PlatformConfigDir));
    // The cached summaries, read once so the first paint shows every repo
    // before git is read (PERF-3); the refresh starts after git detection.
    let summaries = Arc::new(Summaries::load(
        SummaryCache::open(&PlatformCacheDir),
        &workspaces,
    ));
    tauri::Builder::default()
        // The folder picker, used from Rust only; the window has no dialog
        // permission (SEC-2).
        .plugin(tauri_plugin_dialog::init())
        .invoke_handler(ipc.invoke_handler())
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
        .setup(move |app| {
            ipc.mount_events(app);
            ipc::spawn_git_detection(app.handle().clone());
            Ok(())
        })
        .run(tauri::generate_context!())?;
    Ok(())
}
