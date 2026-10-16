//! The AI settings: the optional `[ai]` table of `settings.toml`, with the
//! configured providers (AI-1), the AI features switch (AI-16), the exclude
//! list (AI-4) and the debug log switch (AI-7). Keys are never here: they
//! live in the OS keychain (SEC-1, D11), which the shell owns.

use std::fmt;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

/// A provider's context budget until the user sets one, in tokens (AI-10).
pub const DEFAULT_CONTEXT_BUDGET: u32 = 100_000;
/// The smallest context budget, in tokens.
pub const MIN_CONTEXT_BUDGET: u32 = 1_000;
/// The largest context budget, in tokens.
pub const MAX_CONTEXT_BUDGET: u32 = 10_000_000;

/// The exclude list until the user edits it (AI-4).
const DEFAULT_EXCLUDE: [&str; 5] = [".env*", "*.pem", "*.key", "*.p12", "id_rsa*"];

/// Identifies a provider: `p1`, `p2` and so on.
#[derive(
    Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, specta::Type,
)]
pub struct ProviderId(pub String);

impl ProviderId {
    fn number(&self) -> Option<u64> {
        self.0.strip_prefix('p')?.parse().ok()
    }
}

impl fmt::Display for ProviderId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// How a provider is called (D4, section 7 "Providers").
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "snake_case")]
pub enum ProviderKind {
    Anthropic,
    /// OpenAI-compatible: OpenAI, OpenRouter, Ollama, LM Studio and similar.
    Openai,
    ClaudeCode,
    Codex,
}

impl ProviderKind {
    /// Whether it runs an installed program rather than calling an API.
    pub fn is_cli(self) -> bool {
        matches!(self, Self::ClaudeCode | Self::Codex)
    }
}

impl fmt::Display for ProviderKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Anthropic => "Anthropic",
            Self::Openai => "OpenAI-compatible",
            Self::ClaudeCode => "Claude Code",
            Self::Codex => "Codex",
        })
    }
}

/// One `[[ai.provider]]`. The IPC shows it as the shell's own view, without
/// the file's defaults.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProviderEntry {
    pub id: ProviderId,
    pub name: String,
    pub kind: ProviderKind,
    /// Required by the API kinds; the CLI kinds use their own default
    /// without one. Never hardcoded (AI-1).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    /// The API's address, with its path; OpenAI-compatible only.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub base_url: Option<String>,
    /// The program; CLI kinds only.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<PathBuf>,
    /// In tokens, [`MIN_CONTEXT_BUDGET`] to [`MAX_CONTEXT_BUDGET`] (AI-10).
    #[serde(default = "default_context_budget")]
    pub context_budget: u32,
    /// The host or program the user consented to sending to (AI-6).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub consent: Option<String>,
}

fn default_context_budget() -> u32 {
    DEFAULT_CONTEXT_BUDGET
}

fn default_exclude() -> Vec<String> {
    DEFAULT_EXCLUDE.map(str::to_owned).to_vec()
}

fn enabled_by_default() -> bool {
    true
}

/// The `[ai]` table. Missing keys take their default; unknown keys are
/// ignored, as in the workspace file's tables.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AiSettings {
    /// AI features (AI-16). Off, no AI request is sent and no CLI provider
    /// is detected or run; the providers stay as they are.
    #[serde(default = "enabled_by_default")]
    pub enabled: bool,
    /// The provider used unless the user picks another (AI-1).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default: Option<ProviderId>,
    /// File patterns never sent (AI-4).
    #[serde(default = "default_exclude")]
    pub exclude: Vec<String>,
    /// Prompts and replies go to the log (AI-7).
    #[serde(default)]
    pub debug_log: bool,
    #[serde(default, rename = "provider", skip_serializing_if = "Vec::is_empty")]
    pub providers: Vec<ProviderEntry>,
}

impl Default for AiSettings {
    fn default() -> Self {
        Self {
            enabled: true,
            default: None,
            exclude: default_exclude(),
            debug_log: false,
            providers: Vec::new(),
        }
    }
}

/// A provider as the user edits it. Without an id it is a new one. The
/// consent is not part of it: saving keeps the stored one.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
pub struct ProviderDraft {
    pub id: Option<ProviderId>,
    pub name: String,
    pub kind: ProviderKind,
    pub model: Option<String>,
    pub base_url: Option<String>,
    pub path: Option<PathBuf>,
    /// `None` takes [`DEFAULT_CONTEXT_BUDGET`].
    pub context_budget: Option<u32>,
}

/// The AI settings other than the providers.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
pub struct AiOptions {
    pub enabled: bool,
    pub exclude: Vec<String>,
    pub debug_log: bool,
}

/// Why a provider entry or the `[ai]` table was refused.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum AiSettingsError {
    #[error("a provider needs a name")]
    NoName,
    #[error("an {0} provider needs a model")]
    NoModel(ProviderKind),
    #[error("an OpenAI-compatible provider needs a base URL")]
    NoBaseUrl,
    #[error("the base URL must start with http:// or https:// and name a host")]
    BaseUrl,
    #[error("a {0} provider needs the path of its program")]
    NoPath(ProviderKind),
    #[error("{kind} providers take no {field}")]
    NotForKind {
        field: &'static str,
        kind: ProviderKind,
    },
    #[error("the context budget is {MIN_CONTEXT_BUDGET} to {MAX_CONTEXT_BUDGET} tokens")]
    Budget,
    #[error("there is no provider {0}")]
    UnknownProvider(ProviderId),
    #[error("two providers have the id {0}")]
    DuplicateId(ProviderId),
    #[error("a provider has an empty id")]
    EmptyId,
}

