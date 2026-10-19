# M5-04 Externalized strings, part 1: the module and the shell

## Goal
The UI's text starts coming from one strings module, and a test fails on any UI text written inline in a component of a converted area. This part converts the window shell, Settings, the tree and the workspace tabs (D15).

## Depends on
M4

## Spec
Respects: D15, ARCH-6, SEC-3, UI-20, UI-23, UI-28, section 2 "translations" (LATER)

## In scope
- `ui/src/strings/en.ts`: the English text of the UI, as one exported `const` object grouped by area (`tree`, `tabs`, `settings`, …). A text with values is a function of them (`(n: number) => string`), which also holds the plural rule. Components import it; there is no runtime language switch and no translation library (section 2: translations are LATER).
- What counts as UI text: JSX text with a letter in it, and string literals that reach the user through `title`, `aria-label`, `aria-description`, `placeholder`, `alt`, `label` and the dialog parts' text props (`dialogs/parts.tsx`). Text that is data is not UI text: names, paths, refs, git output and every string the core sends (see "Core text" below).
- The test `d15_ui_text_comes_from_the_strings_module` (Vitest), using the TypeScript compiler API, which the project already has:
  - It walks every `.tsx` and `.ts` file under `ui/src/` except tests, fakes, generated bindings and the strings module, and reports each piece of UI text with its file and line.
  - Allowed: text without letters (`·`, `…`, `/`), the product name `gitcroc`, and the entries of a short allow list in the test, each with a reason (for example key names shown as keys).
  - A list of areas not converted yet (`ui/src/history/`, …) is skipped. This task starts it with every area that M5-05 and M5-06 convert; they shrink it to empty.
- Convert in this task: `App.tsx`, `index.tsx`, `layout/`, `tree/`, `workspaces/`, `theme/`, `SettingsDialog.tsx`, `SettingsBanner.tsx`, `GitSetup.tsx`, `AskpassDialog.tsx`, `dialogs/`.
- Text moves verbatim: no wording changes in this task. Existing tests that find elements by their text keep passing; they may import the strings module instead of repeating the text.
- Core text: what the core sends (error messages, skip and block reasons, git's output, AI reasons) is shown as it arrives. It is English text built in Rust, one place per type (`Display` of the error and reason types); this task does not change it (README "D15 in the core").

## Out of scope
- The other UI areas (M5-05, M5-06).
- Any wording change, translation, or language setting.

## Acceptance
- `d15_ui_text_comes_from_the_strings_module` passes, with only M5-05's and M5-06's areas skipped.
- The UI looks and reads exactly as before: the Vitest and Playwright suites pass unchanged except for imports, and `just check` passes.
- The pull request gives the number of strings moved.
