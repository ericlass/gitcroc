//! Syntax highlighting of the diff (UI-24), as rc.1's `applyLanguage` in
//! `changes/codemirror.ts` picked a language: by the file's name, with
//! syntect's default syntaxes; an unknown type stays plain text. Each side's
//! lines are parsed in order, so a line's colours follow from the lines
//! above it: in the same hunk inline, in the whole file side by side.
//!
//! Highlighting runs off the UI thread (ARCH-7) and gives each line its runs
//! of [`Syntax`], not colours, so a change of code theme only recolours.

use std::ops::Range;
use std::sync::OnceLock;

use gitcroc_core::LineKind;
use syntect::highlighting::{
    Color as SyntectColor, HighlightState, Highlighter, RangedHighlightIterator, Theme,
};
use syntect::parsing::{ParseState, ScopeStack, SyntaxReference, SyntaxSet};

use crate::diff::{Cell, DiffDoc, DiffRow};
use crate::theme::code_themes::{Syntax, scope_theme};

/// Lines longer than this are shown plain and not parsed: a minified file's
/// one line would hold the rest of the diff back.
pub const MAX_LINE: usize = 4096;

/// Each [`Syntax`] in the order of its number in [`classes`].
const ALL: [Syntax; 8] = [
    Syntax::Plain,
    Syntax::Comment,
    Syntax::String,
    Syntax::Number,
    Syntax::Keyword,
    Syntax::Type,
    Syntax::Function,
    Syntax::Tag,
];

/// A run of a line's text, by byte offsets, that is not plain.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Span {
    pub range: Range<usize>,
    pub syntax: Syntax,
}

/// The runs of one row: an inline line's, or each side's of a pair.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum RowSpans {
    #[default]
    None,
    Line(Vec<Span>),
    Pair(Vec<Span>, Vec<Span>),
}

/// The runs of every row of a [`DiffDoc`], by row before folding.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Highlights {
    rows: Vec<RowSpans>,
}

impl Highlights {
    pub fn row(&self, row: usize) -> Option<&RowSpans> {
        self.rows.get(row)
    }
}

/// syntect's default syntaxes, loaded once, off the UI thread.
fn syntaxes() -> &'static SyntaxSet {
    static SET: OnceLock<SyntaxSet> = OnceLock::new();
    SET.get_or_init(SyntaxSet::load_defaults_newlines)
}

/// A theme that gives each scope the number of its [`Syntax`] in [`ALL`]
/// as its red channel, so the highlighter's styles name the class.
fn classes() -> &'static Theme {
    static THEME: OnceLock<Theme> = OnceLock::new();
    THEME.get_or_init(|| {
        let color = |syntax: Syntax| SyntectColor {
            r: ALL.iter().position(|each| *each == syntax).unwrap_or(0) as u8,
            g: 0,
            b: 0,
            a: 0xff,
        };
        scope_theme("classes", color(Syntax::Plain), color)
    })
}

fn highlighter() -> &'static Highlighter<'static> {
    static HIGHLIGHTER: OnceLock<Highlighter<'static>> = OnceLock::new();
    HIGHLIGHTER.get_or_init(|| Highlighter::new(classes()))
}

/// The syntax of a file named by `path`: by its whole name, as `Makefile`,
/// then by its extension. `None` for a type syntect does not know.
pub fn syntax_for(path: &str) -> Option<&'static SyntaxReference> {
    let name = path.rsplit('/').next().unwrap_or(path);
    let set = syntaxes();
    let found = set.find_syntax_by_extension(name).or_else(|| {
        let (_, extension) = name.rsplit_once('.')?;
        set.find_syntax_by_extension(extension)
    })?;
    (found.name != set.find_syntax_plain_text().name).then_some(found)
}

/// One side's lines, parsed in order.
pub struct Side {
    parse: ParseState,
    highlight: HighlightState,
    /// A line failed to parse; the rest stay plain.
    failed: bool,
}

impl Side {
    pub fn new(syntax: &SyntaxReference) -> Self {
        Self {
            parse: ParseState::new(syntax),
            highlight: HighlightState::new(highlighter(), ScopeStack::new()),
            failed: false,
        }
    }

