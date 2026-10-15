# M3-07 Merged history

## Goal
Selecting a group or several repos shows one list of their commits, newest first by commit date, with a repo column, paged lazily and searchable.

## Depends on
M3-06

## Spec
Implements: PERF-5, UI-9
Respects: UI-11, UI-12, UI-13, UI-14, ARCH-8, PERF-1, PERF-2, ARCH-9, UI-29, UI-31, section 8 "History pane" table, section 9 "Select a group of all 20 repos" row

## In scope
- `gitcroc-git` and `gitcroc-core`: a merged walk, a lazy k-way merge of per-repo walks by commit date (PERF-5):
  - Each repo's walk starts at its `HEAD`, the current branch or a detached commit. An unborn `HEAD` or a missing path contributes no rows.
  - Rows come out newest first by commit date. Ties go by target-set order, then by the repo walk's own order.
  - Only the page asked for is materialized: 500 rows (ARCH-8). Each repo walk keeps its position between pages, and no repo is walked further than the merge needs.
  - The walks use commit-graph files where present, as the single-repo walk does.
  - A commit present in two repos appears once per repo.
- IPC: `history_get` and `history_more` take a `HistoryTarget::Merged` built from the `TargetSet`. They keep the one-walk-per-workspace rule of M1-08: a newer request ends the older walk. Rows carry the `RepoId`. A merged row has no graph lanes.
- Search (UI-12) filters by subject, author or hash prefix in each repo's walk, inside the core, before the merge.
- History pane for a merged list (UI-9):
  - The graph column is replaced by the repo name. The other columns stay: subject with ref labels, author, date, short hash.
  - It is virtualized (PERF-1), and the position is kept per tree selection and per tab, as for a single repo (M1-08, UI-29). A background tab releases it (UI-31).
  - The placeholder line M3-06 put below the summary strip is removed.
- Selecting a merged row shows that commit of that repo in the changes pane (UI-14).
- The commit context menu (UI-13) acts on the row's repo, as for a single repo. Cherry-pick takes a multi-selection only when all the selected rows are of one repo; otherwise it is disabled, with a tooltip.
- Refresh: the merged list is read again when any target repo's `refs_version` goes up (M1-11), with the same rules of rows kept and replaced.
- `gitcroc-cli history --group <name>|--repos r1,r2` for tests and the bench.
- Bench: `group-history-first-page` in `just bench`, on `target-project` (all 20 repos, budget 300 ms) and on `many-repos` (all 200, reported). `bench-ui` gains `select-group`: click a group of all 20 repos until the first merged page is visible, budget 300 ms. `target-project` has no group of all 20, so `bench-ui` creates one in its config copy.
- Tests:
  - `perf_5_merged_history_is_newest_first_across_repos`, with interleaved dates and ties.
  - `perf_5_merged_history_walks_only_what_the_page_needs`: with one repo of 5,000 commits all older than another repo's 600, the first page takes nothing from the older repo beyond its first commit, asserted on the walk's counters.
  - `perf_5_unborn_and_missing_repos_contribute_no_rows`.
  - `ui_9_merged_list_shows_the_repo_column_instead_of_the_graph` (Vitest).
  - `ui_12_merged_search_filters_in_the_core`.
  - `ui_13_cherry_pick_needs_rows_of_one_repo` (Vitest).

## Out of scope
- The "Uncommitted changes (n repos)" row (UI-34, M3-13).

## Acceptance
- `group-history-first-page` is within 300 ms on `target-project` in `just bench`, and `select-group` is within 300 ms in `just bench-ui`.
- All tests listed above exist and pass on all three platforms, and `just check` passes.
