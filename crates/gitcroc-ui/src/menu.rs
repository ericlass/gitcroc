//! A menu over the window, opened at a point (UI-22): the "+" menu below
//! its button, a context menu at the pointer or below its row. The first
//! item that can be chosen has the focus; Up, Down, Home and End move
//! between the items, wrapping around; Enter and Space choose; Escape and a
//! click outside close it. The menu says what was chosen and its owner acts, as rc.1's
//! menus did. gpui-component's popup menu opens only from its own
//! triggers, and Shift+F10 must open these too.
//!
//! An item can have a second menu beside it, such as Add to group's list
//! of groups, as rc.1's tree menus had: a click, Enter or Right opens it
//! with its first item focused, and Escape or Left returns to the item.

use std::cell::Cell;
use std::rc::Rc;

use gpui_kit::prelude::FluentBuilder;
use gpui_kit::{
    Anchor, App, Bounds, Context, Div, ElementId, EventEmitter, FocusHandle, Focusable,
    InteractiveElement, IntoElement, KeyBinding, KeyBindingContextPredicate, MouseButton,
    MouseDownEvent, ParentElement, Pixels, Point, Render, Role, SharedString,
    StatefulInteractiveElement, Styled, TestSupportExt, Window, actions, anchored, canvas,
    deferred, div, px, svg,
};

use crate::theme::ActiveTokens;
use crate::theme::tokens::Tokens;

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
        /// Closes the menu, or the second menu beside an item.
        Dismiss,
        /// Opens the second menu of the item with the focus.
        OpenSubmenu,
        /// Closes the second menu, back to its item.
        CloseSubmenu,
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
        bind("right", Box::new(OpenSubmenu), cx),
        bind("left", Box::new(CloseSubmenu), cx),
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
    /// The items of the second menu it opens; empty for an item that is
    /// chosen itself.
    pub submenu: Vec<MenuItem>,
}

