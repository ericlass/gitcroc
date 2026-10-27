# M6-05 Services without Tauri: `gitcroc-app` becomes the app's API

## Goal
The Tauri app is removed now, not kept running beside the new UI (Eric, 2026-10-27): 1.0.0-rc.1 is its last release, and the GPUI build replaces it. `gitcroc-app` becomes the app's API with no UI toolkit in it (section 4, Crates). It provides an `App` value with one method per operation and one `AppEvent` channel, which `gitcroc-ui` (M6-06) calls.

## Depends on
M6-01

## Spec
Respects: ARCH-5, ARCH-6, ARCH-7, ARCH-8, ARCH-9, PLAT-1, D2, D10, D12, SEC-2, QA-1, QA-3, QA-4, DEV-4

## Split into four pull requests (DEV-4)
1. **Tauri leaves.** The Tauri shell, its workflows and the IPC layer are deleted. `ui/` stays as a reference, out of CI.
2. **`App`, `AppEvent` and the first services:** app info, git setup and git detection, settings, workspaces, tree, palette, groups, history, jobs and watchers. Jobs, tab switches and adding repos need the watchers and a detected git, so both come here rather than in PR 4.
3. **Changes and operations:** changes, diffs, staging, commit, branch, sync, integrate, stash, tag, reset, remote, clone, conflicts, fsmonitor and commit-graph. It also adds the `Dialogs` trait.
4. **The rest:** AI, askpass, updates and group operations.

From PR 1 until M6-06 there is no app to run. PRs 2 to 4 port the command bodies of `crates/gitcroc-app/src/ipc.rs` as it was before PR 1 (`git show <PR 1's parent>:crates/gitcroc-app/src/ipc.rs`), with the tests that went with them.

## In scope

### PR 1: Tauri leaves
- **ADR 0004** gets a dated note: the Tauri app is removed in M6-05, not at the switch (Eric, 2026-10-27). It goes in a docs commit of its own.
- **Deleted from `crates/gitcroc-app`:**
  - the `gitcroc` binary, `lib.rs`'s `run()`, `ipc.rs`, `window.rs`, `update_test.rs` and `smoke_dirs.rs`;
  - from `update.rs`, the Tauri update source and events. `CHECKS`, `RELEASES_PAGE` and the system clock stay;
  - `build.rs`, the `tauri*.json` files, `capabilities/`, `gen/`, `windows-app-manifest.xml` and the `ipc_bindings` example;
  - the `update-test` and `perf-trace` features. `e2e` stays for the mock AI provider, which M6-29's smoke test uses again;
  - from `platform/`: `WindowFrame` and its implementations, `GtkFonts`, and the `bundle_type()` calls.
- **What stays in `gitcroc-app`:**
  - the platform traits and their implementations, apart from those above;
  - `askpass.rs`, `ai.rs` and `proposer.rs`;
  - `DesktopFonts`, moved from `ipc.rs` to `platform/`; every platform reads no fonts until M6-07;
  - `icons/`, until M6-28 packages the new app.
- **Changes in what stays:**
  - **Package detection:** `platform/package.rs` takes our own `BundleKind` enum instead of Tauri's `BundleType`. Until M6-28 marks its packages, no build reports a bundle kind. The Windows portable rule and every package test stay.
  - **Askpass:** `askpass::serve` takes the tokio runtime handle it runs on, instead of `tauri::async_runtime::block_on`.
  - **The askpass tests' helper:** the tests ran the `gitcroc` binary as git's helper. A test-only binary, `askpass-helper`, now does this. It is built the way `gitcroc-ai`'s `fake-cli` is: `test = false` and `doc = false`.
- **Dependencies:**
  - `gitcroc-app` drops `tauri`, `tauri-build`, `tauri-specta`, `specta-typescript`, both plugins and `gtk`.
  - The workspace drops them too, along with the `cargo deny` entries that only they needed.
  - `specta` stays for the core crates' derives until M6-30.
