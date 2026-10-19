# M5-06 Externalized strings, part 3: operation dialogs and stores

## Goal
Every piece of UI text comes from the strings module, and the test that enforces it skips nothing (D15).

## Depends on
M5-05

## Spec
Implements: D15
Respects: ARCH-6, SEC-3, GRP-9, section 2 "translations" (LATER)

## In scope
- Convert, with M5-04's rules: `groups/`, `integrate/`, `clone/`, `remote/`, `reset/`, `stash/`, `sync/`, `tag/`, `stores/` (texts a store builds for the UI, such as dialog titles and confirmation lines), and any file M5-04 and M5-05 left.
- The list of areas not converted yet is removed from the test, so a new component with inline text fails `just check`.
- `CONTRIBUTING.md`: where UI text goes, how a text with values is written, and that the core's text is English built in Rust, one place per type.
- Text moves verbatim; no wording changes.

## Out of scope
- Translations, a language setting, and changing the core's text (README "D15 in the core").

## Acceptance
- `d15_ui_text_comes_from_the_strings_module` passes with nothing skipped, and fails on a component with inline text (shown in the pull request with a throwaway example, not committed).
- `just coverage spec --report-only` no longer lists D15.
- The Vitest and Playwright suites pass unchanged except for imports, and `just check` passes.
