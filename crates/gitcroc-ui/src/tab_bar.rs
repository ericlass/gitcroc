//! The workspace tabs (UI-27, UI-28), a port of `ui/src/layout/TabBar.tsx`:
//! one tab per open workspace in the core's order, in the title bar
//! (UI-35). A click activates a tab; tabs reorder by dragging, and the
//! window's Ctrl/Cmd+Shift+PageUp/PageDown move the active one (UI-22).
//! The arrow keys, Home and End move between tabs, Delete closes one, and
//! Shift+F10 or the context menu key opens its menu: Rename, Export
//! workspace (DATA-2) and Close. Each tab has a close button for the
//! mouse, outside the Tab order. "+" opens the workspace menu. The core
//! keeps the tabs; this sends what the user did. A tab shows a badge while
//! it has jobs, and a marker for a failed job not seen yet (UI-32).

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;

use gitcroc_core::WorkspaceId;
use gpui_component::button::{Button, ButtonVariants};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::{
    Anchor, AnyElement, App, AppContext, Bounds, ClickEvent, Context, ElementId, Entity,
    EventEmitter, FocusHandle, InteractiveElement, IntoElement, KeyBinding,
    KeyBindingContextPredicate, MouseButton, MouseDownEvent, ParentElement, Pixels, Point, Render,
    Role, SharedString, StatefulInteractiveElement, Styled, Subscription, Task, TestSupportExt,
    Window, actions, canvas, div, point, px,
};

use crate::jobs::{Jobs, Marks};
use crate::menu::{Menu, MenuEvent, MenuItem};
use crate::strings::{tabs as text, workspaces as workspace_text};
use crate::theme::ActiveTokens;
use crate::theme::tokens::Tokens;
use crate::workspace_menu::{Choice, entries};
use crate::workspaces::Workspaces;

actions!(
    tab_bar,
    [
        /// Activates the tab left of the focused one.
        ActivateLeft,
        /// Activates the tab right of the focused one.
        ActivateRight,
        ActivateFirst,
        ActivateLast,
        /// Closes the focused tab, as its close button does.
        CloseFocused,
        /// Opens the focused tab's menu.
        OpenTabMenu,
    ]
);

/// The key context of the tab list.
const CONTEXT: &str = "GitcrocTabs";

/// The keys of a focused tab.
pub fn bind(cx: &mut App) {
    let context = KeyBindingContextPredicate::parse(CONTEXT).ok().map(Rc::new);
    let bind = |keys: &str, action: Box<dyn gpui_kit::Action>, cx: &mut App| {
        KeyBinding::load(keys, action, context.clone(), false, None, cx).ok()
    };
    let bindings = [
        bind("left", Box::new(ActivateLeft), cx),
        bind("right", Box::new(ActivateRight), cx),
        bind("home", Box::new(ActivateFirst), cx),
        bind("end", Box::new(ActivateLast), cx),
        bind("delete", Box::new(CloseFocused), cx),
        bind("shift-f10", Box::new(OpenTabMenu), cx),
        bind("menu", Box::new(OpenTabMenu), cx),
    ];
    cx.bind_keys(bindings.into_iter().flatten());
}

/// What the tab bar asks of the window.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TabBarEvent {
    /// An entry of a menu that the window carries out: a dialog, an
    /// export or an import.
    Chose(Choice),
    /// The core refused a tab change; this says why.
    Refused(SharedString),
}

/// A tab being dragged.
#[derive(Debug, Clone)]
struct DraggedTab {
    id: WorkspaceId,
    name: SharedString,
}

/// What follows the pointer while a tab is dragged.
struct DraggedTabView(SharedString);

impl Render for DraggedTabView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let tokens = cx.tokens();
        div()
            .px_3()
            .py_1()
            .bg(tokens.bg_3)
            .text_color(tokens.text_primary)
            .border_1()
            .border_color(tokens.border)
            .rounded(px(4.))
            .child(self.0.clone())
    }
}

/// An open menu and what its items do.
struct OpenMenu {
    menu: Entity<Menu>,
    choices: Vec<Choice>,
    /// Where the focus goes when it closes without a dialog.
    opener: FocusHandle,
    _events: Subscription,
}

