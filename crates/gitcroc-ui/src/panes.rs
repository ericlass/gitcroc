//! The three panes of one workspace (section 8, UI-19): the tree, then the
//! history with the changes pane docked right of it or below it, as rc.1's
//! `layout/Panes.tsx` and `stores/layout.ts` arrange them. A splitter
//! between two panes resizes one by drag or, focused, by the arrow keys.
//! The window keeps one per open workspace, so each tab shows its own
//! layout (ARCH-9, UI-29); the dock and the sizes are saved in the
//! workspace's pane layout (section 10).
//!
//! When the window is too small for the sizes set, flex layout shrinks the
//! panes to fit, each to its minimum: the pane that fills the rest first,
//! then the history, then the tree. The sizes set are kept, so a larger
//! window shows them again, and a resize starts from the size shown.

use std::cell::Cell;
use std::rc::Rc;

use gitcroc_core::{Dock, PaneLayout, WorkspaceId};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::{
    AnyElement, App, AppContext, Bounds, Context, CursorStyle, DragMoveEvent, Entity, FocusHandle,
    InteractiveElement, IntoElement, KeyBinding, KeyBindingContextPredicate, MouseButton,
    MouseDownEvent, Orientation, ParentElement, Pixels, Render, Role, StatefulInteractiveElement,
    Styled, Subscription, TestSupportExt, Window, actions, canvas, div, px,
};

use crate::changes_pane::{ChangesPane, ToggleDock};
use crate::history_pane::HistoryPane;
use crate::services::Services;
use crate::strings::regions as text;
use crate::theme::ActiveTokens;
use crate::tree_pane::TreePane;

/// A pane's smallest width, as rc.1's `MIN_PANE_WIDTH`.
pub const MIN_PANE_WIDTH: f32 = 200.;
/// The smallest height of the history and of the changes pane below it,
/// as rc.1's `MIN_PANE_HEIGHT`.
pub const MIN_PANE_HEIGHT: f32 = 120.;
/// The sizes until the workspace saves its own; the pane that fills the
/// rest has none.
const DEFAULT_TREE: u32 = 280;
const DEFAULT_HISTORY: u32 = 560;
const DEFAULT_CHANGES: u32 = 320;
/// What an arrow key moves a focused splitter by, as in rc.1.
pub(crate) const KEY_STEP: f32 = 16.;
pub(crate) const SPLITTER: Pixels = px(4.);

/// The key context of a splitter.
pub(crate) const CONTEXT: &str = "GitcrocPaneSplitter";

actions!(
    panes,
    [SplitterLeft, SplitterRight, SplitterUp, SplitterDown]
);

/// The keys of a focused splitter.
pub fn bind(cx: &mut App) {
    let context = KeyBindingContextPredicate::parse(CONTEXT).ok().map(Rc::new);
    let bind = |keys: &str, action: Box<dyn gpui_kit::Action>, cx: &mut App| {
        KeyBinding::load(keys, action, context.clone(), false, None, cx).ok()
    };
    let bindings = [
        bind("left", Box::new(SplitterLeft), cx),
        bind("right", Box::new(SplitterRight), cx),
        bind("up", Box::new(SplitterUp), cx),
        bind("down", Box::new(SplitterDown), cx),
    ];
    cx.bind_keys(bindings.into_iter().flatten());
}

/// The pane a splitter resizes: the tree's or the history's width, or the
/// changes pane's height while it is docked at the bottom.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Pane {
    Tree,
    History,
    Changes,
}

impl Pane {
    /// The splitter beside it resizes a height.
    fn vertical(self) -> bool {
        self == Self::Changes
    }

    fn min(self) -> f32 {
        if self.vertical() {
            MIN_PANE_HEIGHT
        } else {
            MIN_PANE_WIDTH
        }
    }

    fn splitter_id(self) -> &'static str {
        match self {
            Self::Tree => "pane-splitter-tree",
            Self::History => "pane-splitter-history",
            Self::Changes => "pane-splitter-changes",
        }
    }

    fn label(self) -> String {
        text::resize(match self {
            Self::Tree => text::TREE,
            Self::History => text::HISTORY,
            Self::Changes => text::CHANGES,
        })
    }
}

/// The sizes set, in pixels: the tree's width, the history's while the
/// changes pane is docked right, and the changes pane's height while it is
/// docked at the bottom.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PaneSizes {
    pub tree: u32,
    pub history: u32,
    pub changes: u32,
}

