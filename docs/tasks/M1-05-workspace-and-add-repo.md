# M1-05 Workspace file and adding repos

## Goal
The user's repos are kept in a workspace file: repos can be added one at a time or by scanning a parent folder, and removed, from the core, the CLI and IPC.

## Depends on
M1-01

## Spec
Implements: GRP-1, GRP-3, PLAT-2
Respects: DATA-1, DATA-2, ARCH-5, ARCH-7, ARCH-9, SEC-2, SEC-6, section 3 "Repo" and "Workspace", section 6 "Repos" row, section 10 "Files" table and workspace file format

## In scope
- `gitcroc-core` workspace model and file:
  - `workspaces/<uuid>.toml` in the config dir, in the section 10 format: `schema = 1`, `name`, and one `[[repo]]` per repo with `id`, `path` and `remote` (the URL of `origin`, or of the only remote, or absent). No `[[group]]` entries are written yet (M3), but a file that has them loads and keeps them on save.
  - Written and loaded with the DATA-1 rules already used for `settings.toml` (atomic write, schema migration hook, newer schema read-only with a warning, a corrupt file is never overwritten). Reuse that code rather than copying it.
  - Until workspace tabs exist (M3), the app has one workspace: on start it opens the most recently modified workspace file, or creates one named "My workspace" when there is none. The workspace id is the file's uuid and replaces the M0 placeholder id everywhere (ARCH-9).
  - Repo ids are `r1`, `r2` and so on, the next unused number. A repo's display name is its folder name.
- Adding (section 6 "Add existing folder"):
  - A repo is keyed by its canonical absolute path; adding it again is a no-op that reports "already in workspace" (GRP-1). On Windows the stored path has no `\\?\` prefix when it can be expressed without one.
  - The folder is a repo with a working tree: add it. A bare repo is refused (section 3). A folder that is not a repo is scanned for repos to a depth of 4 below it. The scan does not descend into a repo it found, does not follow symlinks and skips folders it cannot read.
  - Adding is a job with progress (ARCH-7): the result lists added repos, repos already present, and refused or unreadable folders with the reason.
  - Git's safety errors (for example `safe.directory`) are reported with git's message, never bypassed (SEC-6).
- Removing a repo from the workspace changes only the workspace file, never anything on disk (GRP-3).
- Paths are `PathBuf` end to end and converted to strings only for display and for the TOML file (PLAT-2). A path that is not valid Unicode is refused with a clear message, because the TOML file cannot store it.
- Folder picker: a command that opens the native folder dialog from Rust through `tauri-plugin-dialog`'s Rust API. The front end gets no dialog permission, so the capability file still lists only events (SEC-2).
- IPC commands, generated as usual: `workspace_get`, `workspace_pick_and_add`, `workspace_add_path`, `workspace_remove_repo`.
- `gitcroc-cli workspace show|add <path>|remove <id>`, with `--config-dir` so tests use a temp dir.
- Tests:
  - `grp_1_adding_a_repo_twice_is_a_no_op`, including through a different spelling of the same path (a trailing separator, a `..` segment, and a symlink to it on Unix).
  - `grp_3_removing_a_repo_leaves_the_disk_untouched`: the repo's files and `.git` are byte-identical before and after.
  - `plat_2_case_insensitive_paths_are_one_repo`: on a case-insensitive file system (Windows and default macOS) `Repo` and `repo` are the same repo; skipped with a message elsewhere.
  - `plat_2_windows_long_paths`: on Windows, a repo whose path exceeds 260 characters can be added and its summary read; skipped elsewhere.
  - `plat_2_non_ascii_paths`: a repo under a folder with non-ASCII characters and spaces.
  - Scan: finds repos at depths 1 to 4, not at 5, not inside a found repo, refuses a bare repo, survives an unreadable folder (Unix).
  - Workspace file: round trip, `[[group]]` entries are kept, newer schema is read-only.

## Out of scope
- The tree pane and its "Add repository" button (M1-07).
- Clone (M2), groups and several workspaces (M3), export and import (DATA-2, M3).
- Open in file manager and open in terminal (M2, behind the PLAT-1 traits).

## Acceptance
- `gitcroc-cli workspace add <folder>` adds every repo under a folder, and `workspace show` lists them.
- All tests listed above exist and pass on all three platforms.
