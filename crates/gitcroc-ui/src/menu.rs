//! A menu over the window, opened at a point (UI-22): the "+" menu below
//! its button, a context menu at the pointer or below its row. The first
//! item that can be chosen has the focus; Up, Down, Home and End move
//! between the items, wrapping around; Enter and Space choose; Escape and a
//! click outside close it. The menu says what was chosen and its owner acts, as rc.1's
//! menus did. gpui-component's popup menu opens only from its own
//! triggers, and Shift+F10 must open these too.

use std::rc::Rc;

use gpui_kit::prelude::FluentBuilder;
use gpui_kit::{
    Anchor, App, Context, ElementId, EventEmitter, FocusHandle, Focusable, InteractiveElement,
    IntoElement, KeyBinding, KeyBindingContextPredicate, MouseButton, ParentElement, Pixels, Point,
    Render, Role, SharedString, StatefulInteractiveElement, Styled, TestSupportExt, Window,
    actions, anchored, deferred, div, px,
};

use crate::theme::ActiveTokens;

actions!(
    menu,
    [
        /// Moves the focus to the next item, from the last to the first.
        SelectNext,
        /// Moves the focus to the previous item, from the first to the last.
        SelectPrevious,
        SelectFirst,
        SelectLast,
        /// Chooses the item with the focus.
        Choose,
        /// Closes the menu.
        Dismiss,
    ]
);

/// The key context of an open menu.
const CONTEXT: &str = "GitcrocMenu";

/// The keys of an open menu, which belong to it alone.
pub fn bind(cx: &mut App) {
    let context = KeyBindingContextPredicate::parse(CONTEXT).ok().map(Rc::new);
    let bind = |keys: &str, action: Box<dyn gpui_kit::Action>, cx: &mut App| {
        KeyBinding::load(keys, action, context.clone(), false, None, cx).ok()
    };
    let bindings = [
        bind("down", Box::new(SelectNext), cx),
        bind("up", Box::new(SelectPrevious), cx),
        bind("home", Box::new(SelectFirst), cx),
        bind("end", Box::new(SelectLast), cx),
        bind("enter", Box::new(Choose), cx),
        bind("space", Box::new(Choose), cx),
        bind("escape", Box::new(Dismiss), cx),
    ];
    cx.bind_keys(bindings.into_iter().flatten());
}

/// One item of a menu.
#[derive(Debug, Clone)]
pub struct MenuItem {
    /// What the item shows.
    pub text: SharedString,
    /// Its name for screen readers and tests, when it says more than the
    /// text, such as "Delete archive…" for "Delete…".
    pub label: Option<SharedString>,
    /// Said after the name, such as why a workspace opens read-only.
    pub description: Option<SharedString>,
    /// A line above it separates it from the items before.
    pub separated: bool,
    /// It is shown, but choosing it does nothing (UI-36).
    pub disabled: bool,
}

impl MenuItem {
    pub fn new(text: impl Into<SharedString>) -> Self {
        Self {
            text: text.into(),
            label: None,
            description: None,
            separated: false,
            disabled: false,
        }
    }

    pub fn label(mut self, label: impl Into<SharedString>) -> Self {
        self.label = Some(label.into());
        self
    }

    pub fn description(mut self, description: Option<SharedString>) -> Self {
        self.description = description;
        self
    }

    pub fn separated(mut self) -> Self {
        self.separated = true;
        self
    }

    /// Shows the item, but choosing it does nothing.
    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }

    /// Its accessible name.
    pub fn name(&self) -> SharedString {
        self.label.clone().unwrap_or_else(|| self.text.clone())
    }
}

/// What happened to an open menu.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MenuEvent {
    /// The item at this index was chosen.
    Chosen(usize),
    /// It was closed without a choice.
    Dismissed,
}

/// An open menu.
pub struct Menu {
    label: SharedString,
    items: Vec<MenuItem>,
    focus: Vec<FocusHandle>,
    /// The menu's corner `anchor` is at `at`, in window coordinates.
    at: Point<Pixels>,
    anchor: Anchor,
}

impl EventEmitter<MenuEvent> for Menu {}

impl Menu {
    pub fn new(
        label: impl Into<SharedString>,
        items: Vec<MenuItem>,
        at: Point<Pixels>,
        anchor: Anchor,
        cx: &mut Context<Self>,
    ) -> Self {
        let focus = items.iter().map(|_| cx.focus_handle()).collect();
        Self {
            label: label.into(),
            items,
            focus,
            at,
            anchor,
        }
    }

    /// Gives the focus to the first item that can be chosen.
    pub fn focus_first(&self, window: &mut Window, cx: &mut App) {
        if let Some(at) = self.first_enabled() {
            self.select(at, window, cx);
        }
    }

    fn first_enabled(&self) -> Option<usize> {
        self.items.iter().position(|item| !item.disabled)
    }

    fn last_enabled(&self) -> Option<usize> {
        self.items.iter().rposition(|item| !item.disabled)
    }

