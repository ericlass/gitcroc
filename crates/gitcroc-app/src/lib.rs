//! Application shell: the Tauri 2 app that wires core and AI together.

pub mod ipc;

use std::sync::Arc;

use gitcroc_core::{GitSetup, JobIds, PlatformConfigDir, SettingsStore, Workspaces};

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
        // Small files too, read once so the first paint has the workspace.
        .manage(Arc::new(Workspaces::load(&PlatformConfigDir)))
        .setup(move |app| {
            ipc.mount_events(app);
            ipc::spawn_git_detection(app.handle().clone());
            Ok(())
        })
        .run(tauri::generate_context!())?;
    Ok(())
}
