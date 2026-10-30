# M6-19b2 Conflict rows, headers and hunk controls

Second implementation PR of [M6-19b](M6-19b-conflict-presentation.md), under
DEV-4. Depends on [M6-19b1](M6-19b1-conflict-code-colours.md). Read the
parent's sources, design, scope and cited requirements as well.

Implements: OPS-3, OPS-4 (rows, headers and hunk controls)
Respects: OPS-8, OPS-9, ARCH-6, PERF-1, UI-22, UI-26, SEC-3, D15, QA-3, QA-4, QA-5

Paint full-width conflict rows under the editor, including hand-edited
hunks, and headers with the core's labels over the marker lines. Untouched
hunks offer Take ours, Take theirs and Take both, calling M6-19a's
transactions. Place everything from the editor's own geometry in the frame
it scrolls in, clipped to its viewport, laying out only visible hunks plus
overscan. Port F7/Shift+F7 control focus and Escape back to the text.
Investigate the spike's first-at-end frame.

Write first: `ops_4_headers_keep_core_labels_for_merge_and_rebase`,
`ops_3_hunk_buttons_follow_anchors_and_manual_edits`,
`ops_3_overlay_bounds_match_editor_ranges`,
`perf_1_conflict_overlays_only_lay_out_visible_rows`,
`ui_22_hunk_controls_are_keyboard_reachable` and the label part of
`sec_3_conflict_labels_and_proposals_are_literal_text`.

Acceptance: these tests and `just check` pass. Show base and proposal cards
remain M6-19b3.
