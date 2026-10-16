//! Codex's non-interactive mode, `codex exec`, with the prompt on stdin
//! (section 7 "Providers", AI-2).

use std::path::{Path, PathBuf};

use crate::cli::{self, Places, SelfTestError};
use crate::http::{cut, tokens, usage};
use crate::provider::{Provider, ProviderError, ProviderFuture, ProviderReply, ProviderRequest};

const PROGRAM: &str = "Codex";

/// The flags before the features, checked on codex-cli 0.163.0: a
/// read-only sandbox that asks for no approval, no MCP servers and no web
/// search. The values are plain words, not TOML strings, so that npm's
/// `.cmd` launcher on Windows passes them on without quotes; Codex takes a
/// value that is not TOML as a string, and checks it.
const SANDBOX: [&str; 9] = [
    "exec",
    "--sandbox",
    "read-only",
    "--config",
    "approval_policy=never",
    "--config",
    "mcp_servers={}",
    "--config",
    "web_search=disabled",
];

/// Every feature of codex-cli 0.163.0 that gives the model a tool, turned
/// off: Codex has no switch for all of them. Its sub-agent tools remain;
/// they run under the same sandbox. Codex refuses a name it does not know,
/// so the self-test checks them in `codex features list`.
const DISABLED_FEATURES: [&str; 18] = [
    "apps",
    "browser_use",
    "browser_use_external",
    "code_mode_host",
    "computer_use",
    "goals",
    "hooks",
    "image_generation",
    "in_app_browser",
    "multi_agent",
    "plugins",
    "remote_plugin",
    "shell_tool",
    "skill_search",
    "sleep_tool",
    "tool_suggest",
    "unified_exec",
    "view_image",
];

/// The flags after the features: outside a git repo, no session saved to
/// disk (AI-7), and the events as JSON on stdout, from which the reply and
/// the usage are read, so the reply is never written to a file (AI-7).
const RUN: [&str; 5] = [
    "--skip-git-repo-check",
    "--ephemeral",
    "--color",
    "never",
    "--json",
];

/// The flags the self-test looks for in `codex exec --help`.
const LISTED: [&str; 8] = [
    "--sandbox",
    "--config",
    "--disable",
    "--skip-git-repo-check",
    "--ephemeral",
    "--color",
    "--json",
    "--model",
];

/// An installed Codex, with its own login.
#[derive(Debug, Clone)]
pub struct CodexProvider {
    path: PathBuf,
    model: Option<String>,
}

impl CodexProvider {
    /// The CLI at `path`, using `model` or, without one, its own default.
    pub fn new(path: impl Into<PathBuf>, model: Option<String>) -> Self {
        Self {
            path: path.into(),
            model: model.filter(|model| !model.trim().is_empty()),
        }
    }

    /// Every `codex` on `PATH` and in the usual install folders.
    pub fn detect() -> Vec<PathBuf> {
        cli::find("codex", &Places::from_env())
    }

    /// Checks that the CLI at `path` runs, lists every flag this provider
    /// uses and has every feature it turns off; returns its version (AI-2).
    pub async fn self_test(path: &Path) -> Result<String, SelfTestError> {
        let version = cli::version(PROGRAM, path).await?;
        let help = cli::query(PROGRAM, path, &["exec", "--help"]).await?;
        let features = cli::query(PROGRAM, path, &["features", "list"]).await?;
        let known: Vec<&str> = features
            .lines()
            .filter_map(|line| line.split_whitespace().next())
            .collect();
        let mut missing = cli::missing_flags(&help, &LISTED);
        missing.extend(
            DISABLED_FEATURES
                .iter()
                .filter(|feature| !known.contains(feature))
                .map(|feature| (*feature).to_owned()),
        );
        cli::verdict(version, missing)
    }

    fn args(&self) -> Vec<String> {
        let mut args: Vec<&str> = SANDBOX.to_vec();
        for feature in DISABLED_FEATURES {
            args.extend(["--disable", feature]);
        }
        args.extend(RUN);
        if let Some(model) = &self.model {
            args.extend(["--model", model]);
        }
        // The prompt comes on stdin.
        args.push("-");
        args.into_iter().map(str::to_owned).collect()
    }

    async fn send(&self, request: &ProviderRequest) -> Result<ProviderReply, ProviderError> {
        let finished = cli::run(
            PROGRAM,
            &self.path,
            &self.args(),
            &cli::prompt_text(request),
        )
        .await?;
        let mut events = Events::default();
        let mut unreadable = false;
        for line in finished
            .stdout
            .lines()
            .filter(|line| !line.trim().is_empty())
        {
            match serde_json::from_str(line) {
                Ok(event) => events.apply(&event),
                Err(_) => unreadable = true,
            }
        }
        if !finished.status.success() {
            let reason = events.failure.as_deref().or(events.error.as_deref());
            return Err(cli::exit_error(PROGRAM, &finished, reason));
        }
        if unreadable {
            return Err(cli::malformed(
                PROGRAM,
                "printed something other than its JSON events",
                &finished,
            ));
        }
        if let Some(failure) = events.failure {
            return Err(ProviderError::Failed(cut(&failure)));
        }
        let Some(text) = events.message else {
            return Err(cli::malformed(PROGRAM, "ended without a reply", &finished));
        };
        Ok(ProviderReply {
            text,
            usage: events.usage,
        })
    }
}

impl Provider for CodexProvider {
    fn complete<'a>(&'a self, request: &'a ProviderRequest) -> ProviderFuture<'a> {
        Box::pin(self.send(request))
    }
}

/// What the events of a run said.
#[derive(Debug, Default)]
struct Events {
    /// The last agent message: the reply. Earlier ones are progress.
    message: Option<String>,
    usage: Option<crate::answer::Usage>,
    /// Why the turn failed.
    failure: Option<String>,
    /// The last error event, which may be a passing one such as a
    /// reconnect.
    error: Option<String>,
}

impl Events {
    fn apply(&mut self, event: &serde_json::Value) {
        let text = |pointer: &str| {
            event
                .pointer(pointer)
                .and_then(serde_json::Value::as_str)
                .map(str::to_owned)
        };
        match event.get("type").and_then(serde_json::Value::as_str) {
            Some("item.completed") if text("/item/type").as_deref() == Some("agent_message") => {
                if let Some(message) = text("/item/text") {
                    self.message = Some(message);
                }
            }
            Some("turn.completed") => {
                // Input tokens include the cached ones.
                let counts = event.get("usage");
                self.usage = usage(
                    tokens(counts.and_then(|counts| counts.get("input_tokens"))),
                    tokens(counts.and_then(|counts| counts.get("output_tokens"))),
                );
            }
            Some("turn.failed") => {
                self.failure = text("/error/message").or(Some("the turn failed".to_owned()))
            }
            Some("error") => self.error = text("/message"),
            _ => {}
        }
    }
}
