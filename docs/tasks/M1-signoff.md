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
- Commit measured: branch `m1-12-budgets-and-signoff`, 2026-10-13; again on branches `m1-13-input-latency-during-refresh`, `m1-14-idle-memory`, `m1-15-cold-start` and `m1-16-desktop-font`, 2026-10-13 (the table below is from M1-16, `--runs 10`)
- Measured by, date: Claude for Eric Lass, 2026-10-13

## Single-repo budgets on `target-project` (Linux)

| Section 9 row | Scenario | Measured (median) | p95 | Budget | Pass |
| --- | --- | --- | --- | --- | --- |
| Cold start: tree visible from cache | `bench-ui` `cold-start` | 569 ms | 633 ms | 1 s | pass |
| Select the largest repo: first page of history visible | `bench-ui` `select-largest-repo` | 111 ms | 120 ms | 200 ms | pass |
| Scroll history at any speed: 60 fps | `bench-ui` `scroll-frame` (frame time at 100 px a frame) | 16 ms | 17 ms | 16.7 ms | pass |
| Scroll history: no row blank for more than 100 ms | `bench-ui` `scroll-blank` (longest blank per run) | 0 ms | 0 ms | 100 ms | pass |
| Select a commit with under 5,000 changed lines: file list and first diff visible | `bench-ui` `select-commit` | 104.5 ms | 150 ms | 150 ms | pass |
| Switch selection between two repos: first paint | `bench-ui` `switch-repo` | 76 ms | 106 ms | 100 ms | pass |
| Working copy status of the 60,000-file repo, git defaults | `bench` `status-60k` | 68 ms | 85 ms | 500 ms | pass |
| UI input latency while any job runs | `bench-ui` `input-latency` (a status refresh of all 20 repos runs) | 26 ms | 37 ms | 50 ms | pass |
| Memory, workspace idle, including the webview | `bench-ui` `memory-idle` (PSS of the app and its WebKit processes) | 368 MB | 404 MB | 400 MB | pass |

The rows for the status of all 20 repos, tab switching and group history are M3.

Memory varies between runs by about 40 MB in the web process. In M1-15 the usual five runs gave a median of 399 MB (p95 415 MB), ten runs on the same build 354 MB; M1-14 measured 358 MB.

Core-side parts, from `just bench` on the same machine, for comparison: `cold-start-core` 2.0 ms, `history-first-page` 21 ms, `select-commit` 13 ms (budgets 1 s, 200 ms, 150 ms). The core reads are far within budget. The time in the UI rows is spent in the webview and the IPC.

### Missed budgets

