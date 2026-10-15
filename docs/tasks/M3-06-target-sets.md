# M3-06 Multi-selection, target sets and the summary strip

## Goal
The user selects a repo, a group or several repos, and the toolbar acts on that target set. For a group or several repos, a summary strip lists each repo's state.

## Depends on
M3-05

## Spec
Implements: GRP-5, UI-6, UI-11
Respects: UI-20, UI-29, GRP-4, GRP-8, PERF-3, ARCH-6, ARCH-9, section 3 "Target set"

## In scope
- `gitcroc-core` `TargetSet`, with exactly these kinds (GRP-5):
  - `Repo(RepoId)`;
  - `Group(GroupId)`: the members in group order;
  - `Repos(Vec<RepoId>)`: a multi-selection, in tree order, without duplicates.
  `target_repos(workspace, target)` resolves it to repo ids against the current workspace. A group that no longer exists, or a repo removed meanwhile, is an error the UI shows. Group operations (M3-08) take a `TargetSet`, never a hand-built list.
- Tree selection (UI-6):
  - Single selection as now.
  - Ctrl/Cmd-click toggles a repo row into a multi-selection. Shift-click selects the range of repo rows between the anchor and the row, in visible order, skipping rows that are not repos.
  - Group, branch, tag and stash rows are never multi-selected: clicking one replaces a multi-selection.
  - A repo shown twice (two groups) counts once.
  - Keyboard: Shift+Up/Down extends over repo rows, and Ctrl/Cmd+Space toggles.
  - The selection is kept per tab (UI-29).
- Toolbar (UI-20) acts on the current target set:
  - For a group or several repos, Fetch, Pull, Push, Branch and Merge stay disabled until their group operations exist (M3-10 to M3-12). Stash is disabled, since it is single-repo only (section 6).
  - The disabled state has a tooltip saying why.
- Summary strip (UI-11) above the history for a group or several repos:
  - One line per repo in target-set order: name, branch, ahead and behind, dirty count, the conflict flag, and a warning for a missing path. Stale summaries are marked as in the tree (PERF-3).
  - Clicking a line selects that repo in the tree, under the group being viewed when there is one.
  - It is virtualized beyond 8 lines, with a fixed height and its own scroll.
  - Its data is the summaries the UI already has. The core adds nothing.
- Until M3-07, the history below the strip shows "Merged history arrives in M3-07". This line is removed by M3-07.
- Tests:
  - `grp_5_target_set_is_a_repo_a_group_or_a_multi_selection` (core): the group resolves to its members in order, and the repos resolve in tree order without duplicates.
  - `ui_6_ctrl_and_shift_click_build_a_multi_selection_of_repo_rows_only` (Vitest), keyboard included.
  - `ui_11_summary_strip_lists_each_repo_and_selects_on_click` (Vitest).
  - `ui_20_toolbar_follows_the_target_set` (Vitest): single repo as in M2-07, and a group with the group buttons disabled.

## Out of scope
- Merged history (M3-07). Running anything on a target set (M3-08 onward).

## Acceptance
- With `just dev`, selecting a group shows one strip line per member, and Ctrl-click across two groups builds a selection of repos that the strip lists.
- All tests listed above exist and pass, and `just check` passes.