- **Tests:**
  - Deleted with what they test:
    - `build.rs`, `ipc.rs`, `security.rs`, `start.rs` and `update_test.rs` from `crates/gitcroc-app/tests/integration/`. The D12 and download page tests of `build.rs` move to `update.rs`;
    - `fonts.rs`, whose names come back with M6-07.
  - The package tests in `update.rs` switch to `BundleKind`.
  - The command behaviour that `ipc.rs` tested comes back with its service in PRs 2 to 4.
- **The web front end (`ui/`):**
  - It stays in the tree as the reference the M6 tasks port from, runnable in a browser with its fake core.
  - It leaves `just check` and CI. `ui/src/ipc/bindings.ts` is no longer regenerated.
  - M6-06's parity mode reads its tests, and M6-30 deletes it.
  - Until M6-06, `just coverage` still counts its tests, though CI no longer runs them. The PR says so.
- **`justfile`:**
  - `check` is `check-rust`.
  - `check-ui`, `setup`, `ipc`, `dev`, `bundle`, `smoke` and `bench-ui` go.
  - A new recipe, `ui-reference`, runs the old front end with its fake core.
- **Workflows:**
  - `ci.yml` keeps `check-rust` on the three runners, without WebKitGTK's packages. The `check-ui` and `smoke` jobs go.
  - `build.yml`, `bundles.yml`, `release.yml` and `update-test.yml` go, with the `.github/scripts/` they run. M6-28 writes the GPUI app's own, starting from these in history.
  - `bench.yml` stays: it runs only the core's `just bench`.
