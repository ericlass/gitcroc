# M6-22a The preflight dialog, the result report and group Fetch, Pull and Push

## Goal
A group operation runs in the GPUI app as in rc.1, through the four steps of
section 5: the core's preflight, the preflight dialog when the core asks for
it, the job with its sub-jobs, and the result report in the job list with
Retry failed. Fetch, Pull and Push are the first operations on that path,
from the toolbar and the group row's menu, on a group or a multi-selection.

## Depends on
- M6-15a: the status bar and the job list, which shows a group job's output
  until this task.
- M6-18: the operation dialogs' shared parts and the single-repo sync
  dialogs, whose options these dialogs reuse.
- M6-21: the group row's menu, whose operation entries are disabled until
  M6-22.
- M6-05, PR 3: `group_preflight`, `group_start`, `group_drop`,
  `group_retry_failed` and the `PreflightReady` event.

## Spec
Implements: GRP-6, GRP-7, GRP-9, GRP-10, GRP-11, GRP-12, UI-20 (the group target), UI-21 (a group job's report)
Respects: GRP-5, GRP-8, UI-6, UI-22, UI-23, UI-29, SEC-3, ARCH-6, ARCH-7, ARCH-9, D8, D15, QA-2, QA-3, QA-4

Also implements section 5's "Lifecycle of a group operation", which has no ID.

The core classes every repo, says whether the user must confirm (step 2,
GRP-9, and the "always show" setting), builds Retry failed's target set
from the report (GRP-11, GRP-5) and gives each repo its outcome (GRP-7).
The window only shows what it is given and sends the user's answer, as
rc.1's `ui/src/stores/groupOps.ts` does (ARCH-6).

## In scope
- **A group operation's flow**, a port of `ui/src/stores/groupOps.ts`, per
  workspace (ARCH-9), one preflight open at a time:
  - **request:** `group_preflight` with the workspace, the target set of
    the tree's selection (GRP-5) and the operation, which returns at once
    (ARCH-7);
  - **result:** the matching `PreflightReady`, also when it arrives before
    the request's answer; one for a preflight the user left is ignored;
  - **no dialog:** when the core says no confirmation is needed, the job
    starts at once with `group_start(id, false)`;
  - **cancel:** Cancel or Escape, before or after the result, closes the
    dialog and sends `group_drop`, also for an answer that arrives after it;
  - **refusal:** the core's refusal of the preflight or the start shows
    inside the open dialog.
- **The preflight dialog**, a port of `ui/src/groups/PreflightDialog.tsx`,
  on the shared dialog parts:
  - while the core works, "Checking the repos…" with a spinner, and Cancel;
  - once ready, the operation's label as title, the counts per class, and
    one row per repo with its class and reason, *needs attention* and
    *blocked* first, then target-set order within a class;
  - **destructive** (GRP-9): what cannot be undone, the core's list of
    actions, and every repo it runs in; the run button says the action and
    the count, such as "Force push in 4 repos", in danger colours, and
    Cancel has the focus. Otherwise "Run on n repos", which has the focus;
  - Run is disabled when no repo runs and while the job is starting;
  - the hint that a repo is left out by changing the tree's selection; the
    dialog never edits the target set (GRP-5);
  - repo names and reasons render as text (SEC-3).
- **The result report** in the job list, a port of
  `ui/src/jobs/GroupReport.tsx`:
  - while the job runs: each sub-job with Waiting, a spinner, or its
    outcome, and the job's progress;
  - when it ended: the counts per outcome, then one line per repo of the
    target set in target-set order, with its outcome (GRP-7) and the core's
    reason; *conflict* is not shown as a failure;
  - a line expands to git's output for that repo, as text (SEC-3), or says
    there is none; the repo's name selects it in its workspace's tree;
  - the arrow keys, Home and End move between lines, keeping the column
    (UI-22);
  - **Retry failed** (GRP-11) is enabled only when a repo failed, and runs
    `group_retry_failed` through the same flow; disabled, it says no repo
    failed;
  - a report with a failed or conflict repo opens by itself; others do
    not. The report stays until the app closes, as UI-21 says;
  - Cancel in the job list (M6-15a) cancels the group job; the cancelled
    repos show as *skipped (cancelled)* from the core (GRP-10).
- **Group Fetch, Pull and Push** (UI-20):
  - the toolbar's Fetch, Pull and Push run on a group or a
    multi-selection with section 5's defaults: Prune as git config says;
    the pull strategy of git config; set upstream on, no force;
  - the arrow beside each opens its options dialog, a port of
    `ui/src/groups/GroupSyncDialog.tsx`, titled with the operation and the
    target, such as "Push Backend": Prune; the pull strategy; Set upstream
    and Force with lease, which makes the preflight destructive;
  - the group row's menu enables Fetch, Pull and Push, which run as the
    toolbar's buttons do;
  - the toolbar's Branch and Merge stay disabled for a group until M6-22b;
    Stash says it works on one repository at a time. The reason "Group
    operations arrive with preflight dialogs" is gone.
- **Strings (D15):** the preflight dialog's, the report's and the group
  sync dialogs' text moves from `ui/src/strings/en.ts` to `strings.rs`.

## Tests (QA-3)
Real git on `repo-gen` fixtures (QA-2), in a workspace with a group of four
repos, one with no remote and one in conflict state. `JobEnded` is the
barrier before a fixture changes.
- `grp_9_the_preflight_dialog_appears_only_when_the_core_asks`: a
  preflight with every repo *will run* starts at once; one with a skip
  shows the dialog, and Run starts it confirmed.
- `grp_9_destructive_group_operation_names_every_affected_repo`: Push
  with Force with lease shows the actions and every repo, with "Force push
  in n repos" and Cancel focused.
- `grp_8_a_repo_in_conflict_is_blocked_for_pull_but_fetches`.
- `grp_5_the_dialog_never_changes_the_target_set`.
- `grp_9_cancel_drops_the_preflight_before_and_after_the_result`.
- `grp_7_the_report_lists_every_repo_with_one_outcome`: target-set order,
  counts, git's output on expand, and a conflict shown as such.
- `grp_6_one_repo_failing_does_not_stop_the_others`.
- `grp_10_cancel_reports_skipped_cancelled`.
- `grp_11_retry_failed_targets_only_the_failed_repos`.
- `grp_12_a_refused_pull_is_failed_with_gits_message`: the app stashes
  nothing and the repo's changes stay.
- `ui_20_the_toolbar_runs_fetch_pull_and_push_on_a_group`: defaults, and
  each options dialog's choice reaches the core.
- `ui_21_a_report_with_a_failure_opens_by_itself`.
- `ui_22_the_report_and_the_preflight_dialog_from_the_keyboard`.
- `sec_3_repo_names_and_git_output_render_as_text`.
- `ui_23_no_hardcoded_colors`, `d15_ui_text_comes_from_the_strings_module`
  and the keymap docs test keep passing over the new files.

## Out of scope
- **Checkout, Create and Delete branch, Merge, Rebase and Create tag** on a
  group, the Branches node's menu and its double-click (UI-5, GRP-13,
  GRP-14): M6-22b.
- **Group commit** (UI-34, GRP-15): M6-22c.
- **The group settings**, parallelism and "always show the preflight
  dialog": the settings dialog, M6-26. The core already reads them.
- **The palette's group commands** (UI-22): M6-24.

## Acceptance
- **On the reference machine,** `just dev-next` on `target-project`:
  fetch, pull and push a group and a multi-selection; force push with the
  confirmation and cancel it; open the report of a run with a failure,
  expand a line and Retry failed; all of it with the keyboard only too.
- `just coverage M6` no longer lists GRP-6, GRP-7, GRP-9 to GRP-12.
- `just check` passes.
