# gitcroc

Cross-platform Git client for multi-repo projects. Rust core, native UI in Rust with GPUI, being written in milestone M6 (docs/plans/native-ui-gpui.md). The Tauri app is gone; ui/ is the old front end, kept as a reference until M6-30.
Spec: docs/spec/ (source of truth). Decisions: docs/spec/01-decisions.md and docs/adr/.

## Commands
- just check        # everything CI requires; must pass before you say a task is done
- just ui-reference # the old front end in a browser, with its fake core
- just bench        # performance harness; needs `just bench-data target-project` first

## Invariants (never break; see spec IDs)
- All repo writes and network operations go through the git CLI runner (ARCH-1).
- Never build a shell string; argument arrays only (ARCH-3).
- gitcroc-git, gitcroc-core, gitcroc-ai must not depend on Tauri; only gitcroc-ui depends on GPUI (ARCH-5).
- The UI holds no source of truth and no business logic (ARCH-6).
- No work over 2 ms on the UI thread; slow work is a job with progress events (ARCH-7).
- AI only proposes; it never writes, stages, commits or continues (D9).
- No unsafe; no unwrap/expect outside tests (QA-4).

## How to work
- Read the task file and every spec ID it cites before writing code.
- Write the tests named after the requirement IDs first (QA-3).
- If the spec is silent or contradictory, stop and ask. Do not guess (DEV-2).
- Do not edit docs/spec or docs/adr unless the task says so.
