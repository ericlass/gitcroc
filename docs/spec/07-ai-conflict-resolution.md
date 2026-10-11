# 7. AI conflict resolution

The AI fills in proposals for conflict hunks inside the normal conflict editor (OPS-3); the user accepts, edits or rejects each one, and nothing reaches disk or the index without the user (D9).

## Providers (D4)

| Provider kind | User configures | How it is called |
| --- | --- | --- |
| Anthropic API | API key, model | Messages API, streaming |
| OpenAI-compatible | Base URL, API key (optional), model | Chat Completions API. Covers OpenAI, OpenRouter, Ollama, LM Studio and similar |
| Claude Code CLI | Path (auto-detected), optional model | `claude -p --output-format json`, prompt on stdin ([headless mode](https://docs.claude.com/en/docs/claude-code/headless)) |
| Codex CLI | Path (auto-detected), optional model | Its non-interactive exec mode, prompt on stdin |

- **AI-1** Several providers can be configured; one is the default. Model names are fetched from the provider where it offers a list, otherwise typed by the user. No model name is hardcoded.
- **AI-2** CLI providers run with an empty temporary directory as working directory, never the repo, with all tools disabled. They receive the same prompt and must return the same JSON as API providers. Exact flags are verified against the installed CLI version at implementation time and covered by a startup self-test.

## Where it appears

- **AI-3** The conflict editor has **Resolve with AI** per file. The conflict banner (OPS-2) has **Resolve all with AI**. With no provider configured, both open the provider settings instead.
- **AI-4** A file is eligible when it is a text conflict the editor can show (not OPS-6), is at most 1 MB, and does not match the exclude list. The default exclude list is `.env*`, `*.pem`, `*.key`, `*.p12`, `id_rsa*`; the user can edit it. Ineligible files show the reason.

## What is sent

- **AI-5** Per file, exactly this and nothing else: the file path; the operation type; the labels of both sides (OPS-4); for each unresolved hunk its base, ours and theirs text plus 40 lines of surrounding context; and the messages of up to 10 commits per side that touched the file since the merge base, each cut at 2,000 characters.
- **AI-6** Before a provider's first use, a consent dialog states what is sent (AI-5) and to which host or program. Each workspace has a switch that disables AI entirely.
- **AI-7** Prompts and responses are kept in memory only. They are written to the log only when the user turns on AI debug logging. Keys are never logged.

## What comes back

The model must return one JSON object and nothing else:

```json
{
  "hunks": [
    {
      "id": 1,
      "status": "resolved",
      "resolution": "exact replacement text for the hunk",
      "explanation": "One or two sentences on what was kept from each side and why.",
      "confidence": "high"
    }
  ]
}
```

- **AI-8** `status` is `resolved` or `unresolved`. `confidence` is `high`, `medium` or `low`. An `unresolved` hunk has an explanation and no resolution.
- **AI-9** A response is valid only if it parses, lists every requested hunk exactly once, and no resolution contains a conflict marker line. An invalid response is retried once with the validation error appended. A second failure marks the file "AI failed" with the reason.
- **AI-10** If a file's prompt exceeds the provider's context budget (setting, default 100,000 tokens, estimated as characters divided by 3), hunks are split across several requests. A single hunk that exceeds the budget is returned as unresolved with reason "too large".

## Review and accept

- **AI-11** Each proposed hunk shows the proposed text, the explanation and the confidence, with **Accept**, **Reject** and **Edit**. The file has **Accept all high-confidence**. There is no "accept everything" across files.
- **AI-12** Accept puts the text into the editor buffer and nowhere else (OPS-9). Writing and staging still happen only through Mark resolved (OPS-5), and the operation proceeds only through Continue (OPS-2).
- **AI-13** Resolve all with AI processes up to 4 files at a time, shows progress per file, and can be cancelled. Finished proposals stay; unfinished files stay untouched.
- **AI-14** Token usage is shown per request when the provider reports it.

## Prompt

- **AI-15** The system prompt lives in versioned files in `gitcroc-ai/prompts/`. Temperature is 0 where the provider allows it. A fixture set of at least 30 real-world style conflicts with reviewed acceptable resolutions is kept in the repo and run on demand against a chosen provider to compare prompt versions.
