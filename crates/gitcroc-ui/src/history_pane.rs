//! The history pane of one workspace (UI-9): the search box (UI-12), the
//! commit-graph offer (PERF-2) and the commit list, led by "Uncommitted
//! changes (n)" for a dirty repo (UI-10), virtualized (PERF-1) and paged by
//! the core (ARCH-8), with the graph painter from the spike. A group or
//! several repos show the summary strip (UI-11) above their merged history,
//! which has a repo column instead of the graph and is led by "Uncommitted
//! changes (n repos)" (UI-34). The window keeps
//! one per open workspace, so each tab keeps its own source, search text,
//! rows, selection, scroll position and column order and widths (UI-29,
//! UI-36). Everything shown comes from the core; this lays it out and
//! forwards input.

use crate::branch_dialog::BranchMode;
use crate::row_context::{AskRowMenu, RowChoice};
use gitcroc_core::{BranchOp, StartPoint};
use std::cell::{Cell, RefCell};
use std::collections::{BTreeSet, HashMap};
use std::ops::Range;
use std::rc::Rc;

use gitcroc_app::app::AppEvent;
use gitcroc_core::{
    ColumnLayout, GraphRow, Head, HistoryColumn, HistoryTarget, RefKind, RepoId, TargetSet,
    WorkspaceId,
};
use gpui_component::button::{Button, ButtonVariants};
use gpui_component::input::{Input, InputEvent, InputState};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::{
    Anchor, AnyElement, App, AppContext, Bounds, ClickEvent, ContentMask, Context, CursorStyle,
    DispatchPhase, DragMoveEvent, ElementId, Entity, EventEmitter, FocusHandle, Focusable, Hsla,
    InteractiveElement, IntoElement, KeyBinding, KeyBindingContextPredicate, MouseButton,
    MouseDownEvent, MouseUpEvent, ParentElement, PathBuilder, Pixels, Point, Render, Role,
    ScrollStrategy, SharedString, StatefulInteractiveElement, Styled, Subscription, TestSupportExt,
    UniformListScrollHandle, Window, actions, canvas, div, fill, point, px, size, svg,
    uniform_list,
};

use crate::history::{self, History, Restore, Row, Selection, Show, UNCOMMITTED_ID};
use crate::history_columns::{Columns, KEY_STEP, MIN_COLUMN_WIDTH, MIN_SUBJECT_WIDTH, SizedColumn};
use crate::icons;
use crate::menu::{Menu, MenuEvent, MenuItem};
use crate::services::Services;
use crate::strings::{self, history as text};
use crate::theme::{ActiveTokens, tokens::Tokens};
use crate::tree_pane::TreePane;

actions!(
    history_pane,
    [
        OpenCommitMenu,
        SelectNext,
        SelectPrevious,
        PageNext,
        PagePrevious,
        SelectFirst,
        SelectLast,
        HeaderLeft,
        HeaderRight,
        HeaderFirst,
        HeaderLast,
        OpenHeaderMenu,
        EdgeLeft,
        EdgeRight,
    ]
);

/// The key context of the history list.
const CONTEXT: &str = "GitcrocHistory";
/// The key context of the column headers (UI-36).
const HEADER_CONTEXT: &str = "GitcrocHistoryHeaders";
/// The key context of a column's resize edge.
const EDGE_CONTEXT: &str = "GitcrocHistoryColumnEdge";

/// One row's height; `uniform_list` lays out only those on screen.
pub const ROW_HEIGHT: Pixels = px(24.);
const LANE_WIDTH: f32 = 12.;
/// The next page is asked for when this few loaded rows are left below.
const PREFETCH_ROWS: usize = 100;
/// Placeholder rows below the loaded ones while a page follows.
const PLACEHOLDER_ROWS: usize = 500;
/// Lines the summary strip shows before it scrolls (UI-11).
pub const STRIP_LINES: usize = 8;
const ICON: Pixels = px(12.);

/// The keys of the history list (UI-22).
pub fn bind(cx: &mut App) {
    let context = KeyBindingContextPredicate::parse(CONTEXT).ok().map(Rc::new);
    let bind = |keys: &str, action: Box<dyn gpui_kit::Action>, cx: &mut App| {
        KeyBinding::load(keys, action, context.clone(), false, None, cx).ok()
    };
    let bindings = [
        bind("shift-f10", Box::new(OpenCommitMenu), cx),
        bind("menu", Box::new(OpenCommitMenu), cx),
        bind("down", Box::new(SelectNext), cx),
        bind("up", Box::new(SelectPrevious), cx),
        bind("pagedown", Box::new(PageNext), cx),
        bind("pageup", Box::new(PagePrevious), cx),
        bind("home", Box::new(SelectFirst), cx),
        bind("end", Box::new(SelectLast), cx),
    ];
    cx.bind_keys(bindings.into_iter().flatten());

    let header = KeyBindingContextPredicate::parse(HEADER_CONTEXT)
        .ok()
        .map(Rc::new);
    let bind = |keys: &str, action: Box<dyn gpui_kit::Action>, cx: &mut App| {
        KeyBinding::load(keys, action, header.clone(), false, None, cx).ok()
    };
    let bindings = [
        bind("left", Box::new(HeaderLeft), cx),
        bind("right", Box::new(HeaderRight), cx),
        bind("home", Box::new(HeaderFirst), cx),
        bind("end", Box::new(HeaderLast), cx),
        bind("shift-f10", Box::new(OpenHeaderMenu), cx),
        bind("menu", Box::new(OpenHeaderMenu), cx),
    ];
    cx.bind_keys(bindings.into_iter().flatten());

    let edge = KeyBindingContextPredicate::parse(EDGE_CONTEXT)
        .ok()
        .map(Rc::new);
    let bind = |keys: &str, action: Box<dyn gpui_kit::Action>, cx: &mut App| {
        KeyBinding::load(keys, action, edge.clone(), false, None, cx).ok()
    };
    let bindings = [
        bind("left", Box::new(EdgeLeft), cx),
        bind("right", Box::new(EdgeRight), cx),
    ];
    cx.bind_keys(bindings.into_iter().flatten());
}

/// A header being dragged onto another (UI-36).
#[derive(Debug, Clone, Copy)]
struct DraggedHeader {
    column: HistoryColumn,
}

/// What follows the pointer while a header is dragged.
struct DraggedHeaderView(SharedString);

impl Render for DraggedHeaderView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let tokens = *cx.tokens();
        div()
            .px_2()
            .py_1()
            .bg(tokens.bg_3)
            .text_color(tokens.text_primary)
            .border_1()
            .border_color(tokens.border)
            .rounded(px(4.))
            .text_size(px(12.))
            .child(self.0.clone())
    }
}

/// What a resize drag follows; it draws nothing.
struct DraggedEdge;

impl Render for DraggedEdge {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
    }
}

/// A resize in progress: where the pointer and the column started.
#[derive(Debug, Clone, Copy)]
struct Resizing {
    sized: SizedColumn,
    before: bool,
    start_x: f32,
    start_width: f32,
    start_subject_width: f32,
}

type HeaderBounds = Rc<RefCell<Vec<(HistoryColumn, Bounds<Pixels>)>>>;

