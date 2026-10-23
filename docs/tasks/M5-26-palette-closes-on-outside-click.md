# M5-26 A click outside the command palette closes it

## Goal
Found in M5-20's verification of `1.0.0-rc.1` (Eric, 2026-10-23): the command palette closes only by Escape or by its key, never by a click outside it, unlike Settings and the job list. Eric's decision: a click anywhere outside the palette closes it too. The palette holds nothing to lose but what was typed, so a stray click costs nothing.

## Depends on
M5-20

## Spec
Respects: UI-22, M5-10 (the focus goes back to what had it)

## In scope
- A click on the backdrop around the palette closes it, as Escape does, and the focus goes back to what had it before the palette opened. The click does nothing else: it selects nothing under the backdrop.
- A click inside the palette leaves it open.
- Tests: an end-to-end test that clicks inside the palette, then over the tree outside it.

## Out of scope
- The operation dialogs (branches, sync, tags, reset, remotes): a click outside them could lose what was entered, so they still close only by Escape or Cancel.

## Acceptance
- The test above passes, and `just check` passes.
- A package change: it ships in the next release candidate (M5-20).
