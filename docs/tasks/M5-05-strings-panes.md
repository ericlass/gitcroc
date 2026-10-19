# M5-05 Externalized strings, part 2: history, changes, conflicts, AI and jobs

## Goal
The history and changes panes, the conflict editor, the AI parts and the job list take their text from the strings module (D15).

## Depends on
M5-04

## Spec
Respects: D15, ARCH-6, SEC-3, AI-16, UI-21

## In scope
- Convert, with M5-04's rules: `history/`, `changes/`, `conflict/`, `ai/`, `jobs/`.
- Remove these areas from the test's list of areas not converted yet.
- Text moves verbatim; no wording changes. Existing tests keep passing, importing the strings module where they repeat text.
- Text the core sends (AI-9's "AI failed" reason, "too large", job output) stays as it arrives (README "D15 in the core").

## Out of scope
- The operation dialogs and stores (M5-06).

## Acceptance
- `d15_ui_text_comes_from_the_strings_module` passes with these areas no longer skipped.
- The Vitest and Playwright suites pass unchanged except for imports, and `just check` passes.
- The pull request gives the number of strings moved.
