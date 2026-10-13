# M1-06 Summary cache and cold start

## Goal
On start the app shows every repo's last known summary from a local cache, marked as stale, and replaces it with fresh data as each repo is read.

## Depends on
M1-02, M1-04, M1-05

## Spec
Implements: PERF-3, DATA-3
Respects: ARCH-6, ARCH-7, ARCH-9, PERF-4, UI-3, section 10 "Files" table, section 9 "Cold start" budget

## In scope
- `cache.db` in the platform cache dir under `gitcroc/`, using SQLite through `rusqlite` with its bundled SQLite. Record the version in the pull request and make sure `cargo deny check` passes.
- Content: one row per repo path with the M1-04 `RepoSummary` and the time it was read. A schema version is stored in the database; a database with another version, or one that fails to open, is deleted and recreated, since the cache is disposable (section 10). Deleting `cache.db` loses nothing.
- Start sequence in `gitcroc-core`:
  1. Load the workspace file and the cached summaries, and emit them at once, each marked stale (PERF-3).
  2. Refresh each repo's summary in the background, selected repo first, then the rest in workspace order. Run at most as many refreshes at once as the machine has cores, capped at 8. (M2-01 changed this to one at a time: see "Background refresh reads" in the README.) Each fresh summary is emitted as an event and written to the cache.
  3. A repo whose path is missing gets a "missing" summary (UI-3 warning icon) and keeps its cached data for display.
  - This scheduler is the minimal one M1 needs. M2's job engine replaces it with the shared job pool and the full PERF-4 priority order.
- IPC: `workspace_summaries` returns the cached state; the event `repo_summary` carries one fresh summary with its workspace id and repo id (ARCH-9).
- `gitcroc-cli summaries [--cached-only]` prints the summaries of the current workspace as JSON.
- Bench scenario `cold-start-core` on `target-project`: from process start to all 20 cached summaries available (the core part of the 1 s "Cold start" budget; the tree-visible part is measured in M1-12).
- Tests:
  - `perf_3_cached_summaries_are_shown_stale_then_refreshed`: with a cache from a previous run, the first emitted summaries carry the cached values and `stale = true`; after the refresh each repo has `stale = false` and current values.
  - `perf_3_cache_is_disposable`: a deleted, corrupt or wrong-version `cache.db` is recreated and the app still starts.
  - `data_3_reads_create_no_files_in_the_repo`: after adding a fixture repo, reading its summary, status, history and one diff, the set of files in the working tree and in `.git` and their contents are unchanged, apart from files git itself writes during a CLI fallback.
  - The selected repo is refreshed first.

## Out of scope
- Refresh triggered by file changes or window focus (M1-11).
- The tree pane (M1-07).
- Full status refresh of all repos as a measured budget: it is a multi-repo row, measured in M3.

## Acceptance
- Starting the app a second time shows the repos from the cache before git is read; deleting `cache.db` does no harm.
- All tests listed above exist and pass on all three platforms.
