# Plan: replace the Tauri UI with a native GPUI UI

Status: proposal, 2026-10-26. Nothing in `docs/spec/` changes until task
M6-01 is approved (DEV-1, DEV-3). This plan becomes milestone M6, one task
file per row of the task table below, written once the open decisions are
answered.

## Why

The spike on `spike/gpui-history` (`spikes/gpui-history/`) rebuilt the
history pane and a whole-commit diff in GPUI (through gpui-kit 0.8.1) on
`gitcroc-git`. It held 144 fps on `target-project`, with no dropped frame,
no blank row and 4 ms of CPU per frame at p95. It also kept 144 fps scrolling
a 216,000-line diff while that diff was still loading, and handled 21,000
commits of the Linux kernel. The Tauri UI paces at 60 Hz on the same machine
(`bench-ui` `scroll-frame` median 16 ms), and its diff view stutters. The
webview can't be tuned around this. See the spike's README for the numbers.

## What stays and what goes

The core was kept free of Tauri from M0 (ARCH-5). That is what makes this
migration a UI rewrite, not a rewrite of the app.

| Part | Lines today | Fate |
| --- | --- | --- |
| `gitcroc-git`, `gitcroc-core`, `gitcroc-ai`, `gitcroc-cli`, `tools/` | ~36,000 | Unchanged, with all their tests. |
| `gitcroc-app` `platform/`, `askpass.rs`, `ai.rs`, `proposer.rs` | ~2,400 | Kept. The Tauri types leave their signatures. |
| `gitcroc-app` `ipc.rs` (109 commands, 23 events) | 3,200 | The command bodies become plain methods of a service API. The Tauri attributes and specta go. |
| `gitcroc-app` `update.rs`, `window.rs`, `lib.rs` | ~420 | Rewritten without Tauri. |
| `ui/src` | 28,000 TS, 2,800 CSS | Rewritten in Rust and GPUI. Its stores (5,000 lines) port to view models almost one to one. |
| `ui/src` tests, `ui/e2e`, the fake core | 10,200 + 3,300 + 3,100 | Rewritten as GPUI tests against the real core on `repo-gen` fixtures. There is no fake core any more. |
| `ui/bench`, `ui/webdriver` | — | Replaced by in-app runners behind the `perf-trace` and `e2e` features, as in the spike. |
| Tauri, tauri-specta, specta, the dialog and updater plugins, `gtk`, Node, pnpm, Vite, Playwright, tauri-driver, the WebView2 bootstrapper | — | Removed at the switch (M6-30). Two `cargo deny` advisory ignores go with them. |

## Architecture

### Crates

```text
gitcroc-ui   (new)  GPUI views, theme, keymap, strings; the `gitcroc` binary
    |
gitcroc-app         services: the old command bodies as methods, the event bus,
    |               platform traits, askpass, AI wiring, updater. No UI toolkit
    v
gitcroc-core, gitcroc-ai, gitcroc-git   (unchanged)
```

- `gitcroc-app` loses every UI dependency. Its API is an `App` value that owns
  the stores it owns today (`Workspaces`, `Histories`, `Changes`, `Trees`,
  `Summaries`, `Palettes`, `SettingsStore`, jobs and AI). Its methods are
  grouped by area, as `ipc.rs` already is. Events become one `AppEvent` enum
  on a broadcast channel, replacing the 23 Tauri events. The integration
  tests in `crates/gitcroc-app/tests/` keep running headless.
- `gitcroc-ui` is the only crate that depends on GPUI. ARCH-5 gains this
  clause.
- During the migration, `gitcroc-app` still serves the Tauri app. The Tauri
  commands become one-line wrappers over the service methods, so the shipped
  app keeps working until the switch.

### Threads and state

- GPUI renders on the main thread. Core work stays on the existing tokio
  runtime. A UI call starts a service future on tokio and awaits its result in
  `cx.spawn`. One task per window forwards `AppEvent`s into the GPUI entities.
