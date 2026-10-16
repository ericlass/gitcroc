//! The CLI providers (D4, AI-2) against the fake CLI: where and how they
//! run it, what they send, what they read back, the self-test and cancel.

use std::sync::Arc;

use gitcroc_ai::{
    ClaudeCodeProvider, CodexProvider, Provider, ProviderError, ProviderReply, SelfTestError,
    Usage, validate,
};
use serde_json::json;

use crate::fake_cli::{FakeCli, Hold};
use crate::fake_server::{FakeServer, Reply};
use crate::providers::{MODEL, anthropic, completion, openai, request, retry_request, stream};

/// A valid answer to a request for hunk 1.
const ANSWER: &str = r#"{"hunks":[{"id":1,"status":"resolved","resolution":"both\n","explanation":"Kept both sides.","confidence":"high"}]}"#;

/// Claude Code's `--output-format json` envelope around `result`.
fn envelope(result: &str) -> String {
    json!({
        "type": "result",
        "subtype": "success",
        "is_error": false,
        "result": result,
        "usage": {
            "input_tokens": 2,
            "cache_creation_input_tokens": 100,
            "cache_read_input_tokens": 20,
            "output_tokens": 9,
        },
    })
    .to_string()
}

/// Codex's `--json` events: a progress message, then `message` as the last.
fn events(message: &str) -> String {
    [
        json!({"type": "thread.started", "thread_id": "t1"}),
        json!({"type": "turn.started"}),
        json!({"type": "item.completed", "item": {"id": "item_0", "type": "agent_message", "text": "Looking at the hunk."}}),
        json!({"type": "item.completed", "item": {"id": "item_1", "type": "agent_message", "text": message}}),
        json!({"type": "turn.completed", "usage": {"input_tokens": 300, "cached_input_tokens": 200, "output_tokens": 40, "reasoning_output_tokens": 10}}),
    ]
    .iter()
    .map(|event| format!("{event}\n"))
    .collect()
}

fn claude(script: serde_json::Value) -> FakeCli {
    FakeCli::new("claude", &script)
}

fn codex(script: serde_json::Value) -> FakeCli {
    FakeCli::new("codex", &script)
}