/// The tab bar.
pub struct TabBar {
    workspaces: Entity<Workspaces>,
    /// Each tab's job marks come from the window's jobs.
    jobs: Entity<Jobs>,
    tab_focus: HashMap<WorkspaceId, FocusHandle>,
    plus_focus: FocusHandle,
    /// Where each tab and "+" were drawn, for the menus below them.
    tab_bounds: Rc<RefCell<HashMap<WorkspaceId, Bounds<Pixels>>>>,
    plus_bounds: Rc<Cell<Bounds<Pixels>>>,
    menu: Option<OpenMenu>,
    _workspaces: Subscription,
    _jobs: Subscription,
}

impl EventEmitter<TabBarEvent> for TabBar {}

impl TabBar {
    pub fn new(workspaces: Entity<Workspaces>, jobs: Entity<Jobs>, cx: &mut Context<Self>) -> Self {
        let observed = cx.observe(&workspaces, |_, _, cx| cx.notify());
        let jobs_changed = cx.observe(&jobs, |_, _, cx| cx.notify());
        Self {
            workspaces,
            jobs,
            tab_focus: HashMap::new(),
            plus_focus: cx.focus_handle(),
            tab_bounds: Rc::default(),
            plus_bounds: Rc::default(),
            menu: None,
            _workspaces: observed,
            _jobs: jobs_changed,
        }
    }

    /// Whether a menu of the tab bar is open.
    pub fn menu_open(&self) -> bool {
        self.menu.is_some()
    }

    /// Gives the focus to the active tab, or to "+" when no tab is open:
    /// where F6 lands in the tab bar.
    pub fn focus(&mut self, window: &mut Window, cx: &mut App) {
        let active = self.workspaces.read(cx).tabs().active.clone();
        match active {
            Some(id) => {
                let focus = self.tab_focus(&id, cx);
                window.focus(&focus, cx);
            }
            None => window.focus(&self.plus_focus, cx),
        }
    }

    fn tab_focus(&mut self, id: &WorkspaceId, cx: &mut App) -> FocusHandle {
        self.tab_focus
            .entry(id.clone())
            .or_insert_with(|| cx.focus_handle())
            .clone()
    }

    /// The open workspaces' ids, in tab order.
    fn order(&self, cx: &App) -> Vec<WorkspaceId> {
        self.workspaces
            .read(cx)
            .tabs()
            .open
            .iter()
            .map(|view| view.id.clone())
            .collect()
    }

    /// The tab that has the focus.
    fn focused_tab(&self, window: &Window) -> Option<WorkspaceId> {
        self.tab_focus
            .iter()
            .find(|(_, focus)| focus.is_focused(window))
            .map(|(id, _)| id.clone())
    }

    /// Sends a tab change; a refusal shows below the title bar, and the
    /// focus then goes to tab `focus`, if given.
    fn send(
        &mut self,
        sent: Task<Result<(), SharedString>>,
        focus: Option<WorkspaceId>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        cx.spawn_in(window, async move |this, cx| {
            let result = sent.await;
            let _ = this.update_in(cx, |this, window, cx| match result {
                Ok(()) => {
                    if let Some(id) = focus {
                        let handle = this.tab_focus(&id, cx);
                        window.focus(&handle, cx);
                    }
                }
                Err(error) => cx.emit(TabBarEvent::Refused(error)),
            });
        })
        .detach();
    }

    /// Activates tab `id` and gives it the focus.
    pub fn activate(&mut self, id: WorkspaceId, window: &mut Window, cx: &mut Context<Self>) {
        let sent = Workspaces::activate(&self.workspaces, id.clone(), cx);
        self.send(sent, Some(id), window, cx);
    }

    /// Activates the tab `step` places from the active one, if there is one.
    pub fn activate_beside_active(
        &mut self,
        step: isize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(active) = self.workspaces.read(cx).tabs().active.clone() else {
            return;
        };
        self.activate_beside(&active, step, window, cx);
    }

    fn activate_beside(
        &mut self,
        id: &WorkspaceId,
        step: isize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let order = self.order(cx);
        let next = order
            .iter()
            .position(|open| open == id)
            .and_then(|at| order.get((at as isize + step) as usize));
        if let Some(next) = next.cloned() {
            self.activate(next, window, cx);
        }
    }

    /// Moves the active tab `step` places, keeping the focus on it.
    pub fn move_active(&mut self, step: isize, window: &mut Window, cx: &mut Context<Self>) {
        let Some(active) = self.workspaces.read(cx).tabs().active.clone() else {
            return;
        };
        let sent = Workspaces::move_tab(&self.workspaces, &active, step, cx);
        self.send(sent, Some(active), window, cx);
    }

