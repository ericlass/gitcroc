//! AI providers, prompt building and response validation.

mod answer;
mod anthropic;
mod ask;
mod claude_code;
mod cli;
mod codex;
mod http;
#[cfg(feature = "mock")]
mod mock;
mod openai;
mod prompt;
mod provider;
mod resolve;
mod validate;

pub use answer::{Confidence, FileAnswer, HunkAnswer, HunkStatus, Usage};
pub use anthropic::{ANTHROPIC_MAX_TOKENS, AnthropicProvider};
pub use ask::{FileOutcome, FileResult, ask};
pub use claude_code::ClaudeCodeProvider;
pub use cli::SelfTestError;
pub use codex::CodexProvider;
pub use http::{ApiKey, ModelList, REQUEST_TIMEOUT};
#[cfg(feature = "mock")]
pub use mock::{MockGate, MockProvider};
pub use openai::OpenAiProvider;
pub use prompt::{FileRequest, HunkRequest, Operation, PROMPT_VERSION, estimate_tokens, prompt};
pub use provider::{
    Message, Provider, ProviderError, ProviderFuture, ProviderReply, ProviderRequest, Role,
};
pub use resolve::resolve_file;
pub use validate::{Invalid, Problem, first_marker_line, validate};

/// Errors returned by `gitcroc-ai`.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
}
