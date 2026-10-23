# M5-27 Settings groups its settings in tabs

## Goal
Eric's request during M5-20's verification of `1.0.0-rc.1` (2026-10-23): the Settings dialog is a long column of sections that has to be scrolled. Group the settings in tabs, one group shown at a time, so each fits without scrolling.

## Depends on
M5-20

## Spec
Respects: UI-20, UI-21, UI-22, UI-23, UI-24, UI-26, AI-3, AI-16. The spec does not set the dialog's layout; the grouping below is this task's.

## In scope
- Four tabs, in this order:
  - **Appearance**: theme, code themes, fonts, window.
  - **Git**: the git in use, group operations.
  - **Updates**: the update check.
  - **AI**: the AI settings.
- The line naming the running gitcroc, the settings' error and **Close** stay below every tab.
- The tabs follow the ARIA tabs pattern: one Tab stop, the arrow keys (wrapping), Home and End move between them, and a tab shows its panel as soon as it has the focus.
- Settings opens at **Appearance**. It opens at **AI** where Resolve with AI asks for a provider (AI-3), and at **Updates** from the palette's Check for updates, with the focus on that tab's first control.
- The dialog keeps one size from tab to tab, the size of the tallest tab, so nothing moves when the tab changes. The light and dark code theme previews sit side by side, and under each other in a narrow window.
- The user guide describes the tabs, and its pointers to a Settings section name the tab.
- Tests:
  - End to end: every tab shows its settings without scrolling, at the same size.
  - End to end: the keys move between the tabs.
  - The accessibility scan of each tab, in both themes.
  - The existing tests open the tab they need.

## Out of scope
- Remembering the last tab between openings.
- Moving or renaming any setting.

## Acceptance
- The tests above pass, and `just check` passes.
- A package change: it ships in the next release candidate (M5-20).
