# M4-04 CLI providers: Claude Code and Codex

## Goal
`gitcroc-ai` can resolve a file through an installed Claude Code or Codex CLI. The CLI runs in an empty temporary folder with every tool turned off, gets the same prompt as the API providers, and must return the same JSON. A self-test checks the installed version's flags.

## Depends on
M4-01

## Spec
Implements: AI-2, D4 (CLI half)
Respects: ARCH-3, AI-1, AI-7, AI-14, SEC-5, D9, section 7 "Providers"

## In scope
- `ClaudeCodeProvider { path, model: Option }` and `CodexProvider { path, model: Option }`.
- Detection: `detect()` finds `claude` and `codex` on `PATH`, and in their usual install folders (`~/.local/bin`, `~/.claude/local`, npm's global bin, and `%APPDATA%\npm` on Windows). It returns the paths found; the user can type another (M4-06).
- Running (AI-2), always with an argument array (ARCH-3):
  - The working directory is a new empty temporary folder, removed after the call; never a repo.
  - The prompt goes on stdin, as one text holding the system prompt and the user message.
  - The temperature is not passed; neither CLI takes one.
  - The environment is the app's, so the CLI finds its own login.
- Flags, verified against the installed versions at implementation time and recorded in the code with the version they were checked on. On this machine those are Claude Code 2.1.301 and codex-cli 0.163.0.
  - Claude Code: `-p --output-format json`, plus the flags that turn off every tool and stop it from reading project settings, plus `--model` when set. The reply is the JSON envelope's `result` field; usage comes from its `usage`.
  - Codex: its non-interactive `exec` mode reading the prompt from stdin. Add the flags for a read-only sandbox with no tool approvals, for running outside a git repo, and for writing the last message to a file in the temporary folder, plus `--model` when set. Usage comes from its JSON events when it reports them.
  - Whatever text comes back then goes through M4-01's validation like any API reply.
- Self-test (AI-2): `self_test(path)` runs `--version` and the help text. It checks that every flag the provider uses is listed, and returns the version or the missing flags. M4-05 runs it when a CLI provider is saved and at app start.
- A non-zero exit, or a reply that is not the expected envelope, is a `ProviderError` carrying stderr. Cancelling kills the process (and its process group on Unix, its job object on Windows) and removes the folder.
- Tests, with a fake CLI: a small test binary built from the crate's tests that acts as either CLI. It records its arguments, working directory and stdin, and answers from a script.
  - `ai_2_cli_runs_in_an_empty_temp_dir_never_the_repo`: the recorded working directory is empty and is removed afterwards.
  - `ai_2_cli_tools_are_disabled`: the arguments hold the tool-disabling flags, for both CLIs.
  - `ai_2_cli_gets_the_same_prompt_and_returns_the_same_json`: the stdin equals the API providers' prompt for the same request, and the parsed answer is the same.
  - `ai_2_self_test_reports_missing_flags`, from a fake whose help text lacks one.
  - Cancel kills a fake that waits for release.
  - `d4_all_four_provider_kinds_answer_through_one_trait`: the two API providers (on M4-03's fake server) and the two CLI providers (on the fake CLI) give the same parsed answer to one request.

## Out of scope
- Settings and the provider list (M4-05). Running against the real CLIs in CI (never; M4-12 runs the fixture set by hand).

## Acceptance
- The tests above exist and pass on all three platforms.
- One manual run through each real CLI on this machine is recorded in the pull request. It is not a test.
- `just check` passes.
