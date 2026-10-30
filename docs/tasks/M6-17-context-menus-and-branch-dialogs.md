# M6-17 Tree and history context menus; branch dialogs

## Goal
Every row of the tree and every commit of the history has a context menu,
as in rc.1. A double-click on a local branch checks it out. The branch
operations of section 6 run from the GPUI app, through their dialogs:
checkout, create, rename, delete, delete on the remote, set and unset the
upstream, and checking out a commit or tag detached.

## Depends on
- M6-10: the tree pane and its rows.
- M6-11d: the history list, merged or not, whose rows get the menu.
- M6-15a: the status bar and job list the jobs show in.
- M6-05, PR 3: `op_branch`, `branch_choices`, `branch_name_check`,
  `workspace_remove_repo`, `repo_open_file_manager` and
  `repo_open_terminal`.

## Spec
Implements: UI-7 (the menus and the double-click), UI-13
Respects: section 6 "Branches", GRP-8, GRP-9, SEC-3, SEC-4, ARCH-6, ARCH-7, ARCH-9, UI-6, UI-22, UI-23, UI-29, D15, QA-3, QA-4

UI-7's dragging between groups is M6-21's. The core decides what each
operation does, checks names (SEC-4), refuses blocked repos (GRP-8) and says
what needs a confirmation (GRP-9). The window only sends requests and shows
the answer (ARCH-6).

**Entries for later tasks.** Each menu lists all of its entries now, as rc.1
does, so the menus are built once. An entry whose dialog comes in a later
task is disabled until that task, with the reason "Not available yet". UI-20
already says buttons that can't apply are disabled, not hidden. The later
tasks enable them:
- M6-18: New tag, tag Push and Delete, the stash entries, the remote
  entries, and the commit menu's Create tag here, Cherry-pick, Revert and
  Reset.
- M6-20: Clone from remote, on a missing repo.
- M6-21: the group entries, including those on a repo row.

`gitcroc-next` is not released before M6-30, so no user sees these
disabled entries.

## In scope
- **The tree menus** (UI-7), a port of `ui/src/tree/menus.ts`, on the
  existing `menu::Menu`:
  - the entries of each row kind as `menuFor` lists them: repo, local
    branch, remote branch, tag, stash, the Remotes node, a remote, a group,
    a group's branch and Ungrouped;
  - a blocked repo (GRP-8) has every write disabled, with the reason. Open
    in file manager, Open in terminal and Remove from workspace stay
    enabled;
  - disabled entries that can't apply: Check out and Delete on the current
    branch, Unset upstream on a branch with none;
  - it opens with a right-click on the row, which also selects the row, and
    with Shift+F10 or the Menu key on the selected row, below it. If GPUI
    doesn't report the Menu key on a platform, the PR says so.
- **What this task runs from them:**
  - **branches:** Check out, New branch, New branch from here, Rename,
    Delete, Set upstream and Unset upstream; on a remote branch, Check out,
    New branch from here and Delete on remote; on a tag, Check out
    (detached) and New branch from here;
  - **repos:** Open in file manager, Open in terminal and Remove from
    workspace. A failure shows below the tree, as in rc.1.
- **The double-click** (UI-7): on a local branch row, it checks the branch
  out (`op_branch` with `Checkout`). On the current branch it does nothing.
  On other rows it keeps M6-10's behaviour.
- **The branch dialogs**, a port of `ui/src/tree/BranchDialogs.tsx` and
  `ui/src/stores/branch.ts`, on the shared dialog parts, per workspace
  (ARCH-9):
  - **Create:** the name, checked as typed with `branch_name_check`
    (SEC-4); the start point, as the current `HEAD`, the default branch
    (from `branch_choices`, disabled when there is none) or a revision;
    and "Check out the new branch", on by default. Opened from a branch,
    remote branch, tag or commit, the revision is that row's;
  - **Rename:** the new name, checked as typed;
  - **Delete:** Force (delete when not merged) and "Also delete on the
    remote", which is disabled for a branch that tracks none;
  - **Upstream:** the remote branches from `branch_choices` to pick from,
    or a note that there are none;
  - **Confirm** (GRP-9): when `op_branch` answers `confirm`, a dialog names
    the repo and what will be lost, as text (SEC-3). Cancel has the focus,
    and only its own button sends the request again with `confirmed`;
  - a refusal shows inside the open dialog, which stays open. A started
    job closes the dialog, and its end shows in the status bar;
  - `branch_choices` failing shows inside the dialog, and the rest of the
    dialog still works.
