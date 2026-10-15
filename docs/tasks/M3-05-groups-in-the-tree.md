# M3-05 Groups in the tree pane

## Goal
The user sees groups in the tree with their branch and totals, creates and edits them from context menus, and drags repos between groups.

## Depends on
M3-04

## Spec
Implements: UI-4, UI-7
Respects: UI-1, UI-2, UI-3, UI-5, UI-8, GRP-2, GRP-3, GRP-4, PERF-1, ARCH-6, UI-22

## In scope
- Group row (UI-4): name, member count, the derived branch (`main`, or `mixed (2)`), and the number of dirty members and of members in conflict state, each shown only when not zero. It is expanded and collapsed like other nodes, and the expansion is kept per tab (UI-29).
- The group's **Branches** node shows the core's list with the counts (UI-5). Double-clicking a branch name does nothing until M3-11.
- A repo in several groups shows under each (UI-2). Selecting one place highlights only that row, and both places show the same history.
- Context menus (UI-7):
  - Group row: Rename, Delete (with a confirmation saying the repos are not touched, GRP-3), New group, Move up and Move down.
  - Repo row: **Add to group ▸** (each group the repo is not in, then **New group…**), and, under a group, **Remove from group**, Move up and Move down.
  - Ungrouped: New group.
- Dragging (UI-7):
  - A repo row dropped on another group moves it there from the group it was dragged from. Holding Ctrl (Alt on macOS) adds it instead.
  - A row dragged from Ungrouped is always added.
  - A row dropped on Ungrouped is removed from the group it came from.
  - Within a group, dropping between members reorders. Group rows reorder by dragging between groups.
  - Drop targets highlight while dragging. Every drag is also available from the context menu, for keyboard use (UI-22).
- The filter (UI-8) matches group names as M3-04 defines.
- The virtualized tree (PERF-1) keeps working with the new node kinds. The new rows are rendered from the core's tree only (ARCH-6).
- Tests (Vitest against the fake core):
  - `ui_4_group_row_shows_count_branch_and_totals`, mixed and common branch.
  - `ui_7_drag_moves_between_groups_and_ctrl_or_alt_adds`, including drop on Ungrouped and reorder.
  - `ui_7_group_and_repo_context_menus_offer_the_group_operations`.
  - `ui_2_selecting_one_place_of_a_repo_in_two_groups`.
  - Rename and New group show the core's refusal inline (GRP-2).

## Out of scope
- Multi-selection (M3-06). Group operations from the group menu (M3-10 onward), and double-click on a group branch (M3-11).

## Acceptance
- With `just dev` on `target-project`, the four groups show with their branches. A repo dragged from one group to another lands there, and the workspace file shows the change.
- All tests listed above exist and pass, and `just check` passes.
