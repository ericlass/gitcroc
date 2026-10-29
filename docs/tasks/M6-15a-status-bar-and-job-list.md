# M6-15a Status bar, job list and tab marks

## Goal
The GPUI status bar shows the active tab's running jobs with their progress,
and then the outcome of the job that ended last, as rc.1's does. Clicking it
opens the job list: every job of this run, from every tab, with its outcome
and git's raw output, and Cancel while it runs. A tab shows a badge while it
has jobs, and a marker when one of them ended failed or in conflict and the
user has not seen it.

## Depends on
- M6-06: the status bar region and the dialog layer.
- M6-09: the tab bar, which gets the badges and markers.
- M6-05: `jobs_get`, `job_cancel`, and the `JobProgress` and `JobEnded`
  events.

## Spec
Implements: UI-21, UI-32
Respects: GRP-7, GRP-10, ARCH-6, ARCH-7, ARCH-9, PERF-1, SEC-3, UI-22, UI-23, UI-29, D15, QA-3, QA-4

GRP-7 and GRP-10 are respected, not implemented: the list shows each job's
outcome and cancels it through the app. A group job's report and Retry
failed (GRP-11) are M6-22.

**Job results** (decided by Eric, 2026-10-29): as rc.1. The plan's
"notifications for results" are the status bar's outcome and the tab's
marker; there are no pop-up toasts.

## In scope
- **The jobs model** (ARCH-6), a port of `ui/src/stores/jobs.ts`, one for
  the window (UI-21 covers all tabs):
  - it reads `jobs_get` at start and after lagged events, and mirrors
    `JobProgress` and `JobEnded`; a job already heard is kept when the read
    answers later, as rc.1 does;
  - it decides nothing about a job. It keeps only what the user saw:
    - the job that ended last in each workspace (ARCH-9);
    - the ended jobs the job list has shown;
    - the ended jobs the status bar of their own tab has shown.
- **The status bar** (UI-21), a port of `ui/src/layout/StatusBar.tsx` and
  `ui/src/jobs/outcome.tsx`, in the window's existing status bar region:
  - the active tab's running jobs, each with its label and progress: the
    phase git names (`Receiving objects`), then `done/total`, with "repos"
    for a group job and "files" for Resolve all with AI; a spinner, named
    "Running", while `total` is 0;
  - with no job running, the outcome of the job that ended last in that tab,
    for 10 s from its end;
  - after that, the newest *failed* or *conflict* job of the tab that the
    job list has not shown. It stays until the job list is opened;
  - otherwise "No jobs running";
  - each outcome has its icon and its colour token (`outcome_*`, UI-23).
    A skipped job says why, such as "Skipped (cancelled)";
  - the status bar is a button: a click, or Enter or Space while it has the
    focus (F6 reaches it, UI-22), opens the job list. The status bar's text is
    no live region, so progress is not read out on every step.
- **The job list** (UI-21), a port of `ui/src/jobs/JobList.tsx`, in the
  window's dialog layer:
  - one row per job of this run, newest first, from every tab: label,
    workspace name, repos, outcome or progress, start time and duration.
    A queued job says "waiting for <repos>";
  - the rows are a `uniform_list` (PERF-1);
  - opening it selects the newest job, focuses the list and marks every
    ended job as shown. Up, Down, Home and End move the selection, a click
    selects, and Escape, Close or a click on the backdrop closes it;
  - the selected job's detail: its label and state, its message, and git's
    raw output as text (SEC-3), in a scrolling area; "No output." when git
    printed nothing;
  - a job that has not ended has Cancel (`job_cancel`, GRP-10). The detail
    then says "Stopping git…" or that the job runs to its end, as rc.1
    does. Its end arrives as `JobEnded`. `jobs_get` and `job_cancel`
    cannot fail in the app's Rust API, so rc.1's messages for their
    failures have no place here;
  - jobs stay until the app closes; nothing is saved.
- **Tab marks** (UI-32), a port of the marks in `ui/src/stores/jobs.ts` and
  `ui/src/layout/TabBar.tsx`:
  - a badge with the number of the tab's running and queued jobs, named
    "<n> jobs running or waiting";
  - a marker when one of the tab's jobs ended *failed* or *conflict* and has
    been shown neither in the job list nor in that tab's status bar.
    Activating the tab shows it in the status bar, so the marker goes.
- **Per tab** (UI-29): the status bar follows the active tab; switching tabs
  shows that tab's jobs and its last outcome.
- **Strings (D15):** the status bar's, job list's and tab marks' text moves
  from `ui/src/strings/en.ts` to `strings.rs`.

## Tests (QA-3)
Real jobs on `repo-gen` fixtures (QA-2). A job is held running by a fetch from
a local host that asks for a password: the askpass prompt waits until the
test answers it with `askpass_answer`. The 10 s are waited on the test
executor's clock (`advance_clock`), never in real time (no timing in CI).
- `ui_21_status_bar_shows_running_jobs_with_progress`: a held fetch shows its
  label and the spinner; after the answer, its outcome; after 10 s, "No jobs
  running".
- `ui_21_a_failed_job_stays_in_the_status_bar_until_seen`: a failed fetch is
  still shown after 10 s, and is gone once the job list was opened.
- `ui_21_job_list_shows_outcome_and_raw_output_of_every_job`:
  - jobs that succeeded, failed and conflicted, from two tabs, newest first,
    each labelled with its workspace;
  - the selected job shows git's output;
  - Up, Down, Home and End move the selection, and Escape closes the list.
- `ui_21_cancel_stops_a_running_job`: Cancel on a held fetch ends it
  *skipped (cancelled)*.
- `ui_21_the_job_list_opens_from_the_keyboard`: F6 to the status bar, then
  Enter.
- `ui_32_tab_shows_a_badge_and_an_unseen_failure`:
  - a background tab with a held fetch has a badge of 1;
  - when the fetch fails, it has a marker instead;
  - activating the tab removes the marker;
  - a failure seen in the job list leaves no marker.
- `perf_1_the_job_list_lays_out_only_the_rows_on_screen`.
- `sec_3_job_output_and_labels_render_as_text`: output and a repo name with
  markup show as written.
- Unit tests:
  - the progress and outcome text;
  - the start time and the duration (`850 ms`, `4.2 s`, `3 min 5 s`);
  - which ended job the status bar shows;
  - the tab marks.
- `ui_23_no_hardcoded_colors`, `d15_ui_text_comes_from_the_strings_module`
  and the keymap docs test keep passing over the new files.

## Out of scope
- **A group job's sub-jobs, its result report and Retry failed** (GRP-7,
  GRP-11): M6-22. Until then the detail shows a group job's output.
- **Resolve all with AI's file list** (AI-13): M6-23.
- **The askpass dialog, the update notice and the update dialog** (D10,
  PLAT-4): M6-15b.
- **Announcing a failed job to screen readers**, which rc.1's live region
  did (M5-10): M6-25.
- **Selecting and copying the output**: plain GPUI text can't be selected,
  as with the commit header (M6-12).
- **The toolbar's operations that start jobs**: M6-16 to M6-22. Jobs that
  exist now, such as writing the commit-graph file (M6-11c), already show.

## Acceptance
- **On the reference machine,** `just dev-next` on `target-project`:
  - writing a repo's commit-graph file shows a running job, then its
    outcome, in the status bar;
  - the job list shows it with git's output;
  - the tab's badge shows while it runs.
- `just coverage M6` no longer lists UI-21 or UI-32.
- `just check` passes.