    /// Closes tab `id`; the workspace file stays (UI-28).
    pub fn close(&mut self, id: WorkspaceId, window: &mut Window, cx: &mut Context<Self>) {
        let sent = Workspaces::close(&self.workspaces, id, cx);
        self.send(sent, None, window, cx);
    }

    /// Drops the dragged tab where tab `target` is.
    fn drop_tab(
        &mut self,
        dragged: &DraggedTab,
        target: &WorkspaceId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if &dragged.id == target {
            return;
        }
        let all = self.order(cx);
        let Some(to) = all.iter().position(|id| id == target) else {
            return;
        };
        let mut order: Vec<WorkspaceId> = all.into_iter().filter(|id| id != &dragged.id).collect();
        order.insert(to.min(order.len()), dragged.id.clone());
        let sent = Workspaces::reorder(&self.workspaces, order, cx);
        self.send(sent, None, window, cx);
    }

    fn on_left(&mut self, _: &ActivateLeft, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(id) = self.focused_tab(window) {
            self.activate_beside(&id, -1, window, cx);
        }
    }

    fn on_right(&mut self, _: &ActivateRight, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(id) = self.focused_tab(window) {
            self.activate_beside(&id, 1, window, cx);
        }
    }

    fn on_first(&mut self, _: &ActivateFirst, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(first) = self.order(cx).first().cloned() {
            self.activate(first, window, cx);
        }
    }

    fn on_last(&mut self, _: &ActivateLast, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(last) = self.order(cx).last().cloned() {
            self.activate(last, window, cx);
        }
    }

    fn on_close_focused(&mut self, _: &CloseFocused, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(id) = self.focused_tab(window) {
            self.close(id, window, cx);
        }
    }

    fn on_open_tab_menu(&mut self, _: &OpenTabMenu, window: &mut Window, cx: &mut Context<Self>) {
        let Some(id) = self.focused_tab(window) else {
            return;
        };
        let bounds = self.tab_bounds.borrow().get(&id).copied();
        let at = bounds.map_or(point(px(0.), px(0.)), |bounds| bounds.bottom_left());
        self.open_tab_menu(id, at, window, cx);
    }

    /// Opens tab `id`'s menu at `at`.
    fn open_tab_menu(
        &mut self,
        id: WorkspaceId,
        at: Point<Pixels>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(name) = self
            .workspaces
            .read(cx)
            .view(&id)
            .map(|view| SharedString::from(view.name.clone()))
        else {
            return;
        };
        let opener = self.tab_focus(&id, cx);
        let items = vec![
            (
                MenuItem::new(text::RENAME),
                Choice::Rename(id.clone(), name),
            ),
            (MenuItem::new(text::EXPORT), Choice::Export(id.clone())),
            (MenuItem::new(text::CLOSE_TAB), Choice::Close(id)),
        ];
        self.open_menu(
            text::ACTIONS,
            items,
            at,
            Anchor::TopLeft,
            opener,
            window,
            cx,
        );
    }

    /// Lists the workspace files anew, then opens the menu below "+".
    fn open_plus_menu(&mut self, _: &ClickEvent, window: &mut Window, cx: &mut Context<Self>) {
        let list = self.workspaces.read(cx).list();
        cx.spawn_in(window, async move |this, cx| {
            let result = list.await;
            let _ = this.update_in(cx, |this, window, cx| {
                let items = match result {
                    Some(Ok(items)) => entries(&items),
                    Some(Err(error)) => {
                        cx.emit(TabBarEvent::Refused(
                            workspace_text::cannot_list(&error.message).into(),
                        ));
                        entries(&[])
                    }
                    None => return,
                };
                let at = this.plus_bounds.get().bottom_right();
                let opener = this.plus_focus.clone();
                this.open_menu(
                    workspace_text::MENU,
                    items,
                    at,
                    Anchor::TopRight,
                    opener,
                    window,
                    cx,
                );
            });
        })
        .detach();
    }