impl PaneSizes {
    fn from_layout(layout: &PaneLayout) -> Self {
        Self {
            tree: layout.tree.unwrap_or(DEFAULT_TREE),
            history: layout.history.unwrap_or(DEFAULT_HISTORY),
            changes: layout.changes.unwrap_or(DEFAULT_CHANGES),
        }
    }

    fn set(&mut self, pane: Pane, size: u32) -> bool {
        let field = match pane {
            Pane::Tree => &mut self.tree,
            Pane::History => &mut self.history,
            Pane::Changes => &mut self.changes,
        };
        let changed = *field != size;
        *field = size;
        changed
    }
}

/// Where each pane was drawn last.
#[derive(Default)]
struct Drawn {
    tree: Cell<Bounds<Pixels>>,
    history: Cell<Bounds<Pixels>>,
    changes: Cell<Bounds<Pixels>>,
}

/// A splitter being dragged.
#[derive(Debug, Clone, Copy)]
struct DraggedSplitter(Pane);

/// What follows the pointer while a splitter is dragged: nothing.
struct DraggedSplitterView;

impl Render for DraggedSplitterView {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
    }
}

/// A resize in progress: where the pointer and the pane started, and the
/// most the pane can take.
#[derive(Debug, Clone, Copy)]
struct Resizing {
    pane: Pane,
    start: f32,
    shown: f32,
    max: f32,
}

/// The panes of one workspace.
pub struct Panes {
    services: Services,
    workspace_id: WorkspaceId,
    tree: Entity<TreePane>,
    history: Entity<HistoryPane>,
    changes: Entity<ChangesPane>,
    dock: Dock,
    sizes: PaneSizes,
    tree_splitter: FocusHandle,
    history_splitter: FocusHandle,
    changes_splitter: FocusHandle,
    drawn: Rc<Drawn>,
    resizing: Option<Resizing>,
    saving: bool,
    save_again: bool,
    _dock_watch: Subscription,
}

impl Panes {
    /// The panes as `saved` lays them out.
    pub fn new(
        services: Services,
        workspace_id: WorkspaceId,
        panes: (Entity<TreePane>, Entity<HistoryPane>, Entity<ChangesPane>),
        saved: &PaneLayout,
        cx: &mut Context<Self>,
    ) -> Self {
        let (tree, history, changes) = panes;
        let dock = saved.dock.unwrap_or(Dock::Right);
        changes.update(cx, |changes, cx| changes.set_dock(dock, cx));
        let dock_watch = cx.subscribe(&changes, |this, _, _: &ToggleDock, cx| {
            let next = match this.dock {
                Dock::Right => Dock::Bottom,
                Dock::Bottom => Dock::Right,
            };
            this.set_dock(next, cx);
        });
        Self {
            services,
            workspace_id,
            tree,
            history,
            changes,
            dock,
            sizes: PaneSizes::from_layout(saved),
            tree_splitter: cx.focus_handle(),
            history_splitter: cx.focus_handle(),
            changes_splitter: cx.focus_handle(),
            drawn: Rc::new(Drawn::default()),
            resizing: None,
            saving: false,
            save_again: false,
            _dock_watch: dock_watch,
        }
    }

    pub fn dock(&self) -> Dock {
        self.dock
    }

    pub fn sizes(&self) -> PaneSizes {
        self.sizes
    }

    /// Docks the changes pane right or at the bottom (UI-19), and saves it.
    pub fn set_dock(&mut self, dock: Dock, cx: &mut Context<Self>) {
        if self.dock == dock {
            return;
        }
        self.dock = dock;
        self.changes
            .update(cx, |changes, cx| changes.set_dock(dock, cx));
        self.save(cx);
        cx.notify();
    }

    /// The size `pane` was drawn at: a width, or the changes pane's height.
    fn shown(&self, pane: Pane) -> f32 {
        let drawn = &self.drawn;
        f32::from(match pane {
            Pane::Tree => drawn.tree.get().size.width,
            Pane::History => drawn.history.get().size.width,
            Pane::Changes => drawn.changes.get().size.height,
        })
    }

    /// The most `pane` can take now: what the pane that gives way has above
    /// its minimum. The changes pane fills the rest docked right, and the
    /// history above it docked at the bottom.
    fn max(&self, pane: Pane) -> f32 {
        let drawn = &self.drawn;
        let (changes, history) = (drawn.changes.get().size, drawn.history.get().size);
        let spare = match pane {
            Pane::Tree | Pane::History => f32::from(changes.width) - MIN_PANE_WIDTH,
            Pane::Changes => f32::from(history.height) - MIN_PANE_HEIGHT,
        };
        (self.shown(pane) + spare.max(0.)).max(pane.min())
    }

