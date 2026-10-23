# M5-21 The changes pane stays in the window

## Goal
Found in M5-20's verification of `1.0.0-rc.1` (Eric, 2026-10-23): with a wide window, drag the history splitter to the right so the changes pane is narrow, then make the window narrower. The changes pane moves out past the window's right edge, and nothing brings it back. Eric's decision: the changes pane is always in the window. When the window gets smaller, the pane may shrink to its minimum, but it is never pushed out of the window.

## Depends on
M5-20

## Spec
Respects: UI-19, UI-29, section 8 (three resizable panes)

## In scope
- Docked right: when the window narrows, the changes pane shrinks to its minimum width (200 px) first, then the history down to its minimum, then the tree. The window's minimum width (960 px) fits all three at their minimums.
- Docked at the bottom, the same holds across: the tree never takes the room the history and changes column needs for its minimum width.
- Docked at the bottom: when the window gets shorter, the changes pane shrinks to its minimum height (120 px), then the history does.
- The sizes saved in the workspace file stay as the user set them, so making the window larger again brings the panes back to those sizes.
- A splitter dragged or moved with the keyboard while its pane is shrunk starts from the size shown, not the saved one.
- Tests: an end-to-end test for each dock position that shrinks the window after making the changes pane small (and, docked at the bottom, the tree wide), and checks the changes pane is in the window and at least its minimum size.

## Out of scope
- Other layout changes, and the minimum window size.

## Acceptance
- The tests above pass, and `just check` passes.
- A package change: it ships in the next release candidate (M5-20).