- **Cold start, 1140 ms of 1 s (M1-12); 1094 ms after M1-13, 1137 ms after M1-14; met in M1-15 with 688 ms.** M1-15 measured the start in steps (`just bench-ui --start-only --verbose`, and `--direct` without WebDriver, which gives the same times).
  - Cause. The window was shown as soon as it was created, while WebKitGTK's web process was still starting. The app's main loop then waited up to 500 ms for the web process, and the web process waited for the main loop: both stalls start within 5 ms of each other in an `strace`, and the main loop's wait ends with a timeout. The page started 790 ms after `run()` instead of 290 ms. A bare Tauri 2 app on the same machine and Tauri version shows the same: its first frame comes 808 ms after `main()` with the window shown at creation, 297 ms with the window shown once its page loads. On this machine (NVIDIA RTX 4070, X11) it happens in every start; with WebKit's DMA-BUF renderer off, in about one start of six.
  - Fix. The window is created hidden and shown when its page starts loading (`tauri.conf.json`, `on_page_load` in `crates/gitcroc-app/src/lib.rs`; test `perf_3_the_window_is_shown_once_its_page_loads`). 20 of 20 starts were fast, with WebDriver and without.
  - Phases now, median ms since `run()` started (`--direct`, 10 runs): the process starts 25 ms before `run()`; the window is created at 171; the page starts at 288 (the bare Tauri app: 280, so up to here gitcroc is at the floor); the first script runs at 334; the first IPC answers arrive at 402, late because the page's main thread is busy with its first layout and loading; `tree_get` is answered at 456, after `git_status`, `workspace_get`, the app's first render and `workspace_summaries`; the tree is in the DOM at 465 and visible at 646. The core's part (`cold-start-core`) is still 2 ms.
  - The last 181 ms is the page's first full layout. Most of it is the UI font list: each family in `--font-ui-system` before the first installed one costs about 30 ms in that layout (2,652 fonts are installed here). On this machine `system-ui`, `-apple-system`, `Segoe UI` and `Roboto` are missing, and the list reaches `Ubuntu`, the desktop font. WebKitGTK resolves `system-ui` to fontconfig's default (Noto Sans here), not to the desktop font. With a list starting with an installed family the tree shows at 505 ms instead of 654.
  - Tried, with the effect on the median (`--direct`): no web process sandbox, none; compositing off, the page still started late in most starts; DMA-BUF renderer off (`WEBKIT_DISABLE_DMABUF_RENDERER=1`), 1147 to 611 ms before the fix and 654 to 592 ms after it, but the app cannot set it, since `std::env::set_var` needs `unsafe` (QA-4); the window shown when the page has finished loading instead of when it starts, 666 ms against 645; the changes pane and CodeMirror split into a chunk loaded later, the first script ran 25 ms earlier but the tree showed at the same time; no status refresh at the start, no change; the refresh spinner not animated, no change; no non-Latin glyphs in the tree, 25 ms less layout. Merging the start's IPC calls could save at most the 54 ms between the first answers and `tree_get`'s; not tried.
  - Recommendation: no section 9 change and no D2 revisit; 1 s is met with 300 ms to spare. One optional task: put an installed UI font first (on Linux, the desktop's font as GTK reports it), which is also what UI-26 asks for and saves about 150 ms here. It changes how the default font is chosen, so it needs Eric's decision.
  - Done in M1-16 (Eric, 2026-10-13): the app reads the desktop's UI font from GTK (`gtk-font-name`, e.g. `Ubuntu 11`) and reports its family in `app_info`; the UI puts it before the CSS list (`--font-ui-stack`). A UI font set by the user still comes first. Cold start 688 to 569 ms, and `select-largest-repo` 188 to 111 ms, whose first history layout went through the same list. The tree's first layout takes 70 ms instead of 181. The "Looking for git…" screen is still laid out with the CSS list once, before `app_info` answers (about 30 ms).
- **Input latency while a refresh runs, 106.5 ms of 50 ms (M1-12); met in M1-13 with 27 ms.** With no refresh running the same key took 22 ms (`input-latency-idle`). Unpinned, on all 24 threads, the same run gave 46 ms. The refresh competed with the webview for the 8 cores: up to 8 reads at once, each `gix` status starting threads of its own, 96 threads at the peak. M1-13 runs the refresh's reads at the lowest thread priority (PERF-4), and the threads `gix` starts inherit it on Linux. Now the key takes 27 ms both with the refresh running and without it. A refresh of all 20 repos on its own takes 0.64 s instead of 0.37 s (`gitcroc-cli summaries`, pinned), within the 2 s of the M3 row. M1-14 limits each background status read to 4 threads: 0.9 s, and `input-latency` 25 ms.
- **Memory, 590 MB of 400 MB (M1-12); 509 MB after M1-13.** The gitcroc process holds 360 to 395 MB and the WebKit web process 205 to 280 MB (the network process 15 MB). After M1-13, whose refresh threads end with each refresh, the gitcroc process held 165 MB and the web process 308 MB in the verbose run. Unpinned, the total was 760 MB. More threads giving more memory points at allocator arenas kept by the refresh threads, not at data the app holds. Met in M1-14 with 358 MB: the gitcroc process holds 134 to 153 MB, the web process 186 to 239 MB, the network process 15 MB.
  - Web process. The JS heap holds about 14 MB after a full collection, but JavaScriptCore runs only small collections until the heap reaches about 100 MB, and keeps what they promote. Scrolling the whole history built about 5,000 rows of DOM, and the store wrapped every row it read in a proxy with signals. Now the list keeps its row elements and shows other rows in them as it scrolls, and the store keeps the loaded commits unwrapped. The web process went from 290 to 360 MB to 186 to 239 MB.
  - gitcroc process. The history walk and the changes pane kept their repo open between pages and diffs, and with it the pack parts they read: 24 MB for the largest repo. They now open the repo for each page and diff (0.13 ms, `open-repo`). glibc's malloc keeps freed memory in an arena per thread, and a background status read started a thread per core; it is now limited to 4. One arena (`MALLOC_ARENA_MAX=1`) saved a further 35 MB but cannot be set from the app without `unsafe`; mimalloc as the allocator made the process larger (230 to 257 MB).

Decisions (Eric, 2026-10-13):

- Input latency: fixed in [M1-13](M1-13-input-latency-during-refresh.md); met.
- Memory: fixed in [M1-14](M1-14-idle-memory.md); met.
- Cold start: investigated in [M1-15](M1-15-cold-start-investigation.md), which finds out whether and how 1 s can be reached.

## Stretch results (reported, not gating)

From `taskset -c 0-7 just bench --stretch`, core side only. The UI side runs on `target-project` only.

| Set | Scenario | Measured (median) | Stretch target | Within |
| --- | --- | --- | --- | --- |
| `big-history` | `history-first-page` | 100 ms | 300 ms | yes |
| `big-history` | `history-page-100` | 30 ms | none | |
| `big-history` | `select-commit` | 13 ms | none | |
| `big-tree` | `status-big-tree` (untracked cache on; fsmonitor is off on Linux, where git has no built-in monitor) | 215 ms | 1 s | yes |

Memory with `big-history` scrolled 100,000 rows (stretch target 1.2 GB) is not measured yet: `just bench-ui` runs on `target-project` only.

## No write operation exists yet

- [x] `m1_no_write_operation_exists_yet` (`crates/gitcroc-git/tests/read_only.rs`) passes. It reads the source of the crates in the app. It checks that only the runner starts a process, and that every runner call runs `--version`, `rev-parse` or `status`. The one exception is `commit-graph write` (M1-08). That write is offered to the user and changes only a cache file in the git directory, never a ref, an object, the index or a working file. Kept as allowed (Eric, 2026-10-13).

## Gates

- [x] `just check` runs `just coverage M1`, and M1 has no requirement without a test (QA-3).
- [x] The Bench workflow passes on `main`. Run (URL): https://github.com/ericlass/gitcroc/actions/runs/38375452886
- [x] The Bench workflow, run on demand on a throwaway branch that slows `history-first-page` by more than 20% with a sleep, fails. Run (URL): https://github.com/ericlass/gitcroc/actions/runs/38375457210 (a 15 ms sleep: 47.8 ms against the baseline's 32.7 ms, 46% slower)
- [x] `bench/baseline-linux.json` is committed, made from three runs on the CI runner (ubuntu-22.04). Run (URL): https://github.com/ericlass/gitcroc/actions/runs/38362417076

## Windows (deferred)

| Section 9 row | Measured | Budget | Pass |
| --- | --- | --- | --- |
| All single-repo rows | | | deferred |

## macOS on Apple silicon (deferred)

| Section 9 row | Measured | Budget | Pass |
| --- | --- | --- | --- |
| All single-repo rows | | | deferred |
