//! Application shell: wires core and AI together. Becomes the Tauri crate in M0-04.

/// Errors returned by `gitcroc-app`.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
}