/// A column menu the pane opened, and where its focus returns.
struct OpenColumnsMenu {
    menu: Entity<Menu>,
    column: HistoryColumn,
    choices: Vec<ColumnsChoice>,
    opener: FocusHandle,
    _events: Subscription,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ColumnsChoice {
    MoveLeft,
    MoveRight,
    Reset,
}

/// The history of one workspace.
pub struct HistoryPane {
    services: Services,
    workspace_id: WorkspaceId,
    history: History,
    tree: Entity<TreePane>,
    focus: FocusHandle,
    scroll: UniformListScrollHandle,
    /// The search box (UI-12); the history keeps its text.
    search: Entity<InputState>,
    /// The shown source changed, so the box shows its text at the next
    /// render, which has the window it needs.
    show_text: bool,
    /// The latest first read; older answers are dropped.
    request: u64,
    /// The last `refs_version` heard of each repo (OPS-7).
    refs_versions: HashMap<RepoId, u32>,
    /// The latest resolve of a merged history's target set.
    targets_request: u64,
    /// Repo names for the strip and the repo column, from the tree.
    names: HashMap<RepoId, SharedString>,
    strip_scroll: UniformListScrollHandle,
    /// The workspace's column order and widths (UI-36).
    columns: Columns,
    header_focus: FocusHandle,
    edges: [FocusHandle; 4],
    /// Where the pointer and the column started while an edge is dragged.
    resizing: Option<Resizing>,
    /// The header a dragged column would drop at, and on which side.
    drop_target: Option<(HistoryColumn, bool)>,
    /// Each header's bounds, for opening its menu from the keyboard.
    header_bounds: HeaderBounds,
    /// The subject's laid-out width, so a resize keeps its minimum.
    subject_width: Rc<Cell<f32>>,
    menu: Option<OpenColumnsMenu>,
    marked: BTreeSet<String>,
    selection_anchor: Option<usize>,
    row_bounds: Rc<RefCell<HashMap<usize, Bounds<Pixels>>>>,
    saving: bool,
    save_again: bool,
    _tree_watch: Subscription,
    _search_events: Subscription,
}

impl EventEmitter<AskRowMenu> for HistoryPane {}

impl HistoryPane {
    pub fn new(
        services: Services,
        workspace_id: WorkspaceId,
        tree: Entity<TreePane>,
        saved_columns: Option<ColumnLayout>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let tree_watch = cx.observe(&tree, |this, _, cx| {
            if !this.history.released() {
                this.follow_tree(cx);
            }
        });
        let search = cx.new(|cx| InputState::new(window, cx).placeholder(text::SEARCH));
        let search_events = cx.subscribe(&search, |this, search, event, cx| {
            if let InputEvent::Change = event {
                let value = search.read(cx).value().to_string();
                this.set_text(value, cx);
            }
        });
        let mut this = Self {
            services,
            workspace_id,
            history: History::default(),
            tree,
            focus: cx.focus_handle(),
            scroll: UniformListScrollHandle::new(),
            search,
            show_text: false,
            request: 0,
            refs_versions: HashMap::new(),
            targets_request: 0,
            names: HashMap::new(),
            strip_scroll: UniformListScrollHandle::new(),
            columns: Columns::from_layout(saved_columns.as_ref()),
            header_focus: cx.focus_handle(),
            edges: [
                cx.focus_handle(),
                cx.focus_handle(),
                cx.focus_handle(),
                cx.focus_handle(),
            ],
            resizing: None,
            drop_target: None,
            header_bounds: Rc::new(RefCell::new(Vec::new())),
            subject_width: Rc::new(Cell::new(0.)),
            menu: None,
            marked: BTreeSet::new(),
            selection_anchor: None,
            row_bounds: Rc::default(),
            saving: false,
            save_again: false,
            _tree_watch: tree_watch,
            _search_events: search_events,
        };
        this.follow_tree(cx);
        this
    }

    pub fn history(&self) -> &History {
        &self.history
    }

    /// The workspace's column order and widths (UI-36).
    pub fn columns(&self) -> &Columns {
        &self.columns
    }

    /// What the selected row shows in the changes pane (M6-12).
    pub fn selection(&self) -> Option<Selection> {
        self.history.selection()
    }

    /// Selects commit `hash` of `repo`, as a parent link in the changes
    /// pane asks (UI-14), and scrolls to it when its row is loaded. The
    /// shown history must have that repo's commits.
    pub fn select_commit(&mut self, repo: &RepoId, hash: &str, cx: &mut Context<Self>) {
        let Some(source) = self.history.source() else {
            return;
        };
        let row_id = match &source.repo {
            Some(shown) if shown == repo => hash.to_owned(),
            Some(_) => return,
            None => format!("{}:{hash}", repo.0),
        };
        self.history.select(Some(row_id));
        if let Some(index) = self.history.selected_index() {
            self.scroll.scroll_to_item(index, ScrollStrategy::Nearest);
        }
        cx.notify();
    }

    /// Saves the columns with the workspace's pane layout (UI-36). The
    /// window's pane sizes and dock are kept as they are. A drag asks many
    /// times; only one save runs, and one more follows it with the latest
    /// widths.
    fn save_columns(&mut self, cx: &mut Context<Self>) {
        self.save_again = true;
        if self.saving {
            return;
        }
        self.start_save(cx);
    }

    fn start_save(&mut self, cx: &mut Context<Self>) {
        let Some(view) = self
            .services
            .app()
            .workspace_get()
            .open
            .into_iter()
            .find(|view| view.id == self.workspace_id)
        else {
            self.save_again = false;
            return;
        };
        let mut layout = view.layout;
        layout.columns = self.columns.layout();
        self.save_again = false;
        self.saving = true;
        let workspace = self.workspace_id.clone();
        let call = self
            .services
            .with_app(move |app| async move { app.workspace_set_layout(workspace, layout).await });
        cx.spawn(async move |this, cx| {
            let _ = call.await;
            let _ = this.update(cx, |this, cx| {
                this.saving = false;
                if this.save_again {
                    this.start_save(cx);
                }
                cx.notify();
            });
        })
        .detach();
    }

    fn save_now(&mut self, cx: &mut Context<Self>) {
        self.save_columns(cx);
        cx.notify();
    }

    fn set_active(&mut self, column: HistoryColumn, cx: &mut Context<Self>) {
        if self.columns.active() == column {
            return;
        }
        self.columns.set_active(column);
        cx.notify();
    }

    fn header_step(&mut self, step: isize, cx: &mut Context<Self>) {
        let order = self.columns.order().to_vec();
        if order.is_empty() {
            return;
        }
        let at = self
            .columns
            .position(self.columns.active())
            .unwrap_or_default();
        let next = (at as isize + step).rem_euclid(order.len() as isize) as usize;
        self.set_active(order[next], cx);
    }

    fn header_left(&mut self, _: &HeaderLeft, _: &mut Window, cx: &mut Context<Self>) {
        cx.stop_propagation();
        self.header_step(-1, cx);
    }

    fn header_right(&mut self, _: &HeaderRight, _: &mut Window, cx: &mut Context<Self>) {
        cx.stop_propagation();
        self.header_step(1, cx);
    }

    fn header_first(&mut self, _: &HeaderFirst, _: &mut Window, cx: &mut Context<Self>) {
        cx.stop_propagation();
        self.set_active(self.columns.first(), cx);
    }

    fn header_last(&mut self, _: &HeaderLast, _: &mut Window, cx: &mut Context<Self>) {
        cx.stop_propagation();
        self.set_active(self.columns.last(), cx);
    }

    fn header_at(&self, column: HistoryColumn) -> Point<Pixels> {
        self.header_bounds
            .borrow()
            .iter()
            .find(|(other, _)| *other == column)
            .map(|(_, bounds)| bounds.bottom_left())
            .unwrap_or_else(|| point(px(0.), px(0.)))
    }

    fn open_column_menu(
        &mut self,
        column: HistoryColumn,
        at: Point<Pixels>,
        anchor: Anchor,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let at_index = self.columns.position(column).unwrap_or_default();
        let items = vec![
            (
                MenuItem::new(text::MOVE_LEFT).disabled(at_index == 0),
                ColumnsChoice::MoveLeft,
            ),
            (
                MenuItem::new(text::MOVE_RIGHT)
                    .disabled(at_index + 1 >= self.columns.order().len()),
                ColumnsChoice::MoveRight,
            ),
            (MenuItem::new(text::RESET_COLUMNS), ColumnsChoice::Reset),
        ];
        let (items, choices): (Vec<MenuItem>, Vec<ColumnsChoice>) = items.into_iter().unzip();
        let label = text::column_menu(column_label(column, self.history.merged()));
        let menu = cx.new(|cx| Menu::new(label, items, at, anchor, cx));
        let events = cx.subscribe_in(&menu, window, |this, _, event, window, cx| {
            this.on_columns_menu(*event, window, cx);
        });
        menu.update(cx, |menu, cx| menu.focus_first(window, cx));
        self.menu = Some(OpenColumnsMenu {
            menu,
            column,
            choices,
            opener: self.header_focus.clone(),
            _events: events,
        });
        self.set_active(column, cx);
        cx.notify();
    }

    fn on_columns_menu(&mut self, event: MenuEvent, window: &mut Window, cx: &mut Context<Self>) {
        let Some(open) = self.menu.take() else {
            return;
        };
        cx.notify();
        window.focus(&open.opener, cx);
        let MenuEvent::Chosen(at) = event else {
            return;
        };
        let Some(choice) = open.choices.get(at).copied() else {
            return;
        };
        let changed = match choice {
            ColumnsChoice::MoveLeft => self.columns.move_left(open.column),
            ColumnsChoice::MoveRight => self.columns.move_right(open.column),
            ColumnsChoice::Reset => {
                self.columns.reset();
                true
            }
        };
        if changed {
            self.save_now(cx);
        }
        cx.notify();
    }

    fn header_click(&mut self, column: HistoryColumn, window: &mut Window, cx: &mut Context<Self>) {
        window.focus(&self.header_focus, cx);
        let at = self.header_at(column);
        self.open_column_menu(column, at, Anchor::TopLeft, window, cx);
    }

    fn header_right_click(
        &mut self,
        column: HistoryColumn,
        at: Point<Pixels>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        cx.stop_propagation();
        window.focus(&self.header_focus, cx);
        self.open_column_menu(column, at, Anchor::TopLeft, window, cx);
    }

    fn open_header_menu(
        &mut self,
        _: &OpenHeaderMenu,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        cx.stop_propagation();
        let column = self.columns.active();
        let at = self.header_at(column);
        self.open_column_menu(column, at, Anchor::TopLeft, window, cx);
    }

