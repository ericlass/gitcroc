//! Claude Code's headless mode, `claude -p --output-format json`, with the
//! prompt on stdin (section 7 "Providers", AI-2).

use std::path::{Path, PathBuf};

use crate::anthropic::input_tokens;
use crate::cli::{self, Places, SelfTestError};
use crate::http::{cut, tokens, usage};
use crate::provider::{Provider, ProviderError, ProviderFuture, ProviderReply, ProviderRequest};

const PROGRAM: &str = "Claude Code";

/// The flags of every call, checked on Claude Code 2.1.301: print mode with
/// the JSON envelope, no tools at all, no user, project or local settings
/// files, no MCP servers, no skills or slash commands, and no session saved
/// to disk (AI-7). The login is not a settings file, so it still works.
const ARGS: [&str; 10] = [
    "-p",
    "--output-format",
    "json",
    "--tools",
    "",
    "--setting-sources",
    "",
    "--strict-mcp-config",
    "--disable-slash-commands",
    "--no-session-persistence",
];

/// The flags the self-test looks for in `--help`.
const LISTED: [&str; 8] = [
    "-p",
    "--output-format",
    "--tools",
    "--setting-sources",
    "--strict-mcp-config",
    "--disable-slash-commands",
    "--no-session-persistence",
    "--model",
];

/// An installed Claude Code, with its own login.
#[derive(Debug, Clone)]
pub struct ClaudeCodeProvider {
    path: PathBuf,
    model: Option<String>,
}

impl ClaudeCodeProvider {
    /// The CLI at `path`, using `model` or, without one, its own default.
    pub fn new(path: impl Into<PathBuf>, model: Option<String>) -> Self {
        Self {
            path: path.into(),
            model: model.filter(|model| !model.trim().is_empty()),
        }
    }

    /// Every `claude` on `PATH` and in the usual install folders.
    pub fn detect() -> Vec<PathBuf> {
        cli::find("claude", &Places::from_env())
    }

    /// Checks that the CLI at `path` runs and lists every flag this
    /// provider uses; returns its version (AI-2).
    pub async fn self_test(path: &Path) -> Result<String, SelfTestError> {
        let version = cli::version(PROGRAM, path).await?;
        let help = cli::query(PROGRAM, path, &["--help"]).await?;
        cli::verdict(version, cli::missing_flags(&help, &LISTED))
    }

    fn args(&self) -> Vec<String> {
        let mut args: Vec<String> = ARGS.iter().map(|arg| (*arg).to_owned()).collect();
        if let Some(model) = &self.model {
            args.extend(["--model".to_owned(), model.clone()]);
        }
        args
    }

    async fn send(&self, request: &ProviderRequest) -> Result<ProviderReply, ProviderError> {
        let finished = cli::run(
            PROGRAM,
            &self.path,
            &self.args(),
            &cli::prompt_text(request),
        )
        .await?;
        let envelope = serde_json::from_str::<serde_json::Value>(finished.stdout.trim())
            .ok()
            .filter(|value| {
                value.get("type").and_then(serde_json::Value::as_str) == Some("result")
            });
        let failed = envelope
            .as_ref()
            .filter(|envelope| {
                envelope
                    .get("is_error")
                    .and_then(serde_json::Value::as_bool)
                    == Some(true)
            })
            .map(|envelope| {
                ["result", "subtype"]
                    .iter()
                    .find_map(|name| envelope.get(*name).and_then(serde_json::Value::as_str))
                    .unwrap_or("an error")
                    .to_owned()
            });
        if !finished.status.success() {
            return Err(cli::exit_error(PROGRAM, &finished, failed.as_deref()));
        }
        let Some(envelope) = envelope else {
            return Err(cli::malformed(
                PROGRAM,
                "did not print its JSON result",
                &finished,
            ));
        };
        if let Some(reason) = failed {
            return Err(ProviderError::Failed(cut(&reason)));
        }
        let Some(text) = envelope.get("result").and_then(serde_json::Value::as_str) else {
            return Err(cli::malformed(
                PROGRAM,
                "printed a result without its text",
                &finished,
            ));
        };
        let counts = envelope.get("usage");
        Ok(ProviderReply {
            text: text.to_owned(),
            usage: usage(
                counts.and_then(input_tokens),
                tokens(counts.and_then(|counts| counts.get("output_tokens"))),
            ),
        })
    }
}

impl Provider for ClaudeCodeProvider {
    fn complete<'a>(&'a self, request: &'a ProviderRequest) -> ProviderFuture<'a> {
        Box::pin(self.send(request))
    }
}
