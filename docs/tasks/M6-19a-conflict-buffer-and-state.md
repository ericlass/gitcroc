# M6-19a Conflict buffer and kept editor state

## Goal
Build the native conflict editor's single editable buffer, with tracked hunks,
atomic take and accept actions, undo, and state that survives being hidden.
This is the first of three M6-19 PRs; M6-19b draws its controls and M6-19c
connects it to the changes pane and conflict jobs.

## Depends on
- M6-02: the accepted single-buffer editor design and measured workarounds.
- M6-07: code themes and fonts.
- M6-16: working-copy changes and job/refresh integration.

## Spec
Implements: OPS-3, OPS-8, OPS-9, UI-29 (buffer and retained state only)
Respects: OPS-4, AI-12, D9, ARCH-1, ARCH-6, ARCH-7, ARCH-9, UI-22, SEC-3, D15, QA-2, QA-3, QA-4, QA-5

Read the cited requirements before implementation. The parenthesized scope is
partial coverage; M6-19 is complete only after all three slices.

## Sources and design
- Port `ui/src/conflict/buffer.ts`, the buffer transactions and navigation in
  `ui/src/conflict/editor.ts`, and the editor retention in
  `ui/src/conflict/ConflictEditor.tsx` and `ui/src/stores/conflicts.ts`.
- Read the throwaway `spike/gpui-conflict` branch at `192aeb8`, especially
  `spikes/gpui-conflict/README.md`, `src/buffer.rs` and `src/editor.rs`.
  Its accepted findings are recorded in the task README. Port the proven
  techniques into `crates/gitcroc-ui`; do not merge the spike or depend on it.
- Use the pinned gpui-component code editor with one buffer per file. Keep
  soft wrap and folding off: M6-19b's overlays require uniform rows.

## In scope
- Construct the initial text from the core's `ConflictChunk`s and actual
  `ConflictLabels`, never by parsing disk markers. Port newline normalization,
  one-line marker labels, no-final-newline handling, and Take both's ordering
  (ours then theirs) from rc.1. Keep original hunk IDs and texts.
- Position anchors follow edits, undo and redo. Cover edits before, inside,
  across and exactly at hunk boundaries, Unicode byte offsets, adjacent hunks,
  empty sides and empty takes. Use the spike's ghost-anchor workaround where
  the editor drops an emptied range; synthetic anchor text must never leak
  into the result or alter user text.
- Free editing anywhere. A manually changed hunk retains its tracked region
  for highlighting but loses its take/accept actions. Undoing back to its
  original text restores those actions. Taking ours, theirs or both replaces
  the hunk as one undo step; undo restores its original text and controls.
- Provide the same atomic replacement path for accepting a supplied proposal
  (AI-12). Accept cannot overwrite a manually edited hunk. Proposal generation
  and its visible controls are later slices.
- Next/previous hunk navigation wraps, reveals the target and moves the caret.
  Port rc.1's conflict shortcuts using the native keymap conventions; document
  them in the existing keymap documentation. Keep ordinary editor input,
  selection, undo and button activation working. Plain Enter/Space on a pane
  root must act only when the list itself has focus.
- Retain the actual editor entity, anchors, undo history, cursor/selection and
  scroll, keyed by workspace, repo and path. Keep this separate from disposable
  diff/history pages. Restore editor focus when returning from a hidden view
  without stealing it from another focused control. Expose unsaved state and
  explicit file/repo/workspace cleanup to M6-19c.
- Read back the whole result without writing a file, staging or starting a
  Git job. Keep expensive buffer preparation off the UI thread. Repo text is
  literal text; externalize new UI strings.

## Tests (QA-3)
Write these tests before production code, using pure model tests where useful
and GPUI tests for the actual editor, transactions, focus and key dispatch:
- `ops_8_buffer_uses_core_chunks_and_real_labels`: misleading marker-like disk
  text cannot create hunks; cover CRLF, Unicode and missing final newlines.
- `ops_3_anchors_follow_edits_and_boundaries`: positions remain correct through
  the edit cases above and do not bind one hunk's actions to another.
- `ops_3_take_ours_theirs_both_is_one_undo_step`: include empty replacements
  at the start/end of the document; undo/redo restores text and action state.
- `ops_3_manual_edit_disables_actions_until_undone`.
- `ai_12_accept_only_changes_the_untouched_buffer`: supplied text uses the
  atomic path, undo restores the hunk, and edited hunks are protected.
- `ui_22_conflict_navigation_and_take_keys`: real key dispatch reveals the
  correct hunk and leaves typing, undo and focused button activation intact.
- `ui_29_conflict_editor_entity_keeps_cursor_scroll_and_undo`: hide/show and
  switch workspace/repo/path keys; identical paths cannot share state.
- `ops_9_buffer_edits_never_write_or_stage`: use a real conflicted fixture,
  compare disk and index before/after take, accept, editing and switching.
- `arch_7_conflict_preparation_does_not_block_the_ui`: verify background work
  and stale preparation results cannot replace a newer editor.

Fixture repos set local identity and `core.autocrlf=false`; do not rely on the
developer's global Git config. Use enlarged test windows when needed.

## Out of scope
- Rows, headers, colours, base/proposal cards: M6-19b.
- File selection, edited-outside question, choice conflicts, Mark resolved,
  Abort/Continue and refresh-driven cleanup: M6-19c. This slice supplies the
  buffer/state API those flows use.
- AI providers, consent, jobs and Resolve all: M6-23; palette: M6-24.
- Spec/ADR changes and dependency upgrades.

## Acceptance
- All named tests pass; a test host can edit, take, undo, hide/show and read
  back a buffer without any repo write.
- `just check` passes. No new claim of full OPS-3/OPS-8/OPS-9 coverage is made
  before M6-19b/c land.
- Follow DEV-4: if the implementation exceeds about 400 changed non-test lines,
  split this slice further before expanding it into a large PR.