    #[allow(clippy::too_many_arguments)]
    fn open_menu(
        &mut self,
        label: &'static str,
        items: Vec<(MenuItem, Choice)>,
        at: Point<Pixels>,
        anchor: Anchor,
        opener: FocusHandle,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let (items, choices): (Vec<MenuItem>, Vec<Choice>) = items.into_iter().unzip();
        let menu = cx.new(|cx| Menu::new(label, items, at, anchor, cx));
        let events = cx.subscribe_in(&menu, window, |this, _, event, window, cx| {
            this.on_menu(*event, window, cx);
        });
        menu.update(cx, |menu, cx| menu.focus_first(window, cx));
        self.menu = Some(OpenMenu {
            menu,
            choices,
            opener,
            _events: events,
        });
        cx.notify();
    }

    fn on_menu(&mut self, event: MenuEvent, window: &mut Window, cx: &mut Context<Self>) {
        let Some(open) = self.menu.take() else {
            return;
        };
        cx.notify();
        // The focus goes back to what opened the menu, before a dialog an
        // entry opens takes it.
        window.focus(&open.opener, cx);
        let MenuEvent::Chosen(at) = event else {
            return;
        };
        match open.choices.get(at).cloned() {
            Some(Choice::Close(id)) => self.close(id, window, cx),
            Some(Choice::Open(id, name)) => {
                let sent = Workspaces::open(&self.workspaces, id.clone(), cx);
                let sent = cx.spawn(async move |_, _| {
                    sent.await
                        .map_err(|error| workspace_text::cannot_open(&name, &error).into())
                });
                self.send(sent, Some(id), window, cx);
            }
            Some(choice) => cx.emit(TabBarEvent::Chose(choice)),
            None => {}
        }
    }

