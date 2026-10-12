# M1-11 File watching and refresh

## Goal
A change made in a terminal or another tool shows up in the app without user action: repo summaries, the tree, history and changes refresh when a repo changes on disk or the window regains focus.

## Depends on
M1-07, M1-08, M1-09

## Spec
Implements: PERF-6, OPS-7
Respects: ARCH-6, ARCH-7, ARCH-9, PERF-3, PERF-4, UI-30, section 9 "UI input latency" budget

## In scope
- `gitcroc-core` watcher using `notify` with a debouncer. Record versions in the pull request.
- What is watched (PERF-6):
  - for every repo in the workspace: `HEAD`, the index and the refs (`refs/` and `packed-refs`) of its git directory, plus the files OPS-1 uses for conflict state (`MERGE_HEAD`, `rebase-merge`, `rebase-apply`, `CHERRY_PICK_HEAD`, `REVERT_HEAD`);
  - for the one selected repo only: the working tree, recursively, excluding the git directory. The watch moves when the selection moves.
  - Worktrees and `.git` files that point elsewhere are followed to the real git directory.
- Events are debounced per repo by 200 ms. Lock files git creates and removes (`index.lock`, `*.lock`) do not trigger a refresh on their own.
- What a change refreshes (OPS-7): the repo's summary (tree row and cache, M1-06), its refs when expanded, and, if it is the selected repo, the uncommitted changes row and lists, and the history when refs or `HEAD` changed. The history keeps the user's scroll position and selected commit when that commit still exists.
- Window focus: when the window regains focus, all repos in the workspace refresh, through the same scheduler as M1-06 with the selected repo first.
- Refresh never blocks the UI; it runs through the M1-06 scheduler, and the UI only re-renders from the state the core emits (ARCH-6, ARCH-7).
- Watcher errors, such as reaching the OS watch limit, are logged and shown once as a warning; the app keeps working with focus refresh only.
- The watcher belongs to the workspace (ARCH-9), so M3 can stop it for background tabs (UI-30).
- Tests:
  - `perf_6_watches_git_files_of_every_repo_and_tree_of_selected_only`: with three fixture repos, the watch list contains the git paths of all three and the working tree of the selected one only; selecting another repo moves the working tree watch.
  - `ops_7_external_commit_refreshes_the_summary`: a commit made with `git` in a fixture repo emits a new summary within one second.
  - `ops_7_external_edit_refreshes_the_selected_repo`: editing a file in the selected repo's working tree emits a new working copy state; editing one in an unselected repo emits nothing for its working copy.
  - `ops_7_focus_refreshes_all_repos`: the focus command refreshes every repo, selected first.
  - Lock files alone trigger nothing; a burst of 100 changes in 100 ms gives one refresh.

## Out of scope
- Refresh after jobs (M2, with the job engine).
- Background tabs (M3).

## Acceptance
- `just dev`: committing, checking out or editing a file in a terminal updates the app within a second, without losing the history position.
- All tests listed above exist and pass on all three platforms.