impl MenuItem {
    pub fn new(text: impl Into<SharedString>) -> Self {
        Self {
            text: text.into(),
            label: None,
            description: None,
            separated: false,
            disabled: false,
            submenu: Vec::new(),
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

    /// Opens `items` beside it instead of being chosen.
    pub fn submenu(mut self, items: Vec<MenuItem>) -> Self {
        self.submenu = items;
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
    /// Item `.1` of the second menu of item `.0` was chosen.
    ChosenIn(usize, usize),
    /// It was closed without a choice.
    Dismissed,
}

/// The second menu beside item `at`, with its items' focus.
struct Submenu {
    at: usize,
    focus: Vec<FocusHandle>,
}

/// An open menu.
pub struct Menu {
    label: SharedString,
    items: Vec<MenuItem>,
    focus: Vec<FocusHandle>,
    fallback: FocusHandle,
    /// The menu's corner `anchor` is at `at`, in window coordinates.
    at: Point<Pixels>,
    anchor: Anchor,
    /// The open second menu, if any.
    sub: Option<Submenu>,
    /// Where the second menu was drawn, so a click in it is not outside.
    sub_bounds: Rc<Cell<Option<Bounds<Pixels>>>>,
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
            fallback: cx.focus_handle(),
            label: label.into(),
            items,
            focus,
            at,
            anchor,
            sub: None,
            sub_bounds: Rc::default(),
        }
    }

    /// Gives the focus to the first item that can be chosen.
    pub fn focus_first(&self, window: &mut Window, cx: &mut App) {
        if let Some(at) = self.first_enabled() {
            self.select(at, window, cx);
        } else {
            window.focus(&self.fallback, cx);
        }
    }

    fn first_enabled(&self) -> Option<usize> {
        self.items.iter().position(|item| !item.disabled)
    }

    /// The items and focus of the list the focus is in: the second menu's
    /// while it has the focus, else the menu's own.
    fn list(&self, window: &Window) -> (&[MenuItem], &[FocusHandle]) {
        match &self.sub {
            Some(sub) if sub.focus.iter().any(|focus| focus.is_focused(window)) => {
                let items = self
                    .items
                    .get(sub.at)
                    .map_or(&[][..], |item| item.submenu.as_slice());
                (items, &sub.focus)
            }
            _ => (&self.items, &self.focus),
        }
    }

    /// The item with the focus.
    fn focused(&self, window: &Window) -> Option<usize> {
        self.focus.iter().position(|focus| focus.is_focused(window))
    }

    /// The item of the second menu with the focus, after its item's index.
    fn focused_sub(&self, window: &Window) -> Option<(usize, usize)> {
        let sub = self.sub.as_ref()?;
        let at = sub
            .focus
            .iter()
            .position(|focus| focus.is_focused(window))?;
        Some((sub.at, at))
    }

    fn select(&self, at: usize, window: &mut Window, cx: &mut App) {
        if self.items.get(at).is_some_and(|item| !item.disabled)
            && let Some(focus) = self.focus.get(at)
        {
            window.focus(focus, cx);
        }
    }

    /// Focuses item `at` of the list the focus is in, if it can be chosen.
    fn select_in(&self, at: usize, window: &mut Window, cx: &mut App) {
        let (items, focus) = self.list(window);
        if items.get(at).is_some_and(|item| !item.disabled)
            && let Some(focus) = focus.get(at).cloned()
        {
            window.focus(&focus, cx);
        }
    }

    fn step(&mut self, step: isize, window: &mut Window, cx: &mut Context<Self>) {
        let (items, focus) = self.list(window);
        let enabled: Vec<usize> = items
            .iter()
            .enumerate()
            .filter(|(_, item)| !item.disabled)
            .map(|(at, _)| at)
            .collect();
        if enabled.is_empty() {
            return;
        }
        let current = focus
            .iter()
            .position(|focus| focus.is_focused(window))
            .and_then(|at| enabled.iter().position(|other| *other == at));
        let next = match current {
            Some(at) => enabled[(at as isize + step).rem_euclid(enabled.len() as isize) as usize],
            None if step > 0 => enabled[0],
            None => enabled[enabled.len() - 1],
        };
        self.select_in(next, window, cx);
    }

    fn select_next(&mut self, _: &SelectNext, window: &mut Window, cx: &mut Context<Self>) {
        self.step(1, window, cx);
    }

    fn select_previous(&mut self, _: &SelectPrevious, window: &mut Window, cx: &mut Context<Self>) {
        self.step(-1, window, cx);
    }

    fn select_first(&mut self, _: &SelectFirst, window: &mut Window, cx: &mut Context<Self>) {
        let first = self.list(window).0.iter().position(|item| !item.disabled);
        if let Some(first) = first {
            self.select_in(first, window, cx);
        }
    }

    fn select_last(&mut self, _: &SelectLast, window: &mut Window, cx: &mut Context<Self>) {
        let last = self.list(window).0.iter().rposition(|item| !item.disabled);
        if let Some(last) = last {
            self.select_in(last, window, cx);
        }
    }

    /// Opens the second menu of item `at`, its first item focused.
    fn open_sub(&mut self, at: usize, window: &mut Window, cx: &mut Context<Self>) {
        let Some(item) = self.items.get(at).filter(|item| !item.disabled) else {
            return;
        };
        if item.submenu.is_empty() {
            return;
        }
        let focus: Vec<FocusHandle> = item.submenu.iter().map(|_| cx.focus_handle()).collect();
        let first = item.submenu.iter().position(|item| !item.disabled);
        if let Some(first) = first.and_then(|first| focus.get(first)) {
            window.focus(first, cx);
        }
        self.sub = Some(Submenu { at, focus });
        cx.notify();
    }

    /// Closes the second menu; its item gets the focus back.
    fn close_sub(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(sub) = self.sub.take() {
            self.sub_bounds.set(None);
            self.select(sub.at, window, cx);
            cx.notify();
        }
    }

    fn choose(&mut self, _: &Choose, window: &mut Window, cx: &mut Context<Self>) {
        if let Some((at, sub)) = self.focused_sub(window) {
            let enabled = self
                .items
                .get(at)
                .and_then(|item| item.submenu.get(sub))
                .is_some_and(|item| !item.disabled);
            if enabled {
                cx.emit(MenuEvent::ChosenIn(at, sub));
            }
            return;
        }
        if let Some(at) = self
            .focused(window)
            .filter(|at| self.items.get(*at).is_some_and(|item| !item.disabled))
        {
            self.pick(at, window, cx);
        }
    }

    /// Item `at` was chosen: it opens or closes its second menu, or its
    /// owner acts.
    fn pick(&mut self, at: usize, window: &mut Window, cx: &mut Context<Self>) {
        let opens = self
            .items
            .get(at)
            .is_some_and(|item| !item.submenu.is_empty());
        if !opens {
            cx.emit(MenuEvent::Chosen(at));
        } else if self.sub.as_ref().is_some_and(|sub| sub.at == at) {
            self.close_sub(window, cx);
        } else {
            self.open_sub(at, window, cx);
        }
    }

    fn open_submenu(&mut self, _: &OpenSubmenu, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(at) = self.focused(window) {
            self.open_sub(at, window, cx);
        }
    }

    fn close_submenu(&mut self, _: &CloseSubmenu, window: &mut Window, cx: &mut Context<Self>) {
        if self.focused_sub(window).is_some() {
            self.close_sub(window, cx);
        }
    }

    /// Escape closes the second menu first, then the menu.
    fn dismiss(&mut self, _: &Dismiss, window: &mut Window, cx: &mut Context<Self>) {
        if self.sub.is_some() {
            self.close_sub(window, cx);
        } else {
            cx.emit(MenuEvent::Dismissed);
        }
    }

    /// The second menu beside item `at`, when it is open.
    fn render_sub(
        &self,
        at: usize,
        tokens: &Tokens,
        cx: &mut Context<Self>,
    ) -> Option<impl IntoElement + use<>> {
        let sub = self.sub.as_ref().filter(|sub| sub.at == at)?;
        let item = self.items.get(at)?;
        let rows: Vec<_> = item
            .submenu
            .iter()
            .zip(&sub.focus)
            .enumerate()
            .map(|(index, (entry, focus))| {
                item_row(
                    ElementId::NamedInteger("submenu-item".into(), index as u64),
                    entry,
                    focus,
                    false,
                    tokens,
                )
                .when(!entry.disabled, |row| {
                    row.on_click(cx.listener(move |_, _, _, cx| {
                        cx.emit(MenuEvent::ChosenIn(at, index));
                    }))
                })
            })
            .collect();
        let bounds = self.sub_bounds.clone();
        Some(
            surface(tokens)
                .id("submenu")
                .test_support()
                .role(Role::Menu)
                .aria_label(item.name())
                .occlude()
                .absolute()
                .left_full()
                .top(px(-4.))
                .ml(px(6.))
                .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                .child(
                    canvas(move |drawn, _, _| bounds.set(Some(drawn)), |_, _, _, _| {})
                        .absolute()
                        .size_full(),
                )
                .children(rows),
        )
    }
}

/// A menu's box.
fn surface(tokens: &Tokens) -> Div {
    div()
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
}

/// One item's row; `open` while its second menu is shown.
fn item_row(
    id: ElementId,
    item: &MenuItem,
    focus: &FocusHandle,
    open: bool,
    tokens: &Tokens,
) -> impl ParentElement + Styled + StatefulInteractiveElement + IntoElement + use<> {
    let tokens = *tokens;
    let disabled = item.disabled;
    let opens = !item.submenu.is_empty();
    let row = div()
        .id(id)
        .test_support()
        .track_focus(focus)
        .role(Role::MenuItem)
        .aria_label(item.name())
        .when(opens, |row| row.aria_expanded(open))
        .flex()
        .flex_row()
        .items_center()
        .justify_between()
        .gap_2()
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
        })
        .when(open, |row| row.bg(tokens.bg_3))
        .child(item.text.clone())
        .when(opens, |row| {
            row.child(
                svg()
                    .path(crate::icons::CHEVRON_RIGHT)
                    .size(px(12.))
                    .text_color(tokens.text_secondary),
            )
        });
    match &item.description {
        Some(description) => row.aria_description(description.clone()),
        None => row,
    }
}

