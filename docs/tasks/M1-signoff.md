# M1 sign-off

Milestone M1 (docs/spec/13-milestones.md) is done when the single-repo budgets of section 9 are met on `target-project`, the stretch results for `big-history` and `big-tree` are reported, and no write operation exists yet. The person verifying fills in this file and commits it.

A missed budget is recorded below with its numbers. M1 is not complete until the budget is met or section 9 is changed by an explicit decision (section 9). If M1 misses its budgets, decisions D2 and D3 are revisited before M2 starts (section 13).

## How to measure

On the measuring machine, with nothing else busy:

```bash
just bench-data target-project   # and big-history, big-tree for the stretch rows
taskset -c 0-7 just bench --stretch
taskset -c 0-7 just bench-ui
```

`just bench` measures the core side and `just bench-ui` measures the real app (CONTRIBUTING.md, "Benchmarks"). Both print JSON. A row of the table below takes the median of its scenario, and `pass` means the median is within the budget. `just bench-ui --verbose` also prints where cold start time and memory go.

WebKitGTK rounds the page's clock to whole milliseconds, so UI-side times are whole numbers.

## Status

Windows and macOS are deferred (decided for M0 on 2026-10-12: no Windows PC or Mac to measure on). Their rows below are left empty.

## Machine (Linux)

- CPU: AMD Ryzen 9 3900X, 12 cores and 24 threads, pinned to 8 threads with `taskset -c 0-7`
- RAM: 32 GB (section 9 asks for 16 GB)
- Disk: Samsung SSD 970 EVO Plus 1 TB, NVMe
- OS: Ubuntu 24.04.5 LTS, kernel 6.8.0-147, WebKitGTK 2.52.6
- git: 2.43.0
- Commit measured: branch `m1-12-budgets-and-signoff`, 2026-10-13
- Measured by, date: Claude for Eric Lass, 2026-10-13

## Single-repo budgets on `target-project` (Linux)

| Section 9 row | Scenario | Measured (median) | p95 | Budget | Pass |
| --- | --- | --- | --- | --- | --- |
| Cold start: tree visible from cache | `bench-ui` `cold-start` | 1140 ms | 1215 ms | 1 s | **fail** |
| Select the largest repo: first page of history visible | `bench-ui` `select-largest-repo` | 178 ms | 185 ms | 200 ms | pass |
| Scroll history at any speed: 60 fps | `bench-ui` `scroll-frame` (frame time at 100 px a frame) | 16 ms | 17 ms | 16.7 ms | pass |
| Scroll history: no row blank for more than 100 ms | `bench-ui` `scroll-blank` (longest blank per run) | 0 ms | 0 ms | 100 ms | pass |
| Select a commit with under 5,000 changed lines: file list and first diff visible | `bench-ui` `select-commit` | 94 ms | 157 ms | 150 ms | pass |
| Switch selection between two repos: first paint | `bench-ui` `switch-repo` | 88 ms | 119 ms | 100 ms | pass |
| Working copy status of the 60,000-file repo, git defaults | `bench` `status-60k` | 73 ms | 85 ms | 500 ms | pass |
| UI input latency while any job runs | `bench-ui` `input-latency` (a status refresh of all 20 repos runs) | 106.5 ms | 174 ms | 50 ms | **fail** |
| Memory, workspace idle, including the webview | `bench-ui` `memory-idle` (PSS of the app and its WebKit processes) | 590 MB | 690 MB | 400 MB | **fail** |

The rows for the status of all 20 repos, tab switching and group history are M3.

Core-side parts, from `just bench` on the same machine, for comparison: `cold-start-core` 2.1 ms, `history-first-page` 21 ms, `select-commit` 13 ms (budgets 1 s, 200 ms, 150 ms). The core reads are far within budget. The time in the UI rows is spent in the webview and the IPC.

### Missed budgets

- **Cold start, 1140 ms of 1 s.** From the start of `run()` to the page's start takes 555 to 950 ms: Tauri creates the window and WebKitGTK starts its web and network processes. From the page's start to the tree takes 257 to 281 ms, of which about 65 ms is loading the page. The core's part (`cold-start-core`) is 2 ms.
- **Input latency while a refresh runs, 106.5 ms of 50 ms.** With no refresh running the same key takes 22 ms (`input-latency-idle`). Unpinned, on all 24 threads, the same run gave 46 ms. The refresh competes with the webview for the 8 cores, because it does not yet run at low priority: PERF-4's shared job pool comes with M2's job engine (tasks README, "What M1 completes").
- **Memory, 590 MB of 400 MB.** The gitcroc process holds 360 to 395 MB and the WebKit web process 205 to 280 MB (the network process 15 MB). Unpinned, the total was 760 MB. More threads giving more memory points at allocator arenas kept by the refresh threads, not at data the app holds.

Decisions (Eric, 2026-10-13):

- Input latency: fixed in [M1-13](M1-13-input-latency-during-refresh.md).
- Memory: fixed in [M1-14](M1-14-idle-memory.md).
- Cold start: investigated in [M1-15](M1-15-cold-start-investigation.md), which finds out whether and how 1 s can be reached.

## Stretch results (reported, not gating)

From `taskset -c 0-7 just bench --stretch`, core side only. The UI side runs on `target-project` only.

| Set | Scenario | Measured (median) | Stretch target | Within |
| --- | --- | --- | --- | --- |
| `big-history` | `history-first-page` | 96 ms | 300 ms | yes |
| `big-history` | `history-page-100` | 30 ms | none | |
| `big-history` | `select-commit` | 14 ms | none | |
| `big-tree` | `status-big-tree` (untracked cache on; fsmonitor is off on Linux, where git has no built-in monitor) | 214 ms | 1 s | yes |

Memory with `big-history` scrolled 100,000 rows (stretch target 1.2 GB) is not measured yet: `just bench-ui` runs on `target-project` only.

## No write operation exists yet

- [x] `m1_no_write_operation_exists_yet` (`crates/gitcroc-git/tests/read_only.rs`) passes. It reads the source of the crates in the app. It checks that only the runner starts a process, and that every runner call runs `--version`, `rev-parse` or `status`. The one exception is `commit-graph write` (M1-08). That write is offered to the user and changes only a cache file in the git directory, never a ref, an object, the index or a working file. Kept as allowed (Eric, 2026-10-13).

## Gates

- [x] `just check` runs `just coverage M1`, and M1 has no requirement without a test (QA-3).
- [ ] The Bench workflow passes on `main`. Run (URL):
- [ ] The Bench workflow, run on demand on a throwaway branch that slows `history-first-page` by more than 20% with a sleep, fails. Run (URL):
- [ ] `bench/baseline-linux.json` is committed, made from three runs on the CI runner. Run (URL):

## Windows (deferred)

| Section 9 row | Measured | Budget | Pass |
| --- | --- | --- | --- |
| All single-repo rows | | | deferred |

## macOS on Apple silicon (deferred)

| Section 9 row | Measured | Budget | Pass |
| --- | --- | --- | --- |
| All single-repo rows | | | deferred |
