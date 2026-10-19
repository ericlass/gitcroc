# M5-09 Keyboard pass

## Goal
Every operation of sections 5 and 6 is reachable without a mouse from where it lives in the window, not only through the palette, and a test proves it operation by operation (UI-22).

## Depends on
M5-08

## Spec
Implements: UI-22 (every operation reachable without a mouse)
Respects: ARCH-6, OPS-3, OPS-6, OPS-9, AI-11, UI-6, UI-7, UI-13, UI-15, UI-20, UI-21, UI-28, GRP-9, D15, section 12 "Front end flows" row

## In scope
- One keymap, `ui/src/keys.ts`: every app-wide shortcut with the platform's modifier, used by the handlers and by the palette's shortcut column (M5-08). Shortcuts (README "Keyboard shortcuts"):
  - Ctrl/Cmd+K: command palette (M5-08).
  - F6 and Shift+F6: move focus to the next or previous region: tab bar, toolbar, tree, history, changes, status bar. Focus lands on the region's selected row or first control.
  - Ctrl/Cmd+, : Settings.
  - Ctrl/Cmd+PageDown and Ctrl/Cmd+PageUp: next and previous tab. Moving a tab stays Ctrl/Cmd+Shift+PageUp/PageDown (M3-02).
  - F7 and Shift+F7: next and previous hunk in the diff and in the conflict editor; the focus goes to the hunk's first control.
  - Kept as they are: Shift+F10 and the context menu key for menus, Ctrl/Cmd+Enter to commit, the arrow keys in lists, the splitters' keys.
  - None may shadow a CodeMirror editing key; the pull request lists the CodeMirror keymap that was checked.
- Close the gaps the audit finds, at least:
  - a diff hunk's stage, unstage and discard controls (UI-15) are buttons reachable by Tab and F7;
  - the conflict editor's take ours, take theirs and take both (OPS-3), and OPS-6's choice between versions, are reachable by keyboard; Accept, Reject and Edit of a proposal (AI-11) keep their keys;
  - the history's commit menu (UI-13) opens from the keyboard on the selected commit, and a multi-selection of commits for cherry-pick can be made with Shift+arrows and Ctrl/Cmd+Space;
  - the changes pane's file lists: Space stages or unstages the selected file, Delete (Backspace on macOS) asks to discard it (GRP-9's confirmation);
  - the status bar opens the job list by keyboard, and the job list and result report are navigable (UI-21);
  - the "+" menu and the tab context menu (UI-28) open from the keyboard.
- The user documentation of the keymap is M5-18's, written from `keys.ts`.
- Tests (Playwright against the fake core; no `click`, no mouse event, only `keyboard`):
  - `ui_22_every_operation_reachable_without_a_mouse`: a table with one row per operation of sections 5 and 6 (the same list as M5-07's test). Each row starts from a fixed state, reaches the operation through its own place in the window (toolbar, menu, pane control), not the palette, and asserts that its dialog opened or the fake core received its request.
  - `ui_22_f6_cycles_the_regions`, `ui_22_f7_moves_between_hunks`, `ui_22_tab_switching_keys`, one each.
  - Vitest: `keys.ts` has no two shortcuts on the same keys, and each has a label in the strings module.

## Out of scope
- Screen reader and contrast work (M5-10).
- Custom key bindings.

## Acceptance
- The tests above exist and pass, and `just check` passes.
- The pull request lists each gap found and how it was closed.