- **The commit menu** (UI-13), a port of `ui/src/history/CommitMenu.tsx`:
  - Check out (detached), Create branch here, Create tag here, Cherry-pick,
    Revert, Reset current branch to here and Copy hash;
  - Check out runs `CheckoutDetached`, and Create branch here opens the
    Create dialog with the commit as its revision. Copy hash puts the full
    hash on the clipboard;
  - in the merged list (UI-9), the menu acts on the row's repo;
  - it opens as the tree's menus do, on the commit row.
- **Several commits** (UI-13), as rc.1's history has them:
  - Ctrl or Cmd and click adds or removes a commit, and Shift and click
    selects a run. Only commit rows can be selected together, and the
    changes pane shows the last commit clicked;
  - with several selected, only Cherry-pick is enabled, labelled "Cherry-pick
    <n> commits", and only when they are all of one repo. Otherwise it says
    why. The cherry-pick itself is M6-18's.
- **Strings (D15):** the menus' and the branch dialogs' text moves from
  `ui/src/strings/en.ts` to `strings.rs`, with "Not available yet".

## Tests (QA-3)
Real git on `repo-gen` fixtures (QA-2), with a bare repo as the remote for
the remote branch tests. Jobs are waited for by their `JobEnded`, never by
time (no timing in CI).
- `ui_7_tree_row_menus_list_the_valid_operations`: the entries and which
  are disabled, for each row kind, and in a blocked repo.
- `ui_7_menus_open_from_the_keyboard`: Shift+F10 on a tree row and on a
  commit row; Escape closes the menu and gives the focus back to the row.
- `ui_7_double_click_checks_out_a_local_branch`.
- `ui_7_create_a_branch_from_each_start_point`: the current `HEAD`, the
  default branch, a revision and a tag row, with and without checking it
  out.
- `ui_7_rename_and_delete_a_branch`.
- `ui_7_set_and_unset_the_upstream`.
- `ui_7_check_out_a_remote_branch_and_delete_it_on_the_remote`.
- `grp_9_force_delete_asks_first`: the confirmation names the repo and the
  branch; Cancel sends nothing, and confirming deletes the branch.
- `sec_4_branch_name_is_checked_as_typed`: an invalid name disables Create
  and says why.
- `ui_7_open_in_file_manager_terminal_and_remove`: through fakes of the
  platform's `FileManager` and `Terminal`, and the repo leaves the tree.
- `ui_13_commit_context_menu`: its entries, Check out (detached), Create
  branch here and Copy hash.
- `ui_13_cherry_pick_needs_rows_of_one_repo`: Ctrl and Shift selections in
  a repo and in the merged list.
- `sec_3_branch_dialogs_render_names_as_text`: a branch and repo name with
  markup show as written.
- `ui_23_no_hardcoded_colors`, `d15_ui_text_comes_from_the_strings_module`
  and the keymap docs test keep passing over the new files.

## Out of scope
- **The entries that open other tasks' dialogs:** see "Entries for later
  tasks" above.
- **The toolbar's Branch button** (UI-20): M6-18, with the rest of the
  toolbar.
- **Dragging between groups** (UI-7) and **the group's Branches node
  double-click** (UI-5): M6-21.
- **The palette's branch commands** (UI-22): M6-24.

## Acceptance
- **On the reference machine,** `just dev-next` on `target-project`:
  - create a branch from a commit's menu, check it out with a
    double-click, rename it and delete it;
  - set its upstream and unset it;
  - Copy hash, then paste it elsewhere.
- `just coverage M6` no longer lists UI-7 or UI-13.
- `just check` passes.
