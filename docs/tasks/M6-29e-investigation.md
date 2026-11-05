# M6-29e group input latency

Reference Linux machine and datasets: [M6-29](M6-29-measurements.md).
Baseline is current main `655b8f619c1cb35ba751a5dd45cc204eb8ecfe9d`,
including scrollbar PR #159, merged 2026-11-05 12:49:11 UTC.
The display was verified at 143.98 Hz (`xrandr`), judged at 144 Hz.
The user authorized the foreground runs and closed busy desktop apps.
No builds/tests ran alongside any app measurement. All thresholds and
measurement rules are unchanged, including both 400 MB memory budgets.

## Diagnosis

M6-29's retained traces record group input median 57.548 ms, p95 88.893 ms.
They contain end-to-end input samples and scroll intervals, without phase
timings. An instrumented current-main baseline reproduced the miss at
64.005 ms. Spotify was later found consuming CPU; its load during that
baseline was not sampled. The complete baseline was therefore repeated
after the user idled the desktop: median 60.597 ms, p95 152.218 ms.
Every other gating median passed in that repeat.

Opt-in diagnostics separate the core's blocking-executor queue, filter work,
result delivery to the UI, input dispatch and the completing frame. The
filter calls Tokio's blocking executor directly, without acquiring a shared
job-pool slot. Queue wait is usually tiny, while both full draws and cold
ref reads become variable during a group wave. This is CPU contention,
rather than filter reads waiting behind jobs for a pool permit.

Refresh coalescing was also checked: history reloads ignore in-progress
summaries and refs versions already seen; group-summary publication uses
tickets to discard stale answers. Merged history does notify on tree
updates, but reducing those notifications alone would not remove the
benchmark's explicit full draws. No group/history coalescing change is
selected for this fix.

A separate [per-view capture](results/M6-29e-view-profile.json.gz) on the
idled desktop measured group input at 41.426 ms: dispatch/full draws
36.249 ms, core queue 2.437 ms, core filter work 0.072 ms, and result
delivery 4.925 ms. History prepaint was the largest named view cost
(9.388 ms across the input journal). The diagnostic script omitted scroll
scenes to shorten capture; it does not replace the full budget command.

The pinned gpui-kit 0.8.1 input helper forces a full-window refresh before
typing and after each character. Those draws remain inside the original
measurement. View caching is bypassed by these explicit refreshes, so it
cannot resolve the measured miss. The optional `profile-filter` feature
enables GPUI's draw journal separately from the normal `perf-trace` build.

A [five-run experiment](results/M6-29e-write-priority-ui.json) lowered Linux
Git child CPU priority. It still missed at 50.903 ms median; that change was
removed. Its [scripts, traces and phase log](results/M6-29e-write-priority-traces.json.gz)
are retained.

The group engine runs up to eight sub-jobs. Each holds its job-pool slot
through its post-write status refresh. These refreshes previously ran at
foreground priority on Tokio's blocking workers with unrestricted gix
worker fan-out. In contrast, the existing background refresh runs on
low-priority threads and caps each status read at four workers. Concurrent
post-job refreshes could therefore compete with input and filtered-tree
reads across every pinned CPU.

## Fix and functional regression

Post-job status refreshes now use the existing background CPU scheduler
and four-worker reader. They still start immediately, preserve the same
pool permit/order, re-read every affected repo, and publish fresh state
before the sub-job ends. The scheduler uses a fresh thread, so gix workers
inherit its priority without permanently lowering a reusable Tokio worker.
Its existing fallback still reads if a dedicated thread cannot be started.
Git subprocess launching is unchanged.

Before fixing this path,
`perf_4_post_job_refresh_bounds_workers_and_inherits_background_priority`
failed with refresh/child niceness `(0, 0)` instead of `(19, 19)`; the
reader's thread limit was also unbounded. It now checks the refresh and
spawned worker priority, `Some(4)` worker limit, and that interactive Tokio
blocking workers retain their original priority. There are no timing
assertions. Existing job/summary tests check that fresh state still follows
completed writes. Linux scheduling is tested directly; no Windows/macOS
timing claim is made.

## Complete five-run measurements

Both before and after use one warm-up followed by five measured runs:

```sh
GITCROC_PROFILE_FILTER=1 taskset -c 0-7 just bench-ui --runs 5 --refresh-hz 144 --verbose
```

