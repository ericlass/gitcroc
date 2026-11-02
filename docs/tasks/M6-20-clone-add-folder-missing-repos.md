# M6-20 Clone, add folder and missing repos

## Goal
Repos get into a workspace from the GPUI app as they do in rc.1: the tree's
add menu adds a folder or clones a URL, and says what it added. An import
lists the repos whose folder is missing and clones the ones the user picks,
and so does a missing repo's menu. What an export or import reports shows
in the window.

## Depends on
- M6-15b: the status bar and job list that show the clone and add jobs,
  and the askpass dialog a clone may need.
- M6-17: the repo row's menu, whose Clone from remote entry this task
  enables.
- M6-09: the tab menu's Export workspace and Import workspace entries,
  which already start them.
- M6-05, PR 3 and 4: `workspace_pick_and_add`, `workspace_add_path`,
  `clone_pick_parent`, `clone_default_name`, `clone_target_check`,
  `clone_start`, `workspace_missing_repos`, `workspace_export`,
  `workspace_import`, and the events `WorkspaceAddReport`,
  `CloneParentPicked`, `CloneReport`, `WorkspaceExported` and
  `WorkspaceImported`.

## Spec
Implements: GRP-1, DATA-2
Respects: section 6 "Repos", GRP-2, GRP-10, D10, SEC-2, SEC-3, PLAT-2, ARCH-6, ARCH-7, ARCH-9, UI-20, UI-22, UI-23, D15, QA-3, QA-4

The core opens every file and folder dialog (SEC-2), names the default
folder, checks the target and the group name (GRP-2), and runs the add and
the clone as jobs (ARCH-7). The window only sends requests and shows the
answers (ARCH-6). Paths reach the window as display strings, and it never
builds a path from typed text except as the `parent` and `name` of a
`CloneRequest`, which the core checks (PLAT-2).

## In scope
- **The add menu**, a port of rc.1's in `ui/src/tree/TreePane.tsx`: a
  button beside the tree's filter box with **Add folder…** and **Clone…**.
  It opens with the mouse, and from the keyboard like the other menus.
  Both entries are always there; a read-only workspace's refusal shows below
  the tree. The empty workspace's text names the two entries.
- **Add folder** runs `workspace_pick_and_add`. When its job ends, the
  **add report** below the tree lists, as rc.1's `AddReportView` does:
  - the repos added;
  - those already present (GRP-1: adding a repo again is a no-op);
  - each refused path, with the reason;
  - or that no repo was found.

  The report is kept per workspace (ARCH-9) and closes with its own close
  button or the next add.
- **The clone dialog**, a port of `ui/src/clone/CloneDialog.tsx` and
  `ui/src/stores/clone.ts`, per workspace, on the shared dialog parts:
  - the URL; the parent folder, picked with `clone_pick_parent` and kept
    for the next clone; and the folder name, which follows
    `clone_default_name` of the URL until the user edits it;
  - the target checked as typed with `clone_target_check`. A folder that
    exists and is not empty disables Clone and says why;
  - **Add to group:** none (the default), a group of the workspace, or a
    new group whose name the core checks when Clone is pressed (GRP-2). A
    refusal shows inside the open dialog, which stays open;
  - a started job closes the dialog. Its progress is in the status bar, and
    Cancel in the job list stops it (GRP-10). When its `CloneReport` names
    the repo, the repo is selected in the tree; a failure shows in the
    status bar with git's message.
- **The missing repos dialog** (DATA-2), a port of
  `ui/src/clone/MissingReposDialog.tsx` and `ui/src/stores/share.ts`:
  - it opens after an import that has missing repos, and from **Clone from
    remote** on a missing repo's menu, which this task enables;
  - one row per repo of `workspace_missing_repos`: a check box, checked
    when the repo has a remote, or only the repo the menu was opened for;
    the remote; and the target as parent folder and name, the parent picked
    with `clone_pick_parent` (its `key` names the row). Each target is
    checked as it changes, with `creates_parent`;
  - a repo with no remote can't be checked, and says so;
  - **Clone selected** starts one `clone_start` per checked row, with its
    `repo`, all at once. Each row then shows that it is cloning, and how its
    clone ended, with git's message when it failed. The dialog stays open
    until closed;
  - a refused start shows on its row; the other rows still start.
- **Export and import reports** (DATA-2): `WorkspaceExported` and
  `WorkspaceImported` show as rc.1's notice does, in the window, as text
  (SEC-3): the file written, or the paths kept absolute; or why the import
  was refused, in which case nothing opens.
- **Strings (D15):** the add menu's, the add report's, the clone dialog's,
  the missing repos dialog's and the export and import notices' text moves
  from `ui/src/strings/en.ts` (in git history) to `strings.rs`.

## Tests (QA-3)
Real git, with `file://` bare repos made by `repo-gen` as remotes (QA-2).
The file and folder dialogs are the `Dialogs` fake of M6-05, answering with
a path or a cancel. Jobs are waited for by their `JobEnded`, never by time
(no timing in CI).
- `grp_1_add_folder_reports_added_and_already_present`: the same folder
  twice; the second report lists the repos as already present, and the
  workspace has them once. A folder with no repo says so; a cancel adds
  nothing and shows no report.
- `grp_1_clone_name_follows_the_url_until_edited`: and the clone is added
  and selected.
- `grp_1_clone_refuses_a_folder_that_is_not_empty`: Clone is disabled with
  the core's reason, and no job starts.
- `grp_2_clone_adds_the_repo_to_an_existing_or_a_new_group`: a taken new
  group name is refused inside the open dialog.
- `ui_22_the_add_menu_and_clone_dialog_work_from_the_keyboard`.
- `data_2_import_lists_missing_repos_and_clones_the_checked_ones`: a repo
  with a remote is checked, one without can't be; Clone selected clones the
  checked ones into their rows' targets, and each row shows how it ended;
  a failing URL shows git's message on its row.
- `data_2_a_missing_repos_menu_opens_the_dialog_with_only_it_checked`.
- `data_2_export_and_import_notices`: an export names its file and the
  paths kept absolute; a refused import says why and opens no tab.
- `sec_3_clone_and_missing_repo_text_renders_as_text`: a URL, path and repo
  name with markup show as written.
- `ui_23_no_hardcoded_colors`, `d15_ui_text_comes_from_the_strings_module`
  and the keymap docs test keep passing over the new files.

## Out of scope
- **Group operations, the preflight dialog and the result report:** M6-21
  and M6-22.
- **The palette's Add folder and Clone commands** (UI-22): M6-24.
- **Dropping folders from the system's file manager onto the window:** rc.1
  has none.

## Acceptance
- **On the reference machine,** `just dev-next`:
  - add the `target-project` folder, then add it again;
  - clone a repo from GitHub over HTTPS into a new folder, and into a group;
  - export a workspace, delete one repo's folder, import the file, and clone
    the missing repo from the dialog.
- `just coverage M6` no longer lists GRP-1 or DATA-2.
- `just check` passes.
