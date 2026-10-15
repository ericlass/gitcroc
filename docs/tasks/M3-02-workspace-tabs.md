# M3-02 Workspace tabs

## Goal
The user works with several workspaces in tabs: opens, creates, switches, reorders, renames and closes them. Each tab comes back exactly as it was left.

## Depends on
M3-01

## Spec
Implements: UI-28, UI-29
Respects: UI-27, UI-33, ARCH-6, ARCH-9, OPS-9, UI-22, section 8 window layout

## In scope
- Tab bar (`ui/src/layout/TabBar.tsx`):
  - One tab per open workspace, in the core's order, showing the workspace name.
  - Clicking a tab activates it. Tabs reorder by dragging, and also with Ctrl/Cmd+Shift+PageUp/PageDown, since everything must be reachable without a mouse (UI-22). Each tab has a close button.
  - A tab's context menu offers Rename and Close.
- The "+" button opens a menu:
  - It lists every workspace file, open ones marked. Choosing one opens it, or switches to it if it is already open (UI-27).
  - **New workspace…** asks for a name and shows the core's refusal inline.
  - **Import workspace…** is present and disabled until M3-14.
  - **Delete…** is offered for a workspace that is not open. Its confirmation says that the repos on disk are not touched.
- Closing a tab that holds unsaved conflict editor state (OPS-9) asks first, naming the files. Running jobs need no confirmation: they keep running and stay in the job list.
- UI-29: each tab keeps its own state, keyed by workspace id (ARCH-9), and switching back restores it exactly:
  - tree selection, expanded nodes, tree scroll position and filter text;
  - history position (M1-08), selected commit and changes pane file;
  - pane sizes and dock (already in the workspace file);
  - unsaved conflict editor buffers (OPS-9).
  Audit each store and component for state that is still global or keyed by repo alone, and fix it.
- The toolbar, the panes and the status bar show the active workspace. The job list keeps covering all tabs (UI-21).
- The empty state when no tab is open: a short text and the "+" menu's actions.
- Tests (Vitest against the fake core):
  - `ui_28_plus_menu_lists_workspaces_and_offers_new_and_import`.
  - `ui_28_tabs_reorder_by_drag_and_keyboard_and_close`.
  - `ui_29_switching_tabs_restores_selection_expansion_scroll_filter_and_sizes`: change each piece in tab A, switch to B and back, and assert each one.
  - `ui_29_unsaved_conflict_edits_survive_a_tab_switch` and the confirmation on closing such a tab.
  - The fake core (`ui/src/ipc/core.ts` fakes) gains several workspaces.

## Out of scope
- Releasing a background tab's history and diffs, badges and markers (M3-03).
- The tab-switch first-paint budget (measured in M3-15).
- Import (M3-14).

## Acceptance
- With `just dev`, two workspaces of different repos are open in two tabs, and switching back and forth restores each tab's state exactly.
- All tests listed above exist and pass, and `just check` passes.
