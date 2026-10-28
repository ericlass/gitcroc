//! The code themes (UI-24): the only place their colours are defined, as
//! `tokens.rs` is for the app theme (UI-23). The diff and the conflict
//! editor draw with them; the added, removed, changed and conflict colours
//! keep 4.5:1 against the text (UI-25, `tests/integration/theme.rs`).
//!
//! The values are 1.0.0-rc.1's `ui/src/theme/codeThemes.ts`. Sources, all
//! MIT: GitHub's Primer colours, Ethan Schoonover's Solarized,
//! @codemirror/theme-one-dark, Dracula, Nord, and Monokai as in
//! @uiw/codemirror-theme-monokai. Diff colours are the theme's own green,
//! red, blue and yellow mixed into its background. For WCAG 2.2 AA (M5-10),
//! each syntax colour is lighter or darker than its source, with its hue
//! kept, so it keeps 4.5:1 on the background and on every highlight.

use std::str::FromStr;

use gitcroc_core::CodeTheme;
use syntect::highlighting::{
    Color as SyntectColor, ScopeSelectors, StyleModifier, Theme, ThemeItem, ThemeSettings,
};

use super::tokens::Color;
use crate::strings;

/// The colours of one code theme.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CodeThemeColors {
    pub background: Color,
    pub text: Color,
    pub gutter_background: Color,
    pub gutter_text: Color,
    pub selection: Color,
    pub hunk_background: Color,
    pub hunk_text: Color,
    /// Line and word level highlights (UI-25).
    pub added: Color,
    pub added_word: Color,
    pub removed: Color,
    pub removed_word: Color,
    pub changed: Color,
    pub changed_word: Color,
    pub conflict: Color,
    pub conflict_word: Color,
    /// Syntax.
    pub keyword: Color,
    pub string: Color,
    pub comment: Color,
    pub number: Color,
    pub r#type: Color,
    pub function: Color,
    pub tag: Color,
}

/// A code theme: its name, whether it is a dark one, and its colours.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CodeThemeDef {
    pub label: &'static str,
    pub dark: bool,
    pub colors: CodeThemeColors,
}

/// The themes in the order the Settings dialog lists them.
pub const CODE_THEMES: [CodeTheme; 8] = [
    CodeTheme::GithubLight,
    CodeTheme::GithubDark,
    CodeTheme::SolarizedLight,
    CodeTheme::SolarizedDark,
    CodeTheme::OneDark,
    CodeTheme::Dracula,
    CodeTheme::Nord,
    CodeTheme::Monokai,
];

/// The definition of `theme`.
pub fn code_theme(theme: CodeTheme) -> &'static CodeThemeDef {
    match theme {
        CodeTheme::GithubLight => &GITHUB_LIGHT,
        CodeTheme::GithubDark => &GITHUB_DARK,
        CodeTheme::SolarizedLight => &SOLARIZED_LIGHT,
        CodeTheme::SolarizedDark => &SOLARIZED_DARK,
        CodeTheme::OneDark => &ONE_DARK,
        CodeTheme::Dracula => &DRACULA,
        CodeTheme::Nord => &NORD,
        CodeTheme::Monokai => &MONOKAI,
    }
}

/// What a run of highlighted text is, whatever the theme's colours: the
/// diff keeps these per line, so a theme change only recolours (UI-24).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Syntax {
    Plain,
    Comment,
    String,
    Number,
    Keyword,
    Type,
    Function,
    Tag,
}

/// The TextMate scopes of each syntax colour.
pub const SYNTAX_SCOPES: [(&str, Syntax); 7] = [
    ("comment", Syntax::Comment),
    ("string", Syntax::String),
    (
        "constant.numeric, constant.language, constant.character",
        Syntax::Number,
    ),
    ("keyword, storage", Syntax::Keyword),
    (
        "entity.name.type, entity.name.class, entity.name.struct, entity.name.enum, \
         entity.other.inherited-class, support.type, support.class",
        Syntax::Type,
    ),
    (
        "entity.name.function, support.function, variable.function",
        Syntax::Function,
    ),
    ("entity.name.tag", Syntax::Tag),
];

impl CodeThemeColors {
    /// The colour text of kind `syntax` is drawn in.
    pub fn syntax(&self, syntax: Syntax) -> Color {
        match syntax {
            Syntax::Plain => self.text,
            Syntax::Comment => self.comment,
            Syntax::String => self.string,
            Syntax::Number => self.number,
            Syntax::Keyword => self.keyword,
            Syntax::Type => self.r#type,
            Syntax::Function => self.function,
            Syntax::Tag => self.tag,
        }
    }
}

