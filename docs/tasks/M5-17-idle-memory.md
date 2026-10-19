# M5-17 Idle memory within 400 MB

## Goal
The app, including its webview, is within 400 MB with the `target-project` workspace idle and with a second tab in the background (section 9 "Memory, workspace idle, including the webview"; UI-31). The budget has been missed since M3: M4-12 measured 441 MB and 456 MB, and M4-13 found every commit from M3-15 to M4-11 between 379 MB and 430 MB, with no step. Eric deferred the fix on 2026-10-19 and the same day put it into M5, before the release candidate.

## Depends on
M5-08 to M5-12 (everything in M5 that adds to the UI or the core)

## Spec
Respects: ARCH-6, ARCH-8, PERF-1, PERF-2, PERF-3, UI-31, AI-16, D2, D3, section 9 budgets

## In scope
- Measure first, on the commit this task starts from, with `taskset -c 0-7 just bench-ui --runs 3 --verbose`: the medians and the size of each process (gitcroc, WebKit web process, network process). Record them in the pull request beside M4-13's table.
- Find what each process holds while idle, as M1-14 did. Check at least:
  - the gitcroc process: allocator arenas and freed memory not returned, `gix` caches of repos that are not selected, the summary cache, the job list's kept output, the AI and updater state;
  - the web process: the stores' kept rows (history, lists, the job list), CodeMirror instances kept after their pane closed, the strings module and the palette (M5-04 to M5-08), and parts rendered while hidden (Settings, dialogs).
- Reduce until both rows are within budget, keeping ARCH-6 and every other budget. Examples from M1-14 and M4-13: bound or drop caches when a repo is no longer selected, return freed memory to the OS, render dialogs and settings sections only while open.
- Tests: one per change that it happens (for example, a closed dialog leaves no editor instance), named after the requirement it serves. No test asserts a memory size or a time.

## Out of scope
- Other budgets, which M4-12 measured within budget; they must not get worse.
- The stretch memory target for `big-history`.

## Acceptance
- `taskset -c 0-7 just bench-ui` gives `memory-idle` and `memory-two-tabs` with medians within 400 MB, and no other row is over its budget. The numbers go into the pull request; M5-20's sign-off records the final run.
- `just check` passes.
