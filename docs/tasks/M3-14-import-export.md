# M3-14 Import and export of workspaces, and clone into a group

## Goal
A team shares a workspace: the user exports it to a file with relative paths, and a teammate imports it, sees which repos are missing, and clones them from their remotes into the right place and groups.

## Depends on
M3-02, M3-04

## Spec
Implements: DATA-2
Respects: DATA-1, DATA-3, GRP-1, GRP-2, PLAT-2, SEC-3, D10, UI-28, section 6 "Repos" row (clone into a group), section 10 workspace file format

## In scope
- Export (core and the tab's context menu, **Export workspace…**, with a native save dialog from Rust as in M1-05):
  - It writes the section 10 format with `schema`, `name`, every `[[repo]]` with `id`, `path` and `remote`, and every `[[group]]`, written atomically (DATA-1).
  - Each path is relative to the exported file's folder, with `..` segments where needed and `/` separators on every platform.
  - A path that cannot be made relative (another drive on Windows) stays absolute, and the export reports which ones.
  - The pane layout and the offer answers (commit-graph, fsmonitor) are left out, since they are personal.
- Import (core and the "+" menu's **Import workspace…**, with a native open dialog):
  - It reads the file with the DATA-1 rules and resolves relative paths against the file's folder.
  - It creates a new workspace (new uuid, the file's name, " (2)" and so on appended when the name is taken) and opens it as the active tab.
  - Repos and groups keep their ids, order and membership. A repo whose path exists and is a repo is added as in M1-05 (GRP-1). A repo whose path is missing is kept in the workspace with the missing-path warning (UI-3). Its path is stored resolved and absolute, normalized but not canonical, since it cannot be canonicalized until it exists.
- Missing repos (DATA-2): after import, a dialog lists the repos whose path is missing, with their `remote`:
  - Each row has a check box, checked when it has a remote, and the target path, editable as parent folder plus name as in M2-14.
  - **Clone selected** starts one M2-14 clone job per repo into its target, all at once through the shared pool. A repo without a remote cannot be cloned and says so.
  - A clone that ends well fills in that repo's path, so it keeps its id and groups. A failed clone keeps it missing, with git's message.
  - The dialog can be reopened later from a missing repo's context menu (**Clone from remote…**).
- Untrusted input (SEC-3): the imported file's names, paths and URLs are shown as text only. A URL is passed to git as one argument after `--` (ARCH-3), and paths are checked like any user-chosen path (M2-14's refusal of a non-empty target).
- Clone into a group (section 6 "Repos" note): the M2-14 clone dialog gains an optional **Add to group** picker (none, a group, or a new group name checked by GRP-2). On success the repo is added to it.
- `gitcroc-cli workspace export <id> <file>` and `import <file>`.
- Tests:
  - `data_2_export_writes_paths_relative_to_the_file`: nested, sibling and parent folders, with `/` separators. On Windows another drive stays absolute (skipped elsewhere).
  - `data_2_export_then_import_round_trips_repos_groups_and_ids`, into a different folder that has the same layout.
  - `data_2_import_lists_missing_repos_and_clones_them_from_remote`, with file remotes. The cloned repo keeps its id and groups.
  - Import of a newer schema is refused with the DATA-1 warning, and nothing is created.
  - A name collision appends " (2)".
  - Clone into a group: an existing group, and a new name refused when it collides (GRP-2).
  - The import dialog (Vitest): rows, checks, Clone selected.

## Out of scope
- Syncing a shared workspace file after import. Import makes a copy.

## Acceptance
- A workspace exported on one machine and imported on another with a different home folder opens with the same repos and groups, and the missing ones clone from their remotes.
- All tests listed above exist and pass on all three platforms, and `just check` passes.
