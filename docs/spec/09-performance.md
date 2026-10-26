# 9. Performance requirements

"Very good performance" means the budgets below, measured by an automated harness on a reference machine (8 cores, 16 GB RAM, NVMe SSD) on each of the three platforms.

## Reference data

| Name | What it is | Role |
| --- | --- | --- |
| `target-project` | A generated workspace modelled on Eric's largest project: 20 repos in 4 groups, 200,000 tracked files and 2 million lines of code in total, with one repo holding 60,000 of the files. 5,000 commits and 30 branches per repo | Gating: every budget below must be met on it |
| `big-history` | A clone of the Linux kernel repository (over 1 million commits) | Stretch: history, graph, diff |
| `big-tree` | A generated repo with 500,000 tracked files | Stretch: status, staging, checkout |
| `many-repos` | A generated workspace of 200 repos, 2,000 commits and 20 branches each, in 10 groups | Stretch: startup, group operations, merged history |

The generated sets are produced by a script in the project so every run uses identical data.

## Budgets

| Scenario on `target-project` | Budget (MUST) |
| --- | --- |
| Cold start: tree visible from cache | 1 s |
| Full status refresh of all 20 repos after start | 2 s |
| Select the largest repo: first page of history visible | 200 ms |
| Scroll history or a diff at any speed | The display's refresh rate, up to 144 Hz, with under 1% of frames over 1.5 refresh intervals; no row blank for more than 100 ms |
| Select a commit with under 5,000 changed lines: file list and first diff visible | 150 ms |
| Switch selection between two repos, or between two tabs: first paint | 100 ms |
| Select a group of all 20 repos: first page of merged history | 300 ms |
| Working copy status of the 60,000-file repo, git defaults | 500 ms |
| UI input latency while any job runs | 50 ms |
| Memory, workspace idle, the whole process | 400 MB |

These numbers are proposed targets. Milestone M1 measures the single-repo rows and M3 the multi-repo rows (section 13). A budget that proves wrong is changed here by an explicit decision, never silently missed.

The three stretch sets are measured and reported at the same milestones but do not block them. Stretch targets (SHOULD): first history page on `big-history` in 300 ms; status of `big-tree` in 1 s with fsmonitor and untracked cache on; cold start on `many-repos` in 1.5 s and its full status refresh in 5 s; memory under 1.2 GB with `big-history` scrolled 100,000 rows. The rules below apply regardless, so that the stretch sizes are never made structurally impossible.

On macOS the budgets are verified on Apple silicon only. Intel Mac builds ship in the universal binary (D6) and must pass the same functional tests, but carry no performance guarantee and need no Intel test machine.

## Rules that make the budgets reachable

- **PERF-1** Every list and tree is virtualized. The UI never lays out more rows than fit on screen plus a small overscan.
- **PERF-2** History is walked lazily in pages (ARCH-8). Graph lanes are computed per page as the user scrolls. Nothing ever requires walking the full history first, including the commit count.
- **PERF-3** The last known summary of every repo (branch, ahead and behind, dirty flag) is cached in SQLite and shown at startup marked as stale until refreshed.
- **PERF-4** Background refresh shares the job pool at low priority, ordered: selected repo, then repos visible in the tree, then the rest.
- **PERF-5** The merged history of a target set is a lazy k-way merge of per-repo walks by commit date. Only the page on screen is materialized.
- **PERF-6** File watching covers `HEAD`, the index and the refs of every repo's `.git`. The working tree is watched recursively only for the one selected repo. This keeps 200 repos inside OS watcher limits. Workspaces in background tabs are not watched at all (UI-30).
- **PERF-7** For repos over 50,000 files the app offers, once, to enable `core.fsmonitor` and `core.untrackedCache`. It never changes git config without asking.
- **PERF-8** The benchmark harness lives in `gitcroc-cli`, runs nightly in CI on Linux, and fails the build when a core-side budget regresses by more than 20%. UI-side budgets (frame rate, first paint) are measured by a scripted run, `just bench-ui` in the app's `perf-trace` build, before each release.