- ARCH-6 holds unchanged: GPUI entities are view models. They are rebuilt
  from core state and events, and hold no truth of their own.
- ARCH-9 holds unchanged: one `WorkspaceView` entity per tab owns that tab's
  selection, scroll, layout and conflict editor state.
- ARCH-7 is reworded. There is no IPC to block any more, so the rule becomes:
  no work over 2 ms on the UI thread, and anything over 50 ms is a job with
  progress. The `perf-trace` CPU-per-frame measurement from the spike
  enforces the 2 ms.
- ARCH-8 is kept for memory reasons. History still loads 500 commits a page,
  and diffs still load file by file. Rows are prepared off the UI thread
  into `SharedString`s, as in the spike.

### What each part of the UI becomes

| Area (spec) | GPUI approach |
| --- | --- |
| Window, own title bar (UI-35) | Client-side decorations. `start_window_move` and a double-click toggle; Windows hit-test areas so Snap Layouts work; macOS traffic lights over a transparent title bar. **Use the system title bar** reopens the window with system decorations. |
| Tabs (UI-27 to UI-33) | Tab bar with typed drag and drop for reordering; badges and markers from job events. |
| Panes and docking (UI-19) | gpui-component resizable panels; right or bottom dock. |
| Tree (UI-1 to UI-8) | `uniform_list` over the flattened rows from the core's tree. Multi-select, context menus, and dragging repos between groups with Ctrl/Alt to add. |
| History (UI-9 to UI-13, UI-36) | The spike's list and graph painter. Column headers can be resized and moved; search input; summary strip; merged list. |
| Changes, commit header (UI-14, UI-16) | Header text sits in read-only inputs so it can be selected and copied, since plain GPUI text cannot be selected. |
| Diff (UI-17, UI-18) | The spike's own list, not gpui-component's Diff, whose scroll state is private. Inline and side-by-side modes; `syntect` highlighting off-thread, cached per line. Wrap mode uses `list` with measured heights. Hunk controls; image diffs from bytes with `img()` (no base64); "Load diff" collapse. |
| Conflict editor (OPS-2 to OPS-9, AI-11) | gpui-component's code editor input with hunk decorations and per-hunk buttons, if spike M6-02 proves it works. The fallback is one editable block per hunk between read-only context. |
| Dialogs (about 25) | One dialog scaffold on gpui-component Dialog and Form. Validation stays in the core: branch and tag names, remote names, clone targets. |
| Jobs, status bar (UI-21) | Status bar and job list as today; notifications for results. |
| Palette (UI-22) | The core palette (`palette.rs`) behind an input and a list in an overlay. |
| Themes (UI-23 to UI-25) | `tokens.css` becomes a Rust `Tokens` struct for light and dark, mapped onto gpui-component's theme. It follows the system appearance live, and uses the OS accent colour where a safe API gives it (Windows UISettings, the Linux appearance portal). On macOS that API needs `unsafe`, so macOS keeps the default accent (QA-4). The 8 code themes become `.tmTheme` files for `syntect`; the contrast tests port as they are. |
| Fonts (UI-26) | System UI and monospace font per platform. The `gtk` crate goes; on Linux the font is read through the settings portal or `gsettings`. |
| Strings (D15) | `strings.rs`, ported from `ui/src/strings`. |
| Keyboard (UI-22, keymap) | GPUI actions and `KeyBinding`s with key contexts. `secondary-` gives Cmd on macOS and Ctrl elsewhere. The keymap docs test is ported. |
| Accessibility | GPUI ships AccessKit (`role`, `aria_label`). M6-04 decides what can be tested. |
| Folder picker, askpass | `cx.prompt_for_paths` replaces the dialog plugin; the askpass prompt is a GPUI modal. |
| macOS menus | App, Edit and Window menus through `cx.set_menus`, so Cmd+Q and copy and paste work. |
| Updater (PLAT-4) | A new `UpdateSource` built on `reqwest` and `minisign-verify`. It reads the same `latest.json` from the same URL with the same public key. It installs the NSIS installer passively, unpacks `.app.tar.gz` over the app on macOS, and replaces the AppImage in place on Linux. |

