# M2-02 Status bar and job list

## Goal
The user sees running jobs with their progress in the status bar, and opens a job list where every job of this run shows its outcome and git's raw output.

## Depends on
M2-01

## Spec
Implements: UI-21
Respects: ARCH-6, ARCH-7, ARCH-9, PERF-1, GRP-7, SEC-3, UI-23

## In scope
- Status bar: running jobs of the shown workspace with label and progress (`done/total`, or a spinner when `total` is 0), and after the last job ends, that job's outcome for 10 s. A *failed* or *conflict* outcome stays until the job list is opened.
- Job list: opened by clicking the status bar, closed with Escape. One row per job of this run, across all workspaces, newest first: label, workspace name, repo, outcome or progress, start time and duration. Selecting a row shows git's raw output as text (SEC-3). Running jobs have Cancel (`job_cancel`); a queued job shows "waiting for <repo>".
- Jobs stay listed until the app closes; nothing is persisted.
- The jobs store mirrors `job_progress` and `job_ended` and reads `jobs_get` on start; it derives nothing (ARCH-6).
- Outcome colors and icons as theme tokens for light and dark (UI-23).
- Fake core: jobs with each outcome, a running job with progress, a queued job.
- Tests:
  - `UI-21 status bar shows running jobs with progress` (Vitest).
  - `UI-21 job list shows outcome and raw output of every job` (Vitest): one job per outcome, from two workspaces, labelled with their workspace.
  - `UI-21 a failed job stays in the status bar until seen` (Vitest).
  - `UI-21 cancel sends job_cancel` (Vitest).
  - Playwright, fake core: open the job list from the status bar with the keyboard, select a job, read its output, close it with Escape.

## Out of scope
- The result report of a group job (GRP-7, GRP-11; M3) and tab badges (UI-32; M3).

## Acceptance
- `just dev`: writing the commit-graph file of a repo shows a running job in the status bar and then a succeeded job with git's output in the job list.
- All tests listed above exist and pass, and `just check` passes.
