//! The conflict editor's colours (UI-24, UI-25): all from the code theme,
//! as rc.1's `editor.ts` took them, so they change with it and never with
//! the app theme. gpui-component's editor would draw with the app theme;
//! [`Look::editor_style`] is applied over it each frame.
use std::sync::Arc;

use gpui_kit::base::input::{HighlightStyleResolver, InputEditorStyle};
use gpui_kit::{HighlightStyle, Hsla};

use super::syntax;
use crate::theme::code_themes::{CodeThemeColors, Syntax};
use crate::theme::tokens::Color;

/// Every kind of text the editor colours.
const SYNTAX: [Syntax; 8] = [
    Syntax::Plain,
    Syntax::Comment,
    Syntax::String,
    Syntax::Number,
    Syntax::Keyword,
    Syntax::Type,
    Syntax::Function,
    Syntax::Tag,
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Look {
    pub background: Color,
    pub text: Color,
    pub gutter: Color,
    pub gutter_text: Color,
    pub selection: Color,
    /// The lines of ours, of theirs, and of a hunk edited by hand.
    pub rows: Color,
    /// The marker lines, under their headers, and the edge of a card.
    pub marker: Color,
    /// A proposal, its edge, and its confidence: high, medium, low (AI-11).
    pub proposal: Color,
    pub proposal_edge: Color,
    pub confidence: [Color; 3],
    colors: CodeThemeColors,
}

impl Look {
    pub fn of(colors: &CodeThemeColors) -> Self {
        Self {
            background: colors.background,
            text: colors.text,
            gutter: colors.gutter_background,
            gutter_text: colors.gutter_text,
            selection: colors.selection,
            rows: colors.conflict,
            marker: colors.conflict_word,
            proposal: colors.added,
            proposal_edge: colors.added_word,
            confidence: [colors.added_word, colors.changed_word, colors.removed_word],
            colors: *colors,
        }
    }

    pub fn syntax(&self, syntax: Syntax) -> Color {
        self.colors.syntax(syntax)
    }

    /// Each text colour with what it is drawn on (UI-25).
    pub fn text_pairs(&self) -> Vec<(String, Color, Color)> {
        let mut pairs = Vec::new();
        for (on, back) in [
            ("background", self.background),
            ("rows", self.rows),
            ("marker", self.marker),
        ] {
            for kind in SYNTAX {
                pairs.push((format!("{kind:?} on {on}"), self.syntax(kind), back));
            }
        }
        for (on, back) in [
            ("proposal", self.proposal),
            ("high", self.confidence[0]),
            ("medium", self.confidence[1]),
            ("low", self.confidence[2]),
        ] {
            pairs.push((format!("text on {on}"), self.text, back));
        }
        pairs.push(("line number".into(), self.gutter_text, self.gutter));
        pairs.push(("current line number".into(), self.text, self.gutter));
        pairs
    }

    /// The editor's colours: transparent over the rows, with the code
    /// theme's text, caret, selection, gutter and syntax.
    pub fn editor_style(&self) -> InputEditorStyle {
        InputEditorStyle {
            foreground: self.text.into(),
            muted_foreground: self.gutter_text.into(),
            background: Hsla::transparent_black(),
            border: self.marker.into(),
            selection: self.selection.into(),
            caret: self.text.into(),
            highlight_styles: Arc::new(Resolver(self.colors)),
            editor_gutter_background: Some(self.gutter.into()),
            ..InputEditorStyle::default()
        }
    }
}

/// The syntax names of `syntax.rs` in the code theme's colours.
struct Resolver(CodeThemeColors);

impl HighlightStyleResolver for Resolver {
    fn style(&self, name: &str) -> Option<HighlightStyle> {
        let kind = SYNTAX
            .into_iter()
            .find(|kind| *kind != Syntax::Plain && syntax::name(*kind) == name)?;
        Some(HighlightStyle {
            color: Some(self.0.syntax(kind).into()),
            ..HighlightStyle::default()
        })
    }
}
