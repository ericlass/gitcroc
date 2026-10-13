# M2-10 Conflict editor

## Goal
In conflict state the changes pane lists conflicted files first and opens each in an editor where every conflict hunk is highlighted with take ours, take theirs and take both, the text is freely editable, and Mark resolved stages it; unsaved work survives switching files, repos and tabs until Abort.

## Depends on
M2-04, M2-09

## Spec
Implements: OPS-3, OPS-9
Respects: OPS-2, OPS-4, OPS-5, OPS-6, OPS-8, ARCH-6, ARCH-9, UI-24, UI-25, UI-29, PERF-1, SEC-3, SEC-2

## In scope
- Changes pane in conflict state: the Unmerged list comes first (M1-09), each file with its conflict kind and a mark once it has unsaved edits. Selecting one opens the conflict editor in place of the diff.
- Text conflict (OPS-3): a CodeMirror editor holding the result text from `conflict_get`. Every conflict hunk is highlighted with the code theme's conflict colors (UI-24, UI-25) and carries three buttons: **Take ours**, **Take theirs**, **Take both** (ours then theirs). The headers of each hunk name the sides with the labels of OPS-4. The text is editable anywhere, including inside a hunk; a hunk whose text the user changed keeps its highlight but loses its buttons. Next and previous conflict with the keyboard.
- Base: each hunk can show its base text (a toggle), since the core sends diff3 hunks.
- Choice conflict (OPS-6): two cards, one per side, with the side's label, what it holds (deleted, size, mode, submodule commit) and **Use this version** (`conflict_choose`).
- Edited outside (OPS-8): when `edited_outside` is set, the editor first asks: keep the file as it is (the editor opens the disk text as plain editable text, without hunk buttons) or start over from the conflict.
- **Mark resolved** (OPS-5) sends the buffer to `conflict_resolve`. When the core refuses because markers remain, the message names the first marker line and offers **Mark resolved anyway**. After success the file leaves the Unmerged list and the banner's Continue updates (OPS-2).
- Unsaved state (OPS-9): the buffer and its hunk state are kept per workspace, repo and file (ARCH-9) in a store, while the app runs. Switching files, repos or tabs and back restores the buffer, cursor and scroll. The state of a repo is dropped when its Abort succeeds, or when its conflict state ends by other means (a terminal `--abort`). Nothing is written to the file before Mark resolved.
- Fake core: a conflict of each kind, labels per operation.
- Tests:
  - `OPS-3 conflict hunks with take ours, theirs and both` (Vitest): each button replaces the hunk with the right text; the text stays editable.
  - `OPS-3 conflicted files listed first` (Vitest).
  - `OPS-9 unsaved edits survive switching files and repos` and `OPS-9 unsaved edits are dropped on abort` (Vitest).
  - `OPS-9 nothing is written before Mark resolved` (Rust, with M2-09's core): opening, editing and switching send no write.
  - `OPS-5 mark resolved anyway` (Vitest): the refusal shows the marker line and the override.
  - `OPS-6 choice conflict shows both versions` (Vitest).
  - `OPS-8 edited outside asks first` (Vitest).
  - Playwright, fake core: resolve two files of a merge with the keyboard only, mark them resolved, and Continue becomes enabled.

## Out of scope
- Resolve with AI (section 7; M4); the editor leaves room for proposals per hunk but has no AI control.
- Per-tab state across tabs is tested again with tabs in M3 (UI-29).

## Acceptance
- `just dev`: merge a branch with two conflicted files; resolve one with Take theirs and one by hand; switch to another repo and back and the edits are there; Mark resolved both; Continue finishes the merge. Then repeat and Abort: the edits are gone.
- All tests listed above exist and pass, and `just check` passes.