    /// Sets `pane` to `size`, within its minimum and `max`.
    fn resize(&mut self, pane: Pane, size: f32, max: f32, cx: &mut Context<Self>) -> bool {
        let size = size.min(max).max(pane.min()).round() as u32;
        let changed = self.sizes.set(pane, size);
        if changed {
            cx.notify();
        }
        changed
    }

    fn start_resize(&mut self, pane: Pane, event: &MouseDownEvent) {
        let position = event.position;
        self.resizing = Some(Resizing {
            pane,
            start: f32::from(if pane.vertical() {
                position.y
            } else {
                position.x
            }),
            shown: self.shown(pane),
            max: self.max(pane),
        });
    }

    fn drag(&mut self, event: &DragMoveEvent<DraggedSplitter>, cx: &mut Context<Self>) {
        let DraggedSplitter(pane) = *event.drag(cx);
        let Some(resizing) = self.resizing.filter(|resizing| resizing.pane == pane) else {
            return;
        };
        let position = event.event.position;
        // The changes pane is below its splitter: moving up makes it taller.
        let moved = if pane.vertical() {
            resizing.start - f32::from(position.y)
        } else {
            f32::from(position.x) - resizing.start
        };
        if self.resize(pane, resizing.shown + moved, resizing.max, cx) {
            self.save(cx);
        }
    }

    /// The pane whose splitter has the focus.
    fn focused(&self, window: &Window) -> Option<Pane> {
        [
            (Pane::Tree, &self.tree_splitter),
            (Pane::History, &self.history_splitter),
            (Pane::Changes, &self.changes_splitter),
        ]
        .into_iter()
        .find(|(_, focus)| focus.is_focused(window))
        .map(|(pane, _)| pane)
    }

    /// An arrow key on a focused splitter: right makes a width larger, up
    /// the changes pane's height.
    fn key_resize(&mut self, vertical: bool, step: f32, window: &Window, cx: &mut Context<Self>) {
        let Some(pane) = self
            .focused(window)
            .filter(|pane| pane.vertical() == vertical)
        else {
            return;
        };
        cx.stop_propagation();
        let max = self.max(pane);
        if self.resize(pane, self.shown(pane) + step, max, cx) {
            self.save(cx);
        }
    }

    fn splitter_left(&mut self, _: &SplitterLeft, window: &mut Window, cx: &mut Context<Self>) {
        self.key_resize(false, -KEY_STEP, window, cx);
    }

    fn splitter_right(&mut self, _: &SplitterRight, window: &mut Window, cx: &mut Context<Self>) {
        self.key_resize(false, KEY_STEP, window, cx);
    }

    fn splitter_up(&mut self, _: &SplitterUp, window: &mut Window, cx: &mut Context<Self>) {
        self.key_resize(true, KEY_STEP, window, cx);
    }

    fn splitter_down(&mut self, _: &SplitterDown, window: &mut Window, cx: &mut Context<Self>) {
        self.key_resize(true, -KEY_STEP, window, cx);
    }

    /// Saves the dock and the sizes with the workspace's pane layout; the
    /// history's columns are kept as they are. A drag asks many times; only
    /// one save runs, and one more follows it with the latest sizes.
    fn save(&mut self, cx: &mut Context<Self>) {
        self.save_again = true;
        if !self.saving {
            self.start_save(cx);
        }
    }

    fn start_save(&mut self, cx: &mut Context<Self>) {
        self.save_again = false;
        let Some(view) = self
            .services
            .app()
            .workspace_get()
            .open
            .into_iter()
            .find(|view| view.id == self.workspace_id)
        else {
            return;
        };
        let layout = PaneLayout {
            dock: Some(self.dock),
            tree: Some(self.sizes.tree),
            history: Some(self.sizes.history),
            changes: Some(self.sizes.changes),
            ..view.layout
        };
        self.saving = true;
        let workspace = self.workspace_id.clone();
        let call = self
            .services
            .with_app(move |app| async move { app.workspace_set_layout(workspace, layout).await });
        cx.spawn(async move |this, cx| {
            // A read-only workspace refuses it: the layout then lasts this
            // session only, as in rc.1.
            let _ = call.await;
            let _ = this.update(cx, |this, cx| {
                this.saving = false;
                if this.save_again {
                    this.start_save(cx);
                }
            });
        })
        .detach();
    }

