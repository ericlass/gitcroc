# M6-19b Conflict rows, controls and proposal cards

## Goal
Render the kept conflict buffer with labelled, keyboard-reachable hunk
controls, syntax and conflict colours, and base/proposal cards beside the hunk.
This is the second M6-19 PR; the real application flow lands in M6-19c.

## Implementation split (DEV-4)
- [M6-19b1: code colours and syntax](M6-19b1-conflict-code-colours.md).
- [M6-19b2: rows, headers and hunk controls](M6-19b2-conflict-rows-and-controls.md).
- [M6-19b3: base and proposal cards](M6-19b3-conflict-cards.md).

All three are required to complete this parent slice; its acceptance remains unchanged.

## Depends on
- M6-19a: tracked hunks, atomic replacement, navigation and retained entities.
- M6-07 and M6-13b: code themes, fonts and syntax highlighting.
- M6-02: accepted overlay/card design and remaining spike checks.

## Spec
Implements: OPS-3, OPS-4, UI-24, UI-25, AI-11, AI-12 (editor presentation and supplied proposals)
Respects: OPS-8, OPS-9, D9, AI-16, ARCH-6, ARCH-7, ARCH-9, PERF-1, UI-22, UI-26, UI-29, SEC-3, D15, QA-2, QA-3, QA-4, QA-5

Read the cited requirements and section 9's budgets before implementation.
AI-11/AI-12 here cover review of a supplied core proposal; live proposal
retrieval, provider configuration and AI jobs remain M6-23.

## Sources and design
- Port the decorations, controls and proposal review in
  `ui/src/conflict/editor.ts` and `ui/src/conflict/ConflictEditor.tsx`, with
  their strings from `ui/src/strings/en.ts`. Read `conflict.test.tsx` and
  `ai.test.tsx` for behavior, replacing web tests with native tests.
- Apply the accepted M6-02 design: one gpui-component code editor, transparent
  editor background over full-width conflict rows, headers over marker lines,
  and base/proposal cards beside the hunk. Read the spike's `src/editor.rs`
  and README at `192aeb8`; keep soft wrap and folding off.

## In scope
- Full-width conflict highlights, including manually edited hunks, with
  syntax highlighting using the selected code theme and language. Reuse native
  theme/font infrastructure. App-theme and code-theme changes recolour the
  editor independently; unknown languages remain readable plain text.
- Side headers use the core's actual branch/commit labels, including the
  meaning reversal during rebase. Untouched hunks offer Take ours, Take theirs,
  Take both and Show base. Controls call M6-19a's transactions and move with
  anchors, never by reparsing markers.
- Keep row/header positions in the same frame as the editor scroll. Clip
  overlays to the editor viewport and lay out only visible rows/controls plus
  small overscan. Compare placement against `EditorState::range_to_bounds`;
  cover font/size changes, scrolling, resizing and both pane dock positions.
- Show base in a card beside the hunk. Proposal cards accept supplied typed
  core proposal data and show proposed text, explanation and confidence, with
  Accept, Reject and Edit. Keep edited proposal text and rejection state per
  file/hunk while switching views. Unresolved proposals show their explanation
  without an Accept action. Changed hunks cannot be overwritten by a proposal.
- Accept all high-confidence applies only to eligible untouched hunks in the
  current file, in one undoable transaction. Accept and Edit never write,
  stage or continue. No action accepts proposals across files.
- Keep the card usable at narrow pane widths and long lines; test hit targets,
  clipping and the editor's caret visibility. Give all controls accessible
  names. Port F7/Shift+F7 hunk-control focus navigation, including returning
  to the text, without consuming Enter/Space intended for focused buttons.
- No AI request is made by this slice. Cards are absent without proposal
  data and when AI features are off; clearing supplied proposals clears their
  review state. M6-23 connects the feature setting and job events in the app.
- Investigate the spike's 50–55 ms first frame at the document end. Record a
  reproduction and the cause/fix if found; if it remains upstream, record the
  evidence and report its measured effect rather than claiming it solved.

## Tests (QA-3)
Write these before production code; use GPUI assertions on rendered controls,
editor state and layout, with supplied proposals and no real AI provider:
- `ops_4_headers_keep_core_labels_for_merge_and_rebase`.
- `ops_3_hunk_buttons_follow_anchors_and_manual_edits`.
- `ops_3_overlay_bounds_match_editor_ranges`: first/middle/last visible hunk,
  scrolling, changed font, resize, right/bottom dock and clipped controls.
- `ui_24_conflict_syntax_and_theme_changes`: known and unknown syntax; code
  theme changes update syntax and rows without replacing the buffer.
- `ui_25_conflict_text_contrast_meets_four_point_five`: all shipped code themes.
- `perf_1_conflict_overlays_only_lay_out_visible_rows` on 10,000 lines/50 hunks.
- `ai_11_cards_show_base_and_proposal_review`: confidence/explanation, Reject,
  Edit retention, unresolved proposal and keyboard controls.
- `ai_12_accept_and_accept_high_only_edit_this_file`: undo/redo, edited hunk
  protection, low/medium confidence excluded from bulk acceptance, no disk or
  index write, and no changes to another file's buffer.
- `ai_16_absent_or_disabled_proposals_have_no_controls`.
- `ui_22_hunk_controls_are_keyboard_reachable`: focus entry, traversal, button
  activation and return to text; editor shortcuts do not steal button keys.
- `sec_3_conflict_labels_and_proposals_are_literal_text`.
- Existing strings/theme/keymap checks cover the new files.

## Out of scope
- Core conflict parsing/policy and Git services: reuse existing implementations.
- Mark resolved, choice conflicts, banner and real file switching: M6-19c.
- Live AI calls, provider settings, consent, Resolve with AI, token usage and
  Resolve all: M6-23. Do not claim that milestone complete from fixture cards.
- Palette registration: M6-24; full accessibility sign-off: M6-25.
- Spec/ADR changes and dependency upgrades.

## Acceptance
- Named tests and `just check` pass. Supplied proposal review is exercised in
  the test host and is ready for M6-23's wiring.
- On the reference machine, repeat the spike's 10,000-line/50-hunk scripted
  scroll, typing and open-to-first-paint measurements; record frame intervals,
  frames over 1.5 refresh intervals, CPU per frame, typing latency and the
  first-at-end frame. Assess section 9's applicable frame/input budgets;
  hosted CI timing is not a substitute.
- Verify composition with a composing Japanese or Chinese input method on
  Windows, macOS, X11 and Wayland. Record any unavailable platform check as
  outstanding; the spike did not verify IME composition.
- Visual smoke checks light/dark code themes, narrow/right and bottom docks,
  cards, scrolling and focus. Report unavailable reference-machine checks
  explicitly. Follow DEV-4 and split further if needed.
