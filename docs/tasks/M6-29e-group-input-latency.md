# M6-29e Input latency during a group fetch

## Goal
Filter input while a group fetch runs meets section 9's 50 ms budget on
target-project, without changing the budget or weakening the measurement.

## Depends on
M6-29b: its complete native benchmark and retained scripts/frame traces.

## Spec
Respects: ARCH-6 to ARCH-9, PERF-4, PERF-5, PERF-8, OPS-7, UI-21,
QA-1, QA-3 to QA-5, section 9 UI input latency budget.

## Evidence
[M6-29's recorded five-run result](M6-29-measurements.md):
`input-latency-group-job` median 57.548 ms, p95 88.893 ms, budget 50 ms.
The window stayed active for every scroll frame. Single-repo job input
and refresh input passed, as did every other budgeted row. The failing
sample starts at the key event and ends only while the core still has a
running job; focusing the field is outside the measured interval.

## Scope
- Profile the retained group-fetch scene, separating time waiting for
  the core's filtered tree from UI/event/frame work.
- Check scheduling of reads while the shared job pool is busy and the
  coalescing of group/history refreshes. These are investigation points,
  not a predetermined diagnosis.
- Add a functional regression test for the diagnosed cause before fixing
  it; timing assertions stay off hosted CI.
- Re-run the identical five-run command on the reference machine, pinned
  to CPUs 0–7 at 144 Hz, with the window in front and no builds/tests busy.
- Record both before and after results, including memory and scrolling.

## Acceptance
`input-latency-group-job` median is at most 50 ms; every other gating row
remains within budget, `just check` passes and M6 coverage stays empty.
No spec or threshold changes. This remains open before the M6 sign-off.
