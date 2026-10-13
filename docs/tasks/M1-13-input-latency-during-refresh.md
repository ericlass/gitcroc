# M1-13 Input latency while a refresh runs

## Goal
The UI answers input within 50 ms while a status refresh of every repo runs, on 8 cores (section 9 "UI input latency while any job runs"). M1-12 measured 106.5 ms median (p95 174 ms) pinned to 8 cores, 22 ms with no refresh running, and 46 ms unpinned on 24 threads: the refresh competes with the webview for the CPU.

## Depends on
M1-12

## Spec
Implements: PERF-4
Respects: ARCH-6, ARCH-7, PERF-3, PERF-6, section 9 budgets, D2, D3

## In scope
- Find where the time goes during a refresh, and record it in the pull request. Check at least:
  - CPU: how many threads the refresh uses (the reads run up to 8 at once, and `gix` status may use more threads of its own) against the cores the webview needs;
  - the UI: what each `repo_summary` and `repo_refs_changed` event makes the UI do. For example, whether 20 summaries re-render the tree 20 times.
- Make the background refresh low priority, as PERF-4 asks, with the means M1 has before M2's job engine. Examples: a lower OS thread priority for the read threads, fewer reads at once or fewer status threads, or batching events. A read the user is waiting for, such as the selected repo's history or a commit's changes, stays at normal priority.
- Fix what the UI does per event if that is part of the cost.
- The refresh as a whole may take longer, but must stay within 2 s for all 20 repos (section 9, M3 row) on the same machine.
- Tests: a test named after PERF-4 that the background reads run at the lower priority, or within the lower limit; no test asserts a time (no timing in CI, see the README "Timing in CI" row).

## Out of scope
- M2's shared job pool. This task's mechanism is replaced by it, or carried into it.
- Memory (M1-14) and cold start (M1-15).

## Acceptance
- `taskset -c 0-7 just bench-ui` gives `input-latency` with a median within 50 ms, and no other row gets worse than its budget. The new numbers are written into `docs/tasks/M1-signoff.md`.
- `taskset -c 0-7 just bench` still passes.