    fn edge_before(&self, sized: SizedColumn) -> bool {
        let subject = self
            .columns
            .position(HistoryColumn::Subject)
            .unwrap_or_default();
        let column = self.columns.position(sized.column()).unwrap_or_default();
        column > subject
    }

    fn edge_focus(&self, sized: SizedColumn) -> Option<&FocusHandle> {
        let at = SizedColumn::ALL.iter().position(|other| *other == sized)?;
        self.edges.get(at)
    }

    fn focused_edge(&self, window: &Window) -> Option<SizedColumn> {
        SizedColumn::ALL.into_iter().find(|sized| {
            self.edge_focus(*sized)
                .is_some_and(|f| f.is_focused(window))
        })
    }

    fn start_resize(&mut self, sized: SizedColumn, event: &MouseDownEvent) {
        self.resizing = Some(Resizing {
            sized,
            before: self.edge_before(sized),
            start_x: f32::from(event.position.x),
            start_width: self.columns.width_of(sized) as f32,
            start_subject_width: self.subject_width.get(),
        });
    }

    fn drag_edge(&mut self, event: &DragMoveEvent<SizedColumn>, cx: &mut Context<Self>) {
        let sized = *event.drag(cx);
        let Some(resizing) = self.resizing else {
            return;
        };
        if resizing.sized != sized {
            return;
        }
        let x = f32::from(event.event.position.x);
        let delta = if resizing.before {
            resizing.start_x - x
        } else {
            x - resizing.start_x
        };
        let width = resizing.start_width + delta;
        let extra = (resizing.start_subject_width - MIN_SUBJECT_WIDTH)
            .max(0.)
            .floor() as u32;
        let max = (resizing.start_width.round() as u32).saturating_add(extra);
        if self.columns.set_width_capped(sized, width, max) {
            self.save_columns(cx);
            cx.notify();
        }
    }

    fn edge_resize(&mut self, sized: SizedColumn, towards_left: bool, cx: &mut Context<Self>) {
        let before = self.edge_before(sized);
        let delta = match (before, towards_left) {
            (true, true) | (false, false) => KEY_STEP,
            (true, false) | (false, true) => -KEY_STEP,
        };
        if self.columns.resize(sized, delta, self.subject_width.get()) {
            self.save_now(cx);
            cx.notify();
        }
    }

    fn edge_left(&mut self, _: &EdgeLeft, window: &mut Window, cx: &mut Context<Self>) {
        cx.stop_propagation();
        if let Some(sized) = self.focused_edge(window) {
            self.edge_resize(sized, true, cx);
        }
    }

    fn edge_right(&mut self, _: &EdgeRight, window: &mut Window, cx: &mut Context<Self>) {
        cx.stop_propagation();
        if let Some(sized) = self.focused_edge(window) {
            self.edge_resize(sized, false, cx);
        }
    }

    fn reset_width(&mut self, sized: SizedColumn, cx: &mut Context<Self>) {
        if self.columns.reset_width(sized) {
            self.save_now(cx);
            cx.notify();
        }
    }

    fn drag_over_header(&mut self, event: &DragMoveEvent<DraggedHeader>, cx: &mut Context<Self>) {
        let position = event.event.position;
        let target = self
            .header_bounds
            .borrow()
            .iter()
            .find(|(_, bounds)| bounds.contains(&position))
            .map(|(column, bounds)| {
                (
                    *column,
                    position.x > bounds.origin.x + bounds.size.width / 2.,
                )
            });
        if self.drop_target != target {
            self.drop_target = target;
            cx.notify();
        }
    }

    fn drop_header(
        &mut self,
        dragged: &DraggedHeader,
        target: HistoryColumn,
        cx: &mut Context<Self>,
    ) {
        let Some((target, after)) = self.drop_target.filter(|(column, _)| *column == target) else {
            self.drop_target = None;
            cx.notify();
            return;
        };
        self.drop_target = None;
        if dragged.column == target {
            cx.notify();
            return;
        }
        let rest: Vec<HistoryColumn> = self
            .columns
            .order()
            .iter()
            .copied()
            .filter(|column| *column != dragged.column)
            .collect();
        let Some(at) = rest.iter().position(|column| *column == target) else {
            cx.notify();
            return;
        };
        if self
            .columns
            .move_column(dragged.column, at + usize::from(after))
        {
            self.save_now(cx);
        }
        cx.notify();
    }

    /// Follows the app's events about this workspace.
    pub fn on_event(&mut self, event: &AppEvent, cx: &mut Context<Self>) {
        match event {
            AppEvent::RepoSummary(view) if view.workspace_id == self.workspace_id => {
                if view.refreshing || self.history.released() {
                    return;
                }
                if !self.history.shows_repo(&view.repo_id) {
                    return;
                }
                let seen = self.refs_versions.entry(view.repo_id.clone()).or_insert(0);
                if view.refs_version <= *seen {
                    return;
                }
                *seen = view.refs_version;
                self.reload(cx);
            }
            // A group's members may have changed: its repos are resolved
            // again, and a changed answer reads the history again.
            AppEvent::WorkspaceChanged(view)
                if view.id == self.workspace_id && !self.history.released() =>
            {
                if self.follow_tree(cx) == Show::Kept {
                    self.resolve_targets(cx);
                }
            }
            AppEvent::CommitGraphWritten {
                workspace_id,
                repo_id,
                error: Some(error),
                git_stderr,
                ..
            } if *workspace_id == self.workspace_id => {
                let detail = match git_stderr {
                    Some(stderr) => format!("{error}\n{stderr}"),
                    None => error.clone(),
                };
                self.history
                    .set_offer_error(repo_id.clone(), text::cannot_write_commit_graph(&detail));
                cx.notify();
            }
            _ => {}
        }
    }

    /// Reads the shown history again, after the app's events were missed.
    pub fn read_again(&mut self, cx: &mut Context<Self>) {
        if self.history.released() {
            return;
        }
        if self.history.source().is_some() {
            self.resolve_targets(cx);
            self.reload(cx);
        } else {
            self.follow_tree(cx);
        }
    }

    /// The tab went to the background: the rows go, and reads under way
    /// answer nothing any more (UI-31).
    pub fn release(&mut self, cx: &mut Context<Self>) {
        self.remember_scroll();
        self.next_request();
        self.targets_request = self.targets_request.wrapping_add(1);
        self.history.release();
        cx.notify();
    }

    /// The tab is shown again: what the tree selects is read again, as far
    /// as it was read, and scrolled back.
    pub fn resume(&mut self, cx: &mut Context<Self>) {
        if self.history.released() {
            self.follow_tree(cx);
        }
    }

    /// Shows what the tree selects (section 8 "History pane"): a repo or
    /// one of its refs, or the merged history of a group or several repos.
    fn follow_tree(&mut self, cx: &mut Context<Self>) -> Show {
        let source = {
            let tree = self.tree.read(cx);
            match tree.target() {
                Some(TargetSet::Repo { .. }) => tree.selected_item().and_then(history::source_of),
                Some(target) => Some(history::merged_source(target)),
                None => None,
            }
        };
        let show = self.history.show(source);
        if show != Show::Kept {
            self.marked.clear();
            self.selection_anchor = None;
            self.show_text = true;
        }
        if self.history.merged() {
            // The strip shows the tree's summaries as they arrive.
            self.update_names(cx);
            cx.notify();
        }
        let changes = self.changes(cx);
        if self.history.set_changes(changes) {
            cx.notify();
        }
        match show {
            Show::Kept => {}
            Show::Empty => {
                self.request = self.request.wrapping_add(1);
                cx.notify();
            }
            Show::Load(restore) => {
                self.resolve_targets(cx);
                self.read_first(restore, cx);
            }
        }
        show
    }

    /// The changed paths of the repo shown (UI-10), or the dirty repos of a
    /// merged history's resolved set (UI-34).
    fn changes(&self, cx: &App) -> u32 {
        let tree = self.tree.read(cx).tree();
        let summary = |repo: &RepoId| tree.summary(repo).and_then(|view| view.summary.as_ref());
        match self.history.source().map(|source| source.repo.as_ref()) {
            None => 0,
            Some(Some(repo)) => summary(repo).map_or(0, |summary| summary.changes),
            Some(None) => match self.history.targets() {
                Some(Ok(repos)) => {
                    let dirty = repos
                        .iter()
                        .filter(|repo| summary(repo).is_some_and(|summary| summary.dirty))
                        .count();
                    u32::try_from(dirty).unwrap_or(u32::MAX)
                }
                _ => 0,
            },
        }
    }

    /// Takes the repos' names from the tree when one of the merged
    /// history's repos has none yet.
    fn update_names(&mut self, cx: &App) {
        let Some(Ok(repos)) = self.history.targets() else {
            return;
        };
        if repos.iter().all(|repo| self.names.contains_key(repo)) {
            return;
        }
        self.names = self
            .tree
            .read(cx)
            .tree()
            .repo_names()
            .into_iter()
            .map(|(repo, name)| (repo, name.into()))
            .collect();
    }

