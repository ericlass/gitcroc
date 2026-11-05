# M6-29e investigation

Baseline source: main `655b8f619c1cb35ba751a5dd45cc204eb8ecfe9d`, with
scrollbar PR #159 (merged 2026-11-05 12:49:11 UTC). Branch `m6-29e`.
Reference machine and dataset as in [M6-29](M6-29-measurements.md).
Display verified at 143.98 Hz by `xrandr`; harness judges at 144 Hz.
User authorized the foreground run before it started. No builds/tests ran
alongside the app measurements.

```sh
GITCROC_PROFILE_FILTER=1 taskset -c 0-7 just bench-ui --runs 5 --refresh-hz 144 --verbose
```

The environment flag adds opt-in phase diagnostics; the script, key-event
start, core-running-job check, sampling rules and every threshold are
unchanged, including both 400 MB memory rows. One warm-up plus five runs.
Exit status 1: group input and history scroll budgets fail.

[Summary](results/M6-29e-before-ui.json),
[scripts and traces](results/M6-29e-before-ui-traces.json.gz),
[phase log](results/M6-29e-before-profile.log).

| Scenario | Baseline median | p95 | Unit | Budget | Pass |
| --- | ---: | ---: | --- | ---: | --- |
| `cold-start` | 756.63 | 760.397 | ms | 1000.0 | True |
| `refresh-all-ui` | 850.686 | 901.212 | ms | 2000.0 | True |
| `select-group` | 131.961 | 138.909 | ms | 300.0 | True |
| `input-latency` | 20.751 | 35.252 | ms | 50.0 | True |
| `input-latency-job` | 36.603 | 41.573 | ms | 50.0 | True |
| `input-latency-group-job` | 64.005 | 86.684 | ms | 50.0 | False |
| `select-largest-repo` | 48.615 | 48.629 | ms | 200.0 | True |
| `select-commit` | 104.164 | 118.045 | ms | 150.0 | True |
| `switch-repo` | 48.617 | 62.524 | ms | 100.0 | True |
| `switch-tab` | 13.895 | 20.839 | ms | 100.0 | True |
| `memory-idle` | 286.663 | 291.333 | MB | 400.0 | True |
| `memory-two-tabs` | 314.667 | 326.916 | MB | 400.0 | True |
| `switch-tab-history` | 208.358 | 368.089 | ms | reported | True |
| `scroll-history-frame` | 7.172 | 7.214 | ms | 7.014 | False |
| `scroll-history-janky` | 3.27 | 3.787 | % | 1.0 | False |
| `scroll-diff-frame` | 6.954 | 6.964 | ms | 7.014 | True |
| `scroll-diff-janky` | 0.133 | 0.267 | % | 1.0 | True |
| `scroll-blank` | 0.0 | 0.0 | ms | 100.0 | True |

## Diagnosis in progress

The retained M6-29 traces contain total input samples and scroll intervals,
not core/UI phase timings. Original group input median: 57.548 ms. Current
main reproduces the miss at 64.005 ms. In the five baseline group samples,
core filter work is 0.060–17.311 ms and UI delivery 4.922–21.770 ms;
end-to-end group input is 47.2–86.7 ms. Core queueing is 0.016–2.993 ms.
The filter uses Tokio's blocking executor directly, not the shared job pool.

A separate diagnostic warm-up plus one run adds input-dispatch and next-frame
probes ([log](results/M6-29e-dispatch-profile.log)). Measured group dispatch
itself takes 32.433 ms; result delivery takes 18.229 ms and the next-frame
probe finishes at 54.423 ms. The warm-up also exposes cold ref-read work.

The pinned gpui-kit 0.8.1 `TestWindowExt::input` helper invokes
`render_frame` before typing and after each character; `render_frame` calls
`refresh`, bypassing view caching. Those full draws are inside the existing
measurement and must stay there. A trial functional render-count test
failed (19 to 44 history renders across a filter), but caching also failed
(15 to 34) because of those explicit refreshes. That experiment was removed;
it does not establish a production functional regression. No fix is selected
yet. Per-view draw profiling is the next investigation step.

After results and a functional regression test for the confirmed cause remain
pending. M6-29e and performance sign-off remain open.

## Validation and run coordination

`just coverage M6`: 75 covered, 0 missing. The initial sandboxed
`just check` failed because mock servers could not bind local sockets; it
passed when restarted outside the sandbox, including accessibility checks. A later host process inspection found
Spotify consuming substantial CPU. Its load during the baseline was not
sampled, so the baseline should be repeated under confirmed idle conditions
for final comparison. The user was asked to idle busy apps before the next
foreground capture.

The optional `profile-filter` feature enables GPUI's per-view profiler,
separate from the unchanged `perf-trace` budget build.
