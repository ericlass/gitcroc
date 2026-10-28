//! The main window: the title bar with the workspace tabs, the toolbar, the
//! three panes (tree, history, changes) and the status bar. The tree and the
//! history are one view per open workspace, so each tab keeps its own
//! (UI-29); the changes pane is an empty view for its task to fill. Its state
//! comes from the app (ARCH-6):
//! one task per window receives the app's events and updates it on the
//! main thread, and reads the state again when it fell behind. Until git
//! detection finds a usable git, the window shows that it is looking, then
//! the setup screen (PLAT-3); with no tab open, the workspaces to open
//! (UI-27). Dialogs are drawn over everything.

use std::collections::HashMap;
use std::rc::Rc;

use gitcroc_app::app::AppEvent;
use gitcroc_core::{GitStatus, PaneLayout, WorkspaceId, WorkspaceTabs};
use gpui_component::button::{Button, ButtonVariants};
use gpui_component::resizable::{
    ResizablePanelEvent, ResizableState, h_resizable, resizable_panel,
};
use gpui_kit::{
    AnyElement, AnyView, AppContext, Context, Entity, FocusHandle, Focusable, Hsla,
    InteractiveElement, IntoElement, ParentElement, Pixels, Render, Role, SharedString,
    StatefulInteractiveElement, Styled, Subscription, Task, TestSupportExt, Window, div, px,
};
use tokio::sync::broadcast::error::RecvError;

use crate::dialog::{Closed, Modal, NameDialog, Question};
use crate::git_setup::GitSetup;
use crate::history_pane::HistoryPane;
use crate::keys::{
    self, Minimize, MoveTabLeft, MoveTabRight, NextRegion, NextTab, PreviousRegion, PreviousTab,
    Zoom,
};
use crate::services::Services;
use crate::strings;
use crate::tab_bar::{TabBar, TabBarEvent};
use crate::theme::{ActiveTokens, AppTheme};
use crate::title_bar::{Platform, TitleBar, WindowControls};
use crate::tree_pane::TreePane;
use crate::workspace_menu::{Choice, WorkspaceStart};
use crate::workspaces::Workspaces;

/// The tree's and the history's widths until the workspace saves its own
/// (UI-19); the changes pane takes the rest.
const TREE_WIDTH: Pixels = px(280.);
const HISTORY_WIDTH: Pixels = px(560.);
const TOOLBAR_HEIGHT: Pixels = px(40.);
const STATUS_BAR_HEIGHT: Pixels = px(24.);

/// The changes pane: empty until its task fills it.
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

/// The dialog over the window: there is at most one.
struct OpenDialog {
    view: AnyView,
    /// Where the focus goes back to when it closes.
    restore: Option<FocusHandle>,
    _closed: Subscription,
}

