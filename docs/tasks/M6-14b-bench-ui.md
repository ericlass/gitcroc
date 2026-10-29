# M6-14b `just bench-ui` on GPUI: the single-repo budgets

## Goal
The UI-side budgets of section 9 that need only one repo can be measured
again, now on the GPUI app: starting, selecting a repo or a commit,
switching, and scrolling the history and a diff at the new scroll budget.
`just bench-ui` left with the Tauri app in M6-05; this brings it back as
PERF-8 describes, an in-app runner in the `perf-trace` build.

This is the second of two parts of M6-14; M6-14a is background tabs.

## Depends on
- M6-14a: the panes and tab switching the scenarios drive, in their final
  form.

## Spec
Implements: PERF-8 (its UI-side half)
Respects: section 9 budgets and reference data, PERF-1, PERF-2, ARCH-5,
ARCH-7, QA-1, QA-3, QA-4, PLAT-4 (no update check in a `perf-trace`
build, as decided in M5-12)

## In scope
- **A `perf-trace` Cargo feature on `gitcroc-ui`**, off by default, never in
  a release build (`just bench-ui` builds into its own target folder, as
  rc.1's did). With it, `gitcroc-next --bench <script.json>` runs a
  scripted session in the real window, writes what it measured to a JSON
  file and quits. Without the feature the flag does not exist.
  - The runner drives the window through the same entry points a click or
    key uses (selecting a tree row, a history row, a file, a tab), not
    through the core, so the time includes the UI's own work.
  - **An action's time** is from the moment the runner starts it to the
    present of the first frame that draws its result, as rc.1's recorder
    measured from the input event to the frame after the result.
  - **Scrolling** moves the list by 100 px each frame, as rc.1's
    `scroll-frame` and the M6 history spike did, and records the interval
    between presented frames and the longest time any row on screen was a
    placeholder.
- **The driver, `gitcroc-cli bench-ui`** (PERF-8: the harness lives in
  `gitcroc-cli`; the CLI starts the app as a process and does not depend on
  `gitcroc-ui`, ARCH-5):
  - It sets up a throwaway config and cache with a workspace of the 20
    `target-project` repos, a second tab of the same repos, and filled
    summaries, through the existing `workspace` and `summaries` commands,
    as rc.1's driver did.
  - It starts the app once to warm up and five times to measure
    (`--runs <n>`), each a new process so the cold start is real.
  - It prints a JSON array in the shape `just bench` prints: median and
    95th percentile, the unit, the budget, pass. It exits 1 when a median
    is over its budget.
- **`just bench-ui`** builds `gitcroc-next` in release with `perf-trace`
  into `target/bench-ui` and runs the driver on `target-project`. Linux
  only, as before; run by hand before a release and for a sign-off, never
  in CI (hosted runners have no fixed speed).
- **Scenarios**, on `target-project`, named as rc.1's where the row is the
  same:

  | Scenario | What is timed | Budget |
  | --- | --- | --- |
  | `cold-start` | Process start to the first frame that shows the tree from cache | 1 s |
  | `select-largest-repo` | Click the largest repo to the first frame with its first history page | 200 ms |
  | `select-commit` | Click the commit core `select-commit` picks to the frame with its file list and first diff | 150 ms |
  | `switch-repo` | Click between two repos to the first frame of the other's history | 100 ms |
  | `switch-tab` | Click between the two tabs to the first frame of the other tab | 100 ms |
  | `scroll-history-janky` | The largest repo's history, top to end: share of frames over 1.5 refresh intervals | 1% |
  | `scroll-history-fps` | The same run: frames per second | the display's refresh rate, up to 144 |
  | `scroll-diff-janky` | The diff of the largest repo's root commit (`select-commit-large`), 3,000 frames | 1% |
  | `scroll-diff-fps` | The same run | the display's refresh rate, up to 144 |
  | `scroll-blank` | Both runs: the longest time a row stayed blank | 100 ms |

  The fps rows pass when the median is within 1% of the refresh rate the
  window reports, capped at 144 Hz. The JSON names that rate.
- **`--verbose`** also prints per-run values and, for each action, how its
  time splits between the core's answer and the UI's frames, so a missed
  budget points at a side.
- **Docs:** `CONTRIBUTING.md`'s "UI-side budgets" section describes the
  new `just bench-ui` in place of the line that says M6-29 measures them;
  M6-29 keeps the rows below.

## Tests (QA-3)
None of them times anything (QA-1, no timing in CI).
- `perf_8_bench_ui_judges_each_row`: the driver's summary gives the median
  and 95th percentile, fails a row over its budget, and judges the fps rows
  against the refresh rate it is given, capped at 144.
- `perf_8_frame_stats`: from a list of frame intervals, the share over 1.5
  intervals and the frames per second are right.
- `perf_8_the_runner_plays_a_script`: on a `repo-gen` workspace in a test
  window, the runner plays a script of every scenario and writes one sample
  each, so a renamed or broken entry point fails here and not on the
  reference machine.
- A test that `perf-trace` is off in the default features, and that
  `gitcroc-next --bench` is refused without it.

## Out of scope
These rows need UI that comes later, and go to M6-29's full budget run:
- `refresh-all-ui` (stale marks clearing after the start), `select-group`
  (groups in the tree, M6-21), `input-latency` and its job variants (the
  job list, M6-15, and group operations, M6-22);
- `memory-idle` and `memory-two-tabs` (M6-29's idle memory);
- the release-time run on Windows and macOS (deferred, as for rc.1);
- performance fixes for a budget this finds missed: each becomes its own
  task.

## Acceptance
- **On the reference machine,** `taskset -c 0-7 just bench-ui` prints every
  row above, and the numbers go in the pull request. A row over budget is
  recorded with its numbers, not hidden.
- `just check` passes.
