# M6-07 App tokens, accent colour, fonts and code themes

## Goal
`gitcroc-ui` looks like 1.0.0-rc.1: today's tokens are ported to Rust for light and dark, the app theme follows the setting or the system live, and the OS accent colour is used where a safe API reports it. The UI and code fonts come from the settings, or from the desktop when the setting is empty. The 8 code themes are ported as Rust data with syntect themes built from them. The contrast and colour-literal tests port with them.

## Depends on
- M6-06: the crate, the window, and the D15 scan this task's colour scan is modelled on.

## Spec
Implements: UI-23, UI-24, UI-25, UI-26
Respects: ARCH-6, ARCH-7, D15, QA-3, QA-4, PLAT-1

## In scope
- **Tokens (UI-23)** in `crates/gitcroc-ui/src/theme/tokens.rs`:
  - A `Tokens` struct with every colour of `ui/src/theme/tokens.css`, with the same values and names in snake case:
    - backgrounds, text, borders, accent, danger, warning, success;
    - the focus ring and the selection;
    - the backdrop;
    - the 8 graph colours;
    - the 5 outcome colours.
  - `Tokens::LIGHT` and `Tokens::DARK`, as opaque `0xrrggbb` values (the backdrop with its alpha), so the tests compute the contrast that is on screen.
  - The sizes that aren't colours (radius, splitter width) are constants beside them.
  - `tokens.rs` and `code_themes.rs` are the only files in `src` that hold colour values.
- **The app theme** in `theme/mod.rs`:
  - An `AppTheme` global holds:
    - the theme setting;
    - the resolved mode;
    - the tokens in use, with the accent applied;
    - the code theme in use;
    - the fonts.
  - `Theme::System` follows the window's appearance live, through `observe_window_appearance`. `Light` and `Dark` ignore the system.
  - The tokens are mapped onto gpui-component's theme, so its widgets follow them. A `ThemeConfig` is built for light and for dark, and `Theme::change` applies it. gpui-component's default theme is no longer used.
  - Views read `cx.tokens()`, an extension trait over the global, never a colour of their own. The M6-06 views move to it.
  - Settings reach the theme from `settings_get` at start. Later, they arrive from the `SettingsView` that M6-26's dialog gets back after a change, through one `AppTheme::apply_settings`.
- **The accent colour (UI-23):**
  - **Windows:** `UISettings::GetColorValue(UIColorType::Accent)` from the `windows` crate (0.62, the version GPUI uses, feature `UI_ViewManagement`). Changes arrive through `ColorValuesChanged`.
  - **Linux:** the appearance portal's `accent-color`, through ashpd (0.13, the version and features GPUI uses), with its change stream. Without a portal, or when it reports no accent, the token accent is used.
  - **macOS:** the token accent, because the API needs `unsafe` (QA-4).
  - **Contrast:** the system accent keeps its hue and gets lighter or darker until every pair that uses the accent passes, in the theme in use:
    - the accent as text on each background and on the selection;
    - `accent_text` on the accent;
    - the focus ring and the selection mark.

    If no lightness passes, the token accent is used. The focus ring and the selection mark take the adjusted accent too.
  - A change arrives on a channel, and a main-thread task applies it to the theme.
- **Fonts (UI-26):**
  - **Defaults:** with no font setting, the UI font is the desktop's UI font and the code font is the desktop's monospace font.
  - **Reading the desktop's fonts:** `PlatformFonts` in `gitcroc-app` reads them as follows.
    - **Linux:** through the settings portal (ashpd), within 500 ms:
      - GNOME's `org.gnome.desktop.interface` `font-name` and `monospace-font-name`, read as their family (the `font_family` parser from rc.1, back with its test);
      - otherwise KDE's `org.kde.kdeglobals.General` `font` and `fixed`, whose family ends at the first comma.
    - **Windows and macOS:** no names. GPUI's `.SystemUIFont` is already the system UI font there. The monospace default is gpui-component's platform default and its installed fallback.
  - **Sizes:** the UI and code sizes from the settings set gpui-component's `font_size` and `mono_font_size`.
  - **Missing families:**
    - A family from the settings or the desktop that isn't in `all_font_names()` falls back to the default, with a warning in the log. A missing family never reaches GPUI's panic.
    - **No font at all:** when `all_font_names()` is empty at startup, `gitcroc-next` writes a clear message to stderr and the log, and exits with code 1 before it opens a window (M6-03 finding). The Linux packages depend on a font package (M6-28).
