# M5-25 A repo's name stays whole beside a long branch name

## Goal
Found in M5-20's verification of `1.0.0-rc.1` (Eric, 2026-10-23): when the tree is narrower than a repo row, the repo's name shrinks to nothing and the branch name takes its place, so the row seems to show only the branch. Eric's decision: the other way around. The branch name gives up its room first and ends in an ellipsis; the repo's name is cut only when the branch name is gone and the row is still too narrow.

## Depends on
M5-20

## Spec
Respects: UI-3, UI-4, UI-11

## In scope
- Repo and group rows in the tree: the branch name shrinks first, with an ellipsis; the name, the counts and the dirty and conflict markers keep their room.
- The lines of the summary strip above the history (UI-11) follow the same rule.
- Tests: an end-to-end test with a repo on a branch whose name does not fit, checking the repo's name is whole, the branch name is cut, and the markers after it stay in the row.

## Out of scope
- Showing the whole branch name on hover, and other tree layout changes.

## Acceptance
- The test above passes, and `just check` passes.
- A package change: it ships in the next release candidate (M5-20).
