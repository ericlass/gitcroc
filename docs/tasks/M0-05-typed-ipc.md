# M0-05 Typed IPC

## Goal
The UI calls the core through generated TypeScript types, with one working command and one working event as the pattern every later feature copies.

## Depends on
M0-04

## Spec
Implements: ARCH-6, ARCH-7
Respects: ARCH-5, ARCH-8, ARCH-9, section 4 "Stack" table (IPC types row)

## In scope
- Add `tauri-specta` (and `specta`) to `gitcroc-app`. Check the current versions compatible with the Tauri version from M0-04 and record them in the pull request.
- Generated bindings written to `ui/src/ipc/bindings.ts`. The file is committed and marked as generated in its header.
- `just ipc` regenerates the bindings. `just check` fails if regenerating changes the file (bindings out of date).
- ESLint rule or test that forbids importing `@tauri-apps/api` `invoke` or `listen` anywhere outside `ui/src/ipc/`, so hand-written IPC calls are impossible.
- First command `app_info`: returns `{ name, version, os, arch }`. The data type lives in `gitcroc-core`; the Tauri command in `gitcroc-app` only forwards.
- First event `job_progress` with payload `{ job_id, workspace_id, label, done, total }` (types only, plus a debug command `debug_emit_progress` compiled in debug builds only that emits three events 100 ms apart). This establishes the job-with-progress shape of ARCH-7 without building the job engine.
- Typed error type for commands: a serializable enum `IpcError { code, message, git_stderr: Option<String> }` in the bindings.
- UI: a tiny `ipc` module wrapping the generated functions, and a store keyed by workspace id (ARCH-9) that records progress events. The page shows the `app_info` result.
- Fake core for front-end tests: `ui/src/ipc/fake.ts` implementing the same generated interface, selected by a Vite mode. Playwright and Vitest use it.
- Tests:
  - `arch_6_ui_state_comes_only_from_core` (Vitest): with the fake core returning version "9.9.9", the page shows "9.9.9"; nothing is hardcoded.
  - `arch_7_commands_are_async` (Rust): asserts every registered command is `async` (inspect the specta export or use a compile-time marker).
  - `arch_7_progress_events_reach_the_store` (Vitest): three fake `job_progress` events end up in the store under the right workspace id.
  - A Rust test that fails when the committed bindings differ from freshly generated ones.

## Out of scope
- The real job engine, job list and status bar (M2).
- Paging helpers for ARCH-8 (M1, with history).
- Any git command.

## Acceptance
- `just dev` shows the real version, OS and architecture from the core.
- Changing the `app_info` return type in Rust without running `just ipc` makes `just check` fail.
- Importing `invoke` directly in a component makes ESLint fail.
- All tests listed above exist and pass on all three platforms.