## Testing (section 12)

- **View and flow tests.** `#[gpui::test]` with `TestAppContext` and
  `VisualTestContext` (the `test-support` feature). They run headless, with no
  GPU, and simulate keystrokes, clicks and drags. They replace Vitest and
  Playwright. They drive the real core on `repo-gen` fixtures (QA-2) and the
  existing mock AI provider. Test names keep the requirement IDs (QA-3).
- **Parity tracking.** `req-coverage` gains a path filter. While both UIs
  exist, `just coverage M6` lists every UI requirement that has a test under
  `ui/` but none under `crates/gitcroc-ui`. When that list is empty, the
  rewrite is complete. At the switch the tool stops scanning `ui/`.
- **Smoke test** (M5-16 replacement). The `e2e` feature adds a scenario runner
  inside the real binary that clicks through add repo, stage, commit, resolve
  a conflict, the group flow and the AI flow, and asserts on core state. It
  runs under `xvfb-run` with Mesa lavapipe (software Vulkan) on Linux, and
  with WARP on Windows. M6-03 proves this works on hosted runners.
- **UI benchmarks** (`just bench-ui`). The spike's `perf-trace` runner, grown
  to cover every UI-side budget row: scroll history and diffs, first paint
  after selecting a repo, a commit or a tab, and input latency during a job.
  It prints the same JSON as today.
- **QA-1 required checks** drop `tsc`, ESLint, Vitest and Playwright. GPUI
  tests are plain `cargo test`, so `just check` becomes Rust only and the
  `check-ui` CI job goes away.

## Packaging and release (section 11)

| Platform | Today (Tauri bundler) | After |
| --- | --- | --- |
| Windows | NSIS (current user), portable zip, WebView2 bootstrapper | NSIS through `cargo-packager`, same install directory and uninstall registry key so it upgrades in place; portable zip; no WebView2 |
| macOS | `.dmg`, universal, ad-hoc signed; `.app.tar.gz` for updates | Same: `cargo-packager` `.app` and `.dmg`, `lipo` for the universal binary, `codesign -s -` |
| Linux | AppImage, `.deb`, `.rpm`, needs WebKitGTK 4.1 | AppImage and `.deb` through `cargo-packager`; `.rpm` through `cargo-generate-rpm` (cargo-packager has no rpm); needs a Vulkan loader, xkbcommon and Wayland or X11 instead of WebKitGTK |

**Update continuity.** The first GPUI release is signed with the existing
updater key and listed in the same `latest.json`. Installed 1.0.0-rc.1 apps
then update to it through Tauri's updater: it runs the NSIS installer,
replaces the `.app` and replaces the AppImage, none of which cares what UI
the new build uses. Config, cache and workspace files don't change format, so
nothing needs migrating. M6-27 tests this path on CI runners, as M5-15 did.

## Spec changes (M6-01, its own pull request)

- D2 and section 4: stack, diagram, crates table; ARCH-5 (GPUI in
  `gitcroc-ui` only), ARCH-7 (UI thread budget), ARCH-8 (reason). The "IPC
  types" row goes. New ADR 0004 records the decision and the spike numbers.
- Section 8: UI-23 "CSS variables" becomes "theme tokens".
- Section 9:
  - PERF-1: "DOM" becomes "rendered rows".
  - Proposed new scroll budget: the display's refresh rate up to 144 Hz,
    with under 1% of frames over 1.5 refresh intervals.
  - Memory: "including the webview" becomes "the whole process"; the budget
    is lowered once M6-29 has measured it.
- Section 10: SEC-2 is rewritten (no webview, no CSP; nothing is loaded from
  the network but update packages).
