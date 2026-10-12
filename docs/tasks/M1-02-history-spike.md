# M1-02 History spike: paged walk and graph lanes

## Goal
The core returns a repo's history in pages of 500 commits with graph lanes, fast enough to meet the "first page of history" budget, and `gitcroc-cli history` prints it.

This is a spike in the sense of section 14: it carries the M1 performance risk. If the budget cannot be met with `gix`, stop and report the measurements before M1-08 starts, because D2 and D3 are then revisited (section 13).

## Depends on
M1-01

## Spec
Implements: PERF-2, ARCH-8 (history half; M1-03 adds the diff half)
Respects: D3, ARCH-2, ARCH-5, ARCH-7, UI-9, UI-12, PLAT-2, SEC-3, section 9 budgets

## In scope
- Add `gix` to `gitcroc-git`. Pick the current release, choose its features for read performance, record version and features in the pull request, and make sure `cargo deny check` passes.
- `gitcroc-git` read API, behind a trait so a CLI fallback can sit behind it later (ARCH-2):
  - `history(repo, start: HistoryStart, cursor: Option<Cursor>, filter: Option<Filter>) -> Page`.
  - `HistoryStart` is one of: the whole repo, one ref, or one stash entry. "The whole repo" starts from `HEAD`, every branch under `refs/heads` and every branch under `refs/remotes`; tags are not starting points, but appear as labels on the commits they point to.
  - Order: newest first by commit date, and a commit is never listed above one of its children, like `git log --date-order`. Uses the commit-graph file when present.
  - A page holds up to 500 commits (ARCH-8). Each row: full hash, short hash, subject, author name and email, author date, committer date, parent hashes, ref labels (local branch, remote branch, tag, `HEAD`), and its graph lanes.
  - The cursor is opaque to callers and carries the walk state and the lane state, so the next page continues the graph exactly where the previous one stopped. Nothing walks the full history first, the commit count included (PERF-2).
  - Lanes: per row, the lane of the commit, and the lines entering and leaving the row with their lane indices and a stable color index per line. Lanes are computed in the core, never in the UI (ARCH-6).
  - Filter (core part of UI-12): case-insensitive substring of subject or author, or a hash prefix. A filtered page walks only as far as it needs to fill 500 matches, or to the end of history. A filtered page has no lanes, because matches are not contiguous.
  - Text from the repo (subjects, names, ref names) is returned as data: invalid UTF-8 is decoded lossily, never rejected (SEC-3).
- `gitcroc-core`: `history_page` that forwards to the read API, runs off the async runtime's worker threads (`spawn_blocking` or equivalent), and returns typed errors (QA-4).
- `gitcroc-cli history <repo-path> [--ref <name>] [--filter <text>] [--pages <n>]`: prints the pages as JSON.
- Bench scenarios in `gitcroc-cli bench`: `history-first-page` on the largest `target-project` repo (budget 200 ms for the core part; the UI part is measured in M1-12), and on `big-history` as a stretch (300 ms). Also `history-page-100`, the 100th page, to show that later pages do not slow down.
- Tests (fixture repos from M1-01):
  - `perf_2_pages_continue_without_walking_everything`: a 1,200-commit repo gives pages of 500, 500 and 200 with no gaps or repeats; requesting page one walks at most a bounded number of commits (count them through an instrumented walk).
  - `perf_2_lanes_continue_across_pages`: a branch that forks before a page boundary and merges after it keeps its lane and color on both pages.
  - `arch_8_history_page_is_at_most_500_commits`.
  - Lane layout table tests: linear history, one fork and merge, octopus merge, two unrelated roots, a branch that ends.
  - Order: a child is never listed after its parent, including when a commit's date is older than its parent's (clock skew).
  - Ref labels for a local branch, a remote branch, a tag and a detached `HEAD`.
  - Filter by subject, by author and by hash prefix.

## Out of scope
- The history pane (M1-08).
- Stash listing in the tree (M1-07); this task only accepts a stash as a start.
- Merged history of several repos (PERF-5, M3).

## Acceptance
- `gitcroc-cli history` on the largest `target-project` repo prints the first page.
- `just bench` reports `history-first-page` under 200 ms on `target-project`; the `big-history` result is reported in the pull request.
- If the budget is missed, the pull request says so with the numbers, and M1 stops for a decision.
- All tests listed above exist and pass on all three platforms.
