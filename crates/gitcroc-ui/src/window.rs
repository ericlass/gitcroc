//! The main window: the title bar, the toolbar, the three panes (tree,
//! history, changes) and the status bar, each an empty view for its task to
//! fill. Its state comes from the app (ARCH-6): one task per window receives
//! the app's events and updates it on the main thread, and reads the state
//! again when it fell behind.

use std::collections::HashMap;
use std::rc::Rc;

use gitcroc_app::app::AppEvent;
use gitcroc_core::{PaneLayout, WorkspaceId, WorkspaceTabs};
use gpui_component::resizable::{
    ResizablePanelEvent, ResizableState, h_resizable, resizable_panel,
};
use gpui_component::{ActiveTheme, Theme, window_border};
use gpui_kit::{
    AppContext, Context, Entity, FocusHandle, Focusable, InteractiveElement, IntoElement,
    ParentElement, Pixels, Render, Role, SharedString, StatefulInteractiveElement, Styled,
    Subscription, Task, TestSupportExt, Window, div, px,
};
use tokio::sync::broadcast::error::RecvError;

use crate::keys::{self, Minimize, NextRegion, PreviousRegion, Zoom};
use crate::services::Services;
use crate::strings;
use crate::title_bar::{Platform, TitleBar, WindowControls};

/// The tree's and the history's widths until the workspace saves its own
/// (UI-19); the changes pane takes the rest.
const TREE_WIDTH: Pixels = px(280.);
const HISTORY_WIDTH: Pixels = px(560.);
const TOOLBAR_HEIGHT: Pixels = px(40.);
const STATUS_BAR_HEIGHT: Pixels = px(24.);

/// One of the three panes: empty until its task fills it.
pub struct Pane {
    id: &'static str,
    label: &'static str,
    focus: FocusHandle,
}

impl Pane {
    fn new(id: &'static str, label: &'static str, cx: &mut Context<Self>) -> Self {
        Self {
            id,
            label,
            focus: cx.focus_handle(),
        }
    }
}

impl Focusable for Pane {
    fn focus_handle(&self, _: &gpui_kit::App) -> FocusHandle {
        self.focus.clone()
    }
}

impl Render for Pane {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
            .id(self.id)
            .test_support()
            .track_focus(&self.focus)
            .role(Role::Group)
            .aria_label(self.label)
            .size_full()
    }
}

/// The pane sizes of one workspace (ARCH-9), and the subscription that
/// saves them.
struct Layout {
    state: Entity<ResizableState>,
    tree: Pixels,
    history: Pixels,
    _saves: Subscription,
}

/// The window's root view.
pub struct GitcrocWindow {
    services: Services,
    platform: Platform,
    system_title_bar: bool,
    title_bar: Entity<TitleBar>,
    tree: Entity<Pane>,
    history: Entity<Pane>,
    changes: Entity<Pane>,
    title_focus: FocusHandle,
    toolbar_focus: FocusHandle,
    status_focus: FocusHandle,
    controls: Rc<dyn WindowControls>,
    tabs: WorkspaceTabs,
    /// Keyed by workspace, `None` while none is open.
    layouts: HashMap<Option<WorkspaceId>, Layout>,
    _bridge: Task<()>,
    _appearance: Subscription,
}

