//! The main window: the title bar, the toolbar, the three panes (tree,
//! history, changes) and the status bar, each an empty view for its task to
//! fill. Its state comes from the app (ARCH-6): one task per window receives
//! the app's events and updates it on the main thread, and reads the state
//! again when it fell behind. Until git detection finds a usable git, the
//! window shows that it is looking, then the setup screen (PLAT-3).

use std::collections::HashMap;
use std::rc::Rc;

use gitcroc_app::app::AppEvent;
use gitcroc_core::{GitStatus, PaneLayout, WorkspaceId, WorkspaceTabs};
use gpui_component::resizable::{
    ResizablePanelEvent, ResizableState, h_resizable, resizable_panel,
};
use gpui_component::window_border;
use gpui_kit::{
    AnyElement, AppContext, Context, Entity, FocusHandle, Focusable, InteractiveElement,
    IntoElement, ParentElement, Pixels, Render, Role, SharedString, StatefulInteractiveElement,
    Styled, Subscription, Task, TestSupportExt, Window, div, px,
};
use tokio::sync::broadcast::error::RecvError;

use crate::git_setup::GitSetup;
use crate::keys::{self, Minimize, NextRegion, PreviousRegion, Zoom};
use crate::services::Services;
use crate::strings;
use crate::theme::{ActiveTokens, AppTheme};
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

/// What the window knows of git detection, as rc.1's `stores/git.ts` did.
#[derive(Debug, Default)]
struct Git {
    /// The latest finished detection; `None` until the first one is done.
    result: Option<GitStatus>,
    /// A detection is running.
    checking: bool,
    /// A `GitStatus` event has arrived, so the first read is older.
    heard: bool,
}

impl Git {
    fn usable(&self) -> bool {
        self.result.as_ref().is_some_and(GitStatus::is_usable)
    }

