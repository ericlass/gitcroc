//! What the editor cannot draw itself (M6-02, item 6), as rc.1's
//! `editor.ts` decorations: full-width rows under its transparent text,
//! and headers with the hunk's controls over the marker lines. Everything
//! is placed from the editor's own text
//! bounds, line height and scroll, which assumes rows of one height: soft
//! wrap and folding stay off. Only hunks in view, plus a few lines, are
//! laid out (PERF-1).
use std::ops::Range;

use gpui_component::button::Button;
use gpui_component::input::EditorState;
use gpui_component::{Sizable, h_flex};
use gpui_kit::base::input::RopeExt;
use gpui_kit::{prelude::*, *};

use super::{ConflictEditor, Look, ReturnToText, Take};
use crate::strings::conflict as text;
use crate::theme::AppTheme;
use crate::theme::tokens::Tokens;

/// The key context of a header's controls, where Escape returns to the text.
pub const CONTROLS: &str = "GitcrocConflictControls";
/// Lines laid out above and below the viewport.
const OVERSCAN: f32 = 3.;

/// Where the editor laid its lines out, as measured after it painted;
/// the scroll is read again for each frame. Overlays are clipped to the
/// editor's viewport, and placed relative to it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct Measured {
    /// The first line's top while nothing is scrolled.
    top: Pixels,
    /// Where the text starts, right of the line numbers.
    gutter: Pixels,
    line_height: Pixels,
    /// The viewport, relative to the conflict view.
    pub viewport: Bounds<Pixels>,
    /// The scroll the editor painted with.
    scroll: Point<Pixels>,
}

impl Measured {
    /// Reads `state` after it painted, in the view whose bounds are `view`.
    pub(super) fn read(state: &EditorState, view: Bounds<Pixels>) -> Option<Self> {
        let line_height = state.line_height()?;
        let text = state.text_bounds()?;
        let scroll = state.scroll_offset();
        let input = state.input_bounds();
        let row = state.visible_row_range()?.start;
        let start = state.text().line_start_offset(row);
        let line = state.range_to_bounds(&(start..start))?;
        Some(Self {
            top: text.origin.y - scroll.y - input.origin.y,
            gutter: line.origin.x - scroll.x - input.origin.x,
            line_height,
            viewport: Bounds::new(input.origin - view.origin, input.size),
            scroll,
        })
    }
}

impl Measured {
    /// Whether overlays placed with `before` at `scroll` are out of place
    /// now: the layout changed, or the editor painted another scroll.
    pub(super) fn moved(&self, before: Option<Self>, scroll: Point<Pixels>) -> bool {
        self.scroll != scroll
            || before.is_none_or(|before| Self { scroll, ..before } != Self { scroll, ..*self })
    }
}

/// The measured layout at the scroll of the frame being drawn.
#[derive(Clone, Copy)]
struct Geometry {
    m: Measured,
    scroll: Point<Pixels>,
}

impl Geometry {
    fn y(&self, row: usize) -> Pixels {
        self.m.top + self.scroll.y + self.m.line_height * row as f32
    }

    /// The rows in view, with the overscan.
    fn rows(&self) -> Range<usize> {
        let from = |y: Pixels| (y - self.m.top - self.scroll.y) / self.m.line_height;
        let first = from(px(0.)) - OVERSCAN;
        let last = from(self.m.viewport.size.height) + OVERSCAN;
        first.floor().max(0.) as usize..last.ceil().max(0.) as usize
    }
}

#[derive(Clone, Copy, PartialEq)]
enum Marker {
    Ours,
    Theirs,
    End,
}

#[derive(Default)]
pub(super) struct Overlays {
    pub rows: Vec<AnyElement>,
    pub headers: Vec<AnyElement>,
}

fn id(name: String) -> ElementId {
    ElementId::Name(name.into())
}