The opt-in phase log leaves the script, key-event start, running-job check,
frame sampling and all thresholds unchanged. The normal benchmark build
uses `perf-trace`, without the extra GPUI profiler. The window stays in
front; fixtures/config/cache are isolated as in M6-29.

| Scenario | Before median | Before p95 | After median | After p95 | Unit | Budget | After pass |
| --- | ---: | ---: | ---: | ---: | --- | ---: | --- |
| `cold-start` | 726.56 | 752.135 | 721.284 | 785.689 | ms | 1000.0 | yes |
| `refresh-all-ui` | 844.628 | 850.533 | 832.406 | 857.067 | ms | 2000.0 | yes |
| `select-group` | 125.0 | 131.959 | 131.959 | 131.965 | ms | 300.0 | yes |
| `input-latency` | 33.508 | 34.64 | 34.605 | 34.627 | ms | 50.0 | yes |
| `input-latency-job` | 27.731 | 35.581 | 27.789 | 29.391 | ms | 50.0 | yes |
| `input-latency-group-job` | 60.597 | 152.218 | 41.568 | 43.256 | ms | 50.0 | yes |
| `select-largest-repo` | 48.616 | 48.625 | 48.615 | 55.562 | ms | 200.0 | yes |
| `select-commit` | 104.161 | 104.17 | 104.163 | 104.166 | ms | 150.0 | yes |
| `switch-repo` | 48.618 | 69.444 | 48.617 | 76.39 | ms | 100.0 | yes |
| `switch-tab` | 13.895 | 20.846 | 13.896 | 27.769 | ms | 100.0 | yes |
| `memory-idle` | 302.042 | 307.97 | 305.108 | 315.584 | MB | 400.0 | yes |
| `memory-two-tabs` | 333.154 | 340.973 | 322.833 | 331.901 | MB | 400.0 | yes |
| `switch-tab-history` | 194.766 | 375.036 | 208.35 | 368.088 | ms | reported | yes |
| `scroll-history-frame` | 6.999 | 7.017 | 7.005 | 7.047 | ms | 7.014 | yes |
| `scroll-history-janky` | 0.775 | 1.033 | 0.775 | 1.463 | % | 1.0 | yes |
| `scroll-diff-frame` | 6.945 | 6.95 | 6.947 | 6.95 | ms | 7.014 | yes |
| `scroll-diff-janky` | 0.0 | 0.067 | 0.033 | 0.067 | % | 1.0 | yes |
| `scroll-blank` | 0.0 | 0.0 | 0.0 | 0.0 | ms | 100.0 | yes |

Before: [summary](results/M6-29e-idle-before-ui.json),
[scripts/traces](results/M6-29e-idle-before-ui-traces.json.gz),
[phase log](results/M6-29e-idle-before-profile.log).

Earlier baseline with unknown desktop load:
[summary](results/M6-29e-before-ui.json),
[scripts/traces](results/M6-29e-before-ui-traces.json.gz),
[phase log](results/M6-29e-before-profile.log).
The earlier input/frame diagnostic warm-up plus one run is also
[retained](results/M6-29e-dispatch-profile.log).

After: [summary](results/M6-29e-after-ui.json),
[scripts/traces](results/M6-29e-after-ui-traces.json.gz),
[phase log](results/M6-29e-after-profile.log). All warm-up and measured
traces report zero inactive scroll frames.

Group input's five measured samples are 43.256, 41.487, 42.444, 41.568, 40.819 ms (the JSON retains full precision). Every gating median
passes with the scrollbar changes in place. History scroll's p95 jank is
1.463%; the unchanged harness judges the five-run median, 0.775%, against
1%. No individual-run or tail guarantee is inferred from that pass.

## Validation

- The regression test was run failing before the fix, then passing after.
- `just check` passed, including real-window accessibility checks.
- `just coverage M6`: 75 covered, 0 missing.
- The full foreground UI command above exited successfully; all gating
  medians pass. Budgets, scenarios, workload and measurement rules are unchanged.
- The initial sandboxed check attempt could not bind local mock servers;
  full checks were run outside the sandbox. No hosted-runner timing assertions
  were added.

M6-29e is ready for review. This resolves the recorded Linux UI performance
miss; remaining platform/manual sign-off work in M6 is unchanged.
