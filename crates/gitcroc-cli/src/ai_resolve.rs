//! `gitcroc-cli conflict ai-resolve`: one conflicted file resolved with AI
//! by the core's proposal job, outside any workspace, after the same
//! checks as the app (M4-08), and the proposal printed as JSON. Nothing is
//! written to the repo (D9). The provider is the default one of
//! `settings.toml`, or `--mock` replies in its place; a key is read only
//! from the environment variable `--key-env` names (SEC-1). With AI debug
//! logging on, prompts and replies go to stderr (AI-7).

use std::path::{Path, PathBuf};
use std::sync::Arc;

use gitcroc_ai::{
    AnthropicProvider, ApiKey, Logged, MockProvider, Provider, ProviderSpec, connect,
};
use gitcroc_core::{
    AiResolveOnce, ConsentRequest, PlatformConfigDir, Proposer, ProviderAccess, ProviderEntry,
    ProviderKind, SettingsStore, ai_resolve_once, consent_request,
};

use crate::proposer::AiProposer;
use crate::workspace::{GivenConfigDir, runtime_and_git};

/// What `ai-resolve` reads besides the file.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ResolveOptions {
    /// The hunks to send, numbered from 1; all when `None`.
    pub hunks: Option<Vec<u32>>,
    /// The platform config dir when `None`.
    pub config_dir: Option<PathBuf>,
    /// A JSON array of the replies the mock gives, in order, in place of
    /// the provider's.
    pub mock: Option<PathBuf>,
    /// The environment variable holding the key.
    pub key_env: Option<String>,
}

/// The CLI's side of the provider checks: no self-test is known, the
/// consent is the one in `settings.toml`, and the key comes from the
/// environment.
struct CliAccess {
    /// The mock's replies, when it stands in for the provider.
    mock: Option<Vec<String>>,
    key_env: Option<String>,
}

/// The key in the environment variable `key_env`, for an API provider
/// (SEC-1). A CLI provider uses its own login.
pub(crate) fn key_from(key_env: Option<&str>, entry: &ProviderEntry) -> Option<ApiKey> {
    key_env
        .and_then(std::env::var_os)
        .and_then(|key| key.into_string().ok())
        .filter(|key| !key.is_empty())
        .filter(|_| !entry.kind.is_cli())
        .map(ApiKey::new)
}

/// The provider `entry` with `key`, as `gitcroc-ai` takes it.
pub(crate) fn spec(entry: &ProviderEntry, key: Option<ApiKey>) -> Result<ProviderSpec, String> {
    let model = entry.model.clone();
    let path = || {
        entry
            .path
            .clone()
            .ok_or_else(|| format!("a {} provider needs the path of its program", entry.kind))
    };
    Ok(match entry.kind {
        ProviderKind::Anthropic => ProviderSpec::Anthropic {
            key: key.ok_or_else(|| {
                "an Anthropic provider needs an API key: name its variable with --key-env"
                    .to_owned()
            })?,
            model: model.unwrap_or_default(),
        },
        ProviderKind::Openai => ProviderSpec::OpenAi {
            base_url: entry.base_url.clone().unwrap_or_default(),
            key,
            model: model.unwrap_or_default(),
        },
        ProviderKind::ClaudeCode => ProviderSpec::ClaudeCode {
            path: path()?,
            model,
        },
        ProviderKind::Codex => ProviderSpec::Codex {
            path: path()?,
            model,
        },
    })
}

impl ProviderAccess for CliAccess {
    fn problem(&self, _entry: &ProviderEntry) -> Option<String> {
        None
    }

    fn consent_needed(&self, entry: &ProviderEntry) -> Option<ConsentRequest> {
        consent_request(entry, AnthropicProvider::DEFAULT_BASE_URL)
    }

    fn proposer(
        &self,
        entry: &ProviderEntry,
        debug_log: bool,
    ) -> Result<Arc<dyn Proposer>, String> {
        let provider: Box<dyn Provider> = match &self.mock {
            Some(replies) => Box::new(MockProvider::texts(replies.clone())),
            None => {
                let key = key_from(self.key_env.as_deref(), entry);
                connect(spec(entry, key)?).map_err(|error| error.to_string())?
            }
        };
        let provider: Box<dyn Provider> = if debug_log {
            Box::new(Logged::new(
                provider,
                Arc::new(|text: &str| eprintln!("{text}")),
            ))
        } else {
            provider
        };
        Ok(Arc::new(AiProposer::new(provider, entry.context_budget)))
    }
}

/// The replies of the mock file: a JSON array of strings.
pub(crate) fn mock_replies(file: &Path) -> Result<Vec<String>, String> {
    let text = std::fs::read_to_string(file)
        .map_err(|error| format!("cannot read {}: {error}", file.display()))?;
    serde_json::from_str(&text)
        .map_err(|error| format!("{} is not a JSON array of replies: {error}", file.display()))
}

/// How a resolution ended, as JSON.
pub enum Resolved {
    /// The proposal.
    Proposal(String),
    /// Refused or "AI failed": the reason, as JSON.
    NotProposed(String),
    /// The file is not in conflict.
    NotInConflict,
}

/// Resolves `git_path` of `repo` with AI once and returns the outcome as
/// JSON.
pub fn resolve(
    repo: &Path,
    git_path: Vec<u8>,
    options: &ResolveOptions,
) -> Result<Resolved, String> {
    let settings = match &options.config_dir {
        Some(dir) => SettingsStore::load(&GivenConfigDir(dir.clone())),
        None => SettingsStore::load(&PlatformConfigDir),
    };
    let mock = options.mock.as_deref().map(mock_replies).transpose()?;
    let access: Arc<dyn ProviderAccess> = Arc::new(CliAccess {
        mock,
        key_env: options.key_env.clone(),
    });
    let (runtime, git) = runtime_and_git()?;
    let once = runtime
        .block_on(ai_resolve_once(
            &settings,
            access,
            git,
            repo.to_owned(),
            git_path,
            options.hunks.clone(),
        ))
        .map_err(|error| error.to_string())?;
    let json = |once: &AiResolveOnce| {
        serde_json::to_string_pretty(once)
            .map_err(|error| format!("cannot write the result as JSON: {error}"))
    };
    Ok(match &once {
        AiResolveOnce::Proposal { .. } => Resolved::Proposal(json(&once)?),
        AiResolveOnce::Failed { .. } | AiResolveOnce::Refused { .. } => {
            Resolved::NotProposed(json(&once)?)
        }
        AiResolveOnce::AlreadyResolved => Resolved::NotInConflict,
    })
}