- **Code themes (UI-24, UI-25)** in `theme/code_themes.rs`:
  - The 8 themes of `ui/src/theme/codeThemes.ts`, ported as Rust data with the same values:
    - background, text, gutter, selection, hunk;
    - added, removed, changed and conflict, each at line and word level;
    - the 7 syntax colours.
  - The theme in use is the light-mode choice while the app is light and the dark-mode choice otherwise.
  - **The syntect theme:** a `syntect::highlighting::Theme` is built in code from each theme's syntax colours, with scope selectors for keyword, string, comment, number, type, function and tag.
    - syntect 5.3 with the features gpui-component uses (`parsing`, `default-syntaxes`, `regex-fancy`; no default features), so no C regex library comes in.
    - Highlighting the diff and the conflict editor's buffer with it are M6-13's and M6-19's.
- **The colour scan (UI-23)** is a syn scan like D15's. It fails on a colour value in `crates/gitcroc-ui/src` outside `tokens.rs`, `code_themes.rs` and the tests:
  - calls to gpui's colour constructors and named colours (`rgb`, `rgba`, `hsla`, `black`, `white`, `red`, `green`, `blue`, `yellow`, and the like);
  - `Hsla { .. }` and `Rgba { .. }` literals;
  - `#rrggbb`-shaped strings.

## Tests (QA-3)
- `ui_23_the_formula_is_wcags`.
- `ui_23_app_theme_colors_meet_wcag_contrast`, for light and dark: the pairs of `tokens.test.ts`.
- `ui_23_the_system_accent_keeps_contrast`:
  - an accent that passes is kept;
  - one that fails is adjusted, with its hue kept, until every pair passes;
  - one that can't pass gives the token accent.
- `ui_23_theme_setting_light_dark_and_system`:
  - `Dark` and `Light` set the mode whatever the window's appearance;
  - `System` follows a change of the window's appearance.
- `ui_23_saved_theme_applies_on_start`.
- `ui_23_no_hardcoded_colors`, and `ui_23_the_color_scan_catches_every_kind_of_literal` on inline sources.
- `ui_24_eight_code_themes_ship`.
- `ui_24_code_theme_follows_the_app_theme`.
- `ui_24_syntect_theme_colours_a_keyword`: highlighting `fn main() {}` as Rust gives `fn` the theme's keyword colour, for every theme.
- `ui_25_diff_colors_meet_contrast`.
- `ui_25_syntax_colors_meet_contrast`.
- `ui_26_fonts_apply`: the settings' families and sizes reach gpui-component's theme.
- `ui_26_the_desktop_fonts_are_the_defaults`.
- `ui_26_a_missing_font_falls_back_to_the_default`.
- `ui_26_without_any_font_the_app_exits_with_a_message`.
- In `gitcroc-app`, on Linux: `ui_26_the_desktop_font_is_read_as_its_family`, for the GNOME and KDE formats.

## Out of scope
- The settings dialog, its previews and applying `system_title_bar` at once (M6-26).
- Highlighting in the diff (M6-13) and the conflict editor (M6-19).
- The packages' font dependency (M6-28).

## Acceptance
- On the reference machine (Linux, GNOME), `just dev-next` shows rc.1's colours in light and dark. It follows a system switch live, and it uses the desktop's accent colour and UI font.
- With `"theme": "dark"` in the settings file, the app starts dark on a light desktop.
- `just coverage M6` no longer lists UI-23 to UI-26.
- `just check` passes.