- Section 11: the Webview column becomes the GPU API (DirectX 11, Metal,
  Vulkan); PLAT-4 names the app's own updater.
- Section 12: testing table and QA-1. Section 13: an M6 row. Section 14 and
  `CLAUDE.md`: the stack line, the ARCH-5 invariant, and the removed IPC
  types invariant.
- Decision test `d2_shell_is_tauri_2_and_ui_is_solid_and_strict_typescript`
  becomes a GPUI test. The user docs lose WebView2 and WebKitGTK and get new
  screenshots.

## Tasks (milestone M6)

Each row is one pull request of about 400 changed lines outside tests
(DEV-4). Rows marked † are larger and will be split when their task files
are written. The Tauri app stays the shipped app until M6-30. Until then the
new UI builds as `gitcroc-next` and is not packaged.

| Task | Delivers | Depends on |
| --- | --- | --- |
| **Decide and de-risk** | | |
| M6-01 | Spec edits and ADR 0004 above | decisions below |
| M6-02 | Spike: conflict editor on gpui-component's editor. Editable result, hunk highlights, per-hunk buttons, IME, a 10,000-line file at 144 fps | — |
| M6-03 | Spike: CI. GPUI headless tests on all three runners; a real window under xvfb with lavapipe and with WARP; `cargo-packager` builds NSIS, dmg, AppImage and deb of a hello window; build time and binary size | — |
| M6-04 | Spike: accessibility. Orca, NVDA and VoiceOver read a labelled button and a list row; can a test read the AccessKit tree | — |
| **Foundation** | | |
| M6-05 † | `gitcroc-app` service API and event bus without Tauri; the Tauri commands become wrappers (about 3 PRs: workspace, tree and history; changes and operations; AI, update and askpass) | M6-01 |
| M6-06 | `gitcroc-ui` skeleton: window, own title bar, three panes, status bar, strings, keymap scaffold, macOS menus, CI test job, `cargo deny` | M6-03, M6-05 |
| M6-07 | App tokens light, dark and system; accent colour; fonts; 8 code themes on `syntect`; contrast tests (UI-23 to UI-26) | M6-06 |
| **Read-only parity (M1)** | | |
| M6-08 | Git setup screen and git path (PLAT-3) | M6-06 |
| M6-09 | Workspace tabs and workspace dialogs (UI-27 to UI-33) | M6-06 |
| M6-10 | Tree pane: rows, summaries, filter, selection and multi-selection (UI-1 to UI-8 without menus and drag) | M6-09 |
| M6-11 † | History pane from the spike: columns, search, uncommitted row, merged list, summary strip (UI-9 to UI-12, UI-36) | M6-10 |
| M6-12 | Changes pane: commit header, flat and tree file lists, parent selector (UI-14, UI-16) | M6-11 |
| M6-13 † | Diff view: inline and side-by-side, highlighting, wrap, context, whitespace, binary, image, Load diff, docking (UI-17 to UI-19) | M6-12, M6-07 |
| M6-14 | `just bench-ui` on GPUI; single-repo budgets measured against the new scroll budget | M6-13 |
| **Writes (M2)** | | |
| M6-15 | Status bar, job list, notifications, askpass dialog | M6-06 |
| M6-16 | Stage, unstage and discard by file and hunk; commit and amend box; fsmonitor offer (UI-15) | M6-13, M6-15 |
| M6-17 | Context menus of tree and history; branch dialogs (UI-7, UI-13) | M6-11, M6-15 |
| M6-18 † | Sync, integrate, stash, tag, reset and remote dialogs | M6-17 |
| M6-19 † | Conflict banner and conflict editor (OPS-2 to OPS-9) | M6-02, M6-16 |
| M6-20 | Clone, add folder, missing repos | M6-15 |
| **Multi-repo (M3)** | | |
| M6-21 | Group rows, Branches node, drag between groups, group dialogs (UI-2, UI-4 to UI-7) | M6-17 |
| M6-22 | Preflight dialog, result report, group commit (UI-34) | M6-21, M6-16 |
| **AI (M4)** | | |
| M6-23 † | AI settings, providers, consent; proposals in the conflict editor; Resolve all | M6-19 |
| **Release (M5)** | | |
| M6-24 | Command palette and keyboard pass (UI-22) | M6-18 to M6-23 |
| M6-25 | Accessibility pass, as far as M6-04 found testable | M6-24 |
| M6-26 | Settings dialog, all four tabs | M6-07, M6-23 |
| M6-27 | Updater without Tauri; update from 1.0.0-rc.1 tested on CI runners | M6-05 |
| M6-28 | Packaging and release workflow; install tests | M6-27 |
| M6-29 | Smoke test on Linux and Windows; idle memory; full budget run; user doc screenshots | M6-24 to M6-28 |
| M6-30 | The switch: `gitcroc` is the GPUI binary; `ui/`, Tauri, specta, `gtk` and Node leave the repo and CI; coverage scans Rust only | `just coverage M6` empty |
| M6-31 | Release candidate 1.0.0-rc.2 and the M6 sign-off | M6-30 |

