# M6-29b Full GPUI budgets

## Goal
Complete M6-14b's runner with refresh, group selection, input during
refresh and single/group jobs, and one/two-tab idle process memory.

## Spec
Respects: PERF-1 to PERF-8, UI-30, UI-31, ARCH-3, ARCH-5 to ARCH-7,
QA-1, QA-3 to QA-5, D12, section 9 reference data and budgets.

## Scope
- Extend the existing CLI driver and frame runner, sharing their scripts.
- Judge memory in MB against 400 MB (Eric, this task); a lower limit
  requires a separate decision after measurements.
- Tests first for scenario coverage, judging and runner behavior.
- Run core and UI budgets on target-project, report stretch sets and
  environment, keep raw results. Report each missed budget explicitly.

## Acceptance
Every section 9 row is measured, with raw results and a comparison to
previous measurements; `just check` passes. Hosted CI tests behavior only.
Performance fixes that exceed this slice become explicit follow-up tasks.
