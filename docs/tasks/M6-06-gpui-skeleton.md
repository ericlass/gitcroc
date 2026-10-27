# M6-06 The `gitcroc-ui` skeleton

## Goal
A new crate, `gitcroc-ui`, builds the GPUI app as `gitcroc-next`. It opens one window with gitcroc's own title bar, the three panes and the status bar, all still empty. It runs on `App` from M6-05, with strings, a keymap and the macOS menus in place. CI builds and tests it on all three platforms, and `cargo deny` accepts its dependencies. Every later M6 task fills this skeleton.

## Depends on
- M6-03: its runner results decide the CI steps below.
- M6-05: PR 2 at least, for `App` and `AppEvent`.

## Spec
Implements: UI-35
Respects: ARCH-5, ARCH-6, ARCH-7, ARCH-9, UI-19, UI-21, UI-22, D15, PLAT-1, SEC-2, QA-1, QA-3, QA-4, DATA-1

## In scope
- **The crate.** `crates/gitcroc-ui` is a workspace member with a binary named `gitcroc-next`. The binary is not packaged and not released until M6-30.
  - **Dependencies:**
    - `gitcroc-app`;
    - `gpui-kit` and `gpui-component`, pinned in the workspace at the exact versions the spikes used (`=0.8.1`);
    - `test-support` as a dev-dependency feature only.
  - **gpui-kit's default `assets` feature** is turned off if the app builds without it, because it brings in an HTTP client (M6-03 finding). If the app can't build without it, record why.
- **Startup** (`main.rs`):
  - **Runtime:** a multi-thread tokio runtime is built, and `App::start` gets its handle and the same config, cache and log directories as the Tauri app. GPUI runs on the main thread.
  - **Same files as the Tauri app:** `gitcroc-next` reads and writes the same settings and workspaces as the shipped app, so it can be tried on real workspaces. The core writes files atomically (DATA-1). Running both apps at once is not supported, and the `just dev-next` recipe's comment says so.
  - **Dialogs:** a `Dialogs` implementation on `cx.prompt_for_paths` and `cx.prompt_for_new_path`.
- **The event bridge:**
  - One task per window receives `AppEvent`s and updates the GPUI entities on the main thread.
  - Service calls start on tokio and are awaited in `cx.spawn`. No core work runs on the UI thread (ARCH-7).
  - On `Lagged`, the window reads again the state its entities show (ARCH-6).
- **The window:**
  - **Contents:** a title bar row, a toolbar row, three resizable panes (tree, history, changes), and a status bar.
  - **Built from:** gpui-component's resizable panels. Each pane is an empty view entity for its task to fill.
  - **Per tab:** layout state is kept per workspace (ARCH-9), even before tabs exist (M6-09).
- **The title bar (UI-35):**
  - **Client-side decorations:**
    - Windows and Linux: minimize, maximize or restore, and close buttons at the right end.
    - macOS: the system's traffic lights over a transparent title bar, with room for them at the left end.
  - **Empty space:** a drag on it moves the window, through `start_window_move`. A double-click maximizes or restores the window.
  - **Windows Snap Layouts:** they work through the maximize button's hit-test area.
  - **The buttons:** labelled for screen readers and left out of the tab order.
  - **The setting:** with `system_title_bar` on, the window opens with system decorations. Applying a change at once comes with the settings dialog (M6-26).
  - **Headless tests:** the buttons and the drag reach the window through a small `WindowControls` trait, so tests can assert what was asked.
- **Strings (D15).**
  - `strings.rs` holds the strings the skeleton shows.
  - A test fails on literal UI text in `crates/gitcroc-ui/src`, outside `strings.rs` and the tests. It does what `ui/src/strings/strings.test.ts` does for the web front end.
  - Each later task ports its own strings.
- **The keymap (UI-22).**
  - GPUI actions, and `KeyBinding`s with key contexts. `secondary-` gives Cmd on macOS and Ctrl elsewhere.
  - The bindings live in one table, the same shape as `ui/src/keys.ts`.
  - The test that checks every shortcut is listed in `docs/user/keyboard.md` is ported. While the web front end still exists, it checks the bindings that `gitcroc-ui` has.
