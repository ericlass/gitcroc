# gitcroc

Cross-platform Git client for multi-repo projects. Rust core, Tauri 2 shell, SolidJS UI.
Spec: docs/spec/ (source of truth). Decisions: docs/spec/01-decisions.md and docs/adr/.

## Commands
- just check        # everything CI requires; must pass before you say a task is done
- just dev          # run the app
- just bench        # performance harness

`just dev` and `just bench` do not exist yet: they arrive in M0-04 and M1 (docs/spec/13-milestones.md) and fail with a message until then.

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
