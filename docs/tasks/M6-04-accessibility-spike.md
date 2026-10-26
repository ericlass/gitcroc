# M6-04 Spike: accessibility in GPUI

## Goal
We know what GPUI's AccessKit support gives screen readers on each platform, and what a test can check. M6-01's follow-up decision and M6-25 can then set an accessibility bar that can be met and proven.

This is a spike in the sense of section 14. It ends with findings, not with production code.

## Depends on
None. It can run beside M6-01, M6-02 and M6-03.

## Spec
Respects: UI-22, UI-35, UI-25, QA-3, the accessibility pass of M5-10 (its task file lists what the Tauri app meets today)

## In scope
- **`spikes/gpui-a11y/`.** A standalone crate like `spikes/gpui-history/`, outside the workspace. It has one window with the kinds of element the app uses:
  - a labelled icon button, and a window button that is out of the tab order (UI-35);
  - a text input with a label;
  - a checkbox and a select;
  - a `uniform_list` of 1,000 rows with a selected row;
  - a tree row that expands;
  - a modal dialog;
  - a status message that should be announced, like the Tauri app's live region.
- Roles, labels and states set through GPUI's `role` and `aria_label` and gpui-component's own accessibility support.
- **By hand, with the screen reader of each platform:**
  - Orca on Linux, with both X11 and Wayland;
  - NVDA on Windows;
  - VoiceOver on macOS.

  For each element record what is read and whether it can be reached and used from the keyboard. Record whether focus moves into the dialog and back. Record whether the list announces the selected row and its position. Record whether the status message is announced.
- **In a test.** Find out whether `#[gpui::test]` or the AccessKit tree update can be read in a test, so that a test can assert "every interactive element has a role and a name". If it can, write that test for the spike's window.
- **Gap list.** Compare with what M5-10 made the Tauri app meet: axe checks in both themes, app theme contrast, visible focus, the live region, reduced motion. For each, give how GPUI meets it, or "gap" with an estimate of the work to close it.
  - Contrast tests don't depend on the toolkit; they port as they are.
  - Reduced motion: does GPUI report the system setting?
- **Findings in `spikes/gpui-a11y/README.md`.** Write the per-platform table, the gap list, and a proposed bar for M6-25 that Eric decides on. If that bar is lower than M5-10's, it needs a spec decision before M6-25 starts (DEV-2).

## Out of scope
- Making any gitcroc view accessible.
- Upstream changes to GPUI or gpui-component. The README may suggest them.

## Acceptance
- The README has the screen reader table for all three platforms. A platform that couldn't be tested by hand says so, with what would be needed.
- It has the gap list against M5-10.
- It says whether an accessibility test is possible, and has that test if so.
- It proposes a bar for M6-25.
- `just check` still passes, because nothing in the workspace changed.
