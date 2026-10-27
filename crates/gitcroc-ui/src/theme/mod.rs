//! The app theme (UI-23), the code theme (UI-24) and the fonts (UI-26).
//!
//! [`AppTheme`] is a global with the settings, the system's appearance and
//! accent, and the desktop's fonts. From them it resolves the tokens, the
//! code theme and the font families in use, and maps them onto
//! gpui-component's theme, so its widgets draw with gitcroc's colours.
//! Views read [`ActiveTokens::tokens`]; no view holds a colour of its own.

pub mod accent;
pub mod code_themes;
pub mod contrast;
pub mod fonts;
pub mod tokens;

use std::rc::Rc;

use gitcroc_core::{Appearance, Settings, Theme as ThemeSetting};
use gpui_component::{Theme, ThemeConfig, ThemeMode};
use gpui_kit::{App, Global, SharedString, Window, WindowAppearance};

use self::code_themes::{CodeThemeDef, code_theme};
use self::tokens::{Color, Tokens};

/// The theme in use and what it is resolved from.
pub struct AppTheme {
    setting: ThemeSetting,
    appearance: Appearance,
    desktop_ui_font: Option<String>,
    desktop_code_font: Option<String>,
    system_dark: bool,
    system_accent: Option<Color>,
    /// The text system's families, read once: listing them is slow.
    installed: Vec<String>,
    /// gpui-component's families before any setting: the system UI font
    /// and an installed monospace font.
    default_ui_font: SharedString,
    default_code_font: SharedString,
    /// Resolved by [`Self::apply`].
    tokens: Tokens,
    ui_font: SharedString,
    code_font: SharedString,
    missing_fonts: Vec<String>,
}

impl Global for AppTheme {}

impl AppTheme {
    /// Sets the theme up with the default settings, following a light
    /// system, until the window gives the settings and its appearance.
    pub fn init(cx: &mut App) {
        let theme = Theme::global(cx);
        let (default_ui_font, default_code_font) =
            (theme.font_family.clone(), theme.mono_font_family.clone());
        let installed = cx.text_system().all_font_names();
        cx.set_global(Self {
            setting: ThemeSetting::default(),
            appearance: Appearance::default(),
            desktop_ui_font: None,
            desktop_code_font: None,
            system_dark: false,
            system_accent: None,
            installed,
            ui_font: default_ui_font.clone(),
            code_font: default_code_font.clone(),
            default_ui_font,
            default_code_font,
            tokens: Tokens::LIGHT,
            missing_fonts: Vec::new(),
        });
        Self::apply(cx);
    }

    pub fn global(cx: &App) -> &Self {
        cx.global::<Self>()
    }

    /// Whether the app is dark: the setting, or the system's appearance
    /// while the setting is `System`.
    pub fn dark(&self) -> bool {
        match self.setting {
            ThemeSetting::System => self.system_dark,
            ThemeSetting::Light => false,
            ThemeSetting::Dark => true,
        }
    }

    pub fn tokens(&self) -> &Tokens {
        &self.tokens
    }

    /// The code theme for the app theme in use: the light-mode choice while
    /// the app is light, the dark-mode one otherwise.
    pub fn code_theme(&self) -> &'static CodeThemeDef {
        code_theme(if self.dark() {
            self.appearance.code_theme_dark
        } else {
            self.appearance.code_theme_light
        })
    }

    pub fn ui_font(&self) -> &SharedString {
        &self.ui_font
    }

    pub fn code_font(&self) -> &SharedString {
        &self.code_font
    }

    /// The text system's font families.
    pub fn installed_fonts(&self) -> &[String] {
        &self.installed
    }

    /// The families of the settings or the desktop that are not installed,
    /// so the defaults are used instead.
    pub fn missing_fonts(&self) -> &[String] {
        &self.missing_fonts
    }

    /// Applies the theme, code themes and fonts of `settings`: at start,
    /// and after each change in the Settings dialog (M6-26).
    pub fn apply_settings(settings: &Settings, cx: &mut App) {
        let this = cx.global_mut::<Self>();
        this.setting = settings.theme;
        this.appearance = settings.appearance.clone();
        Self::apply(cx);
    }

    /// The desktop's UI and monospace families, the defaults of an empty
    /// font setting.
    pub fn set_desktop_fonts(ui: Option<String>, code: Option<String>, cx: &mut App) {
        let this = cx.global_mut::<Self>();
        this.desktop_ui_font = ui;
        this.desktop_code_font = code;
        Self::apply(cx);
    }

    /// Follows `window`'s light or dark appearance.
    pub fn sync_window_appearance(window: &Window, cx: &mut App) {
        let dark = matches!(
            window.appearance(),
            WindowAppearance::Dark | WindowAppearance::VibrantDark
        );
        Self::set_system_dark(dark, cx);
    }

    /// Whether the system is dark, which the app follows while the setting
    /// is `System`.
    pub fn set_system_dark(dark: bool, cx: &mut App) {
        if cx.global::<Self>().system_dark != dark {
            cx.global_mut::<Self>().system_dark = dark;
            Self::apply(cx);
        }
    }

    /// The system's accent colour, or `None` for the tokens' own.
    pub fn set_system_accent(accent: Option<Color>, cx: &mut App) {
        if cx.global::<Self>().system_accent != accent {
            cx.global_mut::<Self>().system_accent = accent;
            Self::apply(cx);
        }
    }

    /// Resolves the tokens and fonts, and applies them to gpui-component's
    /// theme in the mode in use.
    fn apply(cx: &mut App) {
        let this = cx.global_mut::<Self>();
        let resolve = |tokens: Tokens, accent: Option<Color>| match accent {
            Some(accent) => tokens.with_accent(contrast::accent_for(accent, &tokens)),
            None => tokens,
        };
        let light = resolve(Tokens::LIGHT, this.system_accent);
        let dark = resolve(Tokens::DARK, this.system_accent);
        let ui = fonts::family(
            this.appearance.ui_font.as_deref(),
            this.desktop_ui_font.as_deref(),
            &this.installed,
            &this.default_ui_font,
        );
        let code = fonts::family(
            this.appearance.code_font.as_deref(),
            this.desktop_code_font.as_deref(),
            &this.installed,
            &this.default_code_font,
        );
        let is_dark = this.dark();
        this.tokens = if is_dark { dark } else { light };
        this.ui_font = ui.name;
        this.code_font = code.name;
        this.missing_fonts = ui.missing.into_iter().chain(code.missing).collect();
        let fonts = Fonts {
            ui: this.ui_font.clone(),
            ui_size: this.appearance.ui_font_size,
            code: this.code_font.clone(),
            code_size: this.appearance.code_font_size,
        };
        let light = Rc::new(config(&light, ThemeMode::Light, &fonts));
        let dark = Rc::new(config(&dark, ThemeMode::Dark, &fonts));
        Theme::update(cx, |theme| {
            theme.light_theme = light;
            theme.dark_theme = dark;
        });
        Theme::change(
            if is_dark {
                ThemeMode::Dark
            } else {
                ThemeMode::Light
            },
            None,
            cx,
        );
    }
}

