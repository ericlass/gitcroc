# M5-10 Accessibility pass

## Goal
The app meets WCAG 2.2 level A and AA as far as automated checks can tell, in both app themes, and works with a screen reader for the main flows (section 13 M5 "accessibility pass"; README "Accessibility target").

## Depends on
M5-09

## Spec
Respects: UI-6, UI-21, UI-22, UI-23, UI-25, UI-26, PERF-1, D15, section 12 "Front end flows" row

## In scope
- Automated checks: `@axe-core/playwright` (a dev dependency; record its version and license in the pull request) in a Playwright file `a11y.spec.ts`, against the fake core:
  - Views: the main window with a repo, a group and the uncommitted changes selected; the conflict editor with a proposal; Settings with every section; the job list and a result report; the preflight dialog; a destructive confirmation; the command palette; the git setup screen; the empty state with no tab open.
  - Each in the light and the dark theme, with the rule tags `wcag2a`, `wcag2aa`, `wcag21a`, `wcag21aa` and `wcag22aa`.
  - No violations. A rule may be turned off only for one element and with a reason in the test, for example a CodeMirror internal the app cannot change.
- App theme contrast: a Vitest test over `ui/src/theme/tokens.css`, like the code theme test of M1-10: in both themes, text tokens against each background level at least 4.5:1, secondary text at least 4.5:1, disabled text exempt (WCAG), and the focus ring, borders of inputs and the selection against their background at least 3:1 (WCAG 1.4.11).
- Fix what the checks find. Then, at least:
  - Names and roles: every control has an accessible name; the tree is a `tree` with `treeitem`s that carry `aria-level`, `aria-expanded` and `aria-selected`; the history, file lists and palette are listboxes or grids with the selection exposed; icons that carry meaning (dirty dot, conflict, warning, spinner; UI-3) have a text alternative.
  - Focus: every focusable element shows the focus ring (`:focus-visible`); every dialog moves focus into itself, keeps Tab inside, closes on Escape where M2 to M4 allowed it, and returns focus to what opened it. Shared in `dialogs/parts.tsx`.
  - Announcements: a job's end with *failed* or *conflict* is announced through one polite live region, which later notices (M5-12's update notice) use too; progress is not announced on every step.
  - Motion: under `prefers-reduced-motion: reduce`, transitions and animations are off: a progress bar still fills, and a spinner (UI-3) becomes a still busy icon with the same text alternative.
  - Forced colors (Windows high contrast): under `forced-colors: active` the selected row and the focus stay visible, by an outline or border, not only a background.
  - Text size: at the largest UI font size (32 px, M1-10) no control's label is cut off in the toolbar, tab bar and dialogs (checked in Playwright by comparing each label's scroll and client width).
- Tests: the axe file above; the contrast test; Playwright tests for dialog focus (one per dialog part), the live region, reduced motion and forced colors (Playwright's `reducedMotion` and `forcedColors` emulation).
- A screen reader walkthrough on Linux with Orca is part of the sign-off (M5-20), not of this task.

## Out of scope
- Screen readers on Windows and macOS (no machine; README "Windows and macOS proof").
- Translations and right-to-left layout (section 2, LATER).

## Acceptance
- The tests above exist and pass, and `just check` passes.
- The pull request lists each violation found and its fix.