/// `text` trimmed, or `None` when that leaves nothing.
fn filled(text: Option<String>) -> Option<String> {
    text.map(|text| text.trim().to_owned())
        .filter(|text| !text.is_empty())
}

impl ProviderDraft {
    /// Checks what reaching the provider needs, so that its models can be
    /// listed or it can be tested before it is saved: the base URL of an
    /// OpenAI-compatible provider, the path of a CLI one.
    pub fn check_connection(&self) -> Result<(), AiSettingsError> {
        match self.kind {
            ProviderKind::Anthropic => Ok(()),
            ProviderKind::Openai => match filled(self.base_url.clone()) {
                None => Err(AiSettingsError::NoBaseUrl),
                Some(url) if !is_http_url(&url) => Err(AiSettingsError::BaseUrl),
                Some(_) => Ok(()),
            },
            ProviderKind::ClaudeCode | ProviderKind::Codex => {
                if self
                    .path
                    .as_ref()
                    .is_none_or(|path| path.as_os_str().is_empty())
                {
                    Err(AiSettingsError::NoPath(self.kind))
                } else {
                    Ok(())
                }
            }
        }
    }

    /// The entry with `id` and `consent`, its text trimmed, checked.
    pub(crate) fn into_entry(
        self,
        id: ProviderId,
        consent: Option<String>,
    ) -> Result<ProviderEntry, AiSettingsError> {
        let entry = ProviderEntry {
            id,
            name: self.name.trim().to_owned(),
            kind: self.kind,
            model: filled(self.model),
            base_url: filled(self.base_url),
            path: self.path.filter(|path| !path.as_os_str().is_empty()),
            context_budget: self.context_budget.unwrap_or(DEFAULT_CONTEXT_BUDGET),
            consent,
        };
        entry.check()?;
        Ok(entry)
    }
}

impl ProviderEntry {
    /// Refuses an entry its kind cannot use.
    pub fn check(&self) -> Result<(), AiSettingsError> {
        if self.id.0.is_empty() {
            return Err(AiSettingsError::EmptyId);
        }
        if self.name.trim().is_empty() {
            return Err(AiSettingsError::NoName);
        }
        let kind = self.kind;
        let not_for = |field| AiSettingsError::NotForKind { field, kind };
        match kind {
            ProviderKind::Anthropic | ProviderKind::Openai => {
                if self
                    .model
                    .as_deref()
                    .is_none_or(|model| model.trim().is_empty())
                {
                    return Err(AiSettingsError::NoModel(kind));
                }
                if self.path.is_some() {
                    return Err(not_for("path"));
                }
            }
            ProviderKind::ClaudeCode | ProviderKind::Codex => {
                if self
                    .path
                    .as_ref()
                    .is_none_or(|path| path.as_os_str().is_empty())
                {
                    return Err(AiSettingsError::NoPath(kind));
                }
            }
        }
        match (kind, &self.base_url) {
            (ProviderKind::Openai, None) => return Err(AiSettingsError::NoBaseUrl),
            (ProviderKind::Openai, Some(url)) if !is_http_url(url) => {
                return Err(AiSettingsError::BaseUrl);
            }
            (ProviderKind::Openai, Some(_)) | (_, None) => {}
            (_, Some(_)) => return Err(not_for("base_url")),
        }
        if !(MIN_CONTEXT_BUDGET..=MAX_CONTEXT_BUDGET).contains(&self.context_budget) {
            return Err(AiSettingsError::Budget);
        }
        Ok(())
    }
}

/// An http or https URL with a host and no spaces.
fn is_http_url(url: &str) -> bool {
    let lower = url.to_ascii_lowercase();
    let Some(rest) = lower
        .strip_prefix("https://")
        .or_else(|| lower.strip_prefix("http://"))
    else {
        return false;
    };
    let host = rest.split(['/', '?', '#']).next().unwrap_or_default();
    !host.is_empty() && !url.chars().any(char::is_whitespace)
}

impl AiSettings {
    /// Checks every entry, that ids are unique and that the default names
    /// one of them.
    pub(crate) fn check(&self) -> Result<(), AiSettingsError> {
        for (at, entry) in self.providers.iter().enumerate() {
            entry.check()?;
            if self.providers[..at]
                .iter()
                .any(|other| other.id == entry.id)
            {
                return Err(AiSettingsError::DuplicateId(entry.id.clone()));
            }
        }
        match &self.default {
            Some(id) if self.provider(id).is_none() => {
                Err(AiSettingsError::UnknownProvider(id.clone()))
            }
            _ => Ok(()),
        }
    }

    /// The provider with `id`.
    pub fn provider(&self, id: &ProviderId) -> Option<&ProviderEntry> {
        self.providers.iter().find(|entry| &entry.id == id)
    }

    /// The number after the highest id in use.
    pub(crate) fn next_id(&self) -> ProviderId {
        let used = self
            .providers
            .iter()
            .filter_map(|entry| entry.id.number())
            .max()
            .unwrap_or(0);
        ProviderId(format!("p{}", used.saturating_add(1)))
    }

    /// Takes `options`, dropping blank patterns.
    pub(crate) fn set_options(&mut self, options: AiOptions) {
        self.enabled = options.enabled;
        self.exclude = options
            .exclude
            .iter()
            .map(|pattern| pattern.trim())
            .filter(|pattern| !pattern.is_empty())
            .map(str::to_owned)
            .collect();
        self.debug_log = options.debug_log;
    }
}
