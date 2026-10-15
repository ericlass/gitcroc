# M3-04 Groups in the core

## Goal
The core manages groups: create, rename, delete and reorder them, add, remove and reorder their members, and derive each group's branch and totals. The tree it returns has the full UI-1 structure.

## Depends on
M3-01

## Spec
Implements: GRP-2, GRP-3, GRP-4, UI-1, UI-2
Respects: D7, GRP-1, UI-4, UI-5, DATA-1, ARCH-8, ARCH-9, section 3 "Group" and "Ungrouped", section 10 workspace file format

## In scope
- `gitcroc-core` group model, kept in `[[group]]` of the workspace file in the section 10 format, schema unchanged:
  - Group ids `g1`, `g2`, … are given in file order when the workspace loads. They are kept in memory only, so a rename keeps the id for the session.
  - GRP-2:
    - names are trimmed, 1 to 64 characters, and unique per workspace ignoring case;
    - the name "Ungrouped" is refused, ignoring case, so it cannot be confused with the built-in node;
    - members keep the user's order;
    - a repo is at most once in one group, and can be in several groups (D7).
  - Operations:
    - create (optionally with members, for "new group from selection"), rename, delete;
    - add repos at a position, remove repos, move repos to a position within the group;
    - move a repo from one group to another (remove and add as one save);
    - reorder groups.
  - GRP-3: all of these change only the workspace file. Removing a repo from the workspace also removes it from every group (already true since M1-05).
- GRP-4, derived on read and never stored:
  - A group's branch is the members' branch name when every member whose summary is known is on the same branch. Otherwise it is "mixed" with the count of distinct values.
  - A detached `HEAD` counts as one value, "detached", however many members are detached.
  - Members with a missing path or no summary yet do not count.
  - A group with no counted member shows no branch.
- Group totals for UI-4: member count, the number of dirty members and the number of members in conflict state.
- Tree (`tree_get`, `tree_filter`), per UI-1:
  - one node per group in user order, then Ungrouped;
  - a group node holds a Branches node, then its members in order; each member has its Local, Remotes, Tags and Stashes nodes as now;
  - Ungrouped holds the repos that are in no group;
  - a repo in several groups appears under each (UI-2), with node ids unique per place, and the same `RepoId`;
  - the group's branch and totals are sent with the group node and updated when a member's summary changes.
- Group Branches node (UI-5's list; double-click arrives in M3-11):
  - It lists every local branch name present in at least one member, with `present/members`, such as `feature/x (3/5)`. Names with `/` fold into folders.
  - It is read when first expanded, as M1-07 reads refs, from the members' local branches. No ahead or behind counts are needed for it.
- The tree filter also matches group names. A group matched by name shows all its members, not loaded.
- IPC: `group_create`, `group_rename`, `group_delete`, `group_add_repos`, `group_remove_repos`, `group_move_repos`, `group_move_repo_between`, `group_reorder`, `group_branches`. Each saves and emits `workspace_changed` and a new tree.
- `gitcroc-cli group create|rename|delete|add|remove|list`.
- Tests:
  - `grp_2_group_names_are_unique_ignoring_case_and_1_to_64_characters`, including "Ungrouped".
  - `grp_2_members_keep_their_order` through add at a position, move and reload.
  - `grp_3_group_changes_never_touch_the_disk`: every operation above leaves the repos byte-identical.
  - `grp_4_group_branch_is_the_common_branch_or_mixed_with_a_count`, with detached and missing members.
  - `ui_1_tree_lists_groups_in_order_then_ungrouped` and `ui_2_a_repo_in_two_groups_appears_under_each` (core tree tests).
  - `ui_5_group_branches_node_counts_members_with_each_branch` (core).
  - The workspace file keeps the section 10 format after a round trip, and a file written by M2 loads unchanged.

## Out of scope
- Group rows, drag and drop and menus in the tree pane (M3-05).
- Multi-selection and target sets (M3-06). Group operations (M3-08 onward).

## Acceptance
- `gitcroc-cli group create Backend r1 r2` writes a `[[group]]` in the section 10 format, and `tree` shows it above Ungrouped.
- All tests listed above exist and pass on all three platforms, and `just check` passes.
