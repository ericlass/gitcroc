# M6-29a Native smoke test

## Goal
The e2e GPUI binary runs a real-window scenario on Linux and Windows,
using temporary files and real git, without WebDriver or Node.

## Spec
Respects: QA-1 to QA-5, ARCH-1, ARCH-3, ARCH-5 to ARCH-7, D9, D12,
PLAT-1, section 12 End to end smoke; M2-15, M3-15 and M4-12 flows.

## Scope
- An e2e-only scripted driver clicks and types through the rendered window.
  Production builds accept neither the scenario flag nor test directories.
- Fixture repos are generated; git verifies commits and resolved merges.
- Restore `just smoke` and run it on Linux (Xvfb) and Windows in CI.
- Retain diagnostic scripts, logs and element snapshots on failure.
- Tests first: script parsing and feature isolation; real-window smoke.

## Acceptance
Smoke passes on both runners; `just check` passes. CI evidence is recorded
when available. No timing budgets run on hosted runners.
