# M6-29f Visible native scrollbars

## Goal
Every overflowing scroll area has a visible scrollbar usable without a
mouse wheel (user feedback on the release build, 2026-11-05).

## Spec
Respects: UI-22, UI-23, UI-17, PERF-1, QA-3, QA-5.

## Scope
- Attach bars to the existing virtualized tree, history, summary, file,
  palette and job-list handles, preserving selection and virtualization.
- Both diff axes share positions with wheel input; wrapping uses the
  variable-height list's own scroll handle.
- Include scrollable dialogs, settings, output, cards and editors.
- Bars stay visible while content overflows, using active theme colors.
- Regression tests click history and diff tracks and cover wrapped text.
- Rebuild the optimized binary for manual testing.

## Acceptance
Scrollbar regression tests and `just check` pass; visually check the real
window under Xvfb. No timing assertions on hosted CI. M6-29's measurements
predate this correction; M6-29e reruns budgets after its performance fix.