    /// Resolves a merged history's target set in the core (GRP-5), for the
    /// strip and the dirty count. An answer that changed an earlier one, as
    /// after a group's members changed, reads the history again.
    fn resolve_targets(&mut self, cx: &mut Context<Self>) {
        let Some(source) = self.history.source() else {
            return;
        };
        let HistoryTarget::Merged { target } = &source.target else {
            return;
        };
        let (key, target) = (source.key.clone(), target.clone());
        self.targets_request = self.targets_request.wrapping_add(1);
        let request = self.targets_request;
        let workspace = self.workspace_id.clone();
        let call = self
            .services
            .with_app(move |app| async move { app.target_repos(&workspace, &target) });
        cx.spawn(async move |this, cx| {
            let Some(result) = call.await else {
                return;
            };
            let _ = this.update(cx, |this, cx| {
                if this.targets_request != request {
                    return;
                }
                let targets = result.map_err(|error| error.message);
                let changed = this.history.set_targets(&key, targets);
                this.update_names(cx);
                let changes = this.changes(cx);
                this.history.set_changes(changes);
                if changed {
                    this.reload(cx);
                }
                cx.notify();
            });
        })
        .detach();
    }

    /// Selects `repo` in the tree, as its strip line asks (UI-11).
    fn select_in_tree(&mut self, repo: &RepoId, cx: &mut Context<Self>) {
        self.tree
            .update(cx, |tree, cx| tree.select_member(repo, cx));
    }

    /// Searches the shown history (UI-12); each change supersedes the
    /// previous one, whose answer is dropped.
    fn set_text(&mut self, value: String, cx: &mut Context<Self>) {
        if self.history.source().is_none() || !self.history.set_text(value) {
            return;
        }
        self.read_first(Restore::default(), cx);
    }

    /// Accepts the shown repo's commit-graph offer: the core writes the
    /// file as a job, whose end arrives as `CommitGraphWritten` (PERF-2).
    fn write_commit_graph(&mut self, repo: RepoId, cx: &mut Context<Self>) {
        self.history.answer_offer(&repo);
        cx.notify();
        let workspace = self.workspace_id.clone();
        let asked = repo.clone();
        let call = self
            .services
            .with_app(move |app| async move { app.commit_graph_write(workspace, asked).await });
        cx.spawn(async move |this, cx| {
            if let Some(Err(error)) = call.await {
                let _ = this.update(cx, |this, cx| {
                    this.history
                        .set_offer_error(repo, text::cannot_write_commit_graph(&error.message));
                    cx.notify();
                });
            }
        })
        .detach();
    }

    /// "Don't ask again": the core keeps the answer in the workspace.
    fn decline_commit_graph(&mut self, repo: RepoId, cx: &mut Context<Self>) {
        self.history.answer_offer(&repo);
        cx.notify();
        let workspace = self.workspace_id.clone();
        let asked = repo.clone();
        let call = self
            .services
            .with_app(move |app| async move { app.commit_graph_decline(workspace, asked).await });
        cx.spawn(async move |this, cx| {
            if let Some(Err(error)) = call.await {
                let _ = this.update(cx, |this, cx| {
                    this.history
                        .set_offer_error(repo, text::cannot_keep_answer(&error.message));
                    cx.notify();
                });
            }
        })
        .detach();
    }

    /// Reads `source` from the start, up to the restored row count.
    fn read_first(&mut self, restore: Restore, cx: &mut Context<Self>) {
        let Some(source) = self.history.source().cloned() else {
            return;
        };
        let request = self.next_request();
        self.history.start_load();
        cx.notify();
        let workspace = self.workspace_id.clone();
        let text = self.history.text().to_owned();
        let call = self.services.with_app(move |app| async move {
            history::read(app, workspace, source, text, restore.rows).await
        });
        cx.spawn(async move |this, cx| {
            let result = call.await;
            let _ = this.update(cx, |this, cx| {
                if this.request != request {
                    return;
                }
                match result {
                    Some(Ok(Some(page))) => {
                        this.history.set_page(page);
                        this.restore_scroll(restore);
                    }
                    Some(Ok(None)) => this.history.set_loading(false),
                    Some(Err(error)) => this.history.set_error(error.message),
                    None => this.history.set_loading(false),
                }
                cx.notify();
            });
        })
        .detach();
    }

    /// Reads the shown history again, keeping its rows until the new ones
    /// are ready and the selected commit while it still exists (OPS-7).
    fn reload(&mut self, cx: &mut Context<Self>) {
        let Some(source) = self.history.source().cloned() else {
            return;
        };
        let request = self.next_request();
        let rows = self.history.rows().len();
        let restore = Restore {
            rows,
            scroll_top: self.history.scroll_top(),
        };
        self.history.set_loading(true);
        cx.notify();
        let workspace = self.workspace_id.clone();
        let text = self.history.text().to_owned();
        let call = self.services.with_app(move |app| async move {
            history::read(app, workspace, source, text, rows).await
        });
        cx.spawn(async move |this, cx| {
            let result = call.await;
            let _ = this.update(cx, |this, cx| {
                if this.request != request {
                    return;
                }
                match result {
                    Some(Ok(Some(page))) => {
                        this.history.set_page(page);
                        this.restore_scroll(restore);
                    }
                    Some(Ok(None)) => this.history.set_loading(false),
                    Some(Err(error)) => this.history.set_error(error.message),
                    None => this.history.set_loading(false),
                }
                cx.notify();
            });
        })
        .detach();
    }

    /// Reads the next page when the list nears the end of the loaded rows.
    fn load_more(&mut self, cx: &mut Context<Self>) {
        if self.history.loading() || self.history.done() {
            return;
        }
        let Some(handle) = self.history.handle() else {
            return;
        };
        let request = self.request;
        self.history.set_loading(true);
        let workspace = self.workspace_id.clone();
        let call = self
            .services
            .with_app(move |app| async move { history::read_more(app, workspace, handle).await });
        cx.spawn(async move |this, cx| {
            let result = call.await;
            let _ = this.update(cx, |this, cx| {
                if this.request != request {
                    return;
                }
                match result {
                    Some(Ok(Some(page))) => this.history.append(page),
                    Some(Ok(None)) => this.history.set_loading(false),
                    Some(Err(error)) => this.history.set_error(error.message),
                    None => this.history.set_loading(false),
                }
                cx.notify();
            });
        })
        .detach();
    }

    fn next_request(&mut self) -> u64 {
        self.request = self.request.wrapping_add(1);
        self.request
    }

    fn restore_scroll(&mut self, restore: Restore) {
        let loaded: BTreeSet<String> = self
            .history
            .rows()
            .iter()
            .map(|row| self.history.row_id(row))
            .collect();
        self.marked.retain(|id| loaded.contains(id));
        if self.marked.is_empty()
            && let Some(id) = self.history.selected()
            && self.history.commit_of(id).is_some()
        {
            self.marked.insert(id.to_owned());
        }
        self.history.set_scroll_top(restore.scroll_top);
        if restore.scroll_top > 0. {
            let state = self.scroll.0.borrow_mut();
            state
                .base_handle
                .set_offset(point(px(0.), px(-restore.scroll_top)));
        } else if let Some(index) = self.history.selected_index() {
            self.scroll.scroll_to_item(index, ScrollStrategy::Nearest);
        }
    }

    fn remember_scroll(&mut self) {
        if self.history.loading() {
            return;
        }
        let top = f32::from(self.scroll.0.borrow().base_handle.offset().y).abs();
        self.history.set_scroll_top(top);
    }

    fn selected_index(&self) -> Option<usize> {
        self.history.selected_index()
    }

    /// Selects loaded list row `index`, and scrolls it into view.
    fn select(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        let Some(row_id) = self.history.list_id_at(index) else {
            return;
        };
        window.focus(&self.focus, cx);
        self.marked.clear();
        if self.history.commit_of(&row_id).is_some() {
            self.marked.insert(row_id.clone());
        }
        self.selection_anchor = Some(index);
        self.history.select(Some(row_id));
        self.scroll.scroll_to_item(index, ScrollStrategy::Nearest);
        cx.notify();
    }

    fn move_to(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        let count = self.history.list_len();
        if count == 0 {
            return;
        }
        self.select(index.min(count - 1), window, cx);
    }

    fn step(&mut self, step: isize, window: &mut Window, cx: &mut Context<Self>) {
        let target = match self.selected_index() {
            Some(index) => index.saturating_add_signed(step),
            None => 0,
        };
        self.move_to(target, window, cx);
    }

    /// The rows a page key moves, one less than fit on screen.
    fn page(&self) -> usize {
        let height = f32::from(self.scroll.0.borrow().base_handle.bounds().size.height);
        let fit = (height / f32::from(ROW_HEIGHT)).floor() as usize;
        fit.saturating_sub(1).max(1)
    }

