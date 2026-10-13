# M1-14 Idle memory

## Goal
The app, including its webview, uses at most 400 MB with the `target-project` workspace idle (section 9 "Memory, workspace idle, including the webview"). M1-12 measured 590 MB median (p95 690 MB) as proportional set size, pinned to 8 cores. The gitcroc process held 360 to 395 MB, the WebKit web process 205 to 280 MB and the network process 15 MB. Unpinned on 24 threads the total was 760 MB.

## Depends on
M1-12

## Spec
Respects: ARCH-6, ARCH-8, PERF-1, PERF-2, PERF-3, section 9 budgets, D2, D3

## In scope
- Find what the gitcroc process holds after `just bench-ui`'s session (largest repo selected, its whole history scrolled, commits selected, repos switched, a refresh run), and record it in the pull request. That memory is either data the core keeps or freed memory the allocator keeps. More memory with more threads points at per-thread malloc arenas. Check at least: `gix` object and pack caches per open repo, the history walk and its pages, the changes view and its diffs, and the summary reads.
- Find what the web process holds: the history rows the store keeps, CodeMirror, the DOM.
- Reduce both until the total is within budget. Examples: bound or drop caches when a repo is no longer selected, release the allocator's free memory, limit the number of arenas, keep fewer history rows in the UI. Each change keeps ARCH-6 and the other budgets.
- `just bench-ui --verbose` already prints the memory per process.
- Tests: a test for each bound or drop that it happens (for example, a repo's cache is dropped when it is no longer selected), named after the requirement it serves; no test asserts a memory size or a time.

## Out of scope
- Memory of `big-history` scrolled 100,000 rows (stretch; reported later).
- Input latency (M1-13) and cold start (M1-15).

## Acceptance
- `taskset -c 0-7 just bench-ui` gives `memory-idle` with a median within 400 MB, and no other row gets worse than its budget. The new numbers are written into `docs/tasks/M1-signoff.md`.
