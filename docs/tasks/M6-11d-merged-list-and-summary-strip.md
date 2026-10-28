# M6-11d Merged list and summary strip

## Goal
Selecting a group or several repos shows rc.1's merged history with a repo
column, the summary strip above it, and the merged uncommitted row.

This is the fourth and last part of M6-11.

## Depends on
- M6-11a: the history source and list.
- M6-11b: the columns, including the graph column's place and width.
- M6-11c: search and the uncommitted row.

## Spec
Implements: UI-9, UI-11, UI-34
Respects: PERF-1, PERF-2, PERF-5, ARCH-6, ARCH-7, ARCH-8, ARCH-9, UI-23, UI-29, UI-36, GRP-5, SEC-3, D15, QA-3, QA-4

## In scope
- **The merged source** (section 8 "History pane" table):
  - a group or a multi-selection resolves to its repos with `target_repos`
    in the core's order (GRP-5);
  - history is read as `HistoryTarget::Merged`, newest first by commit date
    (PERF-5);
  - a multi-selection keeps one position whatever order its repos were
    picked in;
  - a repo's own selection still shows its single-repo history.
- **The merged list** (UI-9):
  - the graph column is replaced by the repo name, in the graph column's
    place and width (UI-36);
  - the other columns stay: subject with ref labels, author, date, short
    hash;
  - a row selects that commit of that repo for the changes pane (M6-12);
  - search keeps the repo column and runs in the core (UI-12, M6-11c);
  - the position is kept per tree selection and per tab (ARCH-9, UI-29).
- **The summary strip** (UI-11), a port of `ui/src/history/TargetView.tsx`:
  - one line per repo of the target set, in the core's order;
  - each line shows the repo name, its branch or short detached hash, ahead
    and behind, its dirty count, and a conflict flag; the marks are named
    for screen readers;
  - a summary from the cache shows dimmed until it is read again (PERF-3);
  - the strip shows up to eight lines and is virtualized beyond that
    (PERF-1);
  - clicking a line selects that repo in the tree;
  - a group that cannot be resolved, or has no repos, says why.
- **The merged uncommitted row** (UI-34):
  - when at least one repo of the set is dirty, the first row is
    "Uncommitted changes (n repos)";
  - `n` counts the dirty repos of the set, not the changed paths;
  - selecting it reports the merged uncommitted target for the changes
    pane (M6-12); the Staged and Unstaged lists, the group commit and the
    absence of Amend are M6-12.
- **Refresh:** a change to any target repo's refs rereads the merged list,
  keeping the position and the selection (OPS-7, M6-11a).
- **Strings (D15):** the strip, repo-column and merged uncommitted text
  moves to `strings.rs`.
- **Test support:** a fixture can make a group and several dirty repos.

## Tests (QA-3)
- `ui_9_merged_list_shows_the_repo_column`:
  - a group shows its repos' commits newest first, with a repo column
    instead of a graph;
  - a merged row selects that commit of that repo;
  - search keeps the repo column and runs in the core;
  - a single repo selected afterwards shows its graph again.
- `ui_11_summary_strip_shows_each_repo_and_selects_it_in_the_tree`:
  - the strip has one line per repo in target order, with branch,
    ahead/behind, dirty count and conflict flag;
  - a cached summary is dimmed;
  - clicking a line selects that repo in the tree;
  - more than eight repos scroll, and only the visible lines are laid out.
- `ui_34_merged_uncommitted_row_counts_dirty_repos`:
  - a dirty group leads with "Uncommitted changes (n repos)";
  - only groups or multi-selections with a dirty repo show it;
  - selecting it reports the merged uncommitted target.
- `ui_12_merged_search_filters_in_the_core`.
- `ui_29_each_tab_keeps_its_merged_history`: source, search, selected row
  and scroll position come back after a tab switch.
- `perf_1_the_summary_strip_lays_out_only_the_lines_on_screen`.
- `sec_3_repo_names_in_the_strip_and_column_render_as_text`.
- `ui_23_no_hardcoded_colors` and
  `d15_ui_text_comes_from_the_strings_module` keep passing over the new
  files.

## Out of scope
- **The changes pane for a merged uncommitted selection (UI-15, UI-34):**
  M6-12, including the sectioned file lists and the group commit.
- **The commit context menu and cherry-pick's multi-selection (UI-13):**
  M6-17.
- **Background tabs releasing loaded pages (UI-31):** M6-14.

## Acceptance
- **On the reference machine,** `just dev-next` on `target-project`:
  - selecting a group shows the summary strip and the merged list with the
    repo column;
  - a dirty group leads with the merged uncommitted row;
  - search and tab switching keep the merged position.
- `just coverage M6` no longer lists UI-9, UI-11 or the history part of
  UI-34.
- `just check` passes.
