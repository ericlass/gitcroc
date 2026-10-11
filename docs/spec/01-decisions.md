# 1. How to read this spec

This spec defines v1 of a free, open source, cross-platform Git client for large multi-repo projects, with group operations and AI-assisted conflict resolution. The project is named **gitcroc**.

Three rules keep the spec free of contradictions:

- **One home per fact.** Each rule is stated once, in its own section. Other sections refer to it by ID instead of restating it.
- **The decision log wins.** If any text disagrees with the table below, the table is right and the text is a bug.
- **Fixed vocabulary.** Terms in the glossary (section 3) have exactly one meaning. MUST means required for v1. SHOULD means expected unless a written reason exists. LATER means explicitly not in v1.

## Decision log

| ID | Topic | Decision | Source |
| --- | --- | --- | --- |
| D1 | Core language | Rust, stable toolchain | Eric |
| D2 | UI stack | Tauri 2 shell, TypeScript + SolidJS front end in the system webview | Eric chose Tauri and asked for the fastest UI layer; SolidJS replaces the earlier proposed default |
| D3 | Git engine | Hybrid: `gix` (gitoxide) for reads, the installed `git` CLI for every write and every network operation | Eric |
| D4 | AI access | API keys (Anthropic, OpenAI-compatible endpoints incl. local models) plus installed agent CLIs (Claude Code, Codex) | Eric |
| D5 | Scale target | First target, which gates v1: a project of about 20 repos with about 200,000 files and 2 million lines of code in total. Stretch, measured but not gating: 200 repos per workspace, single repos up to 1.5 million commits or 500,000 tracked files | Eric chose "both"; first target is his largest real project; stretch numbers are proposed |
| D6 | Platforms | Windows 10/11 x64, macOS 12+ (Intel and Apple silicon), Linux x86_64 | Eric; versions proposed |
| D7 | Group membership | A repo can belong to zero, one or several groups. Groups do not nest in v1 | Proposed |
| D8 | Group operation model | Per-repo independent, run in parallel, never atomic, no automatic rollback, always ends with a per-repo result report | Proposed |
| D9 | AI authority | AI only proposes. It never writes files, stages, commits or continues an operation without an explicit user action | Proposed |
| D10 | Git credentials | Delegated to git (credential helpers, SSH agent). The app stores no git credentials | Proposed |
| D11 | AI credentials | API keys live in the OS keychain only | Proposed |
| D12 | Telemetry | None. The app makes no network calls except git operations, user-configured AI calls and the update check | Proposed |
| D13 | Git dependency | System `git` 2.40 or newer on PATH is required; not bundled in v1 | Eric |
| D14 | License | MIT | Eric |
| D15 | Language of UI | English only in v1, all strings externalized | Proposed |

"Proposed" rows are my defaults so the spec has no gaps. Change any of them in this table first, then fix the section that implements it.