    /// Whether a key of the list must leave the header and its edges alone.
    fn header_has_focus(&self, window: &Window) -> bool {
        self.header_focus.is_focused(window) || self.focused_edge(window).is_some()
    }

    fn select_next(&mut self, _: &SelectNext, window: &mut Window, cx: &mut Context<Self>) {
        if !self.header_has_focus(window) {
            self.step(1, window, cx);
        }
    }

    fn select_previous(&mut self, _: &SelectPrevious, window: &mut Window, cx: &mut Context<Self>) {
        if !self.header_has_focus(window) {
            self.step(-1, window, cx);
        }
    }

    fn page_next(&mut self, _: &PageNext, window: &mut Window, cx: &mut Context<Self>) {
        if !self.header_has_focus(window) {
            let step = self.page() as isize;
            self.step(step, window, cx);
        }
    }

    fn page_previous(&mut self, _: &PagePrevious, window: &mut Window, cx: &mut Context<Self>) {
        if !self.header_has_focus(window) {
            let step = self.page() as isize;
            self.step(-step, window, cx);
        }
    }

    fn select_first(&mut self, _: &SelectFirst, window: &mut Window, cx: &mut Context<Self>) {
        if !self.header_has_focus(window) {
            self.move_to(0, window, cx);
        }
    }

    fn select_last(&mut self, _: &SelectLast, window: &mut Window, cx: &mut Context<Self>) {
        if !self.header_has_focus(window) {
            let count = self.history.list_len();
            if count > 0 {
                self.move_to(count - 1, window, cx);
            }
        }
    }

    fn click(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        self.select(index, window, cx);
    }

    pub fn marked(&self) -> &BTreeSet<String> {
        &self.marked
    }
    fn commit_click(
        &mut self,
        index: usize,
        event: &ClickEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(id) = self.history.list_id_at(index) else {
            return;
        };
        if self.history.commit_of(&id).is_none() {
            self.select(index, window, cx);
            return;
        }
        let mut marked = self.marked.clone();
        let anchor = self.selection_anchor;
        let modifiers = event.modifiers();
        if modifiers.shift {
            marked.clear();
            for at in anchor.unwrap_or(index).min(index)..=anchor.unwrap_or(index).max(index) {
                if let Some(id) = self.history.list_id_at(at)
                    && self.history.commit_of(&id).is_some()
                {
                    marked.insert(id);
                }
            }
        } else if modifiers.secondary(cx) {
            if !marked.remove(&id) {
                marked.insert(id.clone());
            }
        } else {
            marked.clear();
            marked.insert(id);
        }
        self.select(index, window, cx);
        self.marked = marked;
        if modifiers.shift {
            self.selection_anchor = anchor;
        }
        cx.notify();
    }
    fn keyboard_commit_menu(
        &mut self,
        _: &OpenCommitMenu,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.focus.is_focused(window) {
            return;
        }
        if let Some(index) = self.selected_index() {
            let at = self
                .row_bounds
                .borrow()
                .get(&index)
                .map(|b| point(b.left(), b.bottom()))
                .unwrap_or_default();
            self.commit_menu(index, at, window, cx);
        }
    }
    fn commit_menu(
        &mut self,
        index: usize,
        at: Point<Pixels>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(id) = self.history.list_id_at(index) else {
            return;
        };
        let Some((repo, hash)) = self.history.commit_of(&id) else {
            return;
        };
        if !self.marked.contains(&id) {
            self.select(index, window, cx);
        }
        window.focus(&self.focus, cx);
        let count = self.marked.len();
        let same_repo = self
            .marked
            .iter()
            .filter_map(|id| self.history.commit_of(id))
            .all(|(other, _)| other == repo);
        let tree = self.tree.read(cx).tree();
        let blocked = crate::row_context::blocked(tree.summary(&repo));
        let items = crate::row_menus::commit_menu(count, same_repo, blocked)
            .into_iter()
            .enumerate()
            .map(|(at, item)| {
                let choice = match at {
                    0 => Some(RowChoice::Branch(BranchMode::Run {
                        op: BranchOp::CheckoutDetached {
                            revision: hash.clone(),
                        },
                    })),
                    1 => Some(RowChoice::Branch(BranchMode::Create {
                        start: StartPoint::Revision {
                            revision: hash.clone(),
                        },
                    })),
                    6 => Some(RowChoice::Copy(hash.clone())),
                    _ => None,
                };
                (item, choice)
            })
            .collect();
        let repo_name = tree.repo_names().get(&repo).cloned().unwrap_or_default();
        cx.emit(AskRowMenu {
            label: strings::commit_menu::MENU,
            workspace: self.workspace_id.clone(),
            repo: Some(repo),
            repo_name,
            items,
            at,
            focus: self.focus.clone(),
        });
    }

    fn header(&mut self, tokens: &Tokens, window: &Window, cx: &mut Context<Self>) -> AnyElement {
        self.header_bounds.borrow_mut().clear();
        let order = self.columns.order().to_vec();
        let focused = self.header_focus.is_focused(window);
        let mut bar = div()
            .id("history-header")
            .test_support()
            .track_focus(&self.header_focus)
            .tab_stop(true)
            .key_context(HEADER_CONTEXT)
            .on_action(cx.listener(Self::header_left))
            .on_action(cx.listener(Self::header_right))
            .on_action(cx.listener(Self::header_first))
            .on_action(cx.listener(Self::header_last))
            .on_action(cx.listener(Self::open_header_menu))
            .on_drag_move::<SizedColumn>(cx.listener(
                |this, event: &DragMoveEvent<SizedColumn>, _, cx| this.drag_edge(event, cx),
            ))
            .on_drag_move::<DraggedHeader>(cx.listener(
                |this, event: &DragMoveEvent<DraggedHeader>, _, cx| {
                    this.drag_over_header(event, cx)
                },
            ))
            .role(Role::Toolbar)
            .aria_label(text::COLUMNS_LABEL)
            .h(px(28.))
            .flex_shrink_0()
            .flex()
            .flex_row()
            .items_center()
            .px_2()
            .border_b_1()
            .border_color(tokens.border)
            .bg(tokens.bg_2)
            .text_size(px(12.))
            .text_color(tokens.text_secondary);
        for column in order {
            bar = bar.child(self.header_cell(column, focused, tokens, window, cx));
        }
        bar.into_any_element()
    }

