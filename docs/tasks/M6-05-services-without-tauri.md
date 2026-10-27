# M6-05 Services without Tauri: `gitcroc-app` becomes the app's API

## Goal
`gitcroc-app` is the app's API with no UI toolkit in it (section 4, Crates). It provides an `App` value with one method per operation and one `AppEvent` channel. The Tauri app keeps working on top of it, with its commands reduced to one-line wrappers. `gitcroc-ui` (M6-06) can then call the same methods that the shipped app calls.

## Depends on
M6-01

## Spec
Respects: ARCH-5, ARCH-6, ARCH-7, ARCH-8, ARCH-9, PLAT-1, D10, D12, SEC-2, QA-3, QA-4, DEV-4

## Split into four pull requests (DEV-4)
Each PR leaves the Tauri app working and every existing test passing.
1. **The Tauri shell moves out.** It is a move with no logic change.
2. **`App`, `AppEvent` and the first services:** app info, git setup, settings, workspaces, tree, palette, groups, history and jobs.
3. **Changes and operations:** changes, diffs, staging, commit, branch, sync, integrate, stash, tag, reset, remote, clone, conflicts, fsmonitor and commit-graph. It also adds the `Dialogs` trait.
4. **The rest:** AI, askpass, updates, watchers, group operations and git detection. Afterwards `ipc.rs` holds nothing but wrappers.

## In scope

### PR 1: the Tauri shell moves to `crates/gitcroc-tauri`
- **What moves.** A new workspace crate, `gitcroc-tauri`, takes:
  - the `gitcroc` binary, `lib.rs`'s `run()`, `ipc.rs`, `window.rs`, `update.rs`, `update_test.rs` and `smoke_dirs.rs`;
  - `build.rs`, the `tauri*.json` files, `capabilities/`, `icons/`, `gen/`, `windows-app-manifest.xml` and the Tauri examples.
- **Use `git mv`** so history follows the files.
- **The Tauri and gtk parts of `platform/` move too:**
  - `WindowFrame` and its per-platform implementations;
  - `DesktopFontSource`'s gtk implementation;
  - the `bundle_type()` calls.
- **What stays in `gitcroc-app`:**
  - the platform traits;
  - the package decisions of `platform/package.rs`, which now take our own `BundleKind` enum instead of Tauri's `BundleType`;
  - `askpass.rs`, `ai.rs` and `proposer.rs`.
  - `askpass.rs` uses a tokio runtime handle it is given, instead of `tauri::async_runtime::block_on`.
- **What `gitcroc-app` drops:** `tauri`, `tauri-build`, `tauri-specta`, `specta-typescript`, both plugins and `gtk`.
  - It keeps `specta` for its derives, as the core crates do, until M6-30.
- **Every path that names `crates/gitcroc-app` for the Tauri app** now names `crates/gitcroc-tauri`:
  - `justfile`;
  - `ci.yml`, `build.yml`, `bundles.yml`, `release.yml` and `update-test.yml`, including their `paths:` filters;
  - `.github/scripts/`;
  - `ui/vite.config.ts` and the e2e specs.
- **What must not change:**
  - The product name, identifier, binary name, version, icons and updater config stay the same, so the packages are the same apart from the source path.
  - `ui/src/ipc/bindings.ts` is unchanged.
- **Tests:**
  - The tests that drive the Tauri shell move with it: `build.rs`, `ipc.rs`, `security.rs` and `start.rs`, from `crates/gitcroc-app/tests/integration/`.
  - The others stay. The package tests switch to `BundleKind`, and the font-name parsing stays in `gitcroc-app`, apart from the gtk calls.
- **Workspace policy** (`crates/gitcroc-cli/tests/integration/workspace_policy.rs`):
  - `MEMBERS` gains `gitcroc-tauri`.
  - `ALLOWED_EDGES` gains `gitcroc-tauri` to `gitcroc-app`, `gitcroc-core`, `gitcroc-git` and `gitcroc-ai`. A comment says this is temporary until M6-30.

### PR 2 to PR 4: the service API
- **`App`** (`crates/gitcroc-app/src/app.rs`):
  - **What it owns:** the stores that `run()` now registers with Tauri's `manage`:
    - `GitSetup`, `AppLog` and `SettingsStore`;
    - `Workspaces`, `Summaries`, `Trees`, `Palettes`, `Histories` and `Changes`;
    - jobs, group operations, watchers, askpass, `Updates`, AI providers and AI jobs.
  - **`App::start(setup)`:**
    - It takes the config, cache and log directories, a tokio runtime handle and the platform implementations.
    - It does what `run()`'s `setup` closure does now, in the same order: load, log, askpass socket, updater schedule, AI start, watching and git detection.
  - **Methods** are grouped by area into modules, as `ipc.rs` is now: `app`, `workspace`, `tree`, `history`, `changes`, `ops`, `conflict`, `ai`, `update` and `askpass`.
    - Each takes the same arguments as its Tauri command today, without `AppHandle` or `State`.
    - Each returns `Result<T, AppError>`.
    - `AppError` is today's `IpcError`, with its codes and messages, moved and renamed. `IpcError` becomes a type alias in `gitcroc-tauri`, so the bindings don't change.
  - **ARCH-7:**
    - Every method is `async`, or returns at once.
    - Disk and git work runs off the caller's thread, as the command bodies do now with `spawn_blocking` and jobs.
    - Anything over 50 ms is still a job.
  - **ARCH-9:** every method that reads or changes session state takes a `WorkspaceId`, as now.
