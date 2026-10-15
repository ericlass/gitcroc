# M3-01 Several workspaces in the core

## Goal
The core holds several open workspaces at once: it lists every workspace file, creates, opens, closes, renames and deletes workspaces, and restores the open ones, their order and the active one on restart.

## Depends on
M2

## Spec
Implements: UI-27, UI-33
Respects: DATA-1, DATA-3, GRP-3, ARCH-5, ARCH-7, ARCH-9, section 3 "Workspace", section 10 "Files" table

## In scope
- `gitcroc-core` `Workspaces`:
  - `list`: every file in `workspaces/` with its id, name and whether it is open, sorted by name. A file that cannot be read is listed with its warning and opens read-only (DATA-1).
  - `create(name)`: a new file, opened as the last tab and made active. Names are trimmed, 1 to 64 characters, and unique among the workspace files, ignoring case, as GRP-2 has for groups.
  - `open(id)`: a workspace already open is not opened twice; the call makes it active (UI-27). Otherwise it is opened as the last tab and made active.
  - `close(id)`: drops it from the open list. Its file, its repos and anything on disk stay (UI-28). Closing the last tab leaves no workspace open; the window then shows an empty state with the "+" menu.
  - `rename(id, name)` with the rules of `create`. `delete(id)` removes the file, and only that file, of a workspace that is not open (GRP-3 applies by analogy). It needs `confirmed: true`.
  - `reorder(ids)` and `activate(id)`.
- Session (UI-33): an optional `[session]` table in `settings.toml`, schema unchanged: `tabs`, the open workspace ids in tab order, and `active`. It is saved on every open, close, reorder and activate. On start the core opens those workspaces. A missing file is left out, and a warning names it. Without a `[session]` table, the M1 rule applies: open the most recently modified file, or create "My workspace".
- Per-workspace state already keyed by workspace id (ARCH-9) is created on open and dropped on close: watchers, tree, history and changes views, and the summary refresh queue. Closing a tab does not cancel its jobs. Their results stay in the job list, labelled with the workspace's name (UI-21).
- Only the active workspace is watched and refreshed by focus. On activate, the core refreshes the workspace's summaries as the focus refresh does. The rest of UI-30 and UI-31 is M3-03.
- IPC: `workspace_list`, `workspace_create`, `workspace_open`, `workspace_close`, `workspace_rename`, `workspace_delete`, `workspace_reorder`, `workspace_activate`. `workspace_get` returns the open workspaces in tab order, together with the active id. Each change emits `workspaces_changed`.
- `gitcroc-cli workspace list|create|open|close|rename|delete`, with `--config-dir`.
- Tests:
  - `ui_27_opening_an_open_workspace_switches_to_it`: no second copy, the existing one becomes active.
  - `ui_33_open_tabs_order_and_active_tab_survive_a_restart`: three open in a given order with the second active, then `load` again restores the same order and the same active tab.
  - `ui_33_a_missing_workspace_file_is_left_out_with_a_warning`.
  - Names: blank, 65 characters, and a case-insensitive duplicate are refused before any file is written. A rename to the same name with a different case is allowed.
  - Delete refuses an open workspace and an unconfirmed request, and leaves the repos' folders byte-identical.
  - Closing a workspace with a held job: the job still ends and stays in `jobs_get`.

## Out of scope
- The tab bar and the "+" menu (M3-02).
- Background-tab memory release and tab badges (M3-03).
- Import and export (M3-14).

## Acceptance
- `gitcroc-cli workspace create Java` followed by `create "C#"` gives two files. After a restart of the CLI's core with the same config dir, both are open and the last one is active.
- All tests listed above exist and pass on all three platforms, and `just check` passes.