impl ConflictEditor {
    pub(super) fn overlays(
        &mut self,
        look: &Look,
        tokens: &Tokens,
        cx: &mut Context<Self>,
    ) -> Overlays {
        let state = self.editor.read(cx);
        self.laid_out = 0;
        let Some(m) = self.measured.get() else {
            return Overlays::default();
        };
        let g = Geometry {
            m,
            scroll: state.scroll_offset(),
        };
        let text = state.text().clone();
        let visible = g.rows();
        let mut out = Overlays::default();
        for (n, (hunk, range, untouched)) in self.ranges(cx).into_iter().enumerate() {
            let head = text.offset_to_point(range.start).row;
            let tail = text
                .offset_to_point(range.end.saturating_sub(1).max(range.start))
                .row;
            if tail < visible.start || head >= visible.end {
                continue;
            }
            self.laid_out += 1;
            if !untouched {
                out.rows.extend(row(
                    format!("conflict-changed-{hunk}"),
                    head..tail + 1,
                    &g,
                    look.rows,
                ));
                continue;
            }
            let built = &self.hunks[hunk].hunk;
            let separator = text.offset_to_point(range.start + built.separator).row;
            let end = text.offset_to_point(range.start + built.end).row;
            out.rows.extend(row(
                format!("conflict-rows-{hunk}-ours"),
                head + 1..separator,
                &g,
                look.rows,
            ));
            out.rows.extend(row(
                format!("conflict-rows-{hunk}-theirs"),
                separator + 1..end,
                &g,
                look.rows,
            ));
            for (line, kind) in [
                (head, Marker::Ours),
                (separator, Marker::Theirs),
                (end, Marker::End),
            ] {
                if visible.contains(&line) {
                    out.headers.push(self.header(
                        hunk,
                        n + 1,
                        kind,
                        g.y(line),
                        &g,
                        look,
                        tokens,
                        cx,
                    ));
                }
            }
        }
        out
    }

    #[allow(clippy::too_many_arguments)]
    fn header(
        &self,
        hunk: usize,
        number: usize,
        kind: Marker,
        y: Pixels,
        g: &Geometry,
        look: &Look,
        tokens: &Tokens,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let (name, label): (&str, SharedString) = match kind {
            Marker::Ours => ("ours", text::ours(&self.labels.ours).into()),
            Marker::Theirs => ("theirs", text::theirs(&self.labels.theirs).into()),
            Marker::End => ("end", text::end_of(number).into()),
        };
        h_flex()
            .id(id(format!("conflict-{name}-{hunk}")))
            .test_support()
            .occlude()
            .absolute()
            .top(y)
            .left(g.m.gutter)
            .right_0()
            .h(g.m.line_height)
            .overflow_hidden()
            .px_1()
            .gap_2()
            .bg(look.marker)
            .text_color(look.text)
            .text_size(AppTheme::global(cx).code_font_size())
            .role(Role::Heading)
            .aria_level(4)
            .aria_label(label.clone())
            // Repo text, as text (SEC-3).
            .child(
                div()
                    .flex_none()
                    .when(kind != Marker::End, |label| {
                        label.font_weight(FontWeight::SEMIBOLD)
                    })
                    .child(label),
            )
            .when(kind == Marker::Ours, |row| {
                row.child(self.controls(hunk, &text::which(number), g, tokens, cx))
            })
            .into_any_element()
    }

    /// Take ours, theirs and both: they call the editor's
    /// transactions and move with the hunk's anchor.
    fn controls(
        &self,
        hunk: usize,
        which: &str,
        g: &Geometry,
        tokens: &Tokens,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let height = (g.m.line_height - px(2.)).max(px(8.));
        let button = |part: &str, name: &'static str, label: String| {
            Button::new(id(format!("conflict-{hunk}-{part}")))
                .xsmall()
                .compact()
                .outline()
                .h(height)
                .label(name)
                .accessibility_label(label)
        };
        let take = |part: &str, name: &'static str, label: String, side: Take| {
            button(part, name, label).on_click(cx.listener(
                move |this, _: &ClickEvent, window, cx| {
                    this.take(hunk, side, window, cx);
                },
            ))
        };
        h_flex()
            .key_context(CONTROLS)
            .on_action(cx.listener(move |this, _: &ReturnToText, window, cx| {
                this.return_to_text(hunk, window, cx)
            }))
            .flex_none()
            .h_full()
            .items_center()
            .gap_1()
            .px_1()
            .bg(tokens.bg_1)
            .text_color(tokens.text_primary)
            .child(
                take(
                    "take-ours",
                    text::TAKE_OURS,
                    text::take_ours_in(which),
                    Take::Ours,
                )
                .when(self.control_hunk == Some(hunk), |button| {
                    button.track_focus(&self.controls)
                }),
            )
            .child(take(
                "take-theirs",
                text::TAKE_THEIRS,
                text::take_theirs_in(which),
                Take::Theirs,
            ))
            .child(take(
                "take-both",
                text::TAKE_BOTH,
                text::take_both_in(which),
                Take::Both,
            ))
    }
}

/// A full-width run of rows under the editor's text.
fn row(
    name: String,
    lines: Range<usize>,
    g: &Geometry,
    color: impl Into<Hsla>,
) -> Option<AnyElement> {
    (!lines.is_empty()).then(|| {
        div()
            .id(id(name))
            .test_support()
            .absolute()
            .left_0()
            .right_0()
            .top(g.y(lines.start))
            .h(g.m.line_height * lines.len() as f32)
            .bg(color.into())
            .into_any_element()
    })
}