    fn header_cell(
        &mut self,
        column: HistoryColumn,
        header_focused: bool,
        tokens: &Tokens,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let merged = self.history.merged();
        let label = column_label(column, merged);
        let id = header_id(column);
        let width = self.columns.width(column);
        let active = self.columns.active() == column;
        let header_bounds = self.header_bounds.clone();
        let mut cell = div()
            .id(id.clone())
            .test_support()
            .role(Role::ColumnHeader)
            .aria_label(label)
            .relative()
            .flex()
            .items_center()
            .h_full()
            .overflow_hidden()
            .when_some(width, |cell, width| cell.w(px(width as f32)).flex_none())
            .when(width.is_none(), |cell| {
                cell.flex_1().min_w(px(MIN_SUBJECT_WIDTH))
            })
            .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                this.header_click(column, window, cx);
            }))
            .on_mouse_down(
                MouseButton::Right,
                cx.listener(move |this, event: &MouseDownEvent, window, cx| {
                    this.header_right_click(column, event.position, window, cx);
                }),
            )
            .on_drag(DraggedHeader { column }, move |dragged, _, _, cx| {
                cx.new(|_| DraggedHeaderView(column_label(dragged.column, merged).into()))
            })
            .on_drop(cx.listener(move |this, dragged: &DraggedHeader, _, cx| {
                this.drop_header(dragged, column, cx);
            }))
            .child(div().flex_1().min_w_0().truncate().px(px(6.)).child(label));
        let width = width.unwrap_or_default();
        if let Some(sized) = SizedColumn::from_column(column) {
            cell = cell.child(self.edge(sized, width, tokens, window, cx));
        }
        if column == HistoryColumn::Subject {
            let subject_width = self.subject_width.clone();
            cell = cell.child(
                canvas(
                    move |bounds, _, _| subject_width.set(f32::from(bounds.size.width)),
                    |_, _, _, _| {},
                )
                .absolute()
                .size_full(),
            );
        }
        cell = cell.child(
            canvas(
                move |drawn, _, _| header_bounds.borrow_mut().push((column, drawn)),
                |_, _, _, _| {},
            )
            .absolute()
            .size_full(),
        );
        if header_focused && active {
            cell = cell.child(
                div()
                    .absolute()
                    .inset_0()
                    .border_2()
                    .border_color(tokens.focus_ring),
            );
        }
        if let Some((target, after)) = self.drop_target
            && target == column
        {
            let marker = div()
                .absolute()
                .top_0()
                .bottom_0()
                .w(px(2.))
                .bg(tokens.accent)
                .when(after, |marker| marker.right_0())
                .when(!after, |marker| marker.left_0());
            cell = cell.child(marker);
        }
        cell.into_any_element()
    }

    fn edge(
        &mut self,
        sized: SizedColumn,
        width: u32,
        tokens: &Tokens,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let Some(focus) = self.edge_focus(sized).cloned() else {
            return div().into_any_element();
        };
        let before = self.edge_before(sized);
        let focused = focus.is_focused(window);
        let label = text::resize_column(column_label(sized.column(), self.history.merged()));
        div()
            .id(ElementId::Name(
                format!("history-edge-{}", column_key(sized.column())).into(),
            ))
            .test_support()
            .track_focus(&focus)
            .tab_stop(true)
            .key_context(EDGE_CONTEXT)
            .on_action(cx.listener(Self::edge_left))
            .on_action(cx.listener(Self::edge_right))
            .role(Role::Splitter)
            .aria_label(label)
            .aria_numeric_value(width as f64)
            .aria_min_numeric_value(MIN_COLUMN_WIDTH as f64)
            .absolute()
            .top_0()
            .bottom_0()
            .w(px(8.))
            .when(before, |edge| edge.left_0())
            .when(!before, |edge| edge.right_0())
            .cursor(CursorStyle::ResizeLeftRight)
            .hover(move |style| style.bg(tokens.accent))
            .when(focused, |edge| edge.bg(tokens.accent))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, event, _, _| {
                    this.start_resize(sized, event);
                }),
            )
            .on_drag(sized, |_, _, _, cx| cx.new(|_| DraggedEdge))
            .on_click(cx.listener(move |this, event: &ClickEvent, _, cx| {
                cx.stop_propagation();
                if event.click_count() == 2 {
                    this.reset_width(sized, cx);
                }
            }))
            .child(
                div()
                    .absolute()
                    .top(px(4.))
                    .bottom(px(4.))
                    .w(px(1.))
                    .when(before, |line| line.left_0())
                    .when(!before, |line| line.right_0())
                    .bg(tokens.border),
            )
            .into_any_element()
    }

    /// List row `index`: the uncommitted row, a commit or a placeholder.
    /// Commit rows are named by their place among the commits.
    fn render_row(&self, index: usize, window: &Window, cx: &mut Context<Self>) -> AnyElement {
        let tokens = *cx.tokens();
        let Some(commit) = index.checked_sub(self.history.offset()) else {
            return self.uncommitted_row(index, window, cx);
        };
        if let Some(row) = self.history.rows().get(commit) {
            return self.commit_row(index, commit, row, window, cx);
        }
        let mut placeholder = div()
            .id(ElementId::Name(format!("history-row-{commit}").into()))
            .test_support()
            .role(Role::ListBoxOption)
            .aria_label(text::LOADING)
            .h(ROW_HEIGHT)
            .w_full()
            .flex()
            .flex_row()
            .items_center()
            .px_2();
        for column in self.columns.order().iter().copied() {
            let width = self.columns.width(column);
            let skeleton = match column {
                HistoryColumn::Subject => div()
                    .h(px(10.))
                    .w(px(240.))
                    .rounded(px(3.))
                    .bg(tokens.bg_3)
                    .into_any_element(),
                _ => div()
                    .h(px(10.))
                    .w(px(width.unwrap_or_default().saturating_sub(12) as f32))
                    .rounded(px(3.))
                    .bg(tokens.bg_3)
                    .into_any_element(),
            };
            placeholder = placeholder.child(
                div()
                    .flex_none()
                    .when(width.is_none(), |cell| cell.flex_1())
                    .when_some(width, |cell, width| cell.w(px(width as f32)))
                    .px(px(6.))
                    .child(skeleton),
            );
        }
        placeholder.into_any_element()
    }

    /// "Uncommitted changes (n)" (UI-10), or "(n repos)" in a merged history
    /// (UI-34), its text where the subject starts.
    fn uncommitted_row(&self, index: usize, window: &Window, cx: &mut Context<Self>) -> AnyElement {
        let tokens = *cx.tokens();
        let selected = self.history.selected() == Some(UNCOMMITTED_ID);
        let focused = self.focus.is_focused(window);
        let label = if self.history.merged() {
            text::uncommitted_repos(self.history.changes())
        } else {
            text::uncommitted(self.history.changes())
        };
        let before_subject: u32 = self
            .columns
            .order()
            .iter()
            .take_while(|column| **column != HistoryColumn::Subject)
            .filter_map(|column| self.columns.width(*column))
            .sum();
        div()
            .id("history-uncommitted")
            .test_support()
            .role(Role::ListBoxOption)
            .aria_label(label.clone())
            .aria_selected(selected)
            .when(selected && focused, |row| row.aria_active_descendant())
            .h(ROW_HEIGHT)
            .w_full()
            .flex()
            .flex_row()
            .items_center()
            .px_2()
            .overflow_hidden()
            .whitespace_nowrap()
            .text_size(px(13.))
            .italic()
            .text_color(tokens.text_primary)
            .when(selected, |row| row.bg(tokens.selection_bg))
            .when(!selected, |row| row.hover(|style| style.bg(tokens.bg_3)))
            .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                this.click(index, window, cx);
            }))
            .child(
                div()
                    .pl(px(before_subject as f32 + 6.))
                    .truncate()
                    .child(label),
            )
            .into_any_element()
    }

    fn commit_row(
        &self,
        index: usize,
        commit: usize,
        row: &Row,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let tokens = *cx.tokens();
        let id = format!("history-row-{commit}");
        let row_id = self.history.row_id(row);
        let selected = self.marked.contains(&row_id);
        let focused = self.focus.is_focused(window);
        let mut line = div()
            .relative()
            .id(ElementId::Name(id.clone().into()))
            .test_support()
            .role(Role::ListBoxOption)
            .aria_label(row.subject.clone())
            .aria_selected(selected)
            .when(selected && focused, |row| row.aria_active_descendant())
            .h(ROW_HEIGHT)
            .w_full()
            .flex()
            .flex_row()
            .items_center()
            .px_2()
            .overflow_hidden()
            .whitespace_nowrap()
            .text_size(px(13.))
            .when(selected, |row| row.bg(tokens.selection_bg))
            .when(!selected, |row| row.hover(|style| style.bg(tokens.bg_3)))
            .on_mouse_down(
                MouseButton::Right,
                cx.listener(move |this, event: &MouseDownEvent, window, cx| {
                    cx.stop_propagation();
                    this.commit_menu(index, event.position, window, cx);
                }),
            )
            .child({
                let bounds = self.row_bounds.clone();
                canvas(
                    move |b, _, _| {
                        bounds.borrow_mut().insert(index, b);
                    },
                    |_, _, _, _| {},
                )
                .absolute()
                .size_full()
            })
            .on_click(cx.listener(move |this, event: &ClickEvent, window, cx| {
                this.commit_click(index, event, window, cx);
            }));
        for column in self.columns.order().iter().copied() {
            let width = self.columns.width(column);
            let cell = match column {
                // A merged history has the repo's name in the graph's place.
                HistoryColumn::Graph if self.history.merged() => {
                    let name = self.repo_name(&row.repo_id);
                    div()
                        .id(ElementId::Name(format!("{id}-repo").into()))
                        .test_support()
                        .role(Role::Label)
                        .aria_label(name.clone())
                        .w(px(width.unwrap_or_default() as f32))
                        .flex_none()
                        .px(px(6.))
                        .truncate()
                        .text_color(tokens.text_secondary)
                        .child(name)
                        .into_any_element()
                }
                HistoryColumn::Graph => {
                    let graph = row.graph.clone();
                    let colors = tokens.graph.map(Hsla::from);
                    div()
                        .id(ElementId::Name(format!("{id}-graph").into()))
                        .test_support()
                        .w(px(width.unwrap_or_default() as f32))
                        .flex_none()
                        .h_full()
                        .child(
                            canvas(
                                |_, _, _| (),
                                move |bounds, (), window, _| {
                                    if let Some(graph) = &graph {
                                        paint_graph(graph, bounds, &colors, window);
                                    }
                                },
                            )
                            .size_full(),
                        )
                        .into_any_element()
                }
                HistoryColumn::Subject => {
                    let mut subject = div()
                        .id(ElementId::Name(format!("{id}-subject").into()))
                        .test_support()
                        .role(Role::Label)
                        .aria_label(row.subject.clone())
                        .flex()
                        .flex_row()
                        .items_center()
                        .flex_1()
                        .min_w_0()
                        .px(px(6.))
                        .gap_1()
                        .overflow_hidden()
                        .text_color(tokens.text_primary);
                    for (at, (kind, name)) in row.refs.iter().enumerate() {
                        subject =
                            subject.child(self.ref_label(&id, at, *kind, name.clone(), &tokens));
                    }
                    subject
                        .child(div().truncate().child(row.subject.clone()))
                        .into_any_element()
                }
                HistoryColumn::Author => div()
                    .id(ElementId::Name(format!("{id}-author").into()))
                    .test_support()
                    .role(Role::Label)
                    .aria_label(row.author.clone())
                    .w(px(width.unwrap_or_default() as f32))
                    .flex_none()
                    .px(px(6.))
                    .truncate()
                    .text_color(tokens.text_secondary)
                    .child(row.author.clone())
                    .into_any_element(),
                HistoryColumn::Date => div()
                    .id(ElementId::Name(format!("{id}-date").into()))
                    .test_support()
                    .role(Role::Label)
                    .aria_label(row.date.clone())
                    .aria_description(row.dates.clone())
                    .w(px(width.unwrap_or_default() as f32))
                    .flex_none()
                    .px(px(6.))
                    .whitespace_nowrap()
                    .text_color(tokens.text_secondary)
                    .child(row.date.clone())
                    .into_any_element(),
                HistoryColumn::Hash => div()
                    .id(ElementId::Name(format!("{id}-hash").into()))
                    .test_support()
                    .role(Role::Label)
                    .aria_label(row.short_hash.clone())
                    .w(px(width.unwrap_or_default() as f32))
                    .flex_none()
                    .px(px(6.))
                    .whitespace_nowrap()
                    .text_size(px(12.))
                    .text_color(tokens.text_secondary)
                    .child(row.short_hash.clone())
                    .into_any_element(),
            };
            line = line.child(cell);
        }
        line.into_any_element()
    }

    fn ref_label(
        &self,
        row: &str,
        at: usize,
        kind: RefKind,
        name: SharedString,
        tokens: &Tokens,
    ) -> AnyElement {
        let (title, color) = match kind {
            RefKind::Head => (text::HEAD, tokens.danger),
            RefKind::CurrentBranch => (text::CURRENT_BRANCH, tokens.accent),
            RefKind::LocalBranch => (text::LOCAL_BRANCH, tokens.accent),
            RefKind::RemoteBranch => (text::REMOTE_BRANCH, tokens.success),
            RefKind::Tag => (text::TAG, tokens.warning),
        };
        let visible = if kind == RefKind::CurrentBranch {
            text::current_ref(&name).into()
        } else {
            name.clone()
        };
        let label = text::ref_label(title, &name);
        div()
            .id(ElementId::Name(format!("{row}-ref-{at}").into()))
            .test_support()
            .role(Role::Label)
            .aria_label(label)
            .flex_none()
            .px(px(5.))
            .rounded(px(4.))
            .text_size(px(11.))
            .bg(color.alpha(0.14))
            .text_color(color)
            .child(visible)
            .into_any_element()
    }

    fn repo_name(&self, repo: &RepoId) -> SharedString {
        self.names
            .get(repo)
            .cloned()
            .unwrap_or_else(|| repo.0.clone().into())
    }

    /// The summary strip (UI-11): one line per repo of the merged history's
    /// set in the core's order, eight shown and the rest scrolled, laid out
    /// only on screen (PERF-1).
    fn strip(&self, repos: usize, cx: &mut Context<Self>) -> AnyElement {
        let tokens = *cx.tokens();
        div()
            .id("history-strip")
            .test_support()
            .role(Role::List)
            .aria_label(text::STRIP)
            .flex_shrink_0()
            .h(ROW_HEIGHT * repos.min(STRIP_LINES) as f32)
            .border_b_1()
            .border_color(tokens.border)
            .child(
                uniform_list(
                    "history-strip-lines",
                    repos,
                    cx.processor(Self::strip_lines),
                )
                .track_scroll(&self.strip_scroll)
                .size_full(),
            )
            .into_any_element()
    }

    fn strip_lines(
        &mut self,
        range: Range<usize>,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) -> Vec<AnyElement> {
        let repos = match self.history.targets() {
            Some(Ok(repos)) => repos.get(range.clone()).unwrap_or_default().to_vec(),
            _ => Vec::new(),
        };
        range
            .zip(repos)
            .map(|(index, repo)| self.strip_line(index, repo, cx))
            .collect()
    }

    /// One repo of the strip: name, branch or short detached hash, ahead and
    /// behind, the dirty count and the marks, as the tree has them. A
    /// summary from the cache is dimmed until read again (PERF-3); a click
    /// selects the repo in the tree.
    fn strip_line(&self, index: usize, repo: RepoId, cx: &mut Context<Self>) -> AnyElement {
        let tokens = *cx.tokens();
        let id = format!("history-strip-{index}");
        let name = self.repo_name(&repo);
        let tree = self.tree.read(cx).tree();
        let view = tree.summary(&repo).cloned();
        let summary = view.as_ref().and_then(|view| view.summary.as_ref());
        let stale = view.as_ref().is_none_or(|view| view.stale);
        let mut line = div()
            .relative()
            .id(ElementId::Name(id.clone().into()))
            .test_support()
            .role(Role::ListItem)
            .aria_label(name.clone())
            .when(stale, |line| line.aria_description(strings::tree::STALE))
            .h(ROW_HEIGHT)
            .w_full()
            .flex()
            .flex_row()
            .items_center()
            .gap_2()
            .px_2()
            .overflow_hidden()
            .whitespace_nowrap()
            .text_size(px(13.))
            .cursor(CursorStyle::PointingHand)
            .hover(|line| line.bg(tokens.bg_3))
            .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                this.select_in_tree(&repo, cx);
            }))
            .child(
                div()
                    .flex_shrink_0()
                    .text_color(if stale {
                        tokens.text_secondary
                    } else {
                        tokens.text_primary
                    })
                    .child(name),
            );
        if let Some(summary) = summary {
            let head = match &summary.head {
                Head::Branch { name, .. } => name.clone(),
                Head::Detached { hash } => hash.chars().take(7).collect(),
            };
            line = line.child(
                div()
                    .id(ElementId::Name(format!("{id}-head").into()))
                    .test_support()
                    .role(Role::Label)
                    .aria_label(head.clone())
                    .min_w_0()
                    .truncate()
                    .text_color(tokens.text_secondary)
                    .child(head),
            );
            if let Some(upstream) = &summary.upstream {
                let counts = [
                    (
                        "ahead",
                        upstream.ahead,
                        strings::tree::ahead(upstream.ahead),
                        strings::tree::ahead_mark(upstream.ahead),
                    ),
                    (
                        "behind",
                        upstream.behind,
                        strings::tree::behind(upstream.behind),
                        strings::tree::behind_mark(upstream.behind),
                    ),
                ];
                for (what, count, label, shown) in counts {
                    if count > 0 {
                        line = line.child(
                            mark(format!("{id}-{what}"), label)
                                .text_color(tokens.text_secondary)
                                .child(shown),
                        );
                    }
                }
            }
            if summary.dirty && summary.changes > 0 {
                line = line.child(
                    mark(format!("{id}-dirty"), text::changed_files(summary.changes))
                        .text_color(tokens.accent)
                        .child(text::changed_mark(summary.changes)),
                );
            }
        }
        if let Some(view) = &view {
            if view.conflict {
                line = line.child(
                    mark(format!("{id}-conflict"), strings::tree::CONFLICT).child(
                        svg()
                            .path(icons::CONFLICT)
                            .size(ICON)
                            .text_color(tokens.danger),
                    ),
                );
            }
            if view.missing {
                line = line.child(
                    mark(format!("{id}-missing"), strings::tree::FOLDER_MISSING).child(
                        svg()
                            .path(icons::WARNING)
                            .size(ICON)
                            .text_color(tokens.warning),
                    ),
                );
            }
        }
        line.into_any_element()
    }

    fn render_rows(
        &mut self,
        range: Range<usize>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Vec<AnyElement> {
        if !self.history.done() && range.end + PREFETCH_ROWS >= self.history.list_len() {
            self.load_more(cx);
        }
        range
            .map(|index| self.render_row(index, window, cx))
            .collect()
    }
}