/// The window's root view.
pub struct GitcrocWindow {
    services: Services,
    title_bar: Entity<TitleBar>,
    /// Each open workspace's tree, made when its tab is first shown.
    trees: HashMap<WorkspaceId, Entity<TreePane>>,
    /// Each open workspace's history, made with its tree.
    histories: HashMap<WorkspaceId, Entity<HistoryPane>>,
    changes: Entity<Pane>,
    title_focus: FocusHandle,
    toolbar_focus: FocusHandle,
    status_focus: FocusHandle,
    controls: Rc<dyn WindowControls>,
    workspaces: Entity<Workspaces>,
    tab_bar: Entity<TabBar>,
    /// Shown while no tab is open (UI-27).
    start: Option<Entity<WorkspaceStart>>,
    dialog: Option<OpenDialog>,
    /// Why the core refused a tab change, until dismissed.
    refused: Option<SharedString>,
    /// The note on workspace files not found at start was dismissed (UI-33).
    missing_dismissed: bool,
    /// Keyed by workspace, `None` while none is open.
    layouts: HashMap<Option<WorkspaceId>, Layout>,
    git: Git,
    /// Shown while git is not usable.
    git_setup: Option<Entity<GitSetup>>,
    /// `AppInfo::os`, for the setup screen's install hint.
    os: String,
    _bridge: Task<()>,
    _subscriptions: Vec<Subscription>,
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
        let workspaces = cx.new(|_| Workspaces::new(services.clone()));
        let tab_bar = cx.new(|cx| TabBar::new(workspaces.clone(), cx));
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
        let subscriptions = vec![
            appearance,
            cx.observe_in(&workspaces, window, |this, _, window, cx| {
                this.show_tabs(window, cx)
            }),
            cx.subscribe_in(&tab_bar, window, |this, _, event, window, cx| match event {
                TabBarEvent::Chose(choice) => this.choose(choice.clone(), window, cx),
                TabBarEvent::Refused(error) => {
                    this.refused = Some(error.clone());
                    cx.notify();
                }
            }),
        ];
        let mut this = Self {
            services,
            title_bar,
            trees: HashMap::new(),
            histories: HashMap::new(),
            changes: cx.new(|cx| Pane::new("changes", strings::regions::CHANGES, cx)),
            title_focus: cx.focus_handle(),
            toolbar_focus: cx.focus_handle(),
            status_focus: cx.focus_handle(),
            controls,
            workspaces,
            tab_bar,
            start: None,
            dialog: None,
            refused: None,
            missing_dismissed: false,
            layouts: HashMap::new(),
            git: Git::default(),
            git_setup: None,
            os,
            _bridge: bridge,
            _subscriptions: subscriptions,
        };
        this.show_tabs(window, cx);
        this.read_git(window, cx);
        // The keys work only while something has the focus: the title bar
        // has it until git's result shows what is under it.
        window.focus(&this.title_focus, cx);
        this
    }

    /// The open workspaces and the active one, as the window shows them.
    pub fn tabs(&self, cx: &gpui_kit::App) -> WorkspaceTabs {
        self.workspaces.read(cx).tabs().clone()
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
        for tree in self.trees.values() {
            tree.update(cx, |tree, cx| match &event {
                Ok(event) => tree.on_event(event, cx),
                Err(RecvError::Lagged(_)) => tree.read_again(cx),
                Err(RecvError::Closed) => {}
            });
        }
        for history in self.histories.values() {
            history.update(cx, |history, cx| match &event {
                Ok(event) => history.on_event(event, cx),
                Err(RecvError::Lagged(_)) => history.read_again(cx),
                Err(RecvError::Closed) => {}
            });
        }
        match event {
            Ok(AppEvent::GitStatus(status)) => {
                self.git.heard = true;
                self.record_git(status, window, cx);
            }
            Ok(AppEvent::WorkspacesChanged(tabs)) => {
                self.workspaces
                    .update(cx, |workspaces, cx| workspaces.hear_tabs(tabs, cx));
            }
            Ok(AppEvent::WorkspaceChanged(view)) => {
                self.workspaces
                    .update(cx, |workspaces, cx| workspaces.set_view(view, cx));
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
        let asked = self.workspaces.read(cx).heard();
        let call = self
            .services
            .with_app(|app| async move { app.workspace_get() });
        let workspaces = self.workspaces.downgrade();
        cx.spawn(async move |_, cx| {
            if let Some(tabs) = call.await {
                let _ =
                    workspaces.update(cx, |workspaces, cx| workspaces.answer_tabs(asked, tabs, cx));
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
                    self.show_tabs(window, cx);
                    self.focus_main(window, cx);
                    return;
                }
            }
        }
        self.show_tabs(window, cx);
    }

    /// Shows the tabs, or the screen without them, after the tabs or git
    /// changed.
    fn show_tabs(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let usable = self.git.usable();
        let tabs = self.workspaces.read(cx).tabs().clone();
        // Screens without tabs show the app's name (UI-35); the title also
        // names the active workspace for the system.
        let title: SharedString = self
            .workspaces
            .read(cx)
            .active()
            .filter(|_| usable)
            .map_or(strings::APP_NAME.into(), |view| view.name.clone().into());
        let tab_bar: Option<AnyView> = usable.then(|| self.tab_bar.clone().into());
        self.title_bar.update(cx, |title_bar, cx| {
            title_bar.set_title(title, cx);
            title_bar.set_tab_bar(tab_bar, cx);
        });
        // A closed tab's tree and history go; the active tab's are made when
        // first shown.
        self.trees
            .retain(|id, _| tabs.open.iter().any(|view| &view.id == id));
        self.histories
            .retain(|id, _| tabs.open.iter().any(|view| &view.id == id));
        if let Some(active) = tabs.active.clone().filter(|_| usable) {
            if !self.trees.contains_key(&active) {
                let services = self.services.clone();
                let tree = cx.new(|cx| TreePane::new(services, active.clone(), window, cx));
                self.trees.insert(active.clone(), tree);
            }
            if !self.histories.contains_key(&active)
                && let Some(tree) = self.trees.get(&active).cloned()
            {
                let services = self.services.clone();
                let saved_columns = tabs
                    .open
                    .iter()
                    .find(|view| view.id == active)
                    .and_then(|view| view.layout.columns.clone());
                let history = cx
                    .new(|cx| HistoryPane::new(services, active.clone(), tree, saved_columns, cx));
                self.histories.insert(active, history);
            }
        }
        // With every tab closed, the workspaces to open (UI-27).
        if tabs.active.is_none() && usable {
            if self.start.is_none() {
                let workspaces = self.workspaces.clone();
                let start = cx.new(|cx| WorkspaceStart::new(&workspaces, cx));
                // It ends with the start screen, which the window outlives.
                cx.subscribe_in(&start, window, |this, _, choice, window, cx| {
                    this.choose(choice.clone(), window, cx);
                })
                .detach();
                self.start = Some(start);
            }
        } else {
            self.start = None;
        }
        cx.notify();
    }

    /// The active tab's tree.
    pub fn tree(&self, cx: &gpui_kit::App) -> Option<Entity<TreePane>> {
        let active = self.workspaces.read(cx).tabs().active.as_ref()?;
        self.trees.get(active).cloned()
    }

    /// The active tab's history.
    pub fn history(&self, cx: &gpui_kit::App) -> Option<Entity<HistoryPane>> {
        let active = self.workspaces.read(cx).tabs().active.as_ref()?;
        self.histories.get(active).cloned()
    }

    /// Gives the focus to the first region under the title bar.
    fn focus_main(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let first = match (&self.start, self.tree(cx)) {
            (Some(start), _) => Some(start.focus_handle(cx)),
            (None, tree) => tree.map(|tree| tree.focus_handle(cx)),
        };
        if let Some(first) = first {
            window.focus(&first, cx);
        }
    }

    /// Carries out what a menu entry or the start screen chose.
    fn choose(&mut self, choice: Choice, window: &mut Window, cx: &mut Context<Self>) {
        let workspaces = self.workspaces.clone();
        match choice {
            Choice::New => {
                let send = Box::new(move |name: String, cx: &mut gpui_kit::App| {
                    Workspaces::create(&workspaces, name, cx)
                });
                let dialog = cx.new(|cx| {
                    NameDialog::new(
                        strings::workspaces::NEW_DIALOG,
                        strings::workspaces::CREATE,
                        "",
                        send,
                        window,
                        cx,
                    )
                });
                self.open_dialog(dialog, window, cx);
            }
            Choice::Rename(id, name) => {
                let send = Box::new(move |new_name: String, cx: &mut gpui_kit::App| {
                    Workspaces::rename(&workspaces, id.clone(), new_name, cx)
                });
                let dialog = cx.new(|cx| {
                    NameDialog::new(
                        strings::workspaces::rename_dialog(&name),
                        strings::workspaces::RENAME,
                        &name,
                        send,
                        window,
                        cx,
                    )
                });
                self.open_dialog(dialog, window, cx);
            }
            Choice::Delete(id, name) => {
                let send = Box::new(move |(), cx: &mut gpui_kit::App| {
                    Workspaces::delete(&workspaces, id.clone(), cx)
                });
                let dialog = cx.new(|cx| {
                    Question::new(
                        strings::workspaces::delete_dialog(&name),
                        vec![strings::workspaces::DELETE_WHAT.into()],
                        strings::workspaces::DELETE,
                        send,
                        cx,
                    )
                });
                self.open_dialog(dialog, window, cx);
            }
            Choice::Open(id, name) => {
                let sent = Workspaces::open(&workspaces, id, cx);
                cx.spawn_in(window, async move |this, cx| {
                    let result = sent.await;
                    let _ = this.update_in(cx, |this, window, cx| match result {
                        Ok(()) => this.focus_main(window, cx),
                        Err(error) => {
                            let error = strings::workspaces::cannot_open(&name, &error);
                            if let Some(start) = &this.start {
                                start.update(cx, |start, cx| start.set_error(error.into(), cx));
                            }
                        }
                    });
                })
                .detach();
            }
            Choice::Import => {
                let sent = Workspaces::import(&workspaces, cx);
                self.refuse_with(sent, strings::workspaces::cannot_import, cx);
            }
            Choice::Export(id) => {
                let sent = Workspaces::export(&workspaces, id, cx);
                self.refuse_with(sent, strings::workspaces::cannot_export, cx);
            }
            Choice::Close(id) => self
                .tab_bar
                .update(cx, |tab_bar, cx| tab_bar.close(id, window, cx)),
        }
    }

    /// Shows a refusal of `sent`, worded by `say`, below the title bar.
    fn refuse_with(
        &mut self,
        sent: Task<Result<(), SharedString>>,
        say: fn(&str) -> String,
        cx: &mut Context<Self>,
    ) {
        cx.spawn(async move |this, cx| {
            if let Err(error) = sent.await {
                let _ = this.update(cx, |this, cx| {
                    this.refused = Some(say(&error).into());
                    cx.notify();
                });
            }
        })
        .detach();
    }

    /// Shows `dialog` over the window, with the focus in it.
    fn open_dialog<V: Modal>(
        &mut self,
        dialog: Entity<V>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let closed = cx.subscribe_in(&dialog, window, |this, _, _: &Closed, window, cx| {
            if let Some(open) = this.dialog.take()
                && let Some(restore) = open.restore
            {
                window.focus(&restore, cx);
            }
            cx.notify();
        });
        let restore = window.focused(cx);
        dialog.update(cx, |dialog, cx| dialog.focus_first(window, cx));
        self.dialog = Some(OpenDialog {
            view: dialog.into(),
            restore,
            _closed: closed,
        });
        cx.notify();
    }

    /// The tab shortcuts work while no dialog or menu is open (UI-22).
    fn tabs_reachable(&self, cx: &gpui_kit::App) -> bool {
        self.git.usable() && self.dialog.is_none() && !self.tab_bar.read(cx).menu_open()
    }

    fn next_tab(&mut self, _: &NextTab, window: &mut Window, cx: &mut Context<Self>) {
        if self.tabs_reachable(cx) {
            self.tab_bar.update(cx, |tab_bar, cx| {
                tab_bar.activate_beside_active(1, window, cx)
            });
        }
    }

    fn previous_tab(&mut self, _: &PreviousTab, window: &mut Window, cx: &mut Context<Self>) {
        if self.tabs_reachable(cx) {
            self.tab_bar.update(cx, |tab_bar, cx| {
                tab_bar.activate_beside_active(-1, window, cx)
            });
        }
    }

    fn move_tab_right(&mut self, _: &MoveTabRight, window: &mut Window, cx: &mut Context<Self>) {
        if self.tabs_reachable(cx) {
            self.tab_bar
                .update(cx, |tab_bar, cx| tab_bar.move_active(1, window, cx));
        }
    }

    fn move_tab_left(&mut self, _: &MoveTabLeft, window: &mut Window, cx: &mut Context<Self>) {
        if self.tabs_reachable(cx) {
            self.tab_bar
                .update(cx, |tab_bar, cx| tab_bar.move_active(-1, window, cx));
        }
    }

    /// The active workspace's pane sizes, made from its saved layout the
    /// first time it is shown.
    fn layout(&mut self, cx: &mut Context<Self>) -> &Layout {
        let workspaces = self.workspaces.read(cx);
        let key = workspaces.tabs().active.clone();
        let saved = workspaces.active().map(|view| view.layout.clone());
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
                        .workspaces
                        .read(cx)
                        .view(&workspace_id)
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
        if let Some(start) = self.start.as_ref().filter(|_| self.git.usable()) {
            vec![self.title_focus.clone(), start.focus_handle(cx)]
        } else if self.git.usable() {
            let tree = self.tree(cx).map(|tree| tree.focus_handle(cx));
            let history = self.history(cx).map(|history| history.focus_handle(cx));
            [self.title_focus.clone(), self.toolbar_focus.clone()]
                .into_iter()
                .chain(tree)
                .chain(history)
                .chain([self.changes.focus_handle(cx), self.status_focus.clone()])
                .collect()
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
        // In the tab bar, the focus lands on the active tab, or on "+".
        if next == 0 && self.git.usable() {
            self.tab_bar
                .update(cx, |tab_bar, cx| tab_bar.focus(window, cx));
        } else if let Some(region) = regions.get(next as usize) {
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
        let tree = self.tree(cx);
        let history = self.history(cx);
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
                        .child(resizable_panel().size(tree_width).children(tree))
                        .child(resizable_panel().size(history_width).children(history))
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

    /// What a dialog sits on: the window, dimmed, which it keeps from the
    /// pointer.
    fn dialog_layer(&self, cx: &Context<Self>) -> Option<AnyElement> {
        let open = self.dialog.as_ref()?;
        let tokens = cx.tokens();
        let backdrop = Hsla::from(tokens.backdrop).opacity(tokens.backdrop_opacity);
        Some(
            div()
                .id("dialog-layer")
                .absolute()
                .inset_0()
                .occlude()
                .bg(backdrop)
                .flex()
                .items_center()
                .justify_center()
                .p_4()
                .child(open.view.clone())
                .into_any_element(),
        )
    }

    /// The notes under the title bar: a refused tab change, and workspace
    /// files not found at start (UI-33). Each is dismissed by its button.
    fn notes(&self, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let mut notes = Vec::new();
        if let Some(refused) = &self.refused {
            notes.push(self.note(
                "tab-refusal",
                refused.clone(),
                cx.listener(|this, _, _, cx| {
                    this.refused = None;
                    cx.notify();
                }),
                cx,
            ));
        }
        let missing = &self.workspaces.read(cx).tabs().missing;
        if !missing.is_empty() && !self.missing_dismissed {
            let text = strings::app::missing_workspaces(&missing.join(", "));
            notes.push(self.note(
                "missing-workspaces",
                text.into(),
                cx.listener(|this, _, _, cx| {
                    this.missing_dismissed = true;
                    cx.notify();
                }),
                cx,
            ));
        }
        notes
    }

    fn note(
        &self,
        id: &'static str,
        text: SharedString,
        dismiss: impl Fn(&gpui_kit::ClickEvent, &mut Window, &mut gpui_kit::App) + 'static,
        cx: &Context<Self>,
    ) -> AnyElement {
        let tokens = cx.tokens();
        div()
            .id(id)
            .test_support()
            .role(Role::Alert)
            .aria_label(text.clone())
            .flex()
            .flex_row()
            .flex_shrink_0()
            .items_center()
            .gap_2()
            .px_3()
            .py_1()
            .bg(tokens.warning_bg)
            .text_color(tokens.text_primary)
            .border_b_1()
            .border_color(tokens.border)
            // A long text, such as a long path, wraps and leaves the
            // button in the window.
            .child(div().flex_1().min_w_0().child(text))
            .child(
                div().flex_shrink_0().child(
                    Button::new(gpui_kit::ElementId::Name(format!("{id}-dismiss").into()))
                        .ghost()
                        .compact()
                        .label(strings::common::DISMISS)
                        .on_click(dismiss),
                ),
            )
            .into_any_element()
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
        let body = match (&self.start, self.git.usable()) {
            (_, false) => vec![self.git_screen()],
            (Some(start), true) => {
                let start = div().flex_1().min_h_0().child(start.clone());
                let mut body = self.notes(cx);
                body.push(start.into_any_element());
                body
            }
            (None, true) => {
                let mut body = self.notes(cx);
                body.extend(self.main_regions(cx));
                body
            }
        };
        let tokens = *cx.tokens();
        let content = div()
            .id("gitcroc-window")
            .key_context(keys::WINDOW_CONTEXT)
            .on_action(cx.listener(Self::next_region))
            .on_action(cx.listener(Self::previous_region))
            .on_action(cx.listener(Self::minimize))
            .on_action(cx.listener(Self::zoom))
            .on_action(cx.listener(Self::next_tab))
            .on_action(cx.listener(Self::previous_tab))
            .on_action(cx.listener(Self::move_tab_right))
            .on_action(cx.listener(Self::move_tab_left))
            .relative()
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
            .children(body)
            .children(self.dialog_layer(cx));
        // The root `open_window` puts around the window draws the frame's
        // edges and shadow where the window draws its own frame.
        content
    }
}
