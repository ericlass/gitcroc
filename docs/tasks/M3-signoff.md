# M3 sign-off

Milestone M3 (docs/spec/13-milestones.md) is done when the mixed-branch scenario passes as a test, all section 9 budgets are met on `target-project`, and the stretch results for `many-repos` are reported. The person verifying fills in this file and commits it.

## Status

Linux is verified below: every section 9 budget is met on `target-project`. Windows and macOS are deferred, as decided for M0 on 2026-10-12 (no Windows PC or Mac to measure on): CI runs `just check` there on every pull request, but the smoke test and the benches run on Linux only.

## Machine (Linux)

The M1 and M2 sign-offs' machine:

- CPU: AMD Ryzen 9 3900X, pinned to 8 threads with `taskset -c 0-7`
- RAM: 32 GB
- Disk: Samsung SSD 970 EVO Plus 1 TB, NVMe
- OS: Ubuntu 24.04.5 LTS, kernel 6.8.0-147, WebKitGTK 2.52.6
- git: 2.43.0
- Commit measured: branch `m3-15-signoff` on `main` at 8f2273e, 2026-10-16
- Measured by, date: Claude for Eric Lass, 2026-10-16

## Budgets (Linux, `target-project`)

`taskset -c 0-7 just bench-ui --runs 3` and `taskset -c 0-7 just bench --runs 3`. The M2 column is the M2 sign-off's table. Pass is decided by the median, as in M1 and M2.

| Section 9 row | Scenario | M2 median | M3 median | M3 p95 | Budget | Pass |
| --- | --- | --- | --- | --- | --- | --- |
| Cold start: tree visible from cache | `bench-ui` `cold-start` | 606 ms | 617 ms | 643 ms | 1 s | pass |
| Full status refresh of all 20 repos after start | `bench-ui` `refresh-all-ui` | (new) | 1,048 ms | 1,153 ms | 2 s | pass |
| Full status refresh, the core part | `bench` `refresh-all` | (new) | 781 ms | 834 ms | 2 s | pass |
| Select the largest repo: first page of history visible | `bench-ui` `select-largest-repo` | 102 ms | 101 ms | 104 ms | 200 ms | pass |
| Scroll history at any speed: 60 fps | `bench-ui` `scroll-frame` | 16 ms | 16 ms | 17 ms | 16.7 ms | pass |
| Scroll history: no row blank for more than 100 ms | `bench-ui` `scroll-blank` | 0 ms | 0 ms | 0 ms | 100 ms | pass |
| Select a commit with under 5,000 changed lines | `bench-ui` `select-commit` | 96 ms | 103 ms | 163 ms | 150 ms | pass |
| Switch selection between two repos: first paint | `bench-ui` `switch-repo` | 75.5 ms | 88.5 ms | 112 ms | 100 ms | pass |
| Switch between two tabs: first paint (UI-33) | `bench-ui` `switch-tab` | (new) | 86.5 ms | 110 ms | 100 ms | pass |
| Select a group of all 20 repos: first page of merged history | `bench-ui` `select-group` | (new) | 143 ms | 148 ms | 300 ms | pass |
| Select a group of all 20 repos, the core part | `bench` `group-history-first-page` | (new) | 68 ms | 68 ms | 300 ms | pass |
| Working copy status of the 60,000-file repo | `bench` `status-60k` | 69 ms | 74 ms | 79 ms | 500 ms | pass |
| UI input latency while any job runs: a refresh | `bench-ui` `input-latency` | 26 ms | 30.5 ms | 40 ms | 50 ms | pass |
| UI input latency while any job runs: fetch and pull | `bench-ui` `input-latency-job` | 19 ms | 16 ms | 26 ms | 50 ms | pass |
| UI input latency while any job runs: a group fetch and a group pull | `bench-ui` `input-latency-group-job` | (new) | 25 ms | 50 ms | 50 ms | pass |
| Memory, workspace idle, including the webview | `bench-ui` `memory-idle` | 378 MB | 358 MB | 360 MB | 400 MB | pass |
| Memory, with a second tab in the background (UI-31) | `bench-ui` `memory-two-tabs` | (new) | 376 MB | 379 MB | 400 MB | pass |

The p95 of `select-commit`, `switch-repo` and `switch-tab` is over the budget in three runs, as the first two were in M1 and M2. `switch-repo`'s median went from 75.5 to 88.5 ms since M2, still within the budget; its cause is not investigated.

How the new scenarios measure:

- `refresh-all-ui` runs in an app session of its own, on the warm cache, so nothing else is asked of the app meanwhile. It is timed from the process start to the frame after no repo row is marked stale (PERF-3), and fails when the tree does not show all 20 repos. `refresh-all` is the core part: a core started on the same warm cache reads every repo again.
- `switch-tab` opens the second workspace, of the same 20 repos, as a tab, and selects the largest repo in each tab. Each switch is timed from the click on the tab to the frame after the other tab's history shows its top row. Both tabs show the same repo, so its top row has the same hash. The panes are built anew for each tab (UI-29), so the measure ends when the new history list shows a row; 5 switches each way per run.
- `select-group` is M3-07's scenario. The group of all 20 repos is made for the run.
- `input-latency-group-job` types the filter key while a group fetch of all 20 repos from local bare remotes runs, then a group pull, in waves. Both start from the group's menu, as a user starts them, and every remote gets a new commit before each wave. The repos are working copies with bare remotes made for the run with `git clone --shared`, so the reference data is never written. 60 keys landed while a group job ran; after the run every repo is at a wave's commit.

