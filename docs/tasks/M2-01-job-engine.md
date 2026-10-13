# M2-01 Job engine and write runner

## Goal
The core runs every repo write as a job: one at a time per repo, with progress, an outcome, git's output and a re-read of the repo afterwards. Background refreshes share the same pool at low priority.

## Depends on
M1

## Spec
Implements: ARCH-4, OPS-7, PERF-4
Respects: D3, D8, ARCH-1, ARCH-3, ARCH-5, ARCH-7, ARCH-9, GRP-7, GRP-8, GRP-10, GRP-12, SEC-6, DATA-3, section 10 `gitcroc.log`

## In scope
- `gitcroc-git` CLI runner:
  - Writes run with `GIT_EDITOR=true` added to `GIT_ENV`, so git never waits on an editor; an operation that needs a message passes it explicitly. Reads are unchanged.
  - Captures stdout and stderr in full, and streams stderr lines while the process runs, so progress lines (`--progress`) can be parsed. A carriage return ends a progress line like a newline.
  - A running process can be killed (GRP-10: fetch and clone).
  - Every git process, read or write, is logged with its arguments, working directory, duration and exit code to `gitcroc.log` in the log dir: rotating, 5 files of 10 MB (section 10). No stdin, stdout or file content is logged. The watcher warnings of M1-11 move from stderr to the log.
- `gitcroc-git::ops`: the home of every write. One function per operation builds its argument array (ARCH-3, paths after `--`) and returns git's output or a typed error carrying git's stderr. This task adds the module, the commit-graph write of M1-08 moved into it, and nothing else; each later M2 task adds its operations here.
- `gitcroc-core` job engine:
  - A job has an id, a workspace, a label, the repos it touches, progress (`JobProgress`), and on its end one outcome of GRP-7's vocabulary: *succeeded*, *nothing to do*, *failed*, *conflict* (*skipped* arrives with group operations in M3), plus git's combined output.
  - *Nothing to do* is decided by the operation, from refs and `HEAD` before and after, never from git's wording. *Conflict* is decided by reading the repo's conflict state after the run (OPS-1).
  - ARCH-4: at most one mutating git process per repo. Writes on a busy repo wait in that repo's queue in the order they were started. Reads (summaries, history, diffs) are never blocked by a write.
  - One pool runs jobs and the background refresh: 8 workers. User jobs come first; the refresh keeps its PERF-4 order behind them and its lowest thread priority (M1-13).
  - A job that has not started can be cancelled. A running fetch or clone is killed on cancel; any other running process finishes (GRP-10).
  - OPS-7: when a job ends, the repos it touched are re-read at once (summary, refs, and the history and changes views showing them), through the paths M1-11 uses for watcher events. The watcher's own refresh for the same change is not run a second time.
  - `RepoSummary.busy` is set while a write job waits or runs on the repo, so the UI can disable operations (GRP-8) and show the spinner (UI-3).
- IPC (`gitcroc-app`): `job_ended` event with the outcome and output; `job_cancel(job_id)`; `jobs_get` returning this run's jobs, newest first, with outcome and output, for the job list (M2-02). The `debug_emit_progress` debug job is removed.
- Replace `m1_no_write_operation_exists_yet` with `arch_1_writes_go_through_the_runner`: in the crates that ship, only the runner starts a process, and every runner call outside `gitcroc-git::ops` runs a read-only subcommand.
- Tests:
  - `arch_4_one_write_per_repo_at_a_time`: two writes started on one repo run one after the other in order; writes on two repos run at the same time. Asserted on the order of events, with a test runner whose processes block until released, not on timing.
  - `arch_4_reads_are_not_blocked_by_a_write`: a summary read finishes while a write on the same repo is held.
  - `ops_7_a_job_rereads_the_repos_it_touched`: a write that moves `HEAD` ends with the new summary sent, without a watcher event.
  - `perf_4_user_jobs_run_before_the_background_refresh`: with every worker busy with refresh reads, a started job takes the next free worker.
  - `grp_10_cancel_kills_a_running_process` with a fake long-running fetch, and `grp_10_a_queued_job_is_cancelled_before_it_starts`.
  - `grp_7_conflict_is_read_from_the_repo`: a job whose git run leaves `MERGE_HEAD` ends as *conflict*, not *failed*.
  - The log rotates at its size limit and holds no stdout.
  - `arch_1_writes_go_through_the_runner` as above.

## Out of scope
- Any operation of section 6 (M2-04 onward).
- The status bar's job list (M2-02).
- Group jobs, sub-jobs, preflight and the result report (M3).

## Acceptance
- The commit-graph write of M1-08 runs as a job of the engine and appears in `jobs_get`.
- All tests listed above exist and pass on all three platforms, and `just check` passes.