/// A syntect theme that paints each scope of [`SYNTAX_SCOPES`] with
/// `color` of its kind.
pub fn scope_theme(
    name: &str,
    plain: SyntectColor,
    color: impl Fn(Syntax) -> SyntectColor,
) -> Theme {
    Theme {
        name: Some(name.to_owned()),
        author: None,
        settings: ThemeSettings {
            foreground: Some(plain),
            ..ThemeSettings::default()
        },
        // The selectors are constants; the test that highlights with every
        // theme fails if one does not parse.
        scopes: SYNTAX_SCOPES
            .into_iter()
            .filter_map(|(selector, syntax)| {
                Some(ThemeItem {
                    scope: ScopeSelectors::from_str(selector).ok()?,
                    style: StyleModifier {
                        foreground: Some(color(syntax)),
                        background: None,
                        font_style: None,
                    },
                })
            })
            .collect(),
    }
}

fn syntect_color(color: Color) -> SyntectColor {
    let [r, g, b] = color.channels();
    SyntectColor { r, g, b, a: 0xff }
}

/// The syntect theme of `def`, with its colours.
pub fn syntect_theme(def: &CodeThemeDef) -> Theme {
    let colors = &def.colors;
    let mut theme = scope_theme(def.label, syntect_color(colors.text), |syntax| {
        syntect_color(colors.syntax(syntax))
    });
    theme.settings = ThemeSettings {
        foreground: Some(syntect_color(colors.text)),
        background: Some(syntect_color(colors.background)),
        caret: Some(syntect_color(colors.text)),
        selection: Some(syntect_color(colors.selection)),
        gutter: Some(syntect_color(colors.gutter_background)),
        gutter_foreground: Some(syntect_color(colors.gutter_text)),
        ..ThemeSettings::default()
    };
    theme
}

const GITHUB_LIGHT: CodeThemeDef = CodeThemeDef {
    label: strings::code_themes::GITHUB_LIGHT,
    dark: false,
    colors: CodeThemeColors {
        background: Color(0xffffff),
        text: Color(0x1f2328),
        gutter_background: Color(0xf6f8fa),
        gutter_text: Color(0x59636e),
        selection: Color(0xcce5ff),
        hunk_background: Color(0xddf4ff),
        hunk_text: Color(0x59636e),
        added: Color(0xddece1),
        added_word: Color(0xcce2d2),
        removed: Color(0xf8dee0),
        removed_word: Color(0xf4cdd0),
        changed: Color(0xdae9f9),
        changed_word: Color(0xc8ddf7),
        conflict: Color(0xf5edd9),
        conflict_word: Color(0xf1e4c6),
        keyword: Color(0xb51e28),
        string: Color(0x0a3069),
        comment: Color(0x545e68),
        number: Color(0x0550ae),
        r#type: Color(0x953800),
        function: Color(0x6e34da),
        tag: Color(0x116329),
    },
};

const GITHUB_DARK: CodeThemeDef = CodeThemeDef {
    label: strings::code_themes::GITHUB_DARK,
    dark: true,
    colors: CodeThemeColors {
        background: Color(0x0d1117),
        text: Color(0xe6edf3),
        gutter_background: Color(0x151b23),
        gutter_text: Color(0x9198a1),
        selection: Color(0x264f78),
        hunk_background: Color(0x121d2f),
        hunk_text: Color(0x9198a1),
        added: Color(0x12261e),
        added_word: Color(0x163924),
        removed: Color(0x301b1f),
        removed_word: Color(0x4f2325),
        changed: Color(0x13233a),
        changed_word: Color(0x193357),
        conflict: Color(0x272215),
        conflict_word: Color(0x3e3013),
        keyword: Color(0xff7b72),
        string: Color(0xa5d6ff),
        comment: Color(0x959ca4),
        number: Color(0x79c0ff),
        r#type: Color(0xffa657),
        function: Color(0xd2a8ff),
        tag: Color(0x7ee787),
    },
};

const SOLARIZED_LIGHT: CodeThemeDef = CodeThemeDef {
    label: strings::code_themes::SOLARIZED_LIGHT,
    dark: false,
    colors: CodeThemeColors {
        background: Color(0xfdf6e3),
        text: Color(0x073642),
        gutter_background: Color(0xeee8d5),
        gutter_text: Color(0x566b72),
        selection: Color(0xe4dcc4),
        hunk_background: Color(0xeee8d5),
        hunk_text: Color(0x566b72),
        added: Color(0xebe8c1),
        added_word: Color(0xe2e1b0),
        removed: Color(0xf8d9c8),
        removed_word: Color(0xf5caba),
        changed: Color(0xdde6e0),
        changed_word: Color(0xcddee0),
        conflict: Color(0xf2e6c1),
        conflict_word: Color(0xeddeb0),
        keyword: Color(0x546000),
        string: Color(0x1a6560),
        comment: Color(0x505d5e),
        number: Color(0xa62463),
        r#type: Color(0x725600),
        function: Color(0x1a5f90),
        tag: Color(0x9e3a11),
    },
};