### Found and fixed in M3-15

- **Input latency during a group job: 57 ms median, 107 ms p95, of 50 ms.** Right-clicking the group row to run the operation selects the group, so the history shows the merged history of all 20 repos. Each repo the group pull changed reloaded that history: 136 merged-history reads in one run, each of 20 repos. `bench-ui --verbose` now lists the commands the UI called and the event handlers over 10 ms, which showed it. Now changes that come while the history is read again make one more read after it, not one each (`OPS-7 refs changes during a read make one more read, not one each`). After the fix: 25 ms median, 50 ms p95, and 32 reads over 22 group jobs.
- **Context menus drawn under the history.** The smoke test's click on a submenu entry landed on a history row. Positioned history rows come later in the page and painted over a menu opened across them. Menus now sit above the panes; `UI-6 UI-7 a group of a multi-selection, from a menu drawn over the history` (Playwright) fails without the fix.
- **A group from a multi-selection** (Eric, 2026-10-16): the menu of a repo row in a multi-selection keeps the selection, and its **Add to group ▸** entries act on every selected repo; members already in the group keep their place.

### Missed budgets

None.

## Stretch results (reported, not gating)

From `taskset -c 0-7 just bench --runs 3 --stretch`, core side only. `just bench-ui` runs on `target-project` only, so the UI side of the `many-repos` cold start is not measured.

| Set | Scenario | Measured (median) | Stretch target | Within |
| --- | --- | --- | --- | --- |
| `many-repos` | `cold-start-core` (every repo's cached summary, a new process) | 5.9 ms | 1.5 s (the tree visible) | yes, core part |
| `many-repos` | `refresh-all` (full status refresh of 200 repos) | 2,659 ms | 5 s | yes |
| `many-repos` | `group-history-first-page` (all 200 repos) | 218 ms | none | |
| `many-repos` | `group-fetch` (200 repos from local bare remotes, parallelism 8) | 880 ms | none | |
| `many-repos` | `history-first-page` | 20 ms | none | |
| `big-history` | `history-first-page` | 101 ms | 300 ms | yes |
| `big-history` | `history-page-100` | 34 ms | none | |
| `big-history` | `select-commit` | 14 ms | none | |
| `big-tree` | `status-big-tree` (untracked cache on; fsmonitor is off on Linux) | 206 ms | 1 s | yes |

`group-fetch` makes a bare remote and a working copy of each repo with `git clone --shared`, and gives every remote a new commit before each run, untimed. It is timed from the preflight to the job's end, which comes after every repo was read again, and fails unless all 200 repos end *succeeded*.

## Mixed-branch scenario

- [x] `grp_mixed_branch_scenario` (`crates/gitcroc-cli/tests/integration/group_branches.rs`) passes: section 5's mixed-branch scenario on 5 repos, asserted on the result report. Date and commit: 2026-10-16, branch `m3-15-signoff`.

## End-to-end run (Linux)

`just smoke` (`ui/webdriver/smoke.ts`), run under `xvfb-run -a`. After M2's steps (add a repo, stage, commit, resolve a merge conflict by hand), the group flow:

1. **Open workspace → New workspace…** creates the tab `Second`.
2. **Add repository → Add folder…** adds `lib` and `app` (the `e2e` build answers the folder dialog with the folder of both).
3. A click on `lib` and a Ctrl-click on `app` select both; **Add to group ▸ New group…** in the menu makes the group `both` of the two.
4. **Check out branch…** on the group: `feature`, which only `app` has, with **Create the branch from HEAD**.
5. The preflight shows `lib` as *needs attention* and `app` as *will run*; **Run on 2 repos**.
6. The group row shows `feature`; the job list's report has a *succeeded* line for each repo, and git says both repos are on `feature`.

Every step waits for its result in the DOM; none waits for a time. Result: passed, 4 of 4 runs.

- Date and commit: 2026-10-16, branch `m3-15-signoff` (the commit of this file)
- CI: the Linux job runs `xvfb-run -a just smoke` after `just check`.

## Gates

- [x] `just check` runs `just coverage M3`, and M3 has no requirement without a test (QA-3): 32 covered, 0 missing.
- [ ] The baseline gains the new budgeted core scenarios, `refresh-all` and `group-history-first-page`. After this merges, run the Bench workflow on `main` with **baseline** ticked, and commit its `baseline-linux.json` in a pull request that says why. Until then the nightly gate reports them as `new`, and they still fail when over their budgets. Run (URL):
- [ ] The Bench workflow passes on `main` with the new baseline. Run (URL):

## Windows (deferred)

| Section 9 row | Measured | Budget | Pass |
| --- | --- | --- | --- |
| All rows | | | deferred |

## macOS on Apple silicon (deferred)

| Section 9 row | Measured | Budget | Pass |
| --- | --- | --- | --- |
| All rows | | | deferred |