impl Focusable for Menu {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.first_enabled()
            .and_then(|at| self.focus.get(at))
            .cloned()
            .unwrap_or_else(|| self.fallback.clone())
    }
}

impl Render for Menu {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let tokens = *cx.tokens();
        let open = self.sub.as_ref().map(|sub| sub.at);
        let items: Vec<_> = (0..self.items.len())
            .map(|at| {
                let item = &self.items[at];
                let row = item_row(
                    ElementId::NamedInteger("menu-item".into(), at as u64),
                    item,
                    &self.focus[at],
                    open == Some(at),
                    &tokens,
                )
                .when(!item.disabled, |row| {
                    row.on_click(cx.listener(move |this, _, window, cx| this.pick(at, window, cx)))
                });
                let separated = item.separated;
                // A line above an item starts a new part of the menu; the
                // second menu opens beside its item.
                div()
                    .relative()
                    .when(separated, |this| {
                        this.mt_1().pt_1().border_t_1().border_color(tokens.border)
                    })
                    .child(row)
                    .children(self.render_sub(at, &tokens, cx))
            })
            .collect();
        let sub_bounds = self.sub_bounds.clone();
        deferred(
            anchored()
                .position(self.at)
                .anchor(self.anchor)
                .snap_to_window_with_margin(px(4.))
                .child(
                    surface(&tokens)
                        .id("menu")
                        .test_support()
                        .track_focus(&self.fallback)
                        .key_context(CONTEXT)
                        .on_action(cx.listener(Self::select_next))
                        .on_action(cx.listener(Self::select_previous))
                        .on_action(cx.listener(Self::select_first))
                        .on_action(cx.listener(Self::select_last))
                        .on_action(cx.listener(Self::choose))
                        .on_action(cx.listener(Self::dismiss))
                        .on_action(cx.listener(Self::open_submenu))
                        .on_action(cx.listener(Self::close_submenu))
                        .role(Role::Menu)
                        .aria_label(self.label.clone())
                        .occlude()
                        .on_mouse_down_out(cx.listener(
                            move |this, event: &MouseDownEvent, _, cx| {
                                // A click in the second menu is not outside.
                                let inside = this.sub.is_some()
                                    && sub_bounds
                                        .get()
                                        .is_some_and(|bounds| bounds.contains(&event.position));
                                if !inside {
                                    cx.emit(MenuEvent::Dismissed);
                                }
                            },
                        ))
                        // A click inside reaches only the menu.
                        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                        .children(items),
                ),
        )
        .with_priority(1)
    }
}
