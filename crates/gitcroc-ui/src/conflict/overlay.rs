//! What the editor cannot draw itself (M6-02, item 6), as rc.1's
//! `editor.ts` decorations: full-width rows under its transparent text,
//! headers with the hunk's controls over the marker lines, and the card
//! beside the hunk (`card.rs`). Everything is placed from the editor's own text
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
/// A card's share of the editor's width, and the narrowest it gets beside
/// the text before it spans the editor.
const CARD_SHARE: f32 = 0.42;
const CARD_MIN: Pixels = px(260.);
const CARD_MARGIN: Pixels = px(16.);
/// The fewest lines a card is cut to before the next hunk.
const CARD_LINES: f32 = 6.;

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
    /// Where the caret was drawn, if in view.
    caret: Option<Point<Pixels>>,
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
        let cursor = state.cursor();
        Some(Self {
            top: text.origin.y - scroll.y - input.origin.y,
            gutter: line.origin.x - scroll.x - input.origin.x,
            line_height,
            viewport: Bounds::new(input.origin - view.origin, input.size),
            scroll,
            caret: state
                .range_to_bounds(&(cursor..cursor))
                .map(|caret| caret.origin - input.origin),
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
    /// Above the headers: a card must stay usable when it reaches over the
    /// next hunk's controls.
    pub cards: Vec<AnyElement>,
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
        let cursor = state.cursor();
        let visible = g.rows();
        let mut out = Overlays::default();
        let live = self.ranges(cx);
        for (n, (hunk, range, untouched)) in live.into_iter().enumerate() {
            let head = text.offset_to_point(range.start).row;
            let tail = text
                .offset_to_point(range.end.saturating_sub(1).max(range.start))
                .row;
            if tail < visible.start || head >= visible.end {
                continue;
            }
            self.laid_out += 1;
            // The card goes under the hunk's first line, or under the caret
            // when it would cover it, and no further than the viewport's
            // foot: a long card scrolls inside itself rather than off the pane.
            let card_left = if g.m.viewport.size.width * CARD_SHARE < CARD_MIN {
                g.m.gutter
            } else {
                g.m.viewport.size.width * (1. - CARD_SHARE) - CARD_MARGIN
            };
            let card_top = match m.caret {
                Some(caret) if range.contains(&cursor) && caret.x + px(1.) >= card_left => {
                    caret.y + g.m.line_height
                }
                _ => g.y(head + 1),
            };
            let card_height =
                Some((g.m.viewport.size.height - card_top).max(g.m.line_height * CARD_LINES));
            let card = self
                .card(
                    hunk,
                    n + 1,
                    untouched,
                    card_height,
                    g.m.line_height,
                    look,
                    tokens,
                    cx,
                )
                .map(|card| place(hunk, card, card_top, card_left, &g));
            if !untouched {
                out.rows.extend(row(
                    format!("conflict-changed-{hunk}"),
                    head..tail + 1,
                    &g,
                    look.rows,
                ));
                out.cards.extend(card);
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
            // After the hunk's own controls, in Tab order too.
            out.cards.extend(card);
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
                row.child(self.controls(hunk, number, g, tokens, cx))
            })
            .into_any_element()
    }

    /// Take ours, theirs and both, and Show base: they call the editor's
    /// transactions and move with the hunk's anchor.
    fn controls(
        &self,
        hunk: usize,
        number: usize,
        g: &Geometry,
        tokens: &Tokens,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let which = text::which(number);
        let height = (g.m.line_height - px(2.)).max(px(8.));
        // The hunk's controls come first in its tab slot, then its card's
        // buttons (see `card.rs`); the editor is a tab group, so these
        // indices order the editor's controls alone.
        let slot = (number as isize) * 10;
        let button = |part: &str, name: &'static str, label: String, offset: isize| {
            Button::new(id(format!("conflict-{hunk}-{part}")))
                .xsmall()
                .compact()
                .outline()
                .h(height)
                .label(name)
                .accessibility_label(label)
                .tab_index(slot + offset)
        };
        let take = |part: &str, name: &'static str, label: String, side: Take, offset: isize| {
            button(part, name, label, offset).on_click(cx.listener(
                move |this, _: &ClickEvent, window, cx| {
                    this.take(hunk, side, window, cx);
                },
            ))
        };
        let (base, base_label) = if self.show_base.contains(&hunk) {
            (text::HIDE_BASE, text::hide_base_of(&which))
        } else {
            (text::SHOW_BASE, text::show_base_of(&which))
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
                    text::take_ours_in(&which),
                    Take::Ours,
                    0,
                )
                .when(self.control_hunk == Some(hunk), |button| {
                    button.track_focus(&self.controls)
                }),
            )
            .child(take(
                "take-theirs",
                text::TAKE_THEIRS,
                text::take_theirs_in(&which),
                Take::Theirs,
                1,
            ))
            .child(take(
                "take-both",
                text::TAKE_BOTH,
                text::take_both_in(&which),
                Take::Both,
                2,
            ))
            .child(button("base", base, base_label, 3).on_click(cx.listener(
                move |this, _: &ClickEvent, _, cx| {
                    if !this.show_base.remove(&hunk) {
                        this.show_base.insert(hunk);
                    }
                    cx.notify();
                },
            )))
    }
}

/// Puts hunk `hunk`'s card at `top` in the viewport, at the right, or
/// across the text in a narrow editor.
fn place(hunk: usize, card: Div, top: Pixels, left: Pixels, g: &Geometry) -> AnyElement {
    let narrow = left == g.m.gutter;
    div()
        .id(id(format!("conflict-card-{hunk}")))
        .test_support()
        .occlude()
        .absolute()
        .top(top)
        .map(|place| {
            if narrow {
                place.left(left).right(px(8.))
            } else {
                place.right(CARD_MARGIN).w(relative(CARD_SHARE))
            }
        })
        .max_h(g.m.viewport.size.height)
        .overflow_y_scroll()
        .child(card)
        .into_any_element()
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