impl GitcrocWindow {
    /// The window's view. `system_title_bar` is the setting the window was
    /// opened with.
    pub fn new(
        services: Services,
        controls: Rc<dyn WindowControls>,
        platform: Platform,
        system_title_bar: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        // In memory, loaded at start: what the first frame shows.
        let tabs = services.app().workspace_get();
        let title_bar = cx.new(|_| TitleBar::new(controls.clone(), platform, system_title_bar));
        let mut events = services.app().subscribe();
        let bridge = cx.spawn_in(window, async move |this, cx| {
            loop {
                let event = events.recv().await;
                let closed = matches!(event, Err(RecvError::Closed));
                let updated = this.update_in(cx, |this, _, cx| this.on_event(event, cx));
                if closed || updated.is_err() {
                    break;
                }
            }
        });
        // Until M6-07 the window follows the system's light or dark mode.
        Theme::sync_system_appearance(Some(window), cx);
        let appearance = cx.observe_window_appearance(window, |_, window, cx| {
            Theme::sync_system_appearance(Some(window), cx);
        });
        let mut this = Self {
            services,
            platform,
            system_title_bar,
            title_bar,
            tree: cx.new(|cx| Pane::new("tree", strings::regions::TREE, cx)),
            history: cx.new(|cx| Pane::new("history", strings::regions::HISTORY, cx)),
            changes: cx.new(|cx| Pane::new("changes", strings::regions::CHANGES, cx)),
            title_focus: cx.focus_handle(),
            toolbar_focus: cx.focus_handle(),
            status_focus: cx.focus_handle(),
            controls,
            tabs,
            layouts: HashMap::new(),
            _bridge: bridge,
            _appearance: appearance,
        };
        this.show_tabs(cx);
        // The keys work only while something has the focus: the tree has it
        // first.
        window.focus(&this.tree.focus_handle(cx), cx);
        this
    }

    /// The open workspaces and the active one, as the window shows them.
    pub fn tabs(&self) -> &WorkspaceTabs {
        &self.tabs
    }

    pub fn title_bar(&self) -> &Entity<TitleBar> {
        &self.title_bar
    }

    fn on_event(&mut self, event: Result<AppEvent, RecvError>, cx: &mut Context<Self>) {
        match event {
            Ok(AppEvent::WorkspacesChanged(tabs)) => {
                self.tabs = tabs;
                self.show_tabs(cx);
            }
            Ok(AppEvent::WorkspaceChanged(view)) => {
                if let Some(open) = self.tabs.open.iter_mut().find(|open| open.id == view.id) {
                    *open = view;
                    self.show_tabs(cx);
                }
            }
            // Events were dropped: what the window shows is read again.
            Err(RecvError::Lagged(_)) => self.read_again(cx),
            Ok(_) | Err(RecvError::Closed) => {}
        }
    }

    /// Reads the state the window shows from the app again (ARCH-6).
    fn read_again(&mut self, cx: &mut Context<Self>) {
        let call = self
            .services
            .with_app(|app| async move { app.workspace_get() });
        cx.spawn(async move |this, cx| {
            if let Some(tabs) = call.await {
                let _ = this.update(cx, |this, cx| {
                    this.tabs = tabs;
                    this.show_tabs(cx);
                });
            }
        })
        .detach();
    }

    fn show_tabs(&mut self, cx: &mut Context<Self>) {
        let title: SharedString = self
            .active_view()
            .map_or(strings::APP_NAME.into(), |view| view.name.clone().into());
        self.title_bar
            .update(cx, |title_bar, cx| title_bar.set_title(title, cx));
        cx.notify();
    }

    fn active_view(&self) -> Option<&gitcroc_core::WorkspaceView> {
        let active = self.tabs.active.as_ref()?;
        self.tabs.open.iter().find(|view| &view.id == active)
    }

    /// The active workspace's pane sizes, made from its saved layout the
    /// first time it is shown.
    fn layout(&mut self, cx: &mut Context<Self>) -> &Layout {
        let key = self.tabs.active.clone();
        let saved = self.active_view().map(|view| view.layout.clone());
        let services = self.services.clone();
        self.layouts.entry(key.clone()).or_insert_with(|| {
            let saved = saved.unwrap_or_default();
            let width = |saved: Option<u32>, default: Pixels| {
                saved.map_or(default, |width| px(width as f32))
            };
            let state = cx.new(|_| ResizableState::default());
            let saves = cx.subscribe(&state, move |this, state, _: &ResizablePanelEvent, cx| {
                let Some(workspace_id) = key.clone() else {
                    return;
                };
                let sizes = state.read(cx).sizes();
                let (Some(tree), Some(history)) = (sizes.first(), sizes.get(1)) else {
                    return;
                };
                let layout = PaneLayout {
                    tree: Some(f32::from(*tree).round() as u32),
                    history: Some(f32::from(*history).round() as u32),
                    ..this
                        .tabs
                        .open
                        .iter()
                        .find(|view| view.id == workspace_id)
                        .map(|view| view.layout.clone())
                        .unwrap_or_default()
                };
                // Saved in the background: the window already shows it, and
                // the call runs on even when its result is not awaited.
                let _saving = services.with_app(move |app| async move {
                    app.workspace_set_layout(workspace_id, layout).await
                });
            });
            Layout {
                state,
                tree: width(saved.tree, TREE_WIDTH),
                history: width(saved.history, HISTORY_WIDTH),
                _saves: saves,
            }
        })
    }

