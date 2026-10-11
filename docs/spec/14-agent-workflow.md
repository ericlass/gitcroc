# 14. Working with Claude Code

The spec lives in the repo as Markdown, agents work from small task files that cite requirement IDs, and an agent that hits a gap stops and asks instead of guessing.

## Repo layout for documents

```text
CLAUDE.md                     short, always-loaded rules
docs/spec/01-decisions.md     this spec, one file per section
docs/spec/05-groups.md
docs/spec/...
docs/adr/0001-hybrid-git.md   one file per decision made after today
docs/tasks/M1-03-history-paging.md
```

- **DEV-1** `docs/spec/` is the source of truth once this document is exported. Spec changes are their own commits, never mixed with code.
- **DEV-2** An agent MUST stop and ask when the spec is silent, when two requirements conflict, or when a requirement cannot be met. The answer becomes an ADR plus a spec edit before code continues.
- **DEV-3** Agents do not edit `docs/spec/` or `docs/adr/` unless the task says so.
- **DEV-4** One task is one vertical slice that fits one pull request and one agent session. If a task needs more than about 400 changed lines outside tests, it is split.

## CLAUDE.md starter

```markdown
# gitcroc

Cross-platform Git client for multi-repo projects. Rust core, Tauri 2 shell, SolidJS UI.
Spec: docs/spec/ (source of truth). Decisions: docs/spec/01-decisions.md and docs/adr/.

## Commands
- just check        # everything CI requires; must pass before you say a task is done
- just dev          # run the app
- just bench        # performance harness

## Invariants (never break; see spec IDs)
- All repo writes and network operations go through the git CLI runner (ARCH-1).
- Never build a shell string; argument arrays only (ARCH-3).
- gitcroc-git, gitcroc-core, gitcroc-ai must not depend on Tauri (ARCH-5).
- The UI holds no source of truth and no business logic (ARCH-6).
- No IPC call blocks; slow work is a job with progress events (ARCH-7).
- IPC types are generated; never hand-write them.
- AI only proposes; it never writes, stages, commits or continues (D9).
- No unsafe; no unwrap/expect outside tests (QA-4).

## How to work
- Read the task file and every spec ID it cites before writing code.
- Write the tests named after the requirement IDs first (QA-3).
- If the spec is silent or contradictory, stop and ask. Do not guess (DEV-2).
- Do not edit docs/spec or docs/adr unless the task says so.
```

## Task file template

```markdown
# M1-03 History paging

## Goal
One sentence: what the user can do when this is merged.

## Spec
ARCH-8, PERF-2, UI-9, UI-12

## In scope
- ...

## Out of scope
- ...

## Acceptance
- Tests named after each spec ID above exist and pass.
- just check passes.
- (Performance tasks) the named budget from section 9 is met in just bench.
```

## The loop for each task

1. Planning session (this chat or Claude Code plan mode): turn the next part of a milestone into task files. Review them yourself; this is the cheapest place to catch a wrong idea.
2. Implementation session in Claude Code: one task file, fresh context, tests first.
3. Review session with fresh context: give it only the task file, the cited spec sections and the diff, and ask for mismatches against each requirement ID.
4. Merge when `just check` is green in CI on all three platforms.

## First five steps

1. Done: the questions in section 15 are answered and the decision log is updated.
2. Done: this document is exported to `docs/spec/`, one file per section.
3. Create the repository with `CLAUDE.md` and the license.
4. Ask for the M0 task breakdown (expect 6 to 8 task files).
5. Run M0, then the M1 history and diff spikes first, since they carry the performance risk.