/// The arguments of Claude Code 2.1.301: print mode with the JSON envelope,
/// no tools, no settings files, no MCP servers, no skills, no session file.
fn claude_args(model: Option<&str>) -> Vec<String> {
    let mut args = vec![
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
    if let Some(model) = model {
        args.extend(["--model", model]);
    }
    args.into_iter().map(str::to_owned).collect()
}

/// The arguments of codex-cli 0.163.0: exec mode in a read-only sandbox
/// with no approvals, every tool feature it can turn off turned off, no MCP
/// servers and no web search, outside a git repo, no session file, events
/// as JSON, the prompt on stdin.
fn codex_args(model: Option<&str>) -> Vec<String> {
    let mut args = vec![
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
    for feature in [
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
    ] {
        args.extend(["--disable", feature]);
    }
    args.extend([
        "--skip-git-repo-check",
        "--ephemeral",
        "--color",
        "never",
        "--json",
    ]);
    if let Some(model) = model {
        args.extend(["--model", model]);
    }
    args.push("-");
    args.into_iter().map(str::to_owned).collect()
}

#[tokio::test]
async fn ai_2_cli_runs_in_an_empty_temp_dir_never_the_repo() {
    let claude_cli = claude(json!({"stdout": envelope(ANSWER)}));
    let codex_cli = codex(json!({"stdout": events(ANSWER)}));
    ClaudeCodeProvider::new(claude_cli.path(), None)
        .complete(&request())
        .await
        .unwrap();
    CodexProvider::new(codex_cli.path(), None)
        .complete(&request())
        .await
        .unwrap();

    let here = std::env::current_dir().unwrap();
    for run in [claude_cli.run(), codex_cli.run()] {
        assert!(run.cwd_entries.is_empty(), "{run:?}");
        assert_ne!(run.cwd, here);
        assert!(!run.cwd.starts_with(&here), "{run:?}");
        // Removed once the call is over.
        assert!(!run.cwd.exists(), "{run:?}");
    }
    // Each call has a folder of its own.
    assert_ne!(claude_cli.run().cwd, codex_cli.run().cwd);
}

#[tokio::test]
async fn ai_2_cli_tools_are_disabled() {
    for model in [None, Some(MODEL)] {
        let claude_cli = claude(json!({"stdout": envelope(ANSWER)}));
        let codex_cli = codex(json!({"stdout": events(ANSWER)}));
        ClaudeCodeProvider::new(claude_cli.path(), model.map(str::to_owned))
            .complete(&request())
            .await
            .unwrap();
        CodexProvider::new(codex_cli.path(), model.map(str::to_owned))
            .complete(&request())
            .await
            .unwrap();
        assert_eq!(claude_cli.run().args, claude_args(model));
        assert_eq!(codex_cli.run().args, codex_args(model));
    }
}

#[tokio::test]
async fn ai_2_empty_model_is_no_model() {
    let claude_cli = claude(json!({"stdout": envelope(ANSWER)}));
    ClaudeCodeProvider::new(claude_cli.path(), Some(String::new()))
        .complete(&request())
        .await
        .unwrap();
    assert_eq!(claude_cli.run().args, claude_args(None));
}

#[tokio::test]
async fn ai_2_cli_gets_the_same_prompt_and_returns_the_same_json() {
    let claude_cli = claude(json!({"stdout": envelope(ANSWER)}));
    let codex_cli = codex(json!({"stdout": events(ANSWER)}));
    let server = FakeServer::start([Reply::events(stream(&[ANSWER]))]).await;
    let request = request();
    let api = anthropic(&server).complete(&request).await.unwrap();
    let from_claude = ClaudeCodeProvider::new(claude_cli.path(), None)
        .complete(&request)
        .await
        .unwrap();
    let from_codex = CodexProvider::new(codex_cli.path(), None)
        .complete(&request)
        .await
        .unwrap();

    // One text: the system prompt, then the user message.
    let prompt = format!("{}\n\n{}", request.system, request.user);
    assert_eq!(claude_cli.run().stdin, prompt);
    assert_eq!(codex_cli.run().stdin, prompt);
    let sent = server.requests()[0].json();
    assert_eq!(sent["system"], request.system);
    assert_eq!(sent["messages"][0]["content"], request.user);

    let answer = validate(&api.text, &[1]).unwrap();
    assert_eq!(validate(&from_claude.text, &[1]).unwrap(), answer);
    assert_eq!(validate(&from_codex.text, &[1]).unwrap(), answer);
}

#[tokio::test]
async fn ai_2_cli_retry_carries_the_earlier_reply_and_the_error() {
    let claude_cli = claude(json!({"stdout": envelope(ANSWER)}));
    let request = retry_request();
    ClaudeCodeProvider::new(claude_cli.path(), None)
        .complete(&request)
        .await
        .unwrap();
    let expected = format!(
        "{}\n\n{}\n\n# Your previous reply\n\n{}\n\n# Reply from the user\n\n{}",
        request.system, request.user, request.follow_up[0].text, request.follow_up[1].text
    );
    assert_eq!(claude_cli.run().stdin, expected);
}

#[tokio::test]
async fn ai_14_cli_usage_is_reported_when_the_cli_gives_it() {
    let claude_cli = claude(json!({"stdout": envelope(ANSWER)}));
    let codex_cli = codex(json!({"stdout": events(ANSWER)}));
    let from_claude = ClaudeCodeProvider::new(claude_cli.path(), None)
        .complete(&request())
        .await
        .unwrap();
    // Input tokens include those read from or written to the prompt cache.
    assert_eq!(
        from_claude.usage,
        Some(Usage {
            input_tokens: 122,
            output_tokens: 9
        })
    );
    let from_codex = CodexProvider::new(codex_cli.path(), None)
        .complete(&request())
        .await
        .unwrap();
    // Codex's input tokens already include the cached ones.
    assert_eq!(
        from_codex.usage,
        Some(Usage {
            input_tokens: 300,
            output_tokens: 40
        })
    );

    // Without usage, there is none to show.
    let bare = json!({"type": "result", "subtype": "success", "is_error": false, "result": ANSWER});
    let claude_cli = claude(json!({"stdout": bare.to_string()}));
    let reply = ClaudeCodeProvider::new(claude_cli.path(), None)
        .complete(&request())
        .await
        .unwrap();
    assert_eq!(
        reply,
        ProviderReply {
            text: ANSWER.to_owned(),
            usage: None
        }
    );
    let item = json!({"type": "item.completed", "item": {"id": "item_0", "type": "agent_message", "text": ANSWER}});
    let codex_cli = codex(json!({"stdout": format!("{item}\n")}));
    let reply = CodexProvider::new(codex_cli.path(), None)
        .complete(&request())
        .await
        .unwrap();
    assert_eq!(reply.usage, None);
}

/// The help texts of the installed versions, cut to the flags used.
fn claude_help(without: &str) -> String {
    let lines = [
        "  --disable-slash-commands              Disable all skills",
        "  --model <model>                       Model for the current session.",
        "  --no-session-persistence              Disable session persistence",
        "  --output-format <format>              Output format: text, json or stream-json",
        "  -p, --print                           Print response and exit",
        "  --setting-sources <sources>           Comma-separated list of setting sources",
        "  --strict-mcp-config                   Only use MCP servers from --mcp-config",
        "  --tools <tools...>                    Specify the list of available tools",
    ];
    let kept: Vec<&str> = lines
        .into_iter()
        .filter(|line| without.is_empty() || !line.contains(without))
        .collect();
    format!(
        "Usage: claude [options] [command] [prompt]\n\nOptions:\n{}\n",
        kept.join("\n")
    )
}

fn codex_exec_help(without: &str) -> String {
    let lines = [
        "  -c, --config <key=value>",
        "      --disable <FEATURE>",
        "  -m, --model <MODEL>",
        "  -s, --sandbox <SANDBOX_MODE>",
        "      --skip-git-repo-check",
        "      --ephemeral",
        "      --color <COLOR>",
        "      --json",
    ];
    let kept: Vec<&str> = lines
        .into_iter()
        .filter(|line| without.is_empty() || !line.contains(without))
        .collect();
    format!(
        "Run Codex non-interactively\n\nOptions:\n{}\n",
        kept.join("\n")
    )
}

fn codex_features(without: &str) -> String {
    codex_args(None)
        .windows(2)
        .filter(|pair| pair[0] == "--disable" && pair[1] != without)
        .map(|pair| format!("{:<40} stable             true\n", pair[1]))
        .collect()
}

#[tokio::test]
async fn ai_2_self_test_returns_the_version() {
    let claude_cli = claude(json!({"version": "2.1.301 (Claude Code)\n", "help": claude_help("")}));
    assert_eq!(
        ClaudeCodeProvider::self_test(claude_cli.path()).await,
        Ok("2.1.301 (Claude Code)".to_owned())
    );
    let codex_cli = codex(json!({
        "version": "codex-cli 0.163.0\n",
        "exec_help": codex_exec_help(""),
        "features": codex_features(""),
    }));
    assert_eq!(
        CodexProvider::self_test(codex_cli.path()).await,
        Ok("codex-cli 0.163.0".to_owned())
    );
    // The self-test runs nothing but its queries.
    assert!(claude_cli.runs().is_empty());
    assert!(codex_cli.runs().is_empty());
}

#[tokio::test]
async fn ai_2_self_test_reports_missing_flags() {
    let claude_cli =
        claude(json!({"version": "9.0.0 (Claude Code)\n", "help": claude_help("--tools")}));
    assert_eq!(
        ClaudeCodeProvider::self_test(claude_cli.path()).await,
        Err(SelfTestError::MissingFlags {
            version: "9.0.0 (Claude Code)".to_owned(),
            missing: vec!["--tools".to_owned()],
        })
    );
    // `--print` alone does not stand for `-p`, nor `--tools` for `--toolsets`.
    let help = claude_help("--print").replace("--tools <", "--toolsets <")
        + "  --print-all                           Something else\n";
    let claude_cli = claude(json!({"version": "9.0.0 (Claude Code)\n", "help": help}));
    assert_eq!(
        ClaudeCodeProvider::self_test(claude_cli.path()).await,
        Err(SelfTestError::MissingFlags {
            version: "9.0.0 (Claude Code)".to_owned(),
            missing: vec!["-p".to_owned(), "--tools".to_owned()],
        })
    );

    // A flag of exec, and a feature the provider turns off.
    let codex_cli = codex(json!({
        "version": "codex-cli 9.0.0\n",
        "exec_help": codex_exec_help("--ephemeral"),
        "features": codex_features("shell_tool"),
    }));
    assert_eq!(
        CodexProvider::self_test(codex_cli.path()).await,
        Err(SelfTestError::MissingFlags {
            version: "codex-cli 9.0.0".to_owned(),
            missing: vec!["--ephemeral".to_owned(), "shell_tool".to_owned()],
        })
    );
}

#[tokio::test]
async fn ai_2_self_test_of_a_missing_program_fails() {
    let folder = tempfile::tempdir().unwrap();
    let missing = folder.path().join("claude");
    let error = ClaudeCodeProvider::self_test(&missing).await.unwrap_err();
    assert!(
        matches!(&error, SelfTestError::Run(ProviderError::Start { .. })),
        "{error:?}"
    );
}

#[tokio::test]
async fn d4_cli_nonzero_exit_carries_stderr() {
    let claude_cli = claude(json!({"stderr": "Not logged in\n", "exit": 2}));
    let error = ClaudeCodeProvider::new(claude_cli.path(), None)
        .complete(&request())
        .await
        .unwrap_err();
    assert_eq!(
        error,
        ProviderError::Exit {
            program: "Claude Code".to_owned(),
            code: Some(2),
            stderr: "Not logged in".to_owned(),
        }
    );

    // Claude Code puts the reason in its envelope.
    let failed = json!({"type": "result", "subtype": "success", "is_error": true, "result": "There's an issue with the selected model."});
    let claude_cli = claude(
        json!({"stdout": failed.to_string(), "stderr": "[unrecognized_model]\n", "exit": 1}),
    );
    let error = ClaudeCodeProvider::new(claude_cli.path(), Some("no-such-model".to_owned()))
        .complete(&request())
        .await
        .unwrap_err();
    assert_eq!(
        error,
        ProviderError::Exit {
            program: "Claude Code".to_owned(),
            code: Some(1),
            stderr: "[unrecognized_model]\nThere's an issue with the selected model.".to_owned(),
        }
    );

    let turn_failed = json!({"type": "turn.failed", "error": {"message": "stream disconnected"}});
    let codex_cli = codex(
        json!({"stdout": format!("{turn_failed}\n"), "stderr": "Error: turn failed\n", "exit": 1}),
    );
    let error = CodexProvider::new(codex_cli.path(), None)
        .complete(&request())
        .await
        .unwrap_err();
    assert_eq!(
        error,
        ProviderError::Exit {
            program: "Codex".to_owned(),
            code: Some(1),
            stderr: "Error: turn failed\nstream disconnected".to_owned(),
        }
    );
}

#[tokio::test]
async fn d4_cli_reported_errors_fail_the_request() {
    let failed = json!({"type": "result", "subtype": "error_max_turns", "is_error": true, "result": "Reached the turn limit"});
    let claude_cli = claude(json!({"stdout": failed.to_string()}));
    assert_eq!(
        ClaudeCodeProvider::new(claude_cli.path(), None)
            .complete(&request())
            .await,
        Err(ProviderError::Failed("Reached the turn limit".to_owned()))
    );

    let error = json!({"type": "error", "message": "Reconnecting... 1/5"});
    let turn_failed = json!({"type": "turn.failed", "error": {"message": "usage limit reached"}});
    let codex_cli = codex(json!({"stdout": format!("{error}\n{turn_failed}\n")}));
    assert_eq!(
        CodexProvider::new(codex_cli.path(), None)
            .complete(&request())
            .await,
        Err(ProviderError::Failed("usage limit reached".to_owned()))
    );
}

#[tokio::test]
async fn d4_cli_reply_that_is_not_the_envelope_is_an_error() {
    let claude_cli = claude(json!({"stdout": ANSWER, "stderr": "a warning\n"}));
    let error = ClaudeCodeProvider::new(claude_cli.path(), None)
        .complete(&request())
        .await
        .unwrap_err();
    let ProviderError::Malformed(message) = &error else {
        panic!("{error:?}");
    };
    assert!(message.contains("a warning"), "{message}");

    let codex_cli = codex(json!({"stdout": "not json\n", "stderr": "a warning\n"}));
    let error = CodexProvider::new(codex_cli.path(), None)
        .complete(&request())
        .await
        .unwrap_err();
    let ProviderError::Malformed(message) = &error else {
        panic!("{error:?}");
    };
    assert!(message.contains("a warning"), "{message}");

    // Events, but no message.
    let codex_cli = codex(
        json!({"stdout": format!("{}\n", json!({"type": "turn.completed", "usage": {"input_tokens": 1, "output_tokens": 0}}))}),
    );
    let error = CodexProvider::new(codex_cli.path(), None)
        .complete(&request())
        .await
        .unwrap_err();
    assert!(matches!(error, ProviderError::Malformed(_)), "{error:?}");
}

#[tokio::test]
async fn d4_cli_that_cannot_start_is_an_error() {
    let folder = tempfile::tempdir().unwrap();
    let error = CodexProvider::new(folder.path().join("codex"), None)
        .complete(&request())
        .await
        .unwrap_err();
    let ProviderError::Start { program, .. } = &error else {
        panic!("{error:?}");
    };
    assert_eq!(program, "Codex");
}

#[tokio::test]
async fn d4_cancel_kills_the_cli_and_its_children_and_removes_the_folder() {
    for kind in ["claude", "codex"] {
        let hold = Hold::new().await;
        let script = json!({"stdout": envelope(ANSWER), "hold": hold.address, "hold_child": true});
        let cli = FakeCli::new(kind, &script);
        let provider: Arc<dyn Provider> = if kind == "claude" {
            Arc::new(ClaudeCodeProvider::new(cli.path(), None))
        } else {
            Arc::new(CodexProvider::new(cli.path(), None))
        };
        let call = tokio::spawn({
            let provider = Arc::clone(&provider);
            async move { provider.complete(&request()).await }
        });
        // The CLI and the child it started.
        let first = hold.started().await;
        let second = hold.started().await;

        // Cancelling drops the call.
        call.abort();
        assert!(call.await.unwrap_err().is_cancelled());
        first.ended().await;
        second.ended().await;

        // The folder goes once the processes are gone. Polls: the wait
        // ends when it is removed, however long that takes.
        let folder = cli.run().cwd;
        while folder.exists() {
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
    }
}

#[tokio::test]
async fn d4_held_cli_answers_once_released() {
    let hold = Hold::new().await;
    let cli = claude(json!({"stdout": envelope(ANSWER), "hold": hold.address}));
    let provider = ClaudeCodeProvider::new(cli.path(), None);
    let request = request();
    let call = provider.complete(&request);
    let release = async {
        let mut process = hold.started().await;
        process.release().await;
        process.ended().await;
    };
    let (reply, ()) = tokio::join!(call, release);
    assert_eq!(reply.unwrap().text, ANSWER);
}

#[tokio::test]
async fn d4_all_four_provider_kinds_answer_through_one_trait() {
    let anthropic_server =
        FakeServer::start([Reply::events(stream(&[&ANSWER[..20], &ANSWER[20..]]))]).await;
    let openai_server = FakeServer::start([Reply::json(200, completion(ANSWER, None))]).await;
    let claude_cli = claude(json!({"stdout": envelope(ANSWER)}));
    let codex_cli = codex(json!({"stdout": events(ANSWER)}));
    let providers: Vec<Box<dyn Provider>> = vec![
        Box::new(anthropic(&anthropic_server)),
        Box::new(openai(&openai_server, None)),
        Box::new(ClaudeCodeProvider::new(claude_cli.path(), None)),
        Box::new(CodexProvider::new(codex_cli.path(), None)),
    ];
    let request = request();
    let mut answers = Vec::new();
    for provider in &providers {
        let reply = provider.complete(&request).await.unwrap();
        answers.push(validate(&reply.text, &[1]).unwrap());
    }
    assert_eq!(answers.len(), 4);
    assert!(
        answers.iter().all(|answer| *answer == answers[0]),
        "{answers:?}"
    );
}
