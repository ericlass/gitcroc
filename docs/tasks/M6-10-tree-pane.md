# M6-10 Tree pane: rows, summaries, filter, selection

## Goal
`gitcroc-next`'s left pane shows rc.1's tree, with these parts:
- each group, then Ungrouped, with the repos, their Local, Remotes, Tags and Stashes nodes, and the branch names folded into folders;
- each repo row with its summary, and each group row with its totals;
- a filter box;
- single selection, and multi-selection of repo rows.

The tree is virtualized. Each tab keeps its own tree: selection, expanded nodes, scroll position and filter.

## Depends on
- M6-09: the tabs, whose active workspace the tree shows.
- M6-07: the tokens the rows draw with.

## Spec
Implements: UI-1, UI-2, UI-3, UI-4, UI-6, UI-8
Respects: ARCH-6, ARCH-7, ARCH-8, ARCH-9, PERF-1, PERF-3, PERF-4, GRP-4, GRP-5, UI-22, UI-23, UI-29, D15, SEC-3, QA-3, QA-4

## In scope
- **The tree stays in the core** (ARCH-6). The core builds the tree, folds the names and filters; the UI lays out what it reports:
  - `tree_get`, for the skeleton;
  - `refs_get`, the first time one of a repo's four ref nodes is expanded;
  - `group_branches`, the first time a group's Branches node is expanded (ARCH-8);
  - `tree_filter`, for each change of the filter text. An answer a newer one superseded is dropped.
  - `workspace_summaries`, and the `RepoSummary` and `GroupSummary` events, for the rows. A summary event that arrives before the first read is applied after it.
  - `WorkspaceChanged` reads the tree again. `RepoRefsChanged` reloads that repo's loaded places and the loaded Branches nodes, then the filter.
  - `repo_select` tells the core which repo is selected (PERF-4).

  No service changes.
- **One tree per tab** (ARCH-9, UI-29). The window keeps a tree pane per open workspace, made when its tab is first shown and dropped when the tab closes. Switching tabs shows that tab's tree as it was left: selection, expanded nodes, scroll position and filter text.
- **Rows** (`crates/gitcroc-ui/src/tree.rs`, a port of `ui/src/stores/tree.ts` and `ui/src/tree/rows.ts`, without GPUI):
  - Depth first; children show only below expanded nodes.
  - Ungrouped and the groups start expanded, everything else collapsed.
  - While a filter is shown, every node the core sent with children starts expanded. Expanding and collapsing then has its own record, cleared when the text changes.
  - A node not loaded yet shows a "Loading…" row below it. A failed load shows why there.
- **The pane** (`crates/gitcroc-ui/src/tree_pane.rs`, a port of the read-only parts of `ui/src/tree/TreePane.tsx`):
  - **Filter box:** gpui-component's input, named "Filter repos and branches", above the tree (UI-8).
  - **The tree:** a `uniform_list` of fixed-height rows (PERF-1), with role tree, named "Repositories". Rows are tree items with their level, and their expanded and selected states. The row the keyboard is on is the active descendant.
  - **Repo row** (UI-3), a port of `RepoRow.tsx`, with these parts:
    - the name;
    - the current branch, or the short hash when detached;
    - ahead and behind as `↑1 ↓2`, with zero counts left out;
    - a dot when dirty, a conflict icon in conflict state, a warning icon when the folder is missing or the repo cannot be read, and a spinner while it is read again.

    Each mark is named for screen readers. A summary from the cache shows dimmed until it is read again (PERF-3).
  - **Group row** (UI-4), a port of `GroupRow.tsx`, with these parts:
    - the name;
    - the member count;
    - the branch the core derives (GRP-4): a name, "detached" or "mixed (n)";
    - the dirty and conflict totals, each only when not zero.

    The latest `GroupSummary` replaces the one the node came with.
  - **Ref rows:**
    - a local branch shows a mark when it is current, its ahead and behind, and "gone" when its upstream is gone;
    - a group's branch name shows `(3/5)`;
    - every other row shows its label.
  - **Messages:**
    - an empty, unfiltered workspace says that it has no repositories yet;
    - a tree that cannot be read says why, as an alert.
  - **Mouse:**
    - a click selects a row;
    - with Ctrl/Cmd, a click adds a repo row to the selection or takes it out (UI-6);
    - with Shift, a click selects the repo rows from the anchor to it;
    - a click on the chevron, or a double-click on a row that expands, opens or closes it.
  - **Keys on the tree** (UI-22):
    - Up and Down move the selection; with Shift they extend it over repo rows, and with Ctrl/Cmd they move without changing it;
    - Ctrl/Cmd+Space adds or takes out the row the keyboard is on;
    - Right opens a row, then goes to its first child; Left closes it, then goes to its parent;
    - Enter opens or closes a row;
    - Home and End go to the first and the last row.

    The selected row scrolls into view.