    /// The item with the focus.
    fn focused(&self, window: &Window) -> Option<usize> {
        self.focus.iter().position(|focus| focus.is_focused(window))
    }

    fn select(&self, at: usize, window: &mut Window, cx: &mut App) {
        if self.items.get(at).is_some_and(|item| !item.disabled)
            && let Some(focus) = self.focus.get(at)
        {
            window.focus(focus, cx);
        }
    }

    fn step(&mut self, step: isize, window: &mut Window, cx: &mut Context<Self>) {
        let enabled: Vec<usize> = self
            .items
            .iter()
            .enumerate()
            .filter(|(_, item)| !item.disabled)
            .map(|(at, _)| at)
            .collect();
        if enabled.is_empty() {
            return;
        }
        let current = self
            .focused(window)
            .and_then(|at| enabled.iter().position(|other| *other == at));
        let next = match current {
            Some(at) => enabled[(at as isize + step).rem_euclid(enabled.len() as isize) as usize],
            None if step > 0 => enabled[0],
            None => enabled[enabled.len() - 1],
        };
        self.select(next, window, cx);
    }

    fn select_next(&mut self, _: &SelectNext, window: &mut Window, cx: &mut Context<Self>) {
        self.step(1, window, cx);
    }

    fn select_previous(&mut self, _: &SelectPrevious, window: &mut Window, cx: &mut Context<Self>) {
        self.step(-1, window, cx);
    }

    fn select_first(&mut self, _: &SelectFirst, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(first) = self.first_enabled() {
            self.select(first, window, cx);
        }
    }

    fn select_last(&mut self, _: &SelectLast, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(last) = self.last_enabled() {
            self.select(last, window, cx);
        }
    }

    fn choose(&mut self, _: &Choose, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(at) = self
            .focused(window)
            .filter(|at| self.items.get(*at).is_some_and(|item| !item.disabled))
        {
            cx.emit(MenuEvent::Chosen(at));
        }
    }

    fn dismiss(&mut self, _: &Dismiss, _: &mut Window, cx: &mut Context<Self>) {
        cx.emit(MenuEvent::Dismissed);
    }
}

impl Focusable for Menu {
    fn focus_handle(&self, cx: &App) -> FocusHandle {
        self.focus
            .first()
            .cloned()
            .unwrap_or_else(|| cx.focus_handle())
    }
}

impl Render for Menu {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let tokens = *cx.tokens();
        let items = self
            .items
            .iter()
            .zip(&self.focus)
            .enumerate()
            .map(|(at, (item, focus))| {
                let disabled = item.disabled;
                let row = div()
                    .id(ElementId::NamedInteger("menu-item".into(), at as u64))
                    .test_support()
                    .track_focus(focus)
                    .role(Role::MenuItem)
                    .aria_label(item.name())
                    .px_3()
                    .py_1()
                    .rounded(px(4.))
                    .when(disabled, |row| {
                        row.opacity(0.5)
                            .text_color(tokens.text_secondary)
                            .cursor_default()
                    })
                    .when(!disabled, |row| {
                        row.cursor_pointer()
                            .hover(move |style| style.bg(tokens.bg_3))
                            .focus(move |style| style.bg(tokens.selection_bg))
                            .on_click(
                                cx.listener(move |_, _, _, cx| cx.emit(MenuEvent::Chosen(at))),
                            )
                    })
                    .child(item.text.clone());
                let row = match &item.description {
                    Some(description) => row.aria_description(description.clone()),
                    None => row,
                };
                // A line above an item starts a new part of the menu.
                div()
                    .when(item.separated, |this| {
                        this.mt_1().pt_1().border_t_1().border_color(tokens.border)
                    })
                    .child(row)
            });
        deferred(
            anchored()
                .position(self.at)
                .anchor(self.anchor)
                .snap_to_window_with_margin(px(4.))
                .child(
                    div()
                        .id("menu")
                        .test_support()
                        .key_context(CONTEXT)
                        .on_action(cx.listener(Self::select_next))
                        .on_action(cx.listener(Self::select_previous))
                        .on_action(cx.listener(Self::select_first))
                        .on_action(cx.listener(Self::select_last))
                        .on_action(cx.listener(Self::choose))
                        .on_action(cx.listener(Self::dismiss))
                        .role(Role::Menu)
                        .aria_label(self.label.clone())
                        .occlude()
                        .on_mouse_down_out(cx.listener(|_, _, _, cx| cx.emit(MenuEvent::Dismissed)))
                        // A click inside reaches only the menu.
                        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                        .flex()
                        .flex_col()
                        .min_w(px(200.))
                        .p_1()
                        .bg(tokens.bg_1)
                        .text_color(tokens.text_primary)
                        .border_1()
                        .border_color(tokens.border)
                        .rounded(px(6.))
                        .shadow_md()
                        .children(items),
                ),
        )
        .with_priority(1)
    }
}
