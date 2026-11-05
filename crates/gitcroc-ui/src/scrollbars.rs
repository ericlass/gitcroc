//! Scrollbars on native overflow containers, preserving their IDs, focus,
//! layout and accessibility nodes. The bar uses the container's own handle.
use std::panic::Location;

use gpui_component::scroll::{Scrollbar, ScrollbarMode};
use gpui_kit::{
    App, Element, ElementId, InteractiveElement, IntoElement, ParentElement, RenderOnce,
    ScrollHandle, StatefulInteractiveElement, Styled, Window,
};

pub trait ScrollAreaExt:
    StatefulInteractiveElement + ParentElement + Styled + Element + Sized
{
    #[track_caller]
    fn visible_y_scroll(self) -> ScrollArea<Self> {
        ScrollArea {
            element: self,
            id: ElementId::CodeLocation(*Location::caller()),
        }
    }
}
impl<E> ScrollAreaExt for E where E: StatefulInteractiveElement + ParentElement + Styled + Element {}

#[derive(IntoElement)]
pub struct ScrollArea<E: StatefulInteractiveElement + ParentElement + Styled + Element> {
    element: E,
    id: ElementId,
}
impl<E> RenderOnce for ScrollArea<E>
where
    E: StatefulInteractiveElement + ParentElement + Styled + Element + 'static,
{
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let handle = window
            .use_keyed_state(self.id.clone(), cx, |_, _| ScrollHandle::new())
            .read(cx)
            .clone();
        self.element
            .track_scroll(&handle)
            .overflow_y_scroll()
            .child(
                Scrollbar::vertical(&handle)
                    .id((self.id, "scrollbar"))
                    .mode(ScrollbarMode::Always),
            )
    }
}

impl<E> ParentElement for ScrollArea<E>
where
    E: StatefulInteractiveElement + ParentElement + Styled + Element,
{
    fn extend(&mut self, children: impl IntoIterator<Item = gpui_kit::AnyElement>) {
        self.element.extend(children);
    }
}
impl<E> Styled for ScrollArea<E>
where
    E: StatefulInteractiveElement + ParentElement + Styled + Element,
{
    fn style(&mut self) -> &mut gpui_kit::StyleRefinement {
        self.element.style()
    }
}
impl<E> InteractiveElement for ScrollArea<E>
where
    E: StatefulInteractiveElement + ParentElement + Styled + Element,
{
    fn interactivity(&mut self) -> &mut gpui_kit::Interactivity {
        self.element.interactivity()
    }
}
