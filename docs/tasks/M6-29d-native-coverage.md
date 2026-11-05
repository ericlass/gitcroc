# M6-29d Native coverage audit

## Goal
Finish the five native coverage mappings found by `just coverage M6`, so
M6-30 can remove the reference frontend without losing their evidence.

## Spec
Respects: QA-3, ARCH-3, PERF-5, PLAT-1, PLAT-2, SEC-2.

## Scope
- Real native clone into a Unicode path containing shell metacharacters.
- Existing group multi-selection/menu/dialog test at 1024×768 logical
  pixels (the former Windows 125% scaling regression).
- Lazy merged history across two real repositories, past its first page.
- Native images from core bytes and embedded assets; external asset paths
  are refused. This replaces the web frontend's CSP-specific evidence.

## Acceptance
`just coverage M6` has no missing IDs; strengthened/new tests and
`just check` pass. No coverage exemptions or spec edits.
