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

## Findings: the spike's first frame at the end
- Reproduction: `perf_1_conflict_first_frame_at_the_end_is_not_slow`
  (ignored; `just timing`) scrolls the 10,000-line, 50-hunk buffer 100 px a
  frame in a 1920×1003 window, as the spike did, with this slice's rows,
  headers and controls.
- On the GPUI test platform (release build, 2026-10-30), the first step that
  shows the last line took 0.85 ms against a 2.10 ms median per step (two
  frames). The editor's and the overlays' layout show no slow frame at the end.
- The test platform has no GPU and does no real text shaping, so this does
  not reproduce or rule out the spike's 50–55 ms frame; the cause is not
  found. It remains to be measured on the reference machine once M6-19c opens
  the editor in the app.
