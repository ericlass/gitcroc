# M6-19b3 Conflict base and proposal cards

Third implementation PR of [M6-19b](M6-19b-conflict-presentation.md), under
DEV-4. Depends on [M6-19b2](M6-19b2-conflict-rows-and-controls.md). Read the
parent's sources, design, scope and cited requirements as well.

Implements: AI-11, AI-12 (Show base, and review of a supplied core proposal)
Respects: OPS-9, D9, AI-16, ARCH-6, PERF-1, UI-22, SEC-3, D15, QA-3, QA-4, QA-5

Add Show base, and a card beside the hunk with the base and any supplied
proposal: proposed text, explanation and confidence, with Accept, Reject and
Edit. Keep edited text and rejection per file and hunk while views switch.
Unresolved proposals show their explanation without Accept; changed hunks
cannot be overwritten. Accept all high-confidence applies eligible untouched
hunks of this file in one undoable transaction. Cards are absent without
proposal data or with AI features off; clearing proposals clears their
review state. Keep cards usable at narrow widths and long lines without
covering the caret. No AI request is made.

Write first: `ai_11_cards_show_base_and_proposal_review`,
`ai_12_accept_and_accept_high_only_edit_this_file`,
`ai_16_absent_or_disabled_proposals_have_no_controls`, the Show base part of
`ui_22_hunk_controls_are_keyboard_reachable` and the base and proposal part
of `sec_3_conflict_labels_and_proposals_are_literal_text`.

Acceptance: all parent tests and `just check` pass, completing M6-19b in the
test host. M6-23 wires live proposals and the AI features setting.