const SOLARIZED_DARK: CodeThemeDef = CodeThemeDef {
    label: strings::code_themes::SOLARIZED_DARK,
    dark: true,
    colors: CodeThemeColors {
        background: Color(0x002b36),
        text: Color(0xeee8d5),
        gutter_background: Color(0x073642),
        gutter_text: Color(0x8c9c9e),
        selection: Color(0x274642),
        hunk_background: Color(0x073642),
        hunk_text: Color(0x93a1a1),
        added: Color(0x143c2e),
        added_word: Color(0x214629),
        removed: Color(0x212c35),
        removed_word: Color(0x362c35),
        changed: Color(0x06394d),
        changed_word: Color(0x09435c),
        conflict: Color(0x1b392e),
        conflict_word: Color(0x2c4229),
        keyword: Color(0x9eb600),
        string: Color(0x31bdb2),
        comment: Color(0x9cadb3),
        number: Color(0xe78fba),
        r#type: Color(0xd6a200),
        function: Color(0x68b1e4),
        tag: Color(0xef926b),
    },
};

const ONE_DARK: CodeThemeDef = CodeThemeDef {
    label: strings::code_themes::ONE_DARK,
    dark: true,
    colors: CodeThemeColors {
        background: Color(0x282c34),
        text: Color(0xabb2bf),
        gutter_background: Color(0x282c34),
        gutter_text: Color(0x8b94a4),
        selection: Color(0x3e4451),
        hunk_background: Color(0x2c313a),
        hunk_text: Color(0x919aa9),
        added: Color(0x32393a),
        added_word: Color(0x353d3c),
        removed: Color(0x3a323b),
        removed_word: Color(0x47373f),
        changed: Color(0x2d3845),
        changed_word: Color(0x2f3b4a),
        conflict: Color(0x373739),
        conflict_word: Color(0x3a3b3b),
        keyword: Color(0xcf8ee2),
        string: Color(0x98c379),
        comment: Color(0x9fa6b4),
        number: Color(0xd19a66),
        r#type: Color(0xe5c07b),
        function: Color(0x61afef),
        tag: Color(0xe78d94),
    },
};

const DRACULA: CodeThemeDef = CodeThemeDef {
    label: strings::code_themes::DRACULA,
    dark: true,
    colors: CodeThemeColors {
        background: Color(0x282a36),
        text: Color(0xf8f8f2),
        gutter_background: Color(0x282a36),
        gutter_text: Color(0x8591b8),
        selection: Color(0x44475a),
        hunk_background: Color(0x343746),
        hunk_text: Color(0x96a1c2),
        added: Color(0x2d433e),
        added_word: Color(0x305343),
        removed: Color(0x422f3a),
        removed_word: Color(0x52323c),
        changed: Color(0x34414e),
        changed_word: Color(0x3c4f5d),
        conflict: Color(0x423b3c),
        conflict_word: Color(0x524640),
        keyword: Color(0xffa0d7),
        string: Color(0xf1fa8c),
        comment: Color(0xb7bed5),
        number: Color(0xcfb0fb),
        r#type: Color(0x8be9fd),
        function: Color(0x50fa7b),
        tag: Color(0xffa0d7),
    },
};

const NORD: CodeThemeDef = CodeThemeDef {
    label: strings::code_themes::NORD,
    dark: true,
    colors: CodeThemeColors {
        background: Color(0x2e3440),
        text: Color(0xd8dee9),
        gutter_background: Color(0x2e3440),
        gutter_text: Color(0x929db1),
        selection: Color(0x434c5e),
        hunk_background: Color(0x3b4252),
        hunk_text: Color(0x98b2cc),
        added: Color(0x3c4549),
        added_word: Color(0x454f4f),
        removed: Color(0x3f3945),
        removed_word: Color(0x4b3d48),
        changed: Color(0x38414f),
        changed_word: Color(0x3e4a59),
        conflict: Color(0x454649),
        conflict_word: Color(0x53514f),
        keyword: Color(0xb3c7da),
        string: Color(0xb6cca4),
        comment: Color(0xbec5d1),
        number: Color(0xd4bfd0),
        r#type: Color(0xa9cccb),
        function: Color(0x9eccd9),
        tag: Color(0xb3c7da),
    },
};

const MONOKAI: CodeThemeDef = CodeThemeDef {
    label: strings::code_themes::MONOKAI,
    dark: true,
    colors: CodeThemeColors {
        background: Color(0x272822),
        text: Color(0xf8f8f2),
        gutter_background: Color(0x272822),
        gutter_text: Color(0x90908a),
        selection: Color(0x49483e),
        hunk_background: Color(0x3e3d32),
        hunk_text: Color(0xa8a8a3),
        added: Color(0x363e23),
        added_word: Color(0x404c24),
        removed: Color(0x40282c),
        removed_word: Color(0x502731),
        changed: Color(0x2f3d3b),
        changed_word: Color(0x344b4a),
        conflict: Color(0x413522),
        conflict_word: Color(0x513e21),
        keyword: Color(0xfc99bd),
        string: Color(0xe6db74),
        comment: Color(0xbab7a9),
        number: Color(0xc6a7ff),
        r#type: Color(0x66d9ef),
        function: Color(0xa6e22e),
        tag: Color(0xfc99bd),
    },
};
