# M3-09 Preflight dialog, result report and Retry failed

## Goal
The user sees the preflight dialog when a group operation needs a look, confirms destructive operations against a list of the repos, follows sub-jobs while they run, and reads the result report with Retry failed.

## Depends on
M3-08

## Spec
Implements: GRP-9, GRP-11
Respects: GRP-6, GRP-7, GRP-8, GRP-10, UI-21, UI-22, UI-32, ARCH-6, section 5 "Lifecycle of a group operation"

## In scope
- Preflight dialog:
  - While the preflight runs, a progress line. Then one row per repo with its class and reason, *needs attention* and *blocked* first.
  - Counts at the top, for example "4 will run, 1 needs attention, 1 blocked".
  - **Run on N repos** starts the job (N counts *will run* and *needs attention*). **Cancel** drops the preflight.
  - Repos cannot be taken out of the target set here, since GRP-5 allows no other way to build one. The dialog says to change the selection instead.
  - When the core says no confirmation is needed, no dialog appears and the job starts at once.
- GRP-9: a destructive group operation always shows the dialog, which names every affected repo and the destructive part (for example "force-delete branch `old` in 4 repos"). The run button names the action. The same holds for one repo, through the M2 confirmation dialogs.
- Settings: **Group operations** section with `group_parallelism` (1 to 32) and "Always show the preflight dialog".
- Status bar and job list (UI-21):
  - A running group job shows `done/total` repos.
  - Its entry in the job list expands to the sub-jobs with their state while it runs, and to the result report when it ends.
- Result report:
  - One line per repo of the target set, in target-set order, with its outcome and reason, and counts per outcome at the top.
  - Each line expands to git's output. Clicking the repo name selects it in its workspace's tree, switching tab when needed.
  - The report opens by itself when the job ends with any *failed* or *conflict* outcome and the job's tab is active. Otherwise the tab marker of UI-32 shows.
  - It stays available in the job list until the app closes (UI-21).
- GRP-11, **Retry failed**: starts the same operation with the same options on a target set of the failed repos only, with a new preflight. It is enabled only when at least one repo failed. The core takes the job id and builds the target set from the report, so the UI sends no list of repos (GRP-5).
- Keyboard (UI-22): the dialog and the report work with Tab, Enter and Escape, and the report's lines with the arrow keys.
- Tests (Vitest against the fake core):
  - `grp_9_destructive_group_operation_names_every_affected_repo`.
  - `grp_11_retry_failed_targets_only_the_failed_repos` (core: the new job's target set, from the report).
  - The dialog appears only when the core says so, and Run starts with `confirmed: true`.
  - The report lists every repo with one outcome, opens on failure, and survives closing the job list.
  - The settings round-trip through the core, with values outside 1 to 32 refused.

## Out of scope
- Real group operations (M3-10 to M3-13). Until then the dialog is exercised with the test operation and the fake core.

## Acceptance
- Against the fake core, a group run shows the dialog, the sub-jobs and a report with Retry failed. The first real run follows in M3-10.
- All tests listed above exist and pass, and `just check` passes.