- **Workspace policy** (`crates/gitcroc-cli/tests/integration/workspace_policy.rs`):
  - D6 and D14 check `ci.yml` and the crates only. Their checks of the bundles (a universal macOS build, macOS 12, the bundle's license) come back with M6-28.
  - QA-5's list of `just check` commands is QA-1's, which is Rust only.
  - The D2 test checks that no crate depends on a webview: `tauri`, `wry`, `webkit2gtk` or `webview2-com`. M6-06 adds that `gitcroc-ui` uses GPUI at the pinned versions.
  - New test `arch_5_app_crate_has_no_ui_toolkit`.
- **`CLAUDE.md`:** the stack line, the commands and the IPC invariant are updated.
- **Releases from `main`:** none until M6-28. A fix for rc.1 would come from a branch off `v1.0.0-rc.1`, whose workflows are still there.

### PR 2 to PR 4: the service API
- **`App`** (`crates/gitcroc-app/src/app.rs`):
  - **What it owns:** the stores that `run()` registered with Tauri's `manage`:
    - `GitSetup`, `AppLog` and `SettingsStore`;
    - `Workspaces`, `Summaries`, `Trees`, `Palettes`, `Histories` and `Changes`;
    - jobs, group operations, watchers, askpass, `Updates`, AI providers and AI jobs.
  - **`App::start(setup)`:**
    - It takes the config, cache and log directories, a tokio runtime handle and the platform implementations.
    - It does what `run()`'s `setup` closure did, in the same order: load, log, askpass socket, updater schedule, AI start, watching and git detection.
  - **Methods** are grouped by area into modules, as `ipc.rs` was: `app`, `workspace`, `tree`, `history`, `changes`, `ops`, `conflict`, `ai`, `update` and `askpass`.
    - Each takes the same arguments as its Tauri command did, without `AppHandle` or `State`.
    - Each returns `Result<T, AppError>`, or the value alone when it cannot fail, such as `settings_get`.
    - `AppError` is `IpcError` under a new name, with the same codes and messages.
  - **ARCH-7:**
    - Every method is `async`, or returns at once.
    - Disk and git work runs off the caller's thread, as the command bodies did with `spawn_blocking` and jobs.
    - Anything over 50 ms is still a job.
  - **ARCH-9:** every method that reads or changes session state takes a `WorkspaceId`, as the commands did.
- **`AppEvent`:**
  - **The enum:** one variant for each of the 23 Tauri events, with the same payload types. `App::subscribe()` returns a receiver of a `tokio::sync::broadcast` channel.
  - **Order:** one channel keeps events in the order they were sent.
  - **Lag:** a subscriber that falls behind gets `Lagged` and has to read the state again (ARCH-6). Its capacity is set in one constant, with a comment saying why it has that value.
  - **The core's sinks** (`JobEvents`, `AiSink`, `UpdateEvents`, the summary sinks, the watchers) send `AppEvent`s.
- **`Dialogs`** (PLAT-1, one trait for this concern): `pick_folder`, `pick_file` and `save_file`, each returning a future of `Option<PathBuf>`.
  - The calls that opened a dialog from Rust use it: `git_pick_path`, `workspace_pick_and_add`, `clone_pick_parent`, `workspace_export` and `workspace_import`.
  - Tests use a fake. M6-06 implements it with `cx.prompt_for_paths`.
- **The askpass UI** (D10):
  - `WindowAskpassUi` becomes a service: it sends `AppEvent::AskpassPrompt` and waits for `App::askpass_answer`.
  - The timeout and cancel behaviour don't change.
- **Updates** (PR 4): `App::start` takes the `UpdateSource` to use. Until M6-27 provides one, it is a source that never finds an update, and the `Updates` schedule and its tests are as before (D12).
- **Window commands:** `window_control`, `window_maximized` and the `perf-trace` and `e2e` `debug` module are not ported. M6-06 does the window, and later tasks do the runners.
- **`app_info`:** the version, platform and installation come from `App`. The fonts come from the `DesktopFontSource` passed in.

### Tests (QA-3)
- **`arch_5_app_crate_has_no_ui_toolkit`** (PR 1), in `workspace_policy.rs`: `gitcroc-app`'s normal dependencies include no Tauri crate, `wry`, `gtk` or `gpui` crate.
- **The D2 test** (PR 1): no workspace crate depends on `tauri`, `wry`, `webkit2gtk` or `webview2-com`.
- **`arch_7_slow_app_calls_return_jobs`** (PR 2, `crates/gitcroc-app/tests/integration/app.rs`):
  - adding a repo to a workspace on a `repo-gen` fixture returns a `JobId` at once;
  - its progress and end arrive as `AppEvent`s.
- **`arch_6_app_sends_fresh_state_after_a_job`** (PR 3): after a commit job, a `RepoSummary` event carries the new state, and `history_get` shows the commit.
- **`arch_6_events_keep_their_order_and_report_lag`** (PR 2):
  - events from one sender arrive in order;
  - a subscriber that read nothing gets `Lagged`, and can read the state again.
- **`arch_9_app_calls_name_their_workspace`** (PR 2): the same calls on two workspaces keep separate selections and history positions.
- **`d10_askpass_prompt_round_trips_through_app_events`** (PR 4): a prompt arrives as an event, and `askpass_answer` answers it.
- **The command tests of the old `tests/integration/ipc.rs`** that test behaviour come back as `App` tests with their service. The binding and `arch_7_commands_are_async` tests don't, since there are no commands.

## Out of scope
- Any GPUI code (M6-06).
- Any change to what an operation does or returns. A behaviour change found on the way is noted in the PR and left for its own task.
- The update source without Tauri (M6-27). The fonts without gtk (M6-07).
- Packaging and releases (M6-28).
- Removing `ui/`, Node and `specta` (M6-30).

## Acceptance
- `gitcroc-app` builds and its tests pass with no Tauri, `wry` or `gtk` in its dependency tree; `arch_5_app_crate_has_no_ui_toolkit` passes.
- `cargo tree --workspace` shows no `tauri` crate after PR 1.
- After PR 4, every Tauri command of the old `ipc.rs` either has an `App` method or is listed in the PR as a window command for M6-06.
- `just check` passes after each PR.