impl HistoryPane {
    /// The shown repo's commit-graph offer, or why answering it failed
    /// (PERF-2).
    fn offer(&self, tokens: &Tokens, cx: &mut Context<Self>) -> Option<AnyElement> {
        if let Some(repo) = self.history.offer().cloned() {
            let (write, decline) = (repo.clone(), repo);
            return Some(
                div()
                    .id("history-offer")
                    .test_support()
                    .role(Role::Group)
                    .aria_label(text::COMMIT_GRAPH_OFFER)
                    .flex()
                    .flex_row()
                    .flex_shrink_0()
                    .items_center()
                    .gap_2()
                    .px_3()
                    .py_1()
                    .bg(tokens.bg_2)
                    .text_color(tokens.text_primary)
                    .border_b_1()
                    .border_color(tokens.border)
                    .child(div().flex_1().min_w_0().child(text::COMMIT_GRAPH_WHY))
                    .child(
                        div()
                            .flex()
                            .flex_row()
                            .flex_shrink_0()
                            .gap_1()
                            .child(
                                Button::new("history-offer-write")
                                    .primary()
                                    .compact()
                                    .label(text::WRITE_COMMIT_GRAPH)
                                    .on_click(cx.listener(move |this, _, _, cx| {
                                        this.write_commit_graph(write.clone(), cx);
                                    })),
                            )
                            .child(
                                Button::new("history-offer-decline")
                                    .ghost()
                                    .compact()
                                    .label(text::DONT_ASK_AGAIN)
                                    .on_click(cx.listener(move |this, _, _, cx| {
                                        this.decline_commit_graph(decline.clone(), cx);
                                    })),
                            ),
                    )
                    .into_any_element(),
            );
        }
        let (repo, error) = self.history.offer_error()?;
        let (repo, error) = (repo.clone(), SharedString::from(error.to_owned()));
        Some(
            div()
                .id("history-offer-error")
                .test_support()
                .role(Role::Alert)
                .aria_label(error.clone())
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
                .child(div().flex_1().min_w_0().child(error))
                .child(
                    div().flex_shrink_0().child(
                        Button::new("history-offer-error-dismiss")
                            .ghost()
                            .compact()
                            .label(strings::common::DISMISS)
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.history.dismiss_offer_error(&repo);
                                cx.notify();
                            })),
                    ),
                )
                .into_any_element(),
        )
    }
}

