# M6-12 Changes pane

## Goal
The GPUI changes pane shows what the history's selected row is, as rc.1's
does: a commit's header and changed files, flat or as a folder tree, with
a parent selector for a merge commit; or the uncommitted lists of a repo,
or of a group's or several repos' dirty repos, sectioned by repo. A file can
be selected; its diff is M6-13.

## Depends on
- M6-11d: the history pane reports its selection, the merged uncommitted
  row included.

## Spec
Implements: UI-14, UI-16
Respects: UI-15, UI-34, PERF-1, ARCH-6, ARCH-7, ARCH-8, ARCH-9, PLAT-2, OPS-3, OPS-7, GRP-8, UI-22, UI-23, UI-29, SEC-3, D15, QA-3, QA-4

UI-15 and UI-34 are respected, not implemented: this task shows their file
lists; staging, the commit box and the group commit come later (see Out of
scope).

## In scope
- **One changes pane per open workspace** (ARCH-9, UI-29), made with its
  history, as the tree and history panes are. It replaces the empty
  `Pane` in `window.rs`.
- **Following the history** (ARCH-6):
  - the pane shows the active tab's history selection: a commit of a repo,
    a repo's uncommitted row, or the merged uncommitted row;
  - a commit is read with `changes_get` and `ChangesTarget::Commit`, the
    uncommitted rows with `changes_get` and `ChangesTarget::Uncommitted`
    or with `changes_get_merged` (ARCH-7);
  - no selection says "Select a commit or the uncommitted changes to see
    what changed.", and a failed read says why;
  - a newer selection drops an older answer, and a `None` answer that is
    still the newest asks again, as rc.1's store does.
- **The commit header** (UI-14), a port of `ui/src/changes/CommitHeader.tsx`:
  - the full hash, the author and the committer with name, email and date,
    the parents and the full message;
  - a parent is a link that selects that commit in the history when its
    row is loaded;
  - all of it is repo text, rendered as text (SEC-3). Copying the hash is
    the commit menu's (UI-13, M6-17).
- **The parent selector** (UI-16): a merge commit has "Compare with" with
  one entry per parent, the first parent by default; choosing one reads
  the file list against it.
- **The file lists** (UI-14), a port of `ui/src/changes/fileRows.ts` and
  the read-only parts of `FileList.tsx`:
  - a commit has one list; uncommitted changes have Staged and Unstaged
    sections, and an Unmerged section first while files are unmerged,
    always by full path (OPS-3);
  - a merged view has the same sections with one part per repo, in
    target-set order, each headed by the repo's name; a blocked repo says
    why (GRP-8);
  - each file shows its status letter, named for screen readers, its path
    or name, its old path after a rename, its conflict kind, and its added
    and removed line counts;
  - "Flat" and "Tree" switch the lists between full paths and a folder
    tree. A folder whose only child is a folder shows as one row, and
    folders can be collapsed. The choice is the `file_list` changes
    setting, kept with `settings_set_changes` for every workspace;
  - the files header shows the number of files;
  - the lists are one virtualized list (PERF-1).
- **The selected file** (ARCH-8, PLAT-2):
  - files are named by handle, list and id, never by path;
  - a new view selects its first file; a click selects a file, and Up,
    Down, Home and End move the selection between files; a click on a
    folder collapses or expands it, as in rc.1;
  - the diff area names the selected file, with its old path and mode
    change. The diff itself is M6-13.
- **Refresh** (OPS-7): a `RepoSummary` read anew for a repo whose working
  copy is shown, a member's for a merged view, reads the view again. The
  selected file is kept by list and path while it still exists, else in the
  other list, else the first file is selected.
- **Per tab** (UI-29): each tab's pane keeps its selection, parent, view,
  selected file, collapsed folders and list position.
- **Regions (UI-22):** F6's changes region is the active tab's changes
  pane, with the focus on the file list.
- **Strings (D15):** the changes pane's text for this task moves from
  `ui/src/strings/en.ts` to `strings.rs`.

## Tests (QA-3)
- `ui_14_commit_header_and_file_list`:
  - a commit shows its full hash, author, committer, parents and message;
  - its files are listed with status, rename and line counts, and the first
    is selected;
  - clicking a parent selects that commit in the history.
- `ui_14_file_list_flat_and_tree`:
  - "Tree" shows folders with their files, a single-child folder as one
    row, and a folder collapses and expands;
  - the choice is kept in the settings and applies to every tab;
  - the keys move the selection between files.
- `ui_16_merge_commit_parent_selector`: a merge commit compares with its
  first parent; choosing the second reads its files against that parent;
  a commit with one parent has no selector.
- `ui_15_uncommitted_lists_staged_unstaged_and_unmerged`: Staged, Unstaged
  with untracked files, and Unmerged first in a conflict.
- `ui_34_merged_lists_are_sectioned_by_repo`: each section has one part
  per dirty repo, in target order, under the repo's name.
- `ops_7_changes_read_again_when_the_working_copy_changes`: a new file
  appears, and the selected file stays selected.
- `ui_29_each_tab_keeps_its_changes`: selection, file and list position
  come back after a tab switch.
- `perf_1_the_file_list_lays_out_only_the_rows_on_screen`.
- `sec_3_commit_text_and_paths_render_as_text`: a message, author and path
  with markup show as written.
- Unit tests of the rows: the flat and tree rows, single-child folders,
  collapsed folders, the merged sections, and keeping the selected file.
- `ui_23_no_hardcoded_colors`, `d15_ui_text_comes_from_the_strings_module`
  and the keymap docs test keep passing over the new files.

## Out of scope
- **The diff** (UI-17, UI-18), the diff options and the dock button
  (UI-19): M6-13.
- **Stage, unstage and discard, multi-selection of files, the commit and
  Amend box, the fsmonitor offer** (UI-15, PERF-7): M6-16. The M6-11c and
  M6-11d task files named M6-12 for the commit box; the plan table puts it
  in M6-16, after the diff it needs.
- **The group commit box** (UI-34): M6-22, with the preflight it runs.
- **The conflict editor** for an unmerged file (OPS-3): M6-19.
- **Background tabs releasing their view** (UI-31): M6-14.
- **The palette's file operations** (UI-22): M6-24.

## Acceptance
- **On the reference machine,** `just dev-next` on `target-project`:
  - selecting a commit shows its header and files, flat and as a tree;
  - a merge commit switches parent;
  - the uncommitted row shows Staged and Unstaged, and a dirty group's
    merged row shows them sectioned by repo;
  - a tab switch keeps each tab's changes pane.
- `just coverage M6` no longer lists UI-14 or UI-16.
- `just check` passes.
