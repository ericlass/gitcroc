//! AI providers, prompt building and response validation.

/// Errors returned by `gitcroc-ai`.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
}