    /// The regions F6 moves between, in order (UI-22).
    fn regions(&self, cx: &gpui_kit::App) -> [FocusHandle; 6] {
        [
            self.title_focus.clone(),
            self.toolbar_focus.clone(),
            self.tree.focus_handle(cx),
            self.history.focus_handle(cx),
            self.changes.focus_handle(cx),
            self.status_focus.clone(),
        ]
    }

    fn move_region(&mut self, step: isize, window: &mut Window, cx: &mut Context<Self>) {
        let regions = self.regions(cx);
        let count = regions.len() as isize;
        let next = regions
            .iter()
            .position(|region| region.contains_focused(window, cx))
            .map_or(if step > 0 { 0 } else { count - 1 }, |at| {
                (at as isize + step).rem_euclid(count)
            });
        if let Some(region) = regions.get(next as usize) {
            window.focus(region, cx);
        }
    }

    fn next_region(&mut self, _: &NextRegion, window: &mut Window, cx: &mut Context<Self>) {
        self.move_region(1, window, cx);
    }

    fn previous_region(&mut self, _: &PreviousRegion, window: &mut Window, cx: &mut Context<Self>) {
        self.move_region(-1, window, cx);
    }

    fn minimize(&mut self, _: &Minimize, window: &mut Window, _: &mut Context<Self>) {
        self.controls.minimize(window);
    }

    fn zoom(&mut self, _: &Zoom, window: &mut Window, _: &mut Context<Self>) {
        self.controls.toggle_maximize(window);
    }
}

impl Render for GitcrocWindow {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let layout = self.layout(cx);
        let (state, tree_width, history_width) =
            (layout.state.clone(), layout.tree, layout.history);
        let theme = cx.theme();
        let content = div()
            .id("gitcroc-window")
            .key_context(keys::WINDOW_CONTEXT)
            .on_action(cx.listener(Self::next_region))
            .on_action(cx.listener(Self::previous_region))
            .on_action(cx.listener(Self::minimize))
            .on_action(cx.listener(Self::zoom))
            .flex()
            .flex_col()
            .size_full()
            .bg(theme.background)
            .text_color(theme.foreground)
            .child(
                div()
                    .id("title-region")
                    .test_support()
                    .track_focus(&self.title_focus)
                    .role(Role::Group)
                    .aria_label(strings::regions::TITLE_BAR)
                    .child(self.title_bar.clone()),
            )
            .child(
                div()
                    .id("toolbar")
                    .test_support()
                    .track_focus(&self.toolbar_focus)
                    .role(Role::Toolbar)
                    .aria_label(strings::regions::TOOLBAR)
                    .flex_shrink_0()
                    .h(TOOLBAR_HEIGHT)
                    .border_b_1()
                    .border_color(theme.border),
            )
            .child(
                div().flex_1().min_h_0().child(
                    h_resizable("panes")
                        .with_state(&state)
                        .child(resizable_panel().size(tree_width).child(self.tree.clone()))
                        .child(
                            resizable_panel()
                                .size(history_width)
                                .child(self.history.clone()),
                        )
                        .child(resizable_panel().child(self.changes.clone())),
                ),
            )
            .child(
                div()
                    .id("status-bar")
                    .test_support()
                    .track_focus(&self.status_focus)
                    .role(Role::Status)
                    .aria_label(strings::regions::STATUS_BAR)
                    .flex_shrink_0()
                    .h(STATUS_BAR_HEIGHT)
                    .border_t_1()
                    .border_color(theme.border),
            );
        // On Linux, a window that draws its own frame draws its edges and
        // shadow too, and is resized from them.
        if self.platform == Platform::Linux && !self.system_title_bar {
            window_border().child(content).into_any_element()
        } else {
            content.into_any_element()
        }
    }
}
