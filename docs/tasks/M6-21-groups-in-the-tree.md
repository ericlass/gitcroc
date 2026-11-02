# M6-21 Groups in the tree: menus, dialogs and dragging

## Goal
Groups are made and changed in the GPUI app as in rc.1: from the menus of
the group, repo and Ungrouped rows, through the group dialogs, and by
dragging repos and groups in the tree. The group's Branches node counts the
members that have each branch.

## Depends on
- M6-10: the tree pane, its group rows (UI-4), the Branches node and the
  multi-selection (UI-6).
- M6-17: the row menus, whose group entries are disabled until this task.
- M6-05, PR 3: `group_create`, `group_rename`, `group_delete`,
  `group_add_repos`, `group_remove_repos`, `group_move_repos`,
  `group_move_repo_between` and `group_reorder`.

## Spec
Implements: UI-5 (the counts), UI-7 (dragging between groups), GRP-2, GRP-3
Respects: UI-1, UI-2, UI-4, UI-6, UI-8, UI-22, UI-23, UI-29, GRP-5, PERF-1, SEC-3, ARCH-6, ARCH-9, D15, QA-3, QA-4

The core checks and applies every change (GRP-2, GRP-3), and the tree
follows its `WorkspaceChanged`. The window reads the groups and their
members off the core's tree and only decides which request a gesture sends,
as rc.1's `ui/src/tree/groups.ts` does (ARCH-6).

**What stays for M6-22.** The group operations, Fetch to Create tag and the
Branches node's Check out and Delete, start a preflight, and the preflight
dialog is M6-22's. So a group branch's double-click (UI-5) and those
entries come with it, and stay disabled here with "Not available yet". The
membership entries, New group, Rename, Delete, Add to group, Remove from
group, Move up and Move down, are enabled now.

## In scope
- **The group dialogs**, a port of `ui/src/tree/GroupDialogs.tsx`, on the
  shared dialog parts, per workspace (ARCH-9):
  - **New group:** a name; its members are the repos it was opened for,
    or none from a group or Ungrouped row;
  - **Rename:** the new name, filled with the old one;
  - **Delete:** a question naming the group that says its repos stay in the
    workspace and nothing on disk is touched (GRP-3);
  - the core's refusal of a name (GRP-2: empty, over 64 characters, or
    another group's, in any case) shows inside the open dialog, which stays
    open. Names are text (SEC-3).
- **The menus' group entries** (UI-7), as rc.1's `ui/src/tree/menus.ts`
  lists them:
  - **group row:** Rename, Delete, New group, Move up and Move down, the
    last two disabled at the ends;
  - **repo row:** **Add to group**, which opens a second menu beside it
    with each group the repo is not in, then **New group…**; under a group,
    **Remove from group**, Move up and Move down;
  - **Ungrouped:** New group;
  - **a multi-selection** (UI-6): a repo row's menu in a multi-selection
    keeps the selection, and its Add to group and New group act on every
    selected repo (GRP-5);
  - the second menu opens with the mouse and from the keyboard (Right or
    Enter on its entry), and Escape returns to the first.
- **Dragging** (UI-7), a port of rc.1's drop rules in `groups.ts` and
  `TreePane.tsx`, on GPUI's `on_drag` and `on_drop`:
  - a repo row dropped on another group moves it there from the group it
    was dragged from; holding Ctrl, or Alt on macOS, adds it instead;
  - a repo dragged from Ungrouped is always added;
  - a repo dropped on Ungrouped leaves the group it came from only, and a
    repo in no group dropped there changes nothing;
  - within a group, dropping between members reorders; dropped above or
    below a member of another group, the repo goes there;
  - a group row dropped above or below another group moves there, and
    nothing else is a drop target for it;
  - the dragged row and the drop target, into or before or after, are
    marked while dragging, with theme tokens (UI-23);
  - every drag is also in a menu, for keyboard use (UI-22).
- **The Branches node** (UI-5): each branch name of `group_branches` with
  how many members have it, such as `feature/x (3/5)`, read again when a
  member's branches change. M6-10 already shows the node; this task makes
  sure the counts follow the members, including a member added or removed.
- **Strings (D15):** the group dialogs' and the new menu entries' text
  moves from `ui/src/strings/en.ts` (in git history) to `strings.rs`.

## Tests (QA-3)
Real git on `repo-gen` fixtures (QA-2), in a workspace with two groups that
share a repo.
- `ui_7_drag_moves_between_groups_and_ctrl_or_alt_adds`: each rule above,
  with the marks while dragging and none after; the workspace's repos stay
  throughout (GRP-3).
- `ui_7_group_and_repo_context_menus_offer_the_group_operations`: the
  entries of each row kind, which are enabled, and the Add to group menu,
  from the mouse and the keyboard.
- `ui_7_a_multi_selection_is_added_to_a_group_or_a_new_one`.
- `grp_2_rename_and_new_group_show_the_refusal_inline`: an empty name, one
  of 65 characters, and another group's name in other case.
- `grp_3_deleting_a_group_keeps_its_repos`: the question says so, and after
  it the repos are still in the workspace and on disk.
- `ui_5_the_branches_node_counts_the_members`: the counts change when a
  member checks out another branch and when a repo joins or leaves.
- `ui_2_a_repo_in_two_groups_is_dragged_from_one_place`: only the group
  it was dragged from loses it.
- `sec_3_group_names_render_as_text`.
- `perf_1_the_tree_lays_out_only_the_rows_on_screen` keeps passing with
  drag targets on the rows.
- `ui_23_no_hardcoded_colors`, `d15_ui_text_comes_from_the_strings_module`
  and the keymap docs test keep passing over the new files.

## Out of scope
- **The group operations and a group branch's double-click** (UI-5,
  GRP-14): M6-22, with the preflight dialog and the result report.
- **Group commit** (UI-34, GRP-15): M6-22.
- **The palette's group commands** (UI-22): M6-24.

## Acceptance
- **On the reference machine,** `just dev-next` on `target-project`:
  - make a group from a multi-selection, rename it and delete it;
  - drag a repo between two groups, with and without Ctrl, onto Ungrouped,
    and within a group; drag one group above another;
  - do the same from the menus with the keyboard only.
- `just coverage M6` no longer lists UI-5, GRP-2 or GRP-3.
- `just check` passes.
