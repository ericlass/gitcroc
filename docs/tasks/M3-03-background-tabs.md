# M3-03 Background tabs

## Goal
Background tabs cost almost nothing: they are not watched or refreshed, they let go of their history and diffs, and their tab shows when jobs run or end badly.

## Depends on
M3-02

## Spec
Implements: UI-30, UI-31, UI-32
Respects: PERF-3, PERF-6, OPS-7, ARCH-8, ARCH-9, OPS-9, UI-29, section 9 memory budget

## In scope
- UI-30, core side:
  - Deactivating a workspace stops its watchers and takes its repos out of the refresh queue.
  - A job that ends in a background workspace re-reads only the repos it touched (OPS-7 as in M2-01). Nothing else of that workspace is read.
  - Activating a workspace refreshes all its repos, selected repo first (PERF-4 order), and starts its watchers again.
  - Focus refresh covers the active workspace only (M3-01).
- UI-31: when a tab goes to the background:
  - The core ends its history walks and changes views, so their handles return `null` as for a superseded walk. The UI drops its history rows and diff documents.
  - The workspace keeps its summaries, tree skeleton, loaded refs, conflict editor buffers (OPS-9) and every position of UI-29.
  - When the tab becomes active again, the history re-reads its pages and scrolls back, as M1-08 does when the user goes back to a tree selection.
- UI-32, tab marks:
  - A badge with the number of running and queued jobs of that workspace.
  - A marker when a job of that workspace ended *failed* or *conflict* and the user has not seen it. Seen means the job list was opened, or the tab was activated while the job's report was shown in the status bar (M2-02's rule).
  - Both come from the core's job views; the UI derives nothing beyond counting.
- Tests:
  - `ui_30_a_background_workspace_is_not_watched_or_refreshed`: after deactivation no watcher is registered for its repos (`watched` is empty), and a commit made in one of its repos sends no summary until activation.
  - `ui_30_a_job_in_a_background_workspace_rereads_only_its_repos`.
  - `ui_30_activating_refreshes_the_workspace`.
  - `ui_31_a_background_workspace_releases_history_and_diffs`: its history and changes handles return `null`, and the core holds no walk for it. On return, the same rows and scroll position come back (Vitest with the fake core).
  - `ui_32_tab_badge_counts_jobs_and_marks_unseen_failures` (Vitest).
  - `just bench-ui` gains `memory-two-tabs`: memory with `target-project` active and a second workspace of the same repos in the background, after the background tab was scrolled. It is reported against the memory budget, which it must also meet.

## Out of scope
- Tab-switch timing (M3-15).

## Acceptance
- Memory with two tabs stays within 400 MB in `just bench-ui`, and close to one tab.
- All tests listed above exist and pass on all three platforms, and `just check` passes.
