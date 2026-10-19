# M4-13 Idle memory after M4

## Goal
The app, including its webview, is again within 400 MB with the `target-project` workspace idle (section 9 "Memory, workspace idle, including the webview"), and with a second tab in the background (UI-31). M4-12 measured `memory-idle` at 441 MB median and `memory-two-tabs` at 456 MB, against 358 MB and 376 MB in the M3 sign-off. All three were run with `taskset -c 0-7 just bench-ui --runs 3`. On the same machine and data, the M3 commit (b5824b9) still measured 358 MB and 379 MB on 2026-10-19, so the growth came with M4. Per process, `--verbose` showed:

| Commit | gitcroc | WebKit web process | WebKit network process |
| --- | --- | --- | --- |
| b5824b9 (M3) | 151 to 189 MB | 189 to 203 MB | 18 to 19 MB |
| 0d5a6cc (M4-11) | 185 to 199 MB | 232 to 260 MB | 20 MB |

## Depends on
M4-12

## Spec
Respects: ARCH-6, PERF-1, AI-16, section 9 budgets

## In scope
- Find which M4 commit added the memory, by measuring the commits between b5824b9 and 0d5a6cc with `just bench-ui --runs 1 --verbose` (the per-process sizes), and record the results in the pull request.
- Find what each process holds that it did not hold in M3. Check at least:
  - in the core: the HTTP client of the API providers, the CLI detection and self-tests at start, and the AI job state;
  - in the web process: the AI store, the Settings AI section, the consent dialog and the conflict editor's proposal parts, and whether they load or render while nothing uses AI.
- Reduce both until each row is within budget. Examples: build the HTTP client on first use, load the AI parts of the UI on first use. Each change keeps AI-16 and the other budgets.
- Tests: a test for each change that it happens (for example, no provider is built before the first proposal), named after the requirement it serves. No test asserts a memory size or a time.

## Out of scope
- Other budgets, which M4-12 measured within budget.

## Acceptance
- `taskset -c 0-7 just bench-ui` gives `memory-idle` and `memory-two-tabs` with medians within 400 MB, and no other row gets worse than its budget. The new numbers are written into `docs/tasks/M4-signoff.md`.
