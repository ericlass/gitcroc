# M1-10 Code themes and fonts

## Goal
The user picks a code theme for light mode and one for dark mode, and the UI and code fonts with their sizes; diffs follow them at once.

## Depends on
M1-09

## Spec
Implements: UI-24, UI-25, UI-26
Respects: UI-23, DATA-1, ARCH-6, section 10 "Files" table

## In scope
- Eight code themes (UI-24): GitHub Light, GitHub Dark, Solarized Light, Solarized Dark, One Dark, Dracula, Nord, Monokai. Each is a CodeMirror theme plus syntax highlight style, and defines the added, removed, changed and conflict colors for line and word highlights. Theme definitions taken from packages must have a license compatible with MIT (D14); record their sources in the pull request. Code themes are defined in one place in `ui/src/theme/`, which the UI-23 "no hardcoded colors" test is updated to allow, and nowhere else.
- Settings in `settings.toml` (DATA-1 rules; bump the schema and add the migration from schema 1): `code_theme_light` (default GitHub Light), `code_theme_dark` (default GitHub Dark), `ui_font`, `ui_font_size`, `code_font`, `code_font_size`. Fonts default to the system UI font and the system monospace font (UI-26); sizes default to 13 px and are limited to 8 to 32.
- The code theme in use follows the app theme: the light code theme while the app theme resolves to light, the dark one otherwise, switching live with it.
- The Settings dialog gains: code theme for light mode, code theme for dark mode, each with a small diff preview; UI font, code font (free text with the system default as placeholder) and their sizes.
- Fonts apply through CSS variables, so every component follows without a reload.
- Tests:
  - `ui_24_eight_code_themes_ship`: the theme registry contains exactly the eight names, each with light or dark set correctly.
  - `ui_25_diff_colors_meet_contrast`: for every theme, the contrast ratio of the text color against each of the added, removed and conflict backgrounds (line and word level) is at least 4.5:1, computed with the WCAG formula. A theme that fails gets its diff colors adjusted, not the test.
  - `UI-24 code theme follows the app theme` (Vitest): switching the app theme switches the code theme between the two choices.
  - `UI-26 fonts apply` (Vitest): changing the code font sets the CSS variable the diff uses.
  - Settings migration from schema 1 keeps the theme and adds the new defaults (Rust).

## Out of scope
- Custom theme import (section 2, LATER).
- The conflict editor (M2); it uses the same conflict colors defined here.

## Acceptance
- `just dev`: changing either code theme or a font changes the open diff at once, and the choices survive a restart.
- All tests listed above exist and pass on all three platforms.
