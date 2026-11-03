# M6-25c The commit message for screen readers

## Goal
A screen reader user can review what they typed in the commit message, not
only type it, so M5-20's Orca walkthrough can commit. Scripts for NVDA and
VoiceOver are ready for the sign-off (README decisions table, "Accessibility
bar in M6", musts 8 and 9).

## Depends on
M6-25a: its harness and AT-SPI probe.

## Spec
Respects: UI-22, D15, ARCH-6, QA-2, QA-3, QA-4, section 12 "Front end flows" row

## In scope
- **Text runs:**
  - **Where:** the commit box's subject and body.
  - **What they expose:** the field's text, the caret and the selection, as
    AccessKit text runs through synthetic children, as GPUI's accessibility
    guide shows.
  - **How:** in a small wrapper around gpui-component's `Input`, not in
    gpui-component itself.
  - **Other text fields:** they keep only their name and value. LATER
    unless the wrapper makes it free.
  - **Cost:** the runs update on edits and caret moves. The work stays
    under 2 ms per keystroke for a 10,000-character body (ARCH-7), measured
    in the test.
- **The manual scripts:**
  - **What:** the spike's NVDA (Windows) and VoiceOver (macOS) scripts,
    rewritten for `gitcroc-next` on a `repo-gen` fixture, with an Orca
    variant for Linux. They're in
    `crates/gitcroc-ui/tests/a11y_tree/manual.md`, each with a table to
    fill in.
  - **The steps:** open the window; move through the history; Tab through
    the panes; the tree; open a dialog and close it; type a commit message
    and read it back; run a job that fails and hear it announced.
  - **When they run:** before release, at M6-31's sign-off. M6-31's task
    file says so.

## Tests (QA-3)
- **`ui_22_the_commit_message_exposes_its_text_and_caret`:** in M6-25a's
  harness, through AT-SPI. Type `ab`, press Left twice, then read the text
  and the caret offset.
- **A `#[gpui::test]` of the wrapper:** its runs match the text and caret
  after typing, deleting, a selection and a paste, and they stay within the
  budget above.

## Out of scope
- **Text runs for every text field, and the conflict editor:** LATER.
- **Running the Windows and macOS scripts:** M6-31.

## Acceptance
- The tests above exist and pass.
- **On the reference machine:** Orca reads back a typed commit message by
  character and by line.
- `just check` passes.
