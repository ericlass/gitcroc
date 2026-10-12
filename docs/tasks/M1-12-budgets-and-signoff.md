# M1-12 Nightly benchmarks, budgets and M1 sign-off

## Goal
The benchmark harness runs nightly in CI and fails on a regression, the UI-side budgets can be measured on the real app, and milestone M1 is verified against its budgets.

## Depends on
M1-01 to M1-11

## Spec
Implements: PERF-8
Respects: QA-1, QA-3, QA-5, section 9 budgets and stretch targets, section 13 M1 row, D2, D3

## In scope
- `.github/workflows/bench.yml`, nightly and on demand, Linux only (PERF-8):
  - Generates `target-project` with `just bench-data`, cached between runs by the generator version.
  - Runs `just bench` and uploads the JSON as an artifact.
  - Fails when a core-side scenario's median is more than 20% slower than the baseline in `bench/baseline-linux.json`, or over its budget. The baseline is committed and changed only by a pull request that says why. The first baseline is the median of three runs on the CI runner, since a CI runner is not the reference machine.
  - `big-history` and the other stretch sets do not run in CI.
- UI-side measurement (section 9 rows the core cannot measure alone):
  - A `perf-trace` Cargo feature (off in release builds) that records, in the UI, the time from a user action to the next frame after the result is rendered, and the frame times while scrolling, and writes them through a debug-only command to a JSON file.
  - `just bench-ui` builds the app with the feature and drives it on Linux through `tauri-driver` and WebKitWebDriver: cold start to tree visible, select the largest repo to first history page, scroll the history at a fixed fast speed, select a commit with under 5,000 changed lines to file list and first diff, switch between two repos, input latency while a status refresh runs, and memory of the app plus webview processes after the workspace is idle. It prints the same JSON shape as `just bench`.
  - It is run by hand before a release and for this sign-off, not in CI.
- `docs/tasks/M1-signoff.md`: the M1 checklist, filled in by the person verifying:
  - For each single-repo budget row of section 9 on `target-project`: the measured value, the budget, pass or fail. Single-repo rows in M1: cold start, select the largest repo, scroll, select a commit, switch selection between two repos, working copy status of the 60,000-file repo, UI input latency while a job runs, memory. The rows for all 20 repos' status, tab switching and group history are M3.
  - The stretch results for `big-history` and `big-tree`, reported, not gating.
  - The machine used (CPU, cores, RAM, disk, OS). Section 9 asks for a reference machine with 8 cores and 16 GB; a faster machine is pinned to 8 cores (`taskset -c 0-7` on Linux) and its RAM noted.
  - "No write operation exists yet": a check that `gitcroc-git` has no write calls besides the fixture helpers, done with a test (below).
  - Windows and macOS rows are present and marked deferred, as decided for M0 on 2026-10-12.
- A failed budget is recorded with its numbers, and M1 is not complete until it is met or section 9 is changed by an explicit decision (section 9).
- Add `just coverage M1` to `just check` after `just coverage M0`, so every M1 requirement must have a test.
- Update `CONTRIBUTING.md` (bench workflow, `just bench-ui`, baseline updates).
- Tests:
  - `perf_8_regression_over_20_percent_fails`: the comparison logic fails a scenario 21% slower than its baseline and passes one 19% slower, and fails one over its budget.
  - A test that `gitcroc-git`'s non-test code calls the git CLI only with read-only subcommands in M1 (a list kept in the test: `status`, `diff`, `log`, `rev-parse`, `--version` and whatever M1-04 needed), so "no write operation exists yet" is proven, not claimed. M2 replaces this test.

## Out of scope
- Multi-repo budgets (M3) and the release-time UI run on Windows and macOS (deferred).
- Performance fixes beyond what is needed to meet a budget that this task finds missed; those become their own tasks.

## Acceptance
- The Bench workflow passes on `main` and, run on demand on a throwaway branch that slows `history-first-page` by more than 20% with a deliberate sleep, it fails.
- `docs/tasks/M1-signoff.md` is filled in for Linux, and every M1 budget row passes or has a recorded decision.
- `just check` enforces M1 coverage.