    fn splitter(&self, pane: Pane, window: &Window, cx: &mut Context<Self>) -> AnyElement {
        let tokens = *cx.tokens();
        let focus = match pane {
            Pane::Tree => &self.tree_splitter,
            Pane::History => &self.history_splitter,
            Pane::Changes => &self.changes_splitter,
        };
        let focused = focus.is_focused(window);
        let vertical = pane.vertical();
        div()
            .id(pane.splitter_id())
            .test_support()
            .track_focus(focus)
            .tab_stop(true)
            .key_context(CONTEXT)
            .on_action(cx.listener(Self::splitter_left))
            .on_action(cx.listener(Self::splitter_right))
            .on_action(cx.listener(Self::splitter_up))
            .on_action(cx.listener(Self::splitter_down))
            .role(Role::Splitter)
            .aria_label(pane.label())
            .aria_orientation(if vertical {
                Orientation::Horizontal
            } else {
                Orientation::Vertical
            })
            .aria_numeric_value(f64::from(self.shown(pane).round()))
            .aria_min_numeric_value(f64::from(pane.min()))
            .flex_shrink_0()
            .bg(tokens.bg_2)
            .border_color(tokens.border)
            .when(vertical, |splitter| {
                splitter
                    .h(SPLITTER)
                    .w_full()
                    .border_t_1()
                    .border_b_1()
                    .cursor(CursorStyle::ResizeUpDown)
            })
            .when(!vertical, |splitter| {
                splitter
                    .w(SPLITTER)
                    .h_full()
                    .border_l_1()
                    .border_r_1()
                    .cursor(CursorStyle::ResizeLeftRight)
            })
            .hover(move |style| style.bg(tokens.accent))
            .when(focused, |splitter| splitter.bg(tokens.accent))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, event, _, _| this.start_resize(pane, event)),
            )
            .on_drag(DraggedSplitter(pane), |_, _, _, cx| {
                cx.new(|_| DraggedSplitterView)
            })
            .into_any_element()
    }
}

/// A pane's box, which notes where it was drawn in `cell`.
fn pane(
    view: impl IntoElement,
    drawn: Rc<Drawn>,
    cell: fn(&Drawn) -> &Cell<Bounds<Pixels>>,
) -> gpui_kit::Div {
    div()
        .relative()
        .overflow_hidden()
        .flex()
        .flex_col()
        .child(view)
        .child(
            canvas(
                move |bounds, _, _| cell(&drawn).set(bounds),
                |_, _, _, _| {},
            )
            .absolute()
            .size_full(),
        )
}

impl Render for Panes {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let min_width = px(MIN_PANE_WIDTH);
        let min_height = px(MIN_PANE_HEIGHT);
        let tree = pane(self.tree.clone(), self.drawn.clone(), |drawn| &drawn.tree)
            .w(px(self.sizes.tree as f32))
            .min_w(min_width)
            .flex_shrink(1.);
        let history = pane(self.history.clone(), self.drawn.clone(), |drawn| {
            &drawn.history
        });
        let changes = pane(self.changes.clone(), self.drawn.clone(), |drawn| {
            &drawn.changes
        });
        // Beside the tree: the history and the changes pane, which keep
        // their minimum before the tree gives way.
        let rest = match self.dock {
            Dock::Right => div()
                .flex_1()
                .min_w(min_width * 2. + SPLITTER)
                .flex()
                .flex_row()
                .child(
                    history
                        .w(px(self.sizes.history as f32))
                        .min_w(min_width)
                        .flex_shrink(1.),
                )
                .child(self.splitter(Pane::History, window, cx))
                .child(changes.flex_1().min_w(min_width)),
            Dock::Bottom => div()
                .flex_1()
                .min_w(min_width)
                .flex()
                .flex_col()
                .child(history.flex_1().min_h(min_height))
                .child(self.splitter(Pane::Changes, window, cx))
                .child(
                    changes
                        .h(px(self.sizes.changes as f32))
                        .min_h(min_height)
                        .flex_shrink(1.),
                ),
        };
        div()
            .id("panes")
            .on_drag_move::<DraggedSplitter>(cx.listener(
                |this, event: &DragMoveEvent<DraggedSplitter>, _, cx| this.drag(event, cx),
            ))
            .size_full()
            .flex()
            .flex_row()
            .overflow_hidden()
            .child(tree)
            .child(self.splitter(Pane::Tree, window, cx))
            .child(rest)
    }
}
