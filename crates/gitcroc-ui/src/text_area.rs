//! A text area Tab leaves (UI-22): gpui-component's text area and code
//! editor take Tab and Shift+Tab to indent, which would trap the keyboard
//! in a commit message or the conflict editor. In rc.1 Tab moved on from
//! them, as it does from any field; [`tab_moves_on`] keeps that, and the
//! conflict editor binds the same keys.

use std::rc::Rc;

use gpui_kit::{
    App, InteractiveElement, IntoElement, KeyBinding, KeyBindingContextPredicate, ParentElement,
    Styled, Window, actions, div,
};

actions!(text_area, [FocusNext, FocusPrevious]);

/// The key context around a text area.
const CONTEXT: &str = "GitcrocTextArea";

/// Tab and Shift+Tab in a text area and in the conflict editor, over their
/// own.
pub fn bind(cx: &mut App) {
    for around in [CONTEXT, crate::conflict::CONTEXT] {
        let context = KeyBindingContextPredicate::parse(&format!("{around} > Input"))
            .ok()
            .map(Rc::new);
        let bind = |keys: &str, action: Box<dyn gpui_kit::Action>, cx: &mut App| {
            KeyBinding::load(keys, action, context.clone(), false, None, cx).ok()
        };
        let bindings = [
            bind("tab", Box::new(FocusNext), cx),
            bind("shift-tab", Box::new(FocusPrevious), cx),
        ];
        cx.bind_keys(bindings.into_iter().flatten());
    }
}

pub fn focus_next(_: &FocusNext, window: &mut Window, cx: &mut App) {
    window.focus_next(cx);
}

pub fn focus_previous(_: &FocusPrevious, window: &mut Window, cx: &mut App) {
    window.focus_prev(cx);
}

/// `area`, a text area, with Tab and Shift+Tab moving the focus on.
pub fn tab_moves_on(area: impl IntoElement) -> impl IntoElement + ParentElement + Styled {
    div()
        .key_context(CONTEXT)
        .on_action(focus_next)
        .on_action(focus_previous)
        .child(area)
}