- **Selection** (UI-6, GRP-5), as rc.1's store keeps it:
  - one selected row, or a multi-selection of repo rows in which each repo counts once;
  - branch, tag, stash and other rows are always selected alone;
  - the tree pane reports the target set: the selected repo, the selected group, or the multi-selected repos. M6-11 and the toolbar read it.
- **A repo in several groups** (UI-2) shows under each. Each place loads its own refs, and selecting either selects the same repo.
- **Regions (UI-22):** F6's tree region is the active tab's tree, with the focus on the list.
- **Names are text** (SEC-3). Every colour comes from the tokens (UI-23).
- **Icons:** current branch, conflict, warning and spinner join `icons.rs` as SVGs drawn for gitcroc.
- **Strings (D15):** `tree` moves from `ui/src/strings/en.ts` to `strings.rs`, without the entries that only the menus and the add menu use.
- **Test support:** a fixture can make repos with `repo-gen` and add them to a workspace through the app, waiting for the add's report.

## Tests (QA-3)
- `ui_1_tree_shows_groups_then_ungrouped_with_repos_and_ref_nodes`:
  - the rows are in order, with their levels;
  - a ref node loads its refs when first expanded, with folders for names containing `/`;
  - the tree follows a change to the workspace.
- `ui_1_an_empty_workspace_says_it_has_no_repositories`.
- `ui_1_keyboard_moves_opens_and_closes_rows`.
- `ui_2_a_repo_in_two_groups_shows_and_selects_in_each`.
- `ui_3_repo_row_shows_branch_counts_and_marks`:
  - clean shows the name and branch only;
  - dirty shows the dot;
  - ahead and behind show their counts;
  - a detached head shows its short hash;
  - a missing folder shows the warning;
  - a summary from the cache is stale until read again.
- `ui_3_conflict_state_shows_the_conflict_icon`.
- `ui_4_group_row_shows_count_branch_and_totals`: the row follows a member's change.
- `ui_6_ctrl_and_shift_click_build_a_multi_selection_of_repo_rows_only`:
  - the target set follows;
  - a branch row is selected alone;
  - the same works from the keyboard.
- `ui_8_the_filter_shows_the_core_answer_expanded_and_clears`: group names match too.
- `ui_29_each_tab_keeps_its_tree`: selection, expanded nodes, scroll position and filter come back after a tab switch. M6-11 to M6-13 add the other panes' state to this test.
- `perf_1_the_tree_lays_out_only_the_rows_on_screen`.
- `sec_3_repo_and_branch_names_render_as_text`.
- `ui_23_no_hardcoded_colors`, `d15_ui_text_comes_from_the_strings_module` and the keymap docs test keep passing over the new files.

## Out of scope
- **Context menus and double-click checkouts (UI-5, UI-7):**
  - row menus and a local branch's double-click: M6-17;
  - a group branch's double-click, dragging between groups, and the group dialogs: M6-21.

  Here a double-click on a branch row does nothing.
- **The add menu (Add folder…, Clone…) and the add report:** M6-20. The empty workspace only says so.
- **What selecting shows in the history:** M6-11.
- **Background tabs (UI-30, UI-31):** the tree keeps no history or diffs, so nothing is released here.
- **The palette's reveal of a row:** M6-24.

## Acceptance
- **On the reference machine,** `just dev-next` on `target-project`:
  - shows its groups and repos with their summaries;
  - expands refs;
  - filters;
  - selects with the mouse and the keyboard, including multi-selection;
  - keeps each tab's tree when switching tabs.
- `just coverage M6` no longer lists UI-1, UI-2, UI-3, UI-4, UI-6 or UI-8.
- `just check` passes.
