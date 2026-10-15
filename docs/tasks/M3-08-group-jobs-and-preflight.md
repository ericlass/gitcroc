# M3-08 Group jobs and preflight in the core

## Goal
The core runs an operation on a target set as one job with one sub-job per repo. A read-only preflight first classifies every repo, the confirmation rule decides whether to ask, and a result report lists every repo's outcome.

## Depends on
M3-06

## Spec
Implements: GRP-6, GRP-7, GRP-8, GRP-10
Respects: D8, ARCH-4, ARCH-7, PERF-4, GRP-5, GRP-9, GRP-12, OPS-1, OPS-7, UI-30, section 3 "Group operation", "Job", "Preflight" and "Result report", section 5 "Lifecycle of a group operation"

## In scope
- Group operations (`gitcroc-core::group`):
  - `GroupOp` is the generated tagged enum of the ten group-capable operations of section 5, with their options. This task defines the enum and the engine. Each operation's preflight rule and run arrive with M3-10 to M3-13, and until then it is refused as "not available yet".
  - An operation's code supplies two parts: a read-only `classify(repo) -> Class` and the single-repo `run`, which reuses the M2 code of that operation.
- Preflight (step 1):
  - `group_preflight(workspace, target, op)` returns a preflight id at once (ARCH-7). The result arrives as `preflight_ready`.
  - The result gives one class per repo in target-set order: *will run*, *will skip* (reason), *blocked* (reason) or *needs attention* (reason).
  - It reads repos in parallel on the pool at user priority (PERF-4). It is read-only: the runner's read-only check of M2-01 applies.
  - GRP-8, checked for every mutating operation before the operation's own rule: a missing path, conflict state (except for Fetch), or another mutating job on the repo (queued or running) makes it *blocked*.
- Confirm (step 2), decided by the core:
  - `needs_confirmation` is true when any repo is not *will run*, when the operation is destructive (GRP-9), or when the setting "always show the preflight dialog" is on.
  - `group_start(preflight_id, confirmed)` refuses without `confirmed: true` in those cases.
  - A preflight is used once, and is refused when the workspace changed since it ran.
- Run (step 3):
  - One job, labelled with the operation and the target (for example "Pull · Backend (5 repos)"), with one sub-job per repo that preflight classed *will run* or *needs attention*.
  - Sub-jobs run in parallel up to the setting `group_parallelism` (1 to 32, default 8, in `settings.toml`; schema unchanged with the default added). The shared pool's size follows the setting, so single jobs and the refresh share it as before (PERF-4). ARCH-4 holds per repo.
  - At the start of each sub-job the GRP-8 rule is checked again. A repo blocked meanwhile is *skipped* with that reason, and git does not run.
  - GRP-6: one failed sub-job never stops or reverts the others (D8).
- Report (step 4), when every sub-job has ended. The job's `report` lists every repo of the target set, in target-set order, with exactly one outcome of GRP-7 and git's output:
  - *succeeded*, *nothing to do*, *failed* and *conflict* come from the sub-job, decided as in M2-01 (OPS-1);
  - *skipped* covers the preflight's *will skip* and *blocked* repos with their reasons (`SkipReason` gains `Preflight { reason }` and `Blocked { reason }`), and cancelled ones (*skipped (cancelled)*).
  - The status bar's failed or conflict rule of M2-02 applies when any repo ended *failed* or *conflict*.
- Cancel (GRP-10): stops every queued sub-job and kills running fetch and clone processes. Other running sub-jobs finish. Cancelled repos are *skipped (cancelled)*.
- OPS-7: when a sub-job ends, its repo is re-read at once, not when the whole job ends. This holds in a background tab too (UI-30).
- IPC: `group_preflight`, `preflight_ready`, `group_start`. `JobView` gains `report: Option<Vec<RepoResult>>` and live sub-job states. `jobs_get` and `job_ended` carry them.
- Tests (core, with a test operation and a test runner whose processes block until released; no timing):
  - `grp_6_one_failure_does_not_stop_others`.
  - `grp_7_every_repo_gets_exactly_one_outcome`: a target with one of each outcome reports each, in order.
  - `grp_8_missing_conflict_and_busy_repos_are_blocked_and_fetch_is_allowed_in_conflict`.
  - `grp_8_a_repo_blocked_after_preflight_is_skipped_at_run`.
  - `grp_10_cancel_skips_queued_kills_fetch_and_lets_others_finish`.
  - `group_parallelism_bounds_running_sub_jobs` for 1 and 3, asserted on the most sub-jobs held at once.
  - The confirmation rule: all *will run* starts without confirmation; one *will skip*, a destructive operation or the setting each need it.
  - A preflight is refused a second time, and after the workspace changed.

## Out of scope
- The preflight dialog, report view and Retry failed (M3-09).
- Each operation's classification and run (M3-10 to M3-13).

## Acceptance
- A test operation run through `gitcroc-cli group run --test-op` on a group of three fixture repos reports three outcomes.
- All tests listed above exist and pass on all three platforms, and `just check` passes.
