# M1-08 History pane

## Goal
Selecting a repo, branch, tag, remote branch or stash in the tree shows its history in the middle pane with a commit graph, smooth at any scroll speed, with search.

## Depends on
M1-02, M1-07

## Spec
Implements: UI-9, UI-10, UI-12
Respects: PERF-1, PERF-2, PERF-7, ARCH-1, ARCH-6, ARCH-7, ARCH-8, ARCH-9, OPS-2, UI-11, UI-13, SEC-3, UI-23, section 8 "History pane" table, section 9 budgets

## In scope
- What the pane shows follows the section 8 table for single-repo selections: a repo shows the whole repo history; a branch, tag or remote branch shows history from that ref; a stash shows the stash as one commit row. Ungrouped or nothing selected shows an empty state.
- Columns (UI-9): graph, subject with ref labels, author, date, short hash. The date shown is the author date, as local time `YYYY-MM-DD HH:MM`, with the full date and the committer date in a tooltip. Column widths are resizable and kept in the per-workspace layout store.
- Graph: drawn per row as inline SVG from the lanes the core computed (M1-02); the UI only draws, it computes no lanes (ARCH-6). Line colors come from a set of theme tokens added to `tokens.css` for light and dark (UI-23).
- Ref labels distinguish local branches, remote branches, tags and `HEAD`, and mark the current branch.
- Paging (ARCH-8, PERF-2): the list is virtualized (PERF-1) and requests the next 500-commit page when the user nears the end of the loaded rows. While a page loads, its rows show a placeholder skeleton; the list never shows a blank row for more than 100 ms in normal use (section 9 scroll budget). The scrollbar grows as pages load; the total count is never computed up front.
- Uncommitted changes (UI-10): for a repo or its current branch, when the working copy is dirty, the first row is "Uncommitted changes (n)", where n is the number of distinct paths that are staged, unstaged, untracked or unmerged.
- Search (UI-12): a box above the list sends the text to the core, which filters (M1-02). Each keystroke supersedes the previous request. Filtered results have no graph column lines.
- Selecting a commit row or the uncommitted row updates the selection store, keyed by workspace id (ARCH-9); the changes pane uses it in M1-09. Keyboard: up and down move the selection, Page Up and Page Down, Home and End.
- History position (scroll and selected commit) is kept per tree selection, so going back to a repo restores where the user was.
- All repo text rendered as text (SEC-3).
- Commit-graph offer. Without a commit-graph file, the first page explores the whole history first: 9.3 s for `big-history`, against 0.17 s with the file (measured in M1-02).
  - Core: a page reports when its walk had no commit-graph file to use. When that page also took longer than the 200 ms budget, the core offers to write the file. Nothing counts commits up front (PERF-2).
  - UI: the offer is shown once per repo, in the history pane: write the file, or don't ask again. The answer is kept per repo in the workspace. Like PERF-7, the app never writes the file without asking.
  - Writing it runs `git commit-graph write --reachable` through the git CLI runner (ARCH-1), as a low-priority background job with progress (ARCH-7). This took 12.9 s for `big-history`. A failed write is reported, and history keeps working without the file.
  - The app does not keep the file current and does not change git config such as `fetch.writeCommitGraph`. Commits added after the write cost only themselves; git refreshes the file on its next `gc`.
- Fake core: a synthetic history of any length with lanes, for tests.
- Tests:
  - `UI-9 history columns` (Vitest): a row shows graph, subject with labels, author, date and short hash from the fake core.
  - `UI-10 uncommitted changes row` (Vitest): shown first with the count when the fake summary is dirty; absent when clean.
  - `UI-12 search filters in the core` (Vitest): typing sends the filter to the IPC layer and renders what comes back; the component itself filters nothing.
  - `ui_12_filter_by_subject_author_and_hash` (Rust, core side) if M1-02 did not already add it under this name.
  - Playwright, fake core with 100,000 commits: scrolling from top to bottom loads pages on demand, the DOM row count stays bounded, and no row stays blank longer than 100 ms with the fake core's paging delay set to 20 ms.
  - Playwright: selecting a branch in the tree shows that branch's history.
  - `perf_2_slow_walk_without_commit_graph_offers_to_write_it` (Rust): a fixture repo without the file gets the offer when its first page is over the threshold (made small for the test), and never gets it once the file exists or the user has declined.
  - `arch_1_commit_graph_is_written_by_the_git_cli` (Rust): accepting runs the job through the runner; afterwards the repo has the file and the walk uses it.
  - `Commit-graph offer` (Vitest): shown once per repo; "don't ask again" sends the answer to the core and hides the offer.

## Out of scope
- The conflict banner (OPS-2, M2) and commit context menus (UI-13, M2).
- Group and multi-selection history, summary strip (UI-11, PERF-5, M3).
- Measuring first paint and frame rate on the real app (M1-12).

## Acceptance
- `just dev`: selecting a repo in the tree shows its graph; scrolling through thousands of commits stays smooth; search narrows the list.
- All tests listed above exist and pass on all three platforms.