/// The fonts a theme config carries.
struct Fonts {
    ui: SharedString,
    ui_size: u32,
    code: SharedString,
    code_size: u32,
}

/// gpui-component's theme for `tokens`. Its colours that gitcroc has no
/// token for are derived by gpui-component from these.
fn config(tokens: &Tokens, mode: ThemeMode, fonts: &Fonts) -> ThemeConfig {
    let hex = |color: Color| serde_json::Value::String(color.hex());
    let colors = serde_json::json!({
        "background": hex(tokens.bg_1),
        "foreground": hex(tokens.text_primary),
        "caret": hex(tokens.text_primary),
        "border": hex(tokens.border),
        "input.border": hex(tokens.border_input),
        "ring": hex(tokens.focus_ring),
        "link": hex(tokens.accent),
        "primary.background": hex(tokens.accent),
        "primary.foreground": hex(tokens.accent_text),
        "secondary.background": hex(tokens.bg_2),
        "secondary.foreground": hex(tokens.text_primary),
        "secondary.hover.background": hex(tokens.bg_3),
        "secondary.active.background": hex(tokens.bg_3),
        "accent.background": hex(tokens.bg_3),
        "accent.foreground": hex(tokens.text_primary),
        "muted.background": hex(tokens.bg_2),
        "muted.foreground": hex(tokens.text_secondary),
        "danger.background": hex(tokens.danger),
        "danger.foreground": hex(tokens.accent_text),
        "popover.background": hex(tokens.bg_1),
        "popover.foreground": hex(tokens.text_primary),
        "list.background": hex(tokens.bg_1),
        "list.hover.background": hex(tokens.bg_3),
        "list.active.background": hex(tokens.selection_bg),
        "list.active.border": hex(tokens.selection_mark),
        "title_bar.background": hex(tokens.bg_2),
        "title_bar.border": hex(tokens.border),
        "status_bar.background": hex(tokens.bg_2),
        "status_bar.border": hex(tokens.border),
        "tab_bar.background": hex(tokens.bg_2),
        "tab.background": hex(tokens.bg_2),
        "tab.foreground": hex(tokens.text_secondary),
        "tab.active.background": hex(tokens.bg_1),
        "tab.active.foreground": hex(tokens.text_primary),
        "window.border": hex(tokens.border),
        "overlay": serde_json::Value::String(tokens.backdrop.hex_alpha(tokens.backdrop_opacity)),
        "base.red": hex(tokens.danger),
        "base.green": hex(tokens.success),
        "base.blue": hex(tokens.accent),
        "base.yellow": hex(tokens.warning),
    });
    let file = serde_json::json!({
        "name": "gitcroc",
        "mode": if mode.is_dark() { "dark" } else { "light" },
        "font.family": fonts.ui.as_ref(),
        "font.size": fonts.ui_size,
        "mono_font.family": fonts.code.as_ref(),
        "mono_font.size": fonts.code_size,
        "radius": 4,
        "colors": colors,
    });
    // Every key above is one gpui-component reads; a key it does not know
    // is ignored, never an error.
    serde_json::from_value(file).unwrap_or_default()
}

/// The tokens in use, as `cx.tokens()`.
pub trait ActiveTokens {
    fn tokens(&self) -> &Tokens;
}

impl ActiveTokens for App {
    fn tokens(&self) -> &Tokens {
        AppTheme::global(self).tokens()
    }
}
