# M6-29 Native smoke and performance measurements

Measured 2026-11-05 on the reference Linux machine: AMD Ryzen 9 3900X,
32 GB RAM, Samsung SSD 970 EVO Plus NVMe, Ubuntu 24.04.5 LTS,
NVIDIA GeForce RTX 4070 (driver 580.178.04), X11, 144 Hz display.
Rust 1.99.0, git 2.43.0. Both harnesses were pinned to CPUs 0–7;
no builds, tests or other benchmark runs ran alongside the recorded runs.
The measured source is `65f656d`, after main `42477d2`. Later commits
add this record, documentation capture and guide updates.

## Commands and evidence

```sh
xvfb-run -a just smoke
just check
just coverage M6
taskset -c 0-7 just bench --runs 5 --stretch
taskset -c 0-7 just bench-ui --runs 5 --refresh-hz 144 --verbose
```

Each benchmark warms up once, then measures five runs. Pass is based on
the median. The UI build uses `perf-trace` in `target/bench-ui`, with
throwaway config/cache and local clones for jobs; reference repos are not
changed. Both memory rows are the whole app process's proportional set
size (MiB, labelled MB as in the previous harness), sampled off the UI
thread after reads and jobs settle, with a commit and diff loaded in the
largest repo. The keyboard probe runs during a separate full refresh,
after startup is measured. The second row follows populating
history and a diff in another tab and returning to the first. The agreed
400 MB limit is unchanged; lowering it requires a separate decision.

Raw results: [core JSON](results/M6-29-core.json),
[UI JSON](results/M6-29-ui.json),
[every UI script and frame trace, gzip JSON](results/M6-29-ui-traces.json.gz).
The JSON summary keeps the existing `median_ms`/`p95_ms` field names; its
`unit` field distinguishes ms, percent and MB.

## UI budgets on target-project

