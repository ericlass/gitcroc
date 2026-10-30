# M6-19a1 Conflict buffer and recovery model

First implementation PR of [M6-19a](M6-19a-conflict-buffer-and-state.md), split
under DEV-4. Read the parent's sources, scope and cited requirements as well.

Implements: OPS-8, OPS-3 (buffer and recovery model only)
Respects: OPS-4, OPS-9, AI-12, ARCH-1, ARCH-6, ARCH-7, SEC-3, D9, D15, QA-2, QA-3, QA-4, QA-5

Port the core-chunk buffer, normalized labels/newlines, original hunk IDs and
Take text. Supply a pure recovery journal for the actual editor's disappearing
empty anchors. Distinguish identical adjacent hunks and preserve Take boundaries
through coalesced typing. The journal consumes text/range snapshots; M6-19a2
runs it off the UI thread and applies the ranges to the actual editor.

Write `ops_8_buffer_uses_core_chunks_and_real_labels` and
`ops_3_recovery_keeps_atomic_snapshots_across_coalesced_typing` first. Write all
other parent tests before implementing their actual editor paths in M6-19a2.

Acceptance: these tests and `just check` pass. No file writes or jobs.
M6-19a2 supplies the actual editor, atomic transactions, retained entities,
preparation requests, navigation, bindings and focus, completing the parent.
