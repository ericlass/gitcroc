//! A provider from what the settings say of it (D4): the app and the CLI
//! each read their settings and keys, and get the same provider here.

use std::path::PathBuf;

use crate::anthropic::AnthropicProvider;
use crate::claude_code::ClaudeCodeProvider;
use crate::codex::CodexProvider;
use crate::http::ApiKey;
use crate::openai::OpenAiProvider;
use crate::provider::{Provider, ProviderError};

/// A configured provider, with its key where it takes one (SEC-1).
#[derive(Debug, Clone)]
pub enum ProviderSpec {
    Anthropic {
        key: ApiKey,
        model: String,
    },
    OpenAi {
        base_url: String,
        key: Option<ApiKey>,
        model: String,
    },
    ClaudeCode {
        path: PathBuf,
        model: Option<String>,
    },
    Codex {
        path: PathBuf,
        model: Option<String>,
    },
}

/// The provider `spec` describes. Fails only when the HTTP client cannot
/// be built.
pub fn connect(spec: ProviderSpec) -> Result<Box<dyn Provider>, ProviderError> {
    Ok(match spec {
        ProviderSpec::Anthropic { key, model } => Box::new(AnthropicProvider::new(key, model)?),
        ProviderSpec::OpenAi {
            base_url,
            key,
            model,
        } => Box::new(OpenAiProvider::new(base_url, key, model)?),
        ProviderSpec::ClaudeCode { path, model } => Box::new(ClaudeCodeProvider::new(path, model)),
        ProviderSpec::Codex { path, model } => Box::new(CodexProvider::new(path, model)),
    })
}