    /// The runs of `text`, the side's next line, without its terminator.
    pub fn line(&mut self, text: &str) -> Vec<Span> {
        if self.failed || text.len() > MAX_LINE {
            return Vec::new();
        }
        // The syntaxes match a line with its newline.
        let line = format!("{text}\n");
        let Ok(ops) = self.parse.parse_line(&line, syntaxes()) else {
            self.failed = true;
            return Vec::new();
        };
        let mut spans: Vec<Span> = Vec::new();
        for (style, _, range) in
            RangedHighlightIterator::new(&mut self.highlight, &ops, &line, highlighter())
        {
            let syntax = ALL
                .get(usize::from(style.foreground.r))
                .copied()
                .unwrap_or(Syntax::Plain);
            let range = range.start.min(text.len())..range.end.min(text.len());
            if syntax == Syntax::Plain || range.is_empty() {
                continue;
            }
            match spans.last_mut() {
                Some(last) if last.syntax == syntax && last.range.end == range.start => {
                    last.range.end = range.end;
                }
                _ => spans.push(Span { range, syntax }),
            }
        }
        spans
    }
}

/// The runs of every row of `doc`, the diff of the file at `path`. Inline,
/// each hunk starts both sides afresh, and an unchanged line is read on
/// both; side by side, each side is its whole file.
pub fn highlight(doc: &DiffDoc, path: &str) -> Highlights {
    let Some(syntax) = syntax_for(path) else {
        return Highlights::default();
    };
    let inline = !doc.side_by_side_mode();
    let (mut old, mut new) = (Side::new(syntax), Side::new(syntax));
    let cell = |side: &mut Side, cell: &Option<Cell>| {
        cell.as_ref()
            .map_or_else(Vec::new, |cell| side.line(&cell.text))
    };
    let rows = doc
        .rows()
        .iter()
        .map(|row| match row {
            DiffRow::Hunk { .. } => {
                if inline {
                    (old, new) = (Side::new(syntax), Side::new(syntax));
                }
                RowSpans::None
            }
            DiffRow::Line { kind, text, .. } => RowSpans::Line(match kind {
                LineKind::Removed => old.line(text),
                LineKind::Added => new.line(text),
                LineKind::Context => {
                    old.line(text);
                    new.line(text)
                }
            }),
            DiffRow::Pair {
                old: left,
                new: right,
            } => RowSpans::Pair(cell(&mut old, left), cell(&mut new, right)),
        })
        .collect();
    Highlights { rows }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn named(text: &str, spans: &[Span]) -> Vec<(String, Syntax)> {
        spans
            .iter()
            .map(|span| (text[span.range.clone()].to_owned(), span.syntax))
            .collect()
    }

    #[test]
    fn ui_24_the_syntax_comes_from_the_file_name() {
        assert_eq!(
            syntax_for("src/main.rs").map(|s| s.name.as_str()),
            Some("Rust")
        );
        assert_eq!(
            syntax_for("a/b/Makefile").map(|s| s.name.as_str()),
            Some("Makefile")
        );
        assert_eq!(
            syntax_for("README.MD").map(|s| s.name.as_str()),
            Some("Markdown")
        );
        assert!(syntax_for("notes.txt").is_none());
        assert!(syntax_for("data.unknown-type").is_none());
        assert!(syntax_for("LICENSE").is_none());
    }

    #[test]
    fn ui_24_a_line_is_split_into_its_syntax() {
        let mut side = Side::new(syntax_for("a.rs").unwrap());
        let text = "fn main() { let s = \"x\"; } // done";
        let spans = side.line(text);
        let named = named(text, &spans);
        assert!(named.contains(&("fn".into(), Syntax::Keyword)), "{named:?}");
        assert!(
            named.contains(&("main".into(), Syntax::Function)),
            "{named:?}"
        );
        assert!(
            named.contains(&("\"x\"".into(), Syntax::String)),
            "{named:?}"
        );
        assert!(
            named.contains(&("// done".into(), Syntax::Comment)),
            "{named:?}"
        );
        assert!(!named.iter().any(|(text, _)| text == "s"), "{named:?}");
    }

    #[test]
    fn ui_24_state_carries_from_line_to_line() {
        let mut side = Side::new(syntax_for("a.rs").unwrap());
        assert_eq!(
            named("/* open", &side.line("/* open"))[0].1,
            Syntax::Comment
        );
        // Not a keyword here: still in the comment.
        assert_eq!(
            named("fn still */", &side.line("fn still */")),
            [("fn still */".to_owned(), Syntax::Comment)]
        );
        assert_eq!(
            named("fn x", &side.line("fn x"))[0],
            ("fn".to_owned(), Syntax::Keyword)
        );
    }

    #[test]
    fn a_line_over_the_limit_is_plain() {
        let mut side = Side::new(syntax_for("a.rs").unwrap());
        let long = format!("fn {}", "x".repeat(MAX_LINE));
        assert!(side.line(&long).is_empty());
        assert!(!side.line("fn x").is_empty());
    }
}