The earlier column is M4's recorded value, also reproduced in the
[M5 sign-off template](M5-signoff.md#budgets-linux-target-project).
Scroll comparisons are omitted: M4 used 60 Hz; this run uses 144 Hz.
The earlier job rows used fetch/pull waves; these use actual single/group
local fetch jobs, counting only inputs presented while the core still
reports a running job. These are workload comparisons, not isolated
measurements of the UI stack change.

| Scenario | M4 median | Native median | Native p95 | Unit | Budget | Pass |
| --- | ---: | ---: | ---: | --- | ---: | --- |
| `cold-start` | 627 | 734.602 | 766.252 | ms | 1000.0 | yes |
| `refresh-all-ui` | 1096 | 836.856 | 842.644 | ms | 2000.0 | yes |
| `select-group` | 139 | 125.01 | 125.02 | ms | 300.0 | yes |
| `input-latency` | 29.5 | 20.778 | 34.632 | ms | 50.0 | yes |
| `input-latency-job` | 16 | 27.739 | 43.106 | ms | 50.0 | yes |
| `input-latency-group-job` | 24 | 57.548 | 88.893 | ms | 50.0 | **no** |
| `select-largest-repo` | 97 | 48.618 | 48.619 | ms | 200.0 | yes |
| `select-commit` | 98 | 90.283 | 104.163 | ms | 150.0 | yes |
| `switch-repo` | 86 | 48.62 | 69.444 | ms | 100.0 | yes |
| `switch-tab` | 84.5 | 13.898 | 20.846 | ms | 100.0 | yes |
| `memory-idle` | 441 | 300.472 | 306.097 | MB | 400.0 | yes |
| `memory-two-tabs` | 456 | 325.212 | 337.873 | MB | 400.0 | yes |
| `switch-tab-history` | — | 204.883 | 361.139 | ms | reported | — |
| `scroll-history-frame` | — | 6.999 | 7.005 | ms | 7.014 | yes |
| `scroll-history-janky` | — | 0.775 | 0.861 | % | 1.0 | yes |
| `scroll-diff-frame` | — | 6.945 | 6.945 | ms | 7.014 | yes |
| `scroll-diff-janky` | — | 0.0 | 0.0 | % | 1.0 | yes |
| `scroll-blank` | — | 0.0 | 0.0 | ms | 100.0 | yes |

## Core budgets and stretch results

| Data set | Scenario | M4 median | Native-era median | p95 | Budget (ms) | Pass |
| --- | --- | ---: | ---: | ---: | ---: | --- |
| `target-project` | `cold-start-core` | — | 2.366 | 2.874 | 1000.0 | yes |
| `target-project` | `refresh-all` | 854 | 794.563 | 842.497 | 2000.0 | yes |
| `target-project` | `open-repo` | — | 0.138 | 0.23 | reported | — |
| `target-project` | `history-first-page` | — | 21.573 | 21.736 | 200.0 | yes |
| `target-project` | `group-history-first-page` | 75.5 | 67.288 | 67.48 | 300.0 | yes |
| `target-project` | `history-page-10` | — | 19.838 | 24.287 | reported | — |
| `target-project` | `select-commit` | 98 | 13.246 | 13.396 | 150.0 | yes |
| `target-project` | `select-commit-large` | — | 12.013 | 12.504 | reported | — |
| `target-project` | `status-60k` | 81 | 74.626 | 80.969 | 500.0 | yes |
| `big-history` | `open-repo` | — | 0.154 | 0.162 | reported | — |
| `big-history` | `history-first-page` | — | 98.882 | 103.879 | 300.0 | yes |
| `big-history` | `history-page-100` | — | 29.683 | 32.417 | reported | — |
| `big-history` | `select-commit` | — | 13.393 | 13.407 | reported | — |
| `big-tree` | `open-repo` | — | 0.139 | 0.153 | reported | — |
| `big-tree` | `status-big-tree` | — | 217.086 | 222.607 | 1000.0 | yes |
| `many-repos` | `cold-start-core` | — | 5.518 | 6.741 | 1500.0 | yes |
| `many-repos` | `refresh-all` | — | 2753.57 | 2812.826 | 5000.0 | yes |
| `many-repos` | `open-repo` | — | 0.14 | 0.146 | reported | — |
| `many-repos` | `history-first-page` | — | 19.92 | 20.005 | reported | — |
| `many-repos` | `group-history-first-page` | — | 223.187 | 223.432 | reported | — |
| `many-repos` | `group-fetch` | — | 846.017 | 868.183 | reported | — |

Stretch results here are core-side, as in the previous sign-offs. The
many-repos tree's first paint and big-history memory after scrolling
100,000 rows remain unmeasured: the UI driver targets target-project.
Windows/macOS timing and manual desktop/screen-reader checks remain
separate from hosted-runner functional tests and M6-31's sign-off.

## Functional validation

- `xvfb-run -a just smoke` passed: add repos, history, stage, commit,
  manual merge conflict, a group made through tree multi-selection,
  checkout with the missing-branch create policy, mock AI provider,
  AI off/on, consent, proposal acceptance, Mark resolved and Continue.
  Real git checks the resulting commits, branches, clean index and file.
- `just check` passed, including the real-window accessibility trees.
- The e2e feature's parser tests and clippy pass. Screenshot decoding
  tests cover both pixel formats and row padding.
- `just coverage M6`: zero missing IDs. Native tests now cover the five
  remaining mappings: exact OS paths/argument arrays, scaled group menus,
  merged pagination, and embedded assets/images instead of web CSP.
- Linux and Windows CI run the native smoke in the existing required
  checks and upload diagnostic scripts/logs on failure. Windows runner
  evidence awaits running the PR; no Windows success is claimed here.

## Missed budgets

- `target-project` `input-latency-group-job`: 57.548 vs 50.0 (ms).

The remaining performance work is [M6-29e](M6-29e-group-input-latency.md).
M6 performance sign-off is pending that fix.

The visible-scrollbar correction ([M6-29f](M6-29f-visible-scrollbars.md))
was added after these measurements. M6-29e must rerun the complete UI
budgets with that rendering change in place.
