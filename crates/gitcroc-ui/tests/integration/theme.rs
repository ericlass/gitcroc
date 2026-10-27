//! The app theme (UI-23), the code themes (UI-24, UI-25) and the fonts
//! (UI-26). The contrast pairs are those of rc.1's `tokens.test.ts` and
//! `codeThemes.test.ts`, under WCAG 2.2 AA (README "Accessibility target").

use std::rc::Rc;

use gitcroc_core::{Appearance, CodeTheme, Settings, Theme as ThemeSetting};
use gitcroc_ui::theme::code_themes::{CODE_THEMES, CodeThemeColors, code_theme, syntect_theme};
use gitcroc_ui::theme::contrast::{NON_TEXT, TEXT, contrast};
use gitcroc_ui::theme::fonts::{Family, NoFonts, check_installed, family};
use gitcroc_ui::theme::tokens::{Color, Tokens};
use gitcroc_ui::theme::{ActiveTokens, AppTheme, contrast::accent_for};
use gitcroc_ui::title_bar::Platform;
use gitcroc_ui::window::GitcrocWindow;
use gpui_component::{Theme, ThemeMode};
use gpui_kit::{AppContext, Hsla, SharedString, TestAppContext, px};
use syntect::easy::HighlightLines;
use syntect::parsing::SyntaxSet;

use crate::support::{FakeControls, Fixture, open};

/// Whether `a` and `b` are the same colour, to the precision of `Hsla`.
fn same(a: Hsla, b: Hsla) -> bool {
    let (a, b) = (a.to_rgb(), b.to_rgb());
    [(a.r, b.r), (a.g, b.g), (a.b, b.b), (a.a, b.a)]
        .iter()
        .all(|(x, y)| (x - y).abs() < 0.002)
}

#[test]
fn ui_23_the_formula_is_wcags() {
    assert!((contrast(Color(0xffffff), Color(0x000000)) - 21.).abs() < 0.01);
    assert!((contrast(Color(0x777777), Color(0xffffff)) - 4.48).abs() < 0.005);
    assert_eq!(
        contrast(Color(0x0964cf), Color(0xffffff)),
        contrast(Color(0xffffff), Color(0x0964cf))
    );
}

