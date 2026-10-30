//! Syntax colours in the conflict editor (UI-24): the diff's syntect
//! classes (`highlight.rs`), through the editor's highlighter seam. Each
//! change parses the file again off the UI thread (ARCH-7); until that
//! lands, the earlier runs stay, moved with the lines after the edit. The
//! runs name a [`Syntax`], so a code theme change only recolours.
use std::cell::RefCell;
use std::ops::Range;
use std::rc::Rc;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use gpui_component::input::EditorState;
use gpui_kit::base::input::{
    FoldRange, HighlightStyleResolver, InputEdit, InputHighlighter, InputHighlighterFactory, Rope,
    RopeExt,
};
use gpui_kit::{AppContext, Context, HighlightStyle, SharedString, Task, Window};
use syntect::parsing::SyntaxReference;

use crate::highlight::{Side, Span, syntax_for};
use crate::theme::code_themes::Syntax;

/// Each parsed line's start and its runs, from its start.
pub type Lines = Rc<RefCell<Vec<(usize, Vec<Span>)>>>;

/// The name the editor's style resolver knows `syntax` by.
pub fn name(syntax: Syntax) -> &'static str {
    match syntax {
        Syntax::Plain => "",
        Syntax::Comment => "comment",
        Syntax::String => "string",
        Syntax::Number => "number",
        Syntax::Keyword => "keyword",
        Syntax::Type => "type",
        Syntax::Function => "function",
        Syntax::Tag => "tag",
    }
}

/// The factory for a file at `path`: none for a type syntect does not
/// know, which the editor shows as plain text.
pub fn factory(path: &str, lines: Lines) -> InputHighlighterFactory {
    let syntax = syntax_for(path);
    Rc::new(move |_| {
        let syntax = syntax?;
        let highlighter: Box<dyn InputHighlighter> = Box::new(Highlighter {
            syntax,
            lines: lines.clone(),
            cancel: Arc::new(AtomicBool::new(false)),
            _parse: None,
        });
        Some(highlighter)
    })
}

/// The syntax at `offset`, from the last parse.
pub fn at(lines: &Lines, offset: usize) -> Option<Syntax> {
    let lines = lines.borrow();
    let line = lines
        .partition_point(|(start, _)| *start <= offset)
        .checked_sub(1)?;
    let (start, spans) = &lines[line];
    spans
        .iter()
        .find(|span| span.range.contains(&(offset - start)))
        .map(|span| span.syntax)
}

struct Highlighter {
    syntax: &'static SyntaxReference,
    lines: Lines,
    cancel: Arc<AtomicBool>,
    _parse: Option<Task<()>>,
}

fn parse(
    syntax: &SyntaxReference,
    text: &Rope,
    cancel: &AtomicBool,
) -> Option<Vec<(usize, Vec<Span>)>> {
    let mut side = Side::new(syntax);
    let mut lines = Vec::with_capacity(text.lines_len());
    for row in 0..text.lines_len() {
        if row % 256 == 0 && cancel.load(Ordering::Relaxed) {
            return None;
        }
        let line = text.slice_line(row).to_string();
        lines.push((
            text.line_start_offset(row),
            side.line(line.trim_end_matches('\r')),
        ));
    }
    Some(lines)
}

impl InputHighlighter for Highlighter {
    fn language(&self) -> SharedString {
        self.syntax.name.clone().into()
    }

    fn update(
        &mut self,
        edit: Option<InputEdit>,
        text: &Rope,
        _: bool,
        _: &mut Window,
        cx: &mut Context<EditorState>,
    ) {
        if let Some(edit) = edit {
            // Lines after the edit keep their runs where they moved to.
            let delta = edit.new_end_byte as isize - edit.old_end_byte as isize;
            self.lines.borrow_mut().retain_mut(|(start, _)| {
                if *start > edit.start_byte && *start <= edit.old_end_byte {
                    return false;
                }
                if *start > edit.old_end_byte {
                    *start = start.saturating_add_signed(delta);
                }
                true
            });
        }
        self.cancel.store(true, Ordering::Relaxed);
        let cancel = Arc::new(AtomicBool::new(false));
        self.cancel = cancel.clone();
        let (syntax, text, lines) = (self.syntax, text.clone(), self.lines.clone());
        self._parse = Some(cx.spawn(async move |editor, cx| {
            let parsed = cx
                .background_spawn(async move { parse(syntax, &text, &cancel) })
                .await;
            if let Some(parsed) = parsed {
                *lines.borrow_mut() = parsed;
                let _ = editor.update(cx, |_, cx| cx.notify());
            }
        }));
    }

    fn styles(
        &self,
        range: &Range<usize>,
        resolver: &dyn HighlightStyleResolver,
    ) -> Vec<(Range<usize>, HighlightStyle)> {
        let lines = self.lines.borrow();
        let first = lines
            .partition_point(|(start, _)| *start <= range.start)
            .saturating_sub(1);
        let mut runs = Vec::new();
        let mut at = range.start;
        for (start, spans) in lines[first..].iter().take_while(|(s, _)| *s < range.end) {
            for span in spans {
                let from = (start + span.range.start).max(at);
                let to = (start + span.range.end).min(range.end);
                if from >= to {
                    continue;
                }
                if from > at {
                    runs.push((at..from, HighlightStyle::default()));
                }
                let style = resolver.style(name(span.syntax)).unwrap_or_default();
                runs.push((from..to, style));
                at = to;
            }
        }
        if at < range.end {
            runs.push((at..range.end, HighlightStyle::default()));
        }
        runs
    }

    fn fold_ranges(&self, _: &Rope) -> Vec<FoldRange> {
        Vec::new()
    }
}
