//! Application shell: the Tauri 2 app that wires core and AI together.

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
    tauri::Builder::default().run(tauri::generate_context!())?;
    Ok(())
}