    fn tab(
        &mut self,
        at: usize,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> Option<impl IntoElement + use<>> {
        let tokens = *cx.tokens();
        let tabs = self.workspaces.read(cx).tabs();
        let view = tabs.open.get(at)?;
        let id = view.id.clone();
        let name: SharedString = view.name.clone().into();
        let active = tabs.active.as_ref() == Some(&id);
        let job_marks = marks(&self.jobs.read(cx).marks(&id), &id, &tokens);
        // Only the active tab is in the Tab order; the arrow keys reach
        // the others.
        let focus = self.tab_focus(&id, cx).tab_stop(active);
        let bounds = self.tab_bounds.clone();
        let (drop_id, close_id, menu_id, click_id, drop_style) = (
            id.clone(),
            id.clone(),
            id.clone(),
            id.clone(),
            tokens.selection_bg,
        );
        let focused = focus.is_focused(window);
        Some(
            div()
                .id(ElementId::Name(format!("tab-{}", id.0).into()))
                .test_support()
                .track_focus(&focus)
                .role(Role::Tab)
                .aria_label(name.clone())
                .aria_selected(active)
                .relative()
                .flex()
                .flex_row()
                .flex_shrink_0()
                .items_center()
                .gap_1()
                .h_full()
                .max_w(px(220.))
                .pl_3()
                .pr_1()
                .border_r_1()
                .border_color(tokens.border)
                .when(!active, |this| {
                    this.text_color(tokens.text_secondary)
                        .hover(move |style| style.bg(tokens.bg_3))
                })
                // The selection mark and the focus ring are drawn over the
                // tab, so its border between tabs keeps its colour.
                .when(active, |this| {
                    this.bg(tokens.bg_1).child(
                        div()
                            .absolute()
                            .left_0()
                            .right_0()
                            .bottom_0()
                            .h(px(2.))
                            .bg(tokens.selection_mark),
                    )
                })
                .when(focused, |this| {
                    this.child(
                        div()
                            .absolute()
                            .inset_0()
                            .border_2()
                            .border_color(tokens.focus_ring),
                    )
                })
                // A press on a tab never moves the window.
                .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                .on_mouse_down(
                    MouseButton::Right,
                    cx.listener(move |this, event: &MouseDownEvent, window, cx| {
                        cx.stop_propagation();
                        this.open_tab_menu(menu_id.clone(), event.position, window, cx);
                    }),
                )
                .on_click(cx.listener(move |this, _, window, cx| {
                    this.activate(click_id.clone(), window, cx);
                }))
                .on_drag(
                    DraggedTab {
                        id: id.clone(),
                        name: name.clone(),
                    },
                    |dragged, _, _, cx| cx.new(|_| DraggedTabView(dragged.name.clone())),
                )
                .drag_over::<DraggedTab>(move |style, _, _, _| style.bg(drop_style))
                .on_drop(cx.listener(move |this, dragged: &DraggedTab, window, cx| {
                    this.drop_tab(dragged, &drop_id, window, cx);
                }))
                .child(
                    canvas(
                        move |drawn, _, _| {
                            bounds.borrow_mut().insert(id, drawn);
                        },
                        |_, _, _, _| {},
                    )
                    .absolute()
                    .size_full(),
                )
                .child(div().flex_1().truncate().child(name.clone()))
                .children(job_marks)
                .child(
                    // The mouse's way to close; Delete and the tab's menu
                    // are the keyboard's (UI-28).
                    div()
                        .id(ElementId::Name(format!("tab-close-{}", close_id.0).into()))
                        .test_support()
                        .aria_label(text::close(&name))
                        .flex()
                        .items_center()
                        .justify_center()
                        .size(px(20.))
                        .rounded(px(4.))
                        .text_color(tokens.text_secondary)
                        .hover(move |style| style.bg(tokens.bg_3))
                        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                        .on_click(cx.listener(move |this, _, window, cx| {
                            cx.stop_propagation();
                            this.close(close_id.clone(), window, cx);
                        }))
                        .child("×"),
                ),
        )
    }
}

/// A tab's badge with the number of its jobs that run or wait, and its
/// marker for a failed job not seen (UI-32).
fn marks(marks: &Marks, id: &WorkspaceId, tokens: &Tokens) -> Vec<AnyElement> {
    let pill = |name: String, label: SharedString, text: SharedString| {
        div()
            .id(ElementId::Name(name.into()))
            .test_support()
            .role(Role::Status)
            .aria_label(label)
            .flex_shrink_0()
            .min_w(px(16.))
            .h(px(16.))
            .px_1()
            .rounded(px(8.))
            .flex()
            .items_center()
            .justify_center()
            .text_xs()
            .text_color(tokens.accent_text)
            .child(text)
    };
    let badge = (marks.active > 0).then(|| {
        pill(
            format!("tab-badge-{}", id.0),
            text::jobs_title(marks.active).into(),
            marks.active.to_string().into(),
        )
        .bg(tokens.accent)
        .into_any_element()
    });
    let marker = marks.unseen.then(|| {
        pill(
            format!("tab-marker-{}", id.0),
            text::FAILED_TITLE.into(),
            text::FAILED.into(),
        )
        .bg(tokens.outcome_failed)
        .font_weight(gpui_kit::FontWeight::SEMIBOLD)
        .into_any_element()
    });
    badge.into_iter().chain(marker).collect()
}

impl Render for TabBar {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let count = self.workspaces.read(cx).tabs().open.len();
        let open: Vec<WorkspaceId> = self.order(cx);
        self.tab_focus.retain(|id, _| open.contains(id));
        self.tab_bounds
            .borrow_mut()
            .retain(|id, _| open.contains(id));
        let tabs: Vec<_> = (0..count)
            .filter_map(|at| self.tab(at, window, cx))
            .collect();
        let plus_bounds = self.plus_bounds.clone();
        div()
            .id("tab-bar")
            .flex()
            .flex_row()
            .flex_shrink(1.)
            .min_w_0()
            .h_full()
            .items_center()
            .child(
                div()
                    .id("tabs")
                    .test_support()
                    .key_context(CONTEXT)
                    .on_action(cx.listener(Self::on_left))
                    .on_action(cx.listener(Self::on_right))
                    .on_action(cx.listener(Self::on_first))
                    .on_action(cx.listener(Self::on_last))
                    .on_action(cx.listener(Self::on_close_focused))
                    .on_action(cx.listener(Self::on_open_tab_menu))
                    .role(Role::TabList)
                    .aria_label(text::LABEL)
                    .flex()
                    .flex_row()
                    .min_w_0()
                    .h_full()
                    .overflow_x_hidden()
                    .children(tabs),
            )
            .child(
                div()
                    .relative()
                    .flex_shrink_0()
                    .px_1()
                    .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                    .child(
                        canvas(move |drawn, _, _| plus_bounds.set(drawn), |_, _, _, _| {})
                            .absolute()
                            .size_full(),
                    )
                    .child(
                        Button::new("tab-add")
                            .ghost()
                            .compact()
                            .label("+")
                            .accessibility_label(text::OPEN)
                            .tooltip(text::OPEN_TITLE)
                            .track_focus(&self.plus_focus)
                            .on_click(cx.listener(Self::open_plus_menu)),
                    ),
            )
            .children(self.menu.as_ref().map(|open| open.menu.clone()))
    }
}
