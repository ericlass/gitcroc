//! AI providers, prompt building and response validation.

mod answer;
mod ask;
#[cfg(feature = "mock")]
mod mock;
mod provider;
mod validate;

pub use answer::{Confidence, FileAnswer, HunkAnswer, HunkStatus, Usage};
pub use ask::{FileOutcome, FileResult, ask};
#[cfg(feature = "mock")]
pub use mock::{MockGate, MockProvider};
pub use provider::{
    Message, Provider, ProviderError, ProviderFuture, ProviderReply, ProviderRequest, Role,
};
pub use validate::{Invalid, Problem, first_marker_line, validate};

/// Errors returned by `gitcroc-ai`.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
}
