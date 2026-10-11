//! Git access: read API (gix), git CLI runner, output parsers and conflict state detection.

/// Errors returned by `gitcroc-git`.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
}