Parallel lanes: M6-02, M6-03 and M6-04 can run together, and beside M6-01.
After M6-06, the lanes M6-08, M6-09 to M6-14 and M6-15 to M6-20 touch
separate files. In all, about 40 pull requests once the † rows are split.

## Risks

| Risk | Mitigation |
| --- | --- |
| GPUI is pre-1.0, and on crates.io only as a third-party repackaging (`gpui-pre` through gpui-kit); its API changes often | Pin exact versions. Upgrade in dedicated tasks only. Keep gpui-component use behind our own small widgets where an upgrade has bitten before. Zed ships on it on all three platforms, which keeps the platform layer healthy. |
| The conflict editor has no proven component | M6-02 decides before M6-19; the fallback is per-hunk editable blocks. |
| CI runners have no GPU | M6-03. Headless tests need no GPU at all; only the smoke test needs a real window. Fallback: smoke on Linux only, or on a self-hosted runner. |
| Accessibility is younger than in a browser; the axe checks of M5-10 have no equivalent | M6-04, then an explicit spec decision on what M6-25 must meet. |
| Linux needs Vulkan; X11 reports no refresh rate (seen in the spike) | Document the requirement. Use wgpu's GL fallback if M6-03 shows it works. On X11, take the refresh interval from measured frames. |
| Plain GPUI text cannot be selected | Read-only inputs where copying matters: hashes, messages, paths. Copy items in context menus. |
| The NSIS upgrade from the Tauri install lands beside it instead of over it | Match Tauri's install directory and registry keys; M6-27 tests it on Windows. |
| Build time and binary size grow (wgpu, naga, syntect) | Measured in M6-03; CI caches; release builds only in the build workflows. |
| Licences | GPUI and gpui-component are Apache-2.0, `syntect` is MIT, so all are compatible with MIT; `cargo deny` checks in M6-06. |

## Decisions needed before M6-01

1. **v1.0.0 on Tauri or on GPUI.** Recommended: hold v1.0.0 (M5-30) and
   release the GPUI build as 1.0.0-rc.2, then 1.0.0. Releasing 1.0.0 on
   Tauri means running the install and update verification twice, for a UI
   about to be thrown away. rc.1 users update to rc.2 either way.
2. **Scroll budget.** Raise "60 fps" to the display's refresh rate up to
   144 Hz, as proposed above.
3. **Linux requirement.** Replace WebKitGTK 4.1 with a Vulkan-capable driver
   (Mesa covers Intel and AMD; lavapipe as a software fallback).
4. **Side by side on `main`, not a long-lived branch.** Recommended, so every
   task is a normal pull request and the shipped app never breaks.
5. **Look.** Recommended: port today's tokens, so the app keeps its look,
   with the OS accent colour and native window controls added, rather than
   adopting gpui-component's default theme.
