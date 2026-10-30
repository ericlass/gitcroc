# M6-19a2 Conflict editor, state and keys

Depends on [M6-19a1](M6-19a1-conflict-buffer-transactions.md). Second
implementation PR of [M6-19a](M6-19a-conflict-buffer-and-state.md), under DEV-4.
Read the parent's sources, scope and cited requirements as well.

Implements: UI-29, OPS-3, OPS-9 (actual editor, transactions and retention only)
Respects: ARCH-6, ARCH-7, ARCH-9, UI-22, D15, QA-2, QA-3, QA-4, QA-5

Build the actual gpui-component editor, position/ghost anchors, atomic Take and
Accept, manual-edit protection, undo/redo and result read-back. Run the recovery
journal in the background, rejecting stale results. Retain actual editor entities by typed workspace/repo IDs and path, expose
file/repo/workspace cleanup, and restore focus after hiding without taking it
from a different focused control. Add wrapped, revealed hunk navigation and
native scoped keyboard bindings. Document the buffer shortcuts. Prepare
chunks in the background; stale results and later edits invalidate requests.

Write all remaining parent tests first, including `ui_22_conflict_navigation_and_take_keys`,
`ui_29_conflict_editor_entity_keeps_cursor_scroll_and_undo` and
`arch_7_conflict_preparation_does_not_block_the_ui` first. Include focused
button activation, real hide/show, identical paths in different targets and
a preparation overtaken by an edit.

Acceptance: all parent M6-19a tests and `just check` pass. This completes
M6-19a only; presentation and app conflict jobs remain M6-19b/c.