    /// The result, unless it is usable.
    fn problem(&self) -> Option<&GitStatus> {
        self.result.as_ref().filter(|result| !result.is_usable())
    }
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
    git: Git,
    /// Shown while git is not usable.
    git_setup: Option<Entity<GitSetup>>,
    /// `AppInfo::os`, for the setup screen's install hint.
    os: String,
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
                let updated =
                    this.update_in(cx, |this, window, cx| this.on_event(event, window, cx));
                if closed || updated.is_err() {
                    break;
                }
            }
        });
        // The theme, code themes and fonts of the settings (UI-23, UI-24,
        // UI-26), over the system's light or dark mode.
        let info = services.app().app_info();
        AppTheme::set_desktop_fonts(info.ui_font, info.code_font, cx);
        AppTheme::apply_settings(&services.app().settings_get().settings, cx);
        for family in AppTheme::global(cx).missing_fonts() {
            services
                .app()
                .log_warning(&strings::startup::missing_font(family));
        }
        AppTheme::sync_window_appearance(window, cx);
        let appearance = cx.observe_window_appearance(window, |_, window, cx| {
            AppTheme::sync_window_appearance(window, cx);
        });
        let os = info.os.clone();
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
            git: Git::default(),
            git_setup: None,
            os,
            _bridge: bridge,
            _appearance: appearance,
        };
        this.show_tabs(cx);
        this.read_git(window, cx);
        // The keys work only while something has the focus: the title bar
        // has it until git's result shows what is under it.
        window.focus(&this.title_focus, cx);
        this
    }

    /// The open workspaces and the active one, as the window shows them.
    pub fn tabs(&self) -> &WorkspaceTabs {
        &self.tabs
    }

    pub fn title_bar(&self) -> &Entity<TitleBar> {
        &self.title_bar
    }

    fn on_event(
        &mut self,
        event: Result<AppEvent, RecvError>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match event {
            Ok(AppEvent::GitStatus(status)) => {
                self.git.heard = true;
                self.record_git(status, window, cx);
            }
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
            Err(RecvError::Lagged(_)) => {
                self.git.heard = false;
                self.read_git(window, cx);
                self.read_again(cx);
            }
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

    /// Reads git's status from the app, in the background (ARCH-7). An
    /// event that arrives first is newer, so the read is then dropped.
    fn read_git(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let call = self
            .services
            .with_app(|app| async move { app.git_status().await });
        cx.spawn_in(window, async move |this, cx| {
            if let Some(status) = call.await {
                let _ = this.update_in(cx, |this, window, cx| {
                    if !this.git.heard {
                        this.record_git(status, window, cx);
                    }
                });
            }
        })
        .detach();
    }

    /// Records a detection's start or result, and shows the screen it
    /// calls for (PLAT-3).
    fn record_git(&mut self, status: GitStatus, window: &mut Window, cx: &mut Context<Self>) {
        let was_usable = self.git.usable();
        match status {
            GitStatus::Checking => self.git.checking = true,
            status => {
                self.git.checking = false;
                self.git.result = Some(status);
            }
        }
        let checking = self.git.checking;
        match (self.git.problem().cloned(), &self.git_setup) {
            (Some(problem), Some(setup)) => setup.update(cx, |setup, cx| {
                setup.set_problem(problem, cx);
                setup.set_checking(checking, cx);
            }),
            (Some(problem), None) => {
                let (services, os) = (self.services.clone(), self.os.clone());
                let setup = cx.new(|cx| GitSetup::new(services, problem, os, cx));
                setup.update(cx, |setup, cx| {
                    setup.set_checking(checking, cx);
                    setup.focus_first(window, cx);
                });
                self.git_setup = Some(setup);
            }
            (None, _) => {
                self.git_setup = None;
                if !was_usable && self.git.usable() {
                    window.focus(&self.tree.focus_handle(cx), cx);
                }
            }
        }
        self.show_tabs(cx);
    }

    fn show_tabs(&mut self, cx: &mut Context<Self>) {
        // Screens without tabs show the app's name (UI-35).
        let title: SharedString = self
            .active_view()
            .filter(|_| self.git.usable())
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

    /// The regions on screen that F6 moves between, in order (UI-22).
    fn regions(&self, cx: &gpui_kit::App) -> Vec<FocusHandle> {
        if self.git.usable() {
            vec![
                self.title_focus.clone(),
                self.toolbar_focus.clone(),
                self.tree.focus_handle(cx),
                self.history.focus_handle(cx),
                self.changes.focus_handle(cx),
                self.status_focus.clone(),
            ]
        } else {
            let setup = self.git_setup.as_ref().map(|setup| setup.focus_handle(cx));
            std::iter::once(self.title_focus.clone())
                .chain(setup)
                .collect()
        }
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

impl GitcrocWindow {
    /// The toolbar, the panes and the status bar.
    fn main_regions(&mut self, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let layout = self.layout(cx);
        let (state, tree_width, history_width) =
            (layout.state.clone(), layout.tree, layout.history);
        let tokens = *cx.tokens();
        vec![
            div()
                .id("toolbar")
                .test_support()
                .track_focus(&self.toolbar_focus)
                .role(Role::Toolbar)
                .aria_label(strings::regions::TOOLBAR)
                .flex_shrink_0()
                .h(TOOLBAR_HEIGHT)
                .bg(tokens.bg_2)
                .border_b_1()
                .border_color(tokens.border)
                .into_any_element(),
            div()
                .flex_1()
                .min_h_0()
                .child(
                    h_resizable("panes")
                        .with_state(&state)
                        .child(resizable_panel().size(tree_width).child(self.tree.clone()))
                        .child(
                            resizable_panel()
                                .size(history_width)
                                .child(self.history.clone()),
                        )
                        .child(resizable_panel().child(self.changes.clone())),
                )
                .into_any_element(),
            div()
                .id("status-bar")
                .test_support()
                .track_focus(&self.status_focus)
                .role(Role::Status)
                .aria_label(strings::regions::STATUS_BAR)
                .flex_shrink_0()
                .h(STATUS_BAR_HEIGHT)
                .bg(tokens.bg_2)
                .border_t_1()
                .border_color(tokens.border)
                .into_any_element(),
        ]
    }

    /// What the window shows while git is not usable: that it is looking,
    /// or the setup screen (PLAT-3).
    fn git_screen(&self) -> AnyElement {
        let screen = div().flex_1().min_h_0().flex();
        match &self.git_setup {
            Some(setup) => screen.child(setup.clone()).into_any_element(),
            None => screen
                .items_center()
                .justify_center()
                .child(
                    div()
                        .id("looking-for-git")
                        .test_support()
                        .role(Role::Status)
                        .aria_label(strings::git_setup::LOOKING_FOR_GIT)
                        .child(strings::git_setup::LOOKING_FOR_GIT),
                )
                .into_any_element(),
        }
    }
}

impl Render for GitcrocWindow {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let body = if self.git.usable() {
            self.main_regions(cx)
        } else {
            vec![self.git_screen()]
        };
        let tokens = *cx.tokens();
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
            .bg(tokens.bg_1)
            .text_color(tokens.text_primary)
            .child(
                div()
                    .id("title-region")
                    .test_support()
                    .track_focus(&self.title_focus)
                    .role(Role::Group)
                    .aria_label(strings::regions::TITLE_BAR)
                    .child(self.title_bar.clone()),
            )
            .children(body);
        // On Linux, a window that draws its own frame draws its edges and
        // shadow too, and is resized from them.
        if self.platform == Platform::Linux && !self.system_title_bar {
            window_border().child(content).into_any_element()
        } else {
            content.into_any_element()
        }
    }
}