impl Focusable for HistoryPane {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus.clone()
    }
}

fn column_key(column: HistoryColumn) -> &'static str {
    match column {
        HistoryColumn::Graph => "graph",
        HistoryColumn::Subject => "subject",
        HistoryColumn::Author => "author",
        HistoryColumn::Date => "date",
        HistoryColumn::Hash => "hash",
    }
}

fn header_id(column: HistoryColumn) -> ElementId {
    ElementId::Name(format!("history-column-{}", column_key(column)).into())
}

/// A column's name; in a merged history the graph column has the repo's
/// name (UI-9).
fn column_label(column: HistoryColumn, merged: bool) -> &'static str {
    match column {
        HistoryColumn::Graph if merged => text::REPOSITORY,
        HistoryColumn::Graph => text::GRAPH,
        HistoryColumn::Subject => text::SUBJECT,
        HistoryColumn::Author => text::AUTHOR,
        HistoryColumn::Date => text::DATE,
        HistoryColumn::Hash => text::HASH,
    }
}

impl Render for HistoryPane {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.remember_scroll();
        let tokens = *cx.tokens();
        // In another selection, the box shows the text kept with it.
        if self.show_text {
            self.show_text = false;
            let text = self.history.text().to_owned();
            self.search
                .update(cx, |search, cx| search.set_value(text, window, cx));
        }
        // A group or several repos: the strip above their history, or why
        // there is none (UI-11).
        let targets = self
            .history
            .targets()
            .filter(|_| self.history.merged())
            .cloned();
        let (strip, unresolved) = match targets {
            None => (None, false),
            Some(Ok(repos)) if repos.is_empty() => (
                Some(message(
                    "history-no-repos",
                    text::NO_REPOS,
                    tokens.text_secondary,
                )),
                true,
            ),
            Some(Ok(repos)) => (Some(self.strip(repos.len(), cx)), false),
            Some(Err(error)) => {
                let error = text::cannot_show(&error);
                let alert = div()
                    .id("history-targets-error")
                    .test_support()
                    .role(Role::Alert)
                    .aria_label(error.clone())
                    .p_3()
                    .text_color(tokens.danger)
                    .child(error)
                    .into_any_element();
                (Some(alert), true)
            }
        };
        let shown = self.history.source().is_some() && !unresolved;
        let search = shown.then(|| {
            div().flex_shrink_0().p_1().child(
                Input::new(&self.search)
                    .id("history-search")
                    .aria_label(text::SEARCH_LABEL)
                    .cleanable(true),
            )
        });
        let offer = self.offer(&tokens, cx);
        let header = shown.then(|| self.header(&tokens, window, cx));
        let view = cx.entity().downgrade();
        let error = self.history.error().filter(|_| shown).map(|error| {
            let error = text::cannot_read(error);
            div()
                .id("history-error")
                .test_support()
                .role(Role::Alert)
                .aria_label(error.clone())
                .px_2()
                .py_1()
                .text_color(tokens.danger)
                .child(error)
        });
        let body = if self.history.source().is_none() {
            message("history-empty", text::NO_SOURCE, tokens.text_secondary)
        } else if unresolved {
            div().into_any_element()
        } else if self.history.list_len() == 0
            && self.history.done()
            && self.history.error().is_none()
        {
            let empty = if self.history.text().is_empty() {
                text::NO_COMMITS
            } else {
                text::NO_MATCH
            };
            message("history-empty", empty, tokens.text_secondary)
        } else {
            let count = self.history.list_len()
                + if self.history.done() {
                    0
                } else {
                    PLACEHOLDER_ROWS
                };
            div()
                .id("history-list")
                .test_support()
                .role(Role::ListBox)
                .aria_label(text::LABEL)
                .flex_1()
                .min_h_0()
                .child(
                    uniform_list("history-rows", count, cx.processor(Self::render_rows))
                        .track_scroll(&self.scroll)
                        .size_full(),
                )
                .into_any_element()
        };
        div()
            .id("history")
            .test_support()
            .track_focus(&self.focus)
            .key_context(CONTEXT)
            .role(Role::Group)
            .aria_label(strings::regions::HISTORY)
            .on_action(cx.listener(Self::keyboard_commit_menu))
            .on_action(cx.listener(Self::select_next))
            .on_action(cx.listener(Self::select_previous))
            .on_action(cx.listener(Self::page_next))
            .on_action(cx.listener(Self::page_previous))
            .on_action(cx.listener(Self::select_first))
            .on_action(cx.listener(Self::select_last))
            .size_full()
            .flex()
            .flex_col()
            .text_sm()
            .children(strip)
            .children(search)
            .children(offer)
            .children(header)
            .children(error)
            .child(body)
            .child(
                canvas(
                    |_, _, _| (),
                    move |_, (), window, _| {
                        let view = view.clone();
                        window.on_mouse_event(move |_: &MouseUpEvent, phase, _, cx| {
                            if phase == DispatchPhase::Bubble {
                                let _ = view.update(cx, |this, _| this.resizing = None);
                            }
                        });
                    },
                )
                .absolute()
                .size_full(),
            )
            .children(self.menu.as_ref().map(|open| open.menu.clone()))
    }
}

/// A mark on a strip line, named for screen readers.
fn mark(
    id: String,
    label: impl Into<SharedString>,
) -> impl ParentElement + Styled + StatefulInteractiveElement + IntoElement {
    div()
        .id(ElementId::Name(id.into()))
        .test_support()
        .role(Role::Image)
        .aria_label(label)
        .flex_shrink_0()
        .flex()
        .items_center()
}

fn message(id: &'static str, text: &'static str, color: impl Into<Hsla>) -> AnyElement {
    div()
        .id(id)
        .test_support()
        .role(Role::Label)
        .aria_label(text)
        .p_3()
        .text_color(color)
        .child(text)
        .into_any_element()
}

fn fill_quad(bounds: Bounds<Pixels>, color: Hsla) -> gpui_kit::PaintQuad {
    fill(bounds, color)
}

/// One row's lines and dot, from the lanes the core computed (ARCH-6).
fn paint_graph(graph: &GraphRow, bounds: Bounds<Pixels>, colors: &[Hsla; 8], window: &mut Window) {
    let x = |lane: u32| bounds.origin.x + px(LANE_WIDTH / 2. + lane as f32 * LANE_WIDTH);
    let top = bounds.origin.y;
    let middle = top + px(f32::from(ROW_HEIGHT) / 2.);
    let bottom = top + ROW_HEIGHT;
    // Lanes past the column's right edge are not drawn at all.
    let lanes = (f32::from(bounds.size.width) / LANE_WIDTH).ceil() as u32;
    window.with_content_mask(Some(ContentMask { bounds }), |window| {
        for (color, fill) in colors.iter().enumerate() {
            let mut path = PathBuilder::stroke(px(1.5));
            let mut any = false;
            for (edges, from_y, to_y) in [(&graph.up, top, middle), (&graph.down, middle, bottom)] {
                for edge in edges.iter().filter(|edge| {
                    edge.color as usize % 8 == color && edge.from.min(edge.to) < lanes
                }) {
                    if edge.from == edge.to {
                        // Most edges pass straight down: a quad, no tessellation.
                        let line = Bounds::new(
                            point(x(edge.from) - px(0.75), from_y),
                            size(px(1.5), to_y - from_y),
                        );
                        window.paint_quad(fill_quad(line, *fill));
                    } else {
                        path.move_to(point(x(edge.from), from_y));
                        path.line_to(point(x(edge.to), to_y));
                        any = true;
                    }
                }
            }
            if any && let Ok(path) = path.build() {
                window.paint_path(path, *fill);
            }
        }
        let dot = Bounds::new(
            point(x(graph.lane) - px(4.), middle - px(4.)),
            size(px(8.), px(8.)),
        );
        window.paint_quad(fill(dot, colors[graph.color as usize % 8]).corner_radii(px(4.)));
    });
}