- **`AppEvent`:**
  - **The enum:** one variant for each of today's 23 events, with the same payload types. `App::subscribe()` returns a receiver of a `tokio::sync::broadcast` channel.
  - **Order:** one channel keeps events in the order they were sent.
  - **Lag:** a subscriber that falls behind gets `Lagged` and has to read the state again (ARCH-6). Its capacity is set in one constant, with a comment saying why it has that value.
  - **The core's sinks** (`JobEvents`, `AiSink`, `UpdateEvents`, the summary sinks, the watchers) send `AppEvent`s instead of calling `emit`.
- **`Dialogs`** (PLAT-1, one trait for this concern): `pick_folder`, `pick_file` and `save_file`, each returning a future of `Option<PathBuf>`.
  - The commands that open a dialog from Rust today use it: `git_pick_path`, `workspace_pick_and_add`, `clone_pick_parent`, `workspace_export` and `workspace_import`.
  - `gitcroc-tauri` implements it with the dialog plugin. M6-06 implements it with `cx.prompt_for_paths`.
- **The askpass UI** (D10):
  - `WindowAskpassUi` becomes a service: it sends `AppEvent::AskpassPrompt` and waits for `App::askpass_answer`.
  - The timeout and cancel behaviour don't change.
- **What `gitcroc-tauri` keeps:**
  - Each command is one call to an `App` method and maps the result, plus `#[tauri::command]` and `#[specta::specta]`.
  - **The forwarder:** one task forwards each `AppEvent` to its Tauri event. On `Lagged` it logs a warning, as the webview reads state again on its own.
  - **Commands that only concern the window:** `window_control`, `window_maximized` and the `perf-trace` and `e2e` `debug` module.
  - It also keeps the Tauri `UpdateSource` until M6-27.
- **`app_info`:** the version, platform and installation come from `App`. The fonts still come from the `DesktopFontSource` the shell passes in.

### Tests (QA-3)
- **`arch_5_app_crate_has_no_ui_toolkit`**, in `workspace_policy.rs` (PR 1): `gitcroc-app`'s normal dependencies include no Tauri crate, `wry`, `gtk` or `gpui` crate.
- **`arch_7_slow_app_calls_return_jobs`** (PR 2, `crates/gitcroc-app/tests/integration/app.rs`):
  - adding a repo to a workspace on a `repo-gen` fixture returns a `JobId` at once;
  - its progress and end arrive as `AppEvent`s.
- **`arch_6_app_sends_fresh_state_after_a_job`** (PR 3): after a commit job, a `RepoSummary` event carries the new state, and `history_get` shows the commit.
- **`arch_6_events_keep_their_order_and_report_lag`** (PR 2):
  - events from one sender arrive in order;
  - a subscriber that read nothing gets `Lagged`, and can read the state again.
- **`arch_9_app_calls_name_their_workspace`** (PR 2): the same calls on two workspaces keep separate selections and history positions.
- **`d10_askpass_prompt_round_trips_through_app_events`** (PR 4): a prompt arrives as an event, and `askpass_answer` answers it.
- **`arch_6_every_app_event_has_a_tauri_event`** (PR 4, in `gitcroc-tauri`): the forwarder maps each `AppEvent` variant to one of the registered Tauri events, and each of them is mapped to.
- **Existing tests keep passing**, with only their imports changed:
  - `arch_7_commands_are_async` and `ipc_bindings_are_up_to_date`;
  - Vitest, Playwright and `just smoke`.

## Out of scope
- Any GPUI code (M6-06).
- Any change to what a command does or returns. A behaviour change found on the way is noted in the PR and left for its own task.
- The Tauri updater plugin (M6-27). The fonts without gtk (M6-07).
- Removing `specta` from the crates (M6-30).

## Acceptance
- `gitcroc-app` builds and its tests pass with no Tauri, `wry` or `gtk` in its dependency tree; `arch_5_app_crate_has_no_ui_toolkit` passes.
- `ui/src/ipc/bindings.ts` is the same after each of the four PRs.
- **PR 1 is merged only after:**
  - the build and update test workflows have passed on it (their `paths:` filters include the moved files);
  - the packages it builds have the same file names as before.
- After PR 4, `ipc.rs` holds no logic beyond wrappers, the event forwarder and the window commands.
- `just check` passes after each PR.