/// A colour with its name, for the failure messages.
type Named = (&'static str, Color);

/// Each foreground against each background it is drawn on, and the ratio
/// it needs: `tokens.test.ts`'s pairs.
fn token_failures(tokens: &Tokens) -> Vec<String> {
    let t = tokens;
    let backgrounds = [
        ("bg_1", t.bg_1),
        ("bg_2", t.bg_2),
        ("bg_3", t.bg_3),
        ("selection_bg", t.selection_bg),
    ];
    let pairs: Vec<(Vec<Named>, Vec<Named>, f64)> = vec![
        // Text, and text in a colour that carries meaning (UI-3, GRP-7), on
        // every background level and the selection.
        (
            vec![
                ("text_primary", t.text_primary),
                ("text_secondary", t.text_secondary),
                ("accent", t.accent),
                ("danger", t.danger),
                ("warning", t.warning),
                ("success", t.success),
                ("outcome_succeeded", t.outcome_succeeded),
                ("outcome_nothing_to_do", t.outcome_nothing_to_do),
                ("outcome_skipped", t.outcome_skipped),
                ("outcome_failed", t.outcome_failed),
                ("outcome_conflict", t.outcome_conflict),
            ],
            backgrounds.to_vec(),
            TEXT,
        ),
        // The banners' text (OPS-2).
        (
            vec![
                ("text_primary", t.text_primary),
                ("text_secondary", t.text_secondary),
            ],
            vec![("warning_bg", t.warning_bg)],
            TEXT,
        ),
        // Text on a filled control: a primary or danger button, a pressed
        // toggle, the tab badge and marker.
        (
            vec![("accent_text", t.accent_text)],
            vec![
                ("accent", t.accent),
                ("danger", t.danger),
                ("outcome_failed", t.outcome_failed),
            ],
            TEXT,
        ),
        // Non-text contrast (WCAG 1.4.11).
        (
            vec![
                ("focus_ring", t.focus_ring),
                ("border_input", t.border_input),
                ("selection_mark", t.selection_mark),
            ],
            backgrounds.to_vec(),
            NON_TEXT,
        ),
    ];
    let mut failures = Vec::new();
    for (fronts, backs, ratio) in pairs {
        for (front, front_color) in &fronts {
            for (back, back_color) in &backs {
                let found = contrast(*front_color, *back_color);
                if found < ratio {
                    failures.push(format!("{front} on {back}: {found:.2} < {ratio}"));
                }
            }
        }
    }
    failures
}

#[test]
fn ui_23_app_theme_colors_meet_wcag_contrast() {
    assert_eq!(
        token_failures(&Tokens::LIGHT),
        Vec::<String>::new(),
        "light"
    );
    assert_eq!(token_failures(&Tokens::DARK), Vec::<String>::new(), "dark");
}

#[test]
fn ui_23_the_system_accent_keeps_contrast() {
    // One that passes is kept.
    assert_eq!(
        accent_for(Tokens::LIGHT.accent, &Tokens::LIGHT),
        Tokens::LIGHT.accent
    );
    assert_eq!(
        accent_for(Tokens::DARK.accent, &Tokens::DARK),
        Tokens::DARK.accent
    );
    // GNOME's yellow, teal and slate, Windows' default blue, and KDE's
    // pink: too light for the light theme or too dark for the dark one.
    for system in [
        Color(0xc88800),
        Color(0x2190a4),
        Color(0x6f8396),
        Color(0x0078d4),
        Color(0xd56199),
    ] {
        for tokens in [Tokens::LIGHT, Tokens::DARK] {
            let accent = accent_for(system, &tokens);
            let adjusted = tokens.with_accent(accent);
            assert_eq!(
                token_failures(&adjusted),
                Vec::<String>::new(),
                "{system:?} as {accent:?}"
            );
            // The same hue: the system's colour, lighter or darker.
            let (hue, before) = (Hsla::from(accent).h, Hsla::from(system).h);
            let turn = (hue - before).abs();
            assert!(turn.min(1. - turn) < 0.02, "{system:?} as {accent:?}");
        }
    }
    // When no lightness can pass, the tokens' accent: dark text on the
    // accent needs it light, but white backgrounds need it dark.
    let impossible = Tokens {
        accent_text: Color(0x000000),
        ..Tokens::LIGHT
    };
    assert_eq!(accent_for(Color(0x0078d4), &impossible), impossible.accent);
}

fn settings(theme: ThemeSetting) -> Settings {
    Settings {
        theme,
        ..Settings::default()
    }
}

/// The colours gpui-component draws with are the tokens in use.
fn assert_component_theme(tokens: &Tokens, dark: bool, cx: &gpui_kit::App) {
    let theme = Theme::global(cx);
    assert_eq!(theme.mode.is_dark(), dark);
    assert!(same(theme.background, tokens.bg_1.into()), "background");
    assert!(
        same(theme.foreground, tokens.text_primary.into()),
        "foreground"
    );
    assert!(same(theme.border, tokens.border.into()), "border");
    assert!(same(theme.primary, tokens.accent.into()), "primary");
    assert!(same(theme.ring, tokens.focus_ring.into()), "ring");
}

#[gpui::test]
fn ui_23_theme_setting_light_dark_and_system(cx: &mut TestAppContext) {
    cx.update(|cx| {
        gitcroc_ui::init(cx);
        // System follows the system, live.
        AppTheme::apply_settings(&settings(ThemeSetting::System), cx);
        AppTheme::set_system_dark(false, cx);
        assert!(!AppTheme::global(cx).dark());
        assert_eq!(cx.tokens(), &Tokens::LIGHT);
        assert_component_theme(&Tokens::LIGHT, false, cx);
        AppTheme::set_system_dark(true, cx);
        assert!(AppTheme::global(cx).dark());
        assert_eq!(cx.tokens(), &Tokens::DARK);
        assert_component_theme(&Tokens::DARK, true, cx);

        // Dark and Light ignore it.
        AppTheme::apply_settings(&settings(ThemeSetting::Light), cx);
        assert_component_theme(&Tokens::LIGHT, false, cx);
        AppTheme::set_system_dark(false, cx);
        AppTheme::apply_settings(&settings(ThemeSetting::Dark), cx);
        assert_component_theme(&Tokens::DARK, true, cx);
        AppTheme::set_system_dark(true, cx);
        AppTheme::set_system_dark(false, cx);
        assert_component_theme(&Tokens::DARK, true, cx);

        // The system's accent, made to keep contrast, in both modes.
        AppTheme::set_system_accent(Some(Color(0xc88800)), cx);
        let accent = accent_for(Color(0xc88800), &Tokens::DARK);
        assert_eq!(cx.tokens(), &Tokens::DARK.with_accent(accent));
        assert_component_theme(&Tokens::DARK.with_accent(accent), true, cx);
        AppTheme::set_system_accent(None, cx);
        assert_eq!(cx.tokens(), &Tokens::DARK);
    });
}

fn main_window(
    cx: &mut TestAppContext,
    fixture: &Fixture,
) -> (gpui_kit::VisualTestContext, gpui_kit::Entity<GitcrocWindow>) {
    let services = fixture.services.clone();
    open(cx, move |window, cx| {
        cx.new(|cx| {
            GitcrocWindow::new(
                services,
                Rc::new(FakeControls::default()),
                Platform::Linux,
                false,
                window,
                cx,
            )
        })
    })
}

#[gpui::test]
fn ui_23_saved_theme_applies_on_start(cx: &mut TestAppContext) {
    let fixture = Fixture::start(64);
    let app = fixture.app().clone();
    fixture.runtime.block_on(async {
        app.settings_set_theme(ThemeSetting::Dark).await.unwrap();
    });
    // The test window is light.
    let (mut cx, _window) = main_window(cx, &fixture);
    cx.update(|_, cx| {
        assert!(AppTheme::global(cx).dark());
        assert_component_theme(&Tokens::DARK, true, cx);
    });
}

#[test]
fn ui_24_eight_code_themes_ship() {
    let shipped: Vec<(CodeTheme, &str, bool)> = CODE_THEMES
        .iter()
        .map(|theme| (*theme, code_theme(*theme).label, code_theme(*theme).dark))
        .collect();
    assert_eq!(
        shipped,
        [
            (CodeTheme::GithubLight, "GitHub Light", false),
            (CodeTheme::GithubDark, "GitHub Dark", true),
            (CodeTheme::SolarizedLight, "Solarized Light", false),
            (CodeTheme::SolarizedDark, "Solarized Dark", true),
            (CodeTheme::OneDark, "One Dark", true),
            (CodeTheme::Dracula, "Dracula", true),
            (CodeTheme::Nord, "Nord", true),
            (CodeTheme::Monokai, "Monokai", true),
        ]
    );
}

#[gpui::test]
fn ui_24_code_theme_follows_the_app_theme(cx: &mut TestAppContext) {
    cx.update(|cx| {
        gitcroc_ui::init(cx);
        let mut chosen = settings(ThemeSetting::System);
        chosen.appearance.code_theme_light = CodeTheme::SolarizedLight;
        chosen.appearance.code_theme_dark = CodeTheme::Nord;
        AppTheme::apply_settings(&chosen, cx);
        AppTheme::set_system_dark(false, cx);
        assert_eq!(AppTheme::global(cx).code_theme().label, "Solarized Light");
        AppTheme::set_system_dark(true, cx);
        assert_eq!(AppTheme::global(cx).code_theme().label, "Nord");
        chosen.theme = ThemeSetting::Light;
        AppTheme::apply_settings(&chosen, cx);
        assert_eq!(AppTheme::global(cx).code_theme().label, "Solarized Light");
    });
}

#[test]
fn ui_24_syntect_theme_colours_a_keyword() {
    let syntaxes = SyntaxSet::load_defaults_newlines();
    let rust = syntaxes.find_syntax_by_extension("rs").unwrap();
    for theme in CODE_THEMES {
        let def = code_theme(theme);
        let highlighted = syntect_theme(def);
        let mut lines = HighlightLines::new(rust, &highlighted);
        let ranges = lines
            .highlight_line("fn main() { let s = \"x\"; } // done\n", &syntaxes)
            .unwrap();
        let colour = |text: &str| {
            let (style, _) = ranges
                .iter()
                .find(|(_, part)| part.trim() == text)
                .unwrap_or_else(|| panic!("{text:?} in {ranges:?}"));
            let c = style.foreground;
            Color::from_channels([c.r, c.g, c.b])
        };
        let colors = &def.colors;
        assert_eq!(colour("fn"), colors.keyword, "{}", def.label);
        assert_eq!(colour("main"), colors.function, "{}", def.label);
        assert_eq!(colour("x"), colors.string, "{}", def.label);
        assert_eq!(colour("//"), colors.comment, "{}", def.label);
        assert_eq!(colour("done"), colors.comment, "{}", def.label);
        assert_eq!(colour("s"), colors.text, "{}", def.label);
    }
}

/// The highlights the diff and the conflict editor draw text on, at line
/// and word level (UI-25).
fn highlights(c: &CodeThemeColors) -> [(&'static str, Color); 8] {
    [
        ("added", c.added),
        ("added_word", c.added_word),
        ("removed", c.removed),
        ("removed_word", c.removed_word),
        ("changed", c.changed),
        ("changed_word", c.changed_word),
        ("conflict", c.conflict),
        ("conflict_word", c.conflict_word),
    ]
}

#[test]
fn ui_25_diff_colors_meet_contrast() {
    let mut failures = Vec::new();
    for theme in CODE_THEMES {
        let def = code_theme(theme);
        for (name, background) in highlights(&def.colors) {
            let ratio = contrast(def.colors.text, background);
            if ratio < TEXT {
                failures.push(format!("{} {name}: {ratio:.2}", def.label));
            }
        }
    }
    assert_eq!(failures, Vec::<String>::new());
}

#[test]
fn ui_25_syntax_colors_meet_contrast() {
    // Every colour text is drawn in, on every background it can be drawn
    // on: the editor's, and each highlight (WCAG 1.4.3). Gutter and hunk
    // header text on their own backgrounds.
    let mut failures = Vec::new();
    for theme in CODE_THEMES {
        let def = code_theme(theme);
        let c = &def.colors;
        let syntax = [
            ("text", c.text),
            ("keyword", c.keyword),
            ("string", c.string),
            ("comment", c.comment),
            ("number", c.number),
            ("type", c.r#type),
            ("function", c.function),
            ("tag", c.tag),
        ];
        let mut backgrounds = vec![("background", c.background)];
        backgrounds.extend(highlights(c));
        let mut pairs: Vec<(Named, Named)> = syntax
            .iter()
            .flat_map(|text| {
                backgrounds
                    .iter()
                    .map(move |background| (*text, *background))
            })
            .collect();
        pairs.push((
            ("gutter_text", c.gutter_text),
            ("gutter_background", c.gutter_background),
        ));
        pairs.push((
            ("hunk_text", c.hunk_text),
            ("hunk_background", c.hunk_background),
        ));
        for ((text, front), (background, back)) in pairs {
            let ratio = contrast(front, back);
            if ratio < TEXT {
                failures.push(format!("{} {text} on {background}: {ratio:.2}", def.label));
            }
        }
    }
    assert_eq!(failures, Vec::<String>::new());
}

fn fonts(ui: Option<&str>, ui_size: u32, code: Option<&str>, code_size: u32) -> Settings {
    Settings {
        appearance: Appearance {
            ui_font: ui.map(str::to_owned),
            ui_font_size: ui_size,
            code_font: code.map(str::to_owned),
            code_font_size: code_size,
            ..Appearance::default()
        },
        ..Settings::default()
    }
}

#[gpui::test]
fn ui_26_fonts_apply(cx: &mut TestAppContext) {
    cx.update(|cx| {
        gitcroc_ui::init(cx);
        AppTheme::apply_settings(&fonts(Some("Inter"), 15, Some("Fira Code"), 12), cx);
        let theme = Theme::global(cx);
        assert_eq!(theme.font_family, SharedString::from("Inter"));
        assert_eq!(theme.font_size, px(15.));
        assert_eq!(theme.mono_font_family, SharedString::from("Fira Code"));
        assert_eq!(theme.mono_font_size, px(12.));
        assert_eq!(AppTheme::global(cx).code_font(), "Fira Code");
        // In both modes.
        AppTheme::set_system_dark(true, cx);
        let theme = Theme::global(cx);
        assert_eq!(theme.mode, ThemeMode::Dark);
        assert_eq!(theme.font_family, SharedString::from("Inter"));
        assert_eq!(theme.mono_font_size, px(12.));
    });
}

#[gpui::test]
fn ui_26_the_desktop_fonts_are_the_defaults(cx: &mut TestAppContext) {
    cx.update(|cx| {
        gitcroc_ui::init(cx);
        let defaults = (
            Theme::global(cx).font_family.clone(),
            Theme::global(cx).mono_font_family.clone(),
        );
        AppTheme::apply_settings(&fonts(None, 13, None, 13), cx);
        assert_eq!(
            (
                Theme::global(cx).font_family.clone(),
                Theme::global(cx).mono_font_family.clone()
            ),
            defaults
        );
        AppTheme::set_desktop_fonts(Some("Ubuntu".into()), Some("Ubuntu Mono".into()), cx);
        assert_eq!(Theme::global(cx).font_family, SharedString::from("Ubuntu"));
        assert_eq!(
            Theme::global(cx).mono_font_family,
            SharedString::from("Ubuntu Mono")
        );
        // A setting comes first.
        AppTheme::apply_settings(&fonts(Some("Inter"), 13, None, 13), cx);
        assert_eq!(Theme::global(cx).font_family, SharedString::from("Inter"));
        assert_eq!(
            Theme::global(cx).mono_font_family,
            SharedString::from("Ubuntu Mono")
        );
    });
}

#[test]
fn ui_26_a_missing_font_falls_back_to_the_default() {
    let installed = ["Cantarell".to_owned(), "DejaVu Sans".to_owned()];
    let default = SharedString::from(".SystemUIFont");
    let name = |name: &str| SharedString::from(name.to_owned());
    // The setting, when installed.
    assert_eq!(
        family(Some("DejaVu Sans"), Some("Cantarell"), &installed, &default),
        Family {
            name: name("DejaVu Sans"),
            missing: vec![]
        }
    );
    // Else the desktop's.
    assert_eq!(
        family(Some("Comic Code"), Some("Cantarell"), &installed, &default),
        Family {
            name: name("Cantarell"),
            missing: vec!["Comic Code".to_owned()]
        }
    );
    // Else the default.
    assert_eq!(
        family(Some("Comic Code"), Some("Ubuntu"), &installed, &default),
        Family {
            name: default.clone(),
            missing: vec!["Comic Code".to_owned(), "Ubuntu".to_owned()]
        }
    );
    assert_eq!(
        family(None, None, &installed, &default),
        Family {
            name: default.clone(),
            missing: vec![]
        }
    );
}

#[test]
fn ui_26_without_any_font_the_app_exits_with_a_message() {
    assert_eq!(check_installed(&[]), Err(NoFonts));
    assert!(NoFonts.to_string().contains("no font is installed"));
    assert_eq!(check_installed(&["DejaVu Sans".to_owned()]), Ok(()));

    // The binary itself, on Linux, where a fontconfig file naming only an
    // empty folder leaves GPUI no font. Without a display GPUI runs
    // headless, and its files go to a temporary home.
    #[cfg(target_os = "linux")]
    {
        let home = tempfile::tempdir().unwrap();
        let empty = home.path().join("no-fonts");
        std::fs::create_dir(&empty).unwrap();
        let config = home.path().join("fonts.conf");
        std::fs::write(
            &config,
            format!("<fontconfig><dir>{}</dir></fontconfig>", empty.display()),
        )
        .unwrap();
        let output = std::process::Command::new(env!("CARGO_BIN_EXE_gitcroc-next"))
            .env_remove("DISPLAY")
            .env_remove("WAYLAND_DISPLAY")
            .env("HOME", home.path())
            .env("XDG_CONFIG_HOME", home.path().join("config"))
            .env("XDG_CACHE_HOME", home.path().join("cache"))
            .env("XDG_DATA_HOME", home.path().join("data"))
            .env("XDG_STATE_HOME", home.path().join("state"))
            .env("FONTCONFIG_FILE", &config)
            .output()
            .unwrap();
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert_eq!(output.status.code(), Some(1), "{stderr}");
        assert!(stderr.contains(&NoFonts.to_string()), "{stderr}");
        let log = std::fs::read_to_string(home.path().join("state/gitcroc/gitcroc.log")).unwrap();
        assert!(log.contains("no font is installed"), "{log}");
    }
}