- **macOS menus:** App (About, Quit), Edit (Undo, Redo, Cut, Copy, Paste, Select All) and Window (Minimize, Zoom), through `cx.set_menus`. This makes Cmd+Q, copy and paste work in inputs.
- **The theme:** until M6-07 the skeleton uses gpui-component's theme and follows the system's light or dark appearance. No colour is written in a view.
- **`cargo deny`** (`deny.toml`), from M6-03's findings, each entry with a comment naming the crate that needs it:
  - allow 0BSD (`enum-iterator`);
  - a CC0-1.0 exception for `tiny-keccak`;
  - ignores for the four "unmaintained" advisories: `instant`, `rustls-pemfile`, `rustybuzz` and `ttf-parser`.
- **Workspace policy** (`workspace_policy.rs`):
  - `MEMBERS` gains `gitcroc-ui`.
  - `ALLOWED_EDGES` gains `gitcroc-ui` to `gitcroc-app`, `gitcroc-core`, `gitcroc-git` and `gitcroc-ai`.
  - New test `arch_5_only_the_ui_crate_depends_on_gpui`: no other workspace crate has a `gpui` crate in its dependency tree.
- **Parity tracking** (`tools/req-coverage`):
  - For milestone M6, the report also lists as missing every requirement ID that has a test under `ui/` but none under `crates/gitcroc-ui/`.
  - `just coverage M6` then lists what is still to port. M6-30 starts when that list is empty.
- **CI** (`ci.yml`), following what M6-03's runner jobs proved:
  - **`check-rust` on all three runners:**
    - builds and tests `gitcroc-ui` with the rest of the workspace, with the Linux packages `gitcroc-ui` needs;
    - its headless tests need no display or GPU.
  - **On each runner where M6-03 showed a real window works:** `gitcroc-next --exit-after-first-frame` starts and presents a frame. On Linux this runs under `xvfb-run` with Mesa's lavapipe. Where it doesn't work, the step is left out with a comment linking the M6-03 finding.
  - **Build time:** record in the PR how much longer `check-rust` takes than on `main`, with the cache warm.
- **`just dev-next`** runs `gitcroc-next`. `just check` covers the crate through `cargo test --workspace`.

## Tests (QA-3)
- `ui_35_title_bar_buttons_per_platform`:
  - minimize, maximize and close on Windows and Linux;
  - none on macOS, but the room for the traffic lights;
  - none with `system_title_bar` on;
  - the maximize button shows restore while the window is maximized.
- `ui_35_title_bar_drag_and_double_click`:
  - a press on the empty space asks `WindowControls` to move the window;
  - a double-click asks it to toggle maximize;
  - a press on a button does neither.
- `ui_35_window_buttons_are_labelled_and_not_in_the_tab_order`.
- `arch_6_window_reads_state_again_after_lag`: with a tiny channel, a lagged window reads its state again and shows the latest.
- `arch_7_service_calls_do_not_run_on_the_ui_thread`: a call to a service method that blocks for 100 ms leaves the window rendering frames.
- `arch_5_only_the_ui_crate_depends_on_gpui` (workspace policy).
- `d15_ui_text_comes_from_the_strings_module` (the scan above).
- The ported keymap docs test.
- In `req-coverage`'s tests, `qa_3_m6_lists_ui_tests_not_yet_ported`, on a fixture tree.

## Out of scope
- Any pane's content, the tab bar's tabs, the toolbar's buttons and the status bar's jobs (M6-08 to M6-15).
- App tokens, the accent colour, fonts and code themes (M6-07).
- Packaging `gitcroc-next` (M6-28).
- The accessibility pass (M6-25). This task only labels the title bar buttons.

## Acceptance
- `just dev-next` opens the window on the reference machine: Linux, X11 and Wayland.
- The window can be moved, maximized, restored, minimized and closed from its own title bar.
- CI runs the tests above on Windows, macOS and Linux, and the first-frame step wherever M6-03 allows.
- `cargo deny check` passes with the entries above and no others.
- `just coverage M6` runs and lists the UI requirements still to port.
- `just check` passes.
