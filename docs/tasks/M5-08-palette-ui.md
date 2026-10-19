# M5-08 Command palette in the UI

## Goal
Ctrl/Cmd+K, or the toolbar's Command palette button, opens the palette: the user types, moves through the list with the keyboard, and runs an operation or selects a repo, group or branch in the tree (UI-22).

## Depends on
M5-06, M5-07

## Spec
Implements: UI-22 (the palette; M5-09 completes keyboard reach)
Respects: ARCH-6, ARCH-9, AI-3, AI-16, UI-6, UI-8, UI-20, GRP-5, GRP-9, PERF-1, OPS-9, D15, section 9 "UI input latency"

## In scope
- Opening:
  - Ctrl+K (Cmd+K on macOS) anywhere in the main window, also inside the diff and conflict editors, and the toolbar's Command palette button, which is enabled now (UI-20).
  - Not while another dialog is open. Pressing the key again, or Escape, closes it; focus goes back where it was.
- The window: a text box above a list, as a modal dialog using `dialogs/parts.tsx`. The list is virtualized (PERF-1). Each row shows its kind, its text and, for an operation, its keyboard shortcut if it has one (M5-09 adds the keymap; until then none show). A line under the list says when more items matched than are shown ("100 of 340, keep typing").
- Typing sends `palette_query` for the active workspace on each change; a superseded answer is dropped. The list keeps the previous answer until the new one arrives, so typing never waits (section 9 input latency).
- Keys: Up, Down, PageUp, PageDown, Home and End move the highlight; Enter runs the highlighted row; Escape closes.
- Running a row:
  - An operation runs exactly what its toolbar button, menu entry or pane control runs, through the same function: the same dialog, the same preflight for a target set of several repos, the same confirmation for a destructive one (GRP-9). Resolve with AI without a provider opens the provider settings (AI-3).
  - Whether an operation can apply now comes from the same rules the toolbar and the menus use; the palette adds none. One that cannot apply is shown disabled with the reason, as the toolbar's tooltip gives it, and Enter does nothing on it.
  - An operation whose subject the selection does not give (Rename branch while a repo is selected, Revert without a selected commit) is disabled with a reason naming what to select. The palette never adds a dialog, or a way to run an operation, that the window does not already have.
  - A repo or group selects it in the tree, as a click does: the tree scrolls to it and the history follows. A branch selects its row in the tree, expanding its repo and folders. The palette closes, and focus goes to the tree.
- The palette's text comes from the strings module (D15); operation labels come from the core (M5-07).
- Tests:
  - Vitest with the fake core: the fake answers `palette_query` with the same matching as the core's tests, enough for the UI tests; the UI itself filters nothing.
  - Playwright (`ui_22_palette_*`):
    - opens by key and by button, closes by Escape and by the key, and returns focus;
    - finds and runs an operation: Fetch on a group goes through its preflight; Hard reset shows the destructive confirmation;
    - a disabled operation shows its reason and Enter does nothing;
    - selecting a repo, a group and a remote branch changes the tree selection and the history;
    - the list never holds more rows in the DOM than fit plus overscan (PERF-1);
    - `ui_22_every_palette_operation_runs_its_action`: for each operation id the core lists, choosing it calls the same handler as its toolbar, menu or pane entry (a table over all ids; a missing handler fails TypeScript's exhaustiveness check first);
    - while AI is off the AI entries are absent (AI-16).

## Out of scope
- New keyboard shortcuts and the keymap (M5-09).

## Acceptance
- The tests above exist and pass, and `just check` passes.
- In `just dev` on `target-project`, typing in the palette while the focus refresh runs stays responsive; noted in the pull request (no test asserts a time).
