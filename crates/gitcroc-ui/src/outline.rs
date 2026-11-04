//! The focus ring and the selection mark (UI-23, M6-25b): borders, so that
//! the focus and the selection show in every theme without relying on a
//! background. Both are drawn over the element, so they move nothing; the
//! element must be positioned, relative or absolute. gpui-component's
//! controls keep their own ring.

use gpui_kit::{ParentElement, Styled, div};

use crate::theme::tokens::Tokens;

pub trait Outlined: ParentElement + Sized {
    /// The focus ring, from the `focus_ring` token, while `focused`.
    fn focus_ring(self, focused: bool, tokens: &Tokens) -> Self {
        let color = tokens.focus_ring;
        if !focused {
            return self;
        }
        self.child(div().absolute().inset_0().border_2().border_color(color))
    }

    /// The selection mark at the element's leading edge, from the
    /// `selection_mark` token, which alone keeps 3:1 on every background,
    /// while `selected`.
    fn selection_mark(self, selected: bool, tokens: &Tokens) -> Self {
        let color = tokens.selection_mark;
        if !selected {
            return self;
        }
        self.child(
            div()
                .absolute()
                .top_0()
                .bottom_0()
                .left_0()
                .border_l_2()
                .border_color(color),
        )
    }
}

impl<E: ParentElement> Outlined for E {}
