//! The tree pane of one workspace (UI-1 to UI-4, UI-6, UI-8), a port of the
//! read-only parts of rc.1's `tree/TreePane.tsx`, `RepoRow.tsx` and
//! `GroupRow.tsx`: the filter box and the tree, virtualized (PERF-1), with
//! single selection and multi-selection of repo rows by mouse and keyboard.
//! The window keeps one per open workspace, so each tab keeps its tree
//! (UI-29). Everything shown comes from the core; this lays it out and
//! forwards input.

use crate::branch_dialog::{AskBranch, BranchMode};
use crate::row_context::{AskRowMenu, RowChoice};
use crate::row_menus::{self, TreeAction as A};
use gitcroc_core::{BranchOp, RemoteBranchRef, StartPoint};
use std::cell::RefCell;
use std::collections::HashMap;
use std::ops::Range;
use std::rc::Rc;
use std::time::Duration;

use gitcroc_app::app::AppEvent;
use gitcroc_core::{
    GroupBranch, GroupSummary, Head, RepoId, SummaryView, TargetSet, TreeItem, WorkspaceId,
};
use gpui_component::input::{Input, InputEvent, InputState};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::{
    Animation, AnimationExt, AnyElement, App, AppContext, Bounds, ClickEvent, Context, ElementId,
    Entity, EventEmitter, FocusHandle, Focusable, FontWeight, InteractiveElement, IntoElement,
    KeyBinding, KeyBindingContextPredicate, MouseButton, MouseDownEvent, ParentElement, Pixels,
    Point, Render, Role, ScrollStrategy, SharedString, StatefulInteractiveElement, Styled,
    Subscription, Task, TestSupportExt, Transformation, UniformListScrollHandle, Window, actions,
    canvas, div, percentage, point, px, svg, uniform_list,
};

use crate::icons;
use crate::services::Services;
use crate::strings::{self, tree as text};
use crate::theme::ActiveTokens;
use crate::tree::{Row, RowKind, Tree, repo_of};

actions!(
    tree,
    [
        OpenRowMenu,
        SelectNext,
        SelectPrevious,
        /// Shift+Down: the multi-selection grows over repo rows (UI-6).
        ExtendNext,
        ExtendPrevious,
        /// Ctrl/Cmd+Down: the keyboard moves, the selection stays.
        CursorNext,
        CursorPrevious,
        /// Ctrl/Cmd+Space: the row the keyboard is on joins or leaves the
        /// selection.
        ToggleSelected,
        /// Right: opens the row, then goes to its first child.
        Expand,
        /// Left: closes the row, then goes to its parent.
        Collapse,
        /// Enter: opens or closes the row.
        Activate,
        SelectFirst,
        SelectLast,
    ]
);

/// The key context of the tree.
const CONTEXT: &str = "GitcrocTree";

/// One row's height; `uniform_list` lays out only those on screen.
pub const ROW_HEIGHT: Pixels = px(22.);
const INDENT: f32 = 14.;
const ICON: Pixels = px(12.);

/// The keys of the tree (UI-22).
pub fn bind(cx: &mut App) {
    let context = KeyBindingContextPredicate::parse(CONTEXT).ok().map(Rc::new);
    let bind = |keys: &str, action: Box<dyn gpui_kit::Action>, cx: &mut App| {
        KeyBinding::load(keys, action, context.clone(), false, None, cx).ok()
    };
    let bindings = [
        bind("shift-f10", Box::new(OpenRowMenu), cx),
        bind("menu", Box::new(OpenRowMenu), cx),
        bind("down", Box::new(SelectNext), cx),
        bind("up", Box::new(SelectPrevious), cx),
        bind("shift-down", Box::new(ExtendNext), cx),
        bind("shift-up", Box::new(ExtendPrevious), cx),
        bind("secondary-down", Box::new(CursorNext), cx),
        bind("secondary-up", Box::new(CursorPrevious), cx),
        bind("secondary-space", Box::new(ToggleSelected), cx),
        bind("right", Box::new(Expand), cx),
        bind("left", Box::new(Collapse), cx),
        bind("enter", Box::new(Activate), cx),
        bind("home", Box::new(SelectFirst), cx),
        bind("end", Box::new(SelectLast), cx),
    ];
    cx.bind_keys(bindings.into_iter().flatten());
}

/// How a key or click changes the selection.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum How {
    Select,
    Toggle,
    Extend,
    Cursor,
}

/// The tree of one workspace.
pub struct TreePane {
    services: Services,
    workspace_id: WorkspaceId,
    tree: Tree,
    /// `tree`'s rows, laid out again after each change.
    rows: Rc<Vec<Row>>,
    focus: FocusHandle,
    filter: Entity<InputState>,
    scroll: UniformListScrollHandle,
    /// Summaries heard while the first read runs, applied after it.
    heard: Option<Vec<SummaryView>>,
    row_bounds: Rc<RefCell<HashMap<usize, Bounds<Pixels>>>>,
    operation_error: Option<SharedString>,
    _filter_events: Subscription,
}

impl EventEmitter<AskRowMenu> for TreePane {}
impl EventEmitter<AskBranch> for TreePane {}

impl TreePane {
    pub fn new(
        services: Services,
        workspace_id: WorkspaceId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let filter = cx.new(|cx| InputState::new(window, cx).placeholder(text::FILTER));
        let filter_events = cx.subscribe(&filter, |this, filter, event, cx| {
            if let InputEvent::Change = event {
                let value = filter.read(cx).value().to_string();
                this.set_filter(value, cx);
            }
        });
        let mut this = Self {
            services,
            workspace_id,
            tree: Tree::default(),
            rows: Rc::default(),
            focus: cx.focus_handle(),
            filter,
            scroll: UniformListScrollHandle::new(),
            heard: None,
            row_bounds: Rc::default(),
            operation_error: None,
            _filter_events: filter_events,
        };
        this.read_tree(cx);
        this.read_summaries(cx);
        this
    }

    pub fn tree(&self) -> &Tree {
        &self.tree
    }

    /// The rows on screen.
    pub fn rows(&self) -> &[Row] {
        &self.rows
    }

    /// What the toolbar acts on (GRP-5).
    pub fn target(&self) -> Option<TargetSet> {
        self.tree.target()
    }

    /// The item of the row the keyboard is on, which the history shows.
    pub fn selected_item(&self) -> Option<&TreeItem> {
        self.tree.selected_item()
    }

    /// Follows the app's events about this workspace.
    pub fn on_event(&mut self, event: &AppEvent, cx: &mut Context<Self>) {
        match event {
            AppEvent::RepoSummary(view) if view.workspace_id == self.workspace_id => {
                match &mut self.heard {
                    Some(heard) => heard.push(view.clone()),
                    None => self.tree.set_summary(view.clone()),
                }
                self.changed(cx);
            }
            AppEvent::GroupSummary(summary) if summary.workspace_id == self.workspace_id => {
                self.tree.set_group(summary.clone());
                self.changed(cx);
            }
            AppEvent::WorkspaceChanged(view) if view.id == self.workspace_id => {
                self.read_tree(cx);
            }
            // A repo's refs changed: its loaded places, and the loaded
            // Branches nodes, which may count it, are read again.
            AppEvent::RepoRefsChanged(repo) if repo.workspace_id == self.workspace_id => {
                let stale = self.tree.stale_loads(&repo.repo_id);
                self.reload(stale, cx);
            }
            _ => {}
        }
    }

    /// Reads everything again, after the app's events were missed.
    pub fn read_again(&mut self, cx: &mut Context<Self>) {
        self.read_tree(cx);
        self.read_summaries(cx);
        let loaded = self.tree.all_loads();
        self.reload(loaded, cx);
    }

    fn changed(&mut self, cx: &mut Context<Self>) {
        self.rows = Rc::new(self.tree.rows());
        cx.notify();
    }

    fn read_tree(&mut self, cx: &mut Context<Self>) {
        let id = self.workspace_id.clone();
        let call = self
            .services
            .with_app(|app| async move { app.tree_get(&id) });
        cx.spawn(async move |this, cx| {
            let Some(roots) = call.await else {
                return;
            };
            let _ = this.update(cx, |this, cx| {
                this.tree.set_roots(roots.map_err(|error| error.message));
                this.changed(cx);
                this.refilter(cx);
            });
        })
        .detach();
    }

    /// Reads the summaries; an event heard meanwhile is newer, so it is
    /// applied after the read.
    fn read_summaries(&mut self, cx: &mut Context<Self>) {
        self.heard.get_or_insert_with(Vec::new);
        let id = self.workspace_id.clone();
        let call = self
            .services
            .with_app(|app| async move { app.workspace_summaries(&id) });
        cx.spawn(async move |this, cx| {
            let views = call.await;
            let _ = this.update(cx, |this, cx| {
                if let Some(Ok(views)) = views {
                    this.tree.set_summaries(views);
                }
                for view in this.heard.take().unwrap_or_default() {
                    this.tree.set_summary(view);
                }
                this.changed(cx);
            });
        })
        .detach();
    }

    /// Loads the children of `item`'s node: a repo's refs where it
    /// appears, or a group's Branches node (ARCH-8).
    fn load(&mut self, item: TreeItem, cx: &mut Context<Self>) -> Task<()> {
        self.tree.start_load(&item);
        let workspace = self.workspace_id.clone();
        let asked = item.clone();
        let call = self.services.with_app(|app| async move {
            match asked {
                TreeItem::GroupBranches { group_id } => {
                    app.group_branches(workspace, group_id).await
                }
                TreeItem::Local { repo_id, group_id }
                | TreeItem::Remotes { repo_id, group_id }
                | TreeItem::Tags { repo_id, group_id }
                | TreeItem::Stashes { repo_id, group_id } => {
                    app.refs_get(workspace, repo_id, group_id).await
                }
                _ => Ok(Vec::new()),
            }
        });
        cx.spawn(async move |this, cx| {
            let nodes = call.await;
            let _ = this.update(cx, |this, cx| {
                let nodes = match nodes {
                    Some(nodes) => nodes.map_err(|error| error.message),
                    None => Ok(Vec::new()),
                };
                this.tree.finish_load(item, nodes);
                this.changed(cx);
            });
        })
    }

    /// Loads `items` again, then the filter's answer.
    fn reload(&mut self, items: Vec<TreeItem>, cx: &mut Context<Self>) {
        if items.is_empty() {
            return;
        }
        let loads: Vec<Task<()>> = items.into_iter().map(|item| self.load(item, cx)).collect();
        cx.spawn(async move |this, cx| {
            for load in loads {
                load.await;
            }
            let _ = this.update(cx, |this, cx| this.refilter(cx));
        })
        .detach();
    }

    /// Filters the tree; each change supersedes the previous one (UI-8).
    fn set_filter(&mut self, value: String, cx: &mut Context<Self>) {
        if value == self.tree.filter() {
            return;
        }
        self.tree.set_filter(value);
        self.changed(cx);
        self.refilter(cx);
    }

    /// Asks the core again for the filter text's tree.
    fn refilter(&mut self, cx: &mut Context<Self>) {
        let text = self.tree.filter().to_owned();
        if text.is_empty() {
            return;
        }
        let (workspace, asked) = (self.workspace_id.clone(), text.clone());
        let call = self
            .services
            .with_app(|app| async move { app.tree_filter(workspace, asked).await });
        cx.spawn(async move |this, cx| {
            let result = call.await;
            let _ = this.update(cx, |this, cx| {
                match result {
                    // `None`: a newer request superseded this one.
                    Some(Ok(Some(nodes))) => this.tree.set_filtered(&text, nodes),
                    Some(Err(error)) if this.tree.filter() == text => {
                        this.tree.set_tree_error(error.message);
                    }
                    _ => return,
                }
                this.changed(cx);
            });
        })
        .detach();
    }

    /// Tells the core which repo is selected, so it is read first (PERF-4).
    fn selected_repo(&self, moved: Option<Option<RepoId>>) {
        if let Some(repo) = moved {
            let workspace = self.workspace_id.clone();
            let _sent = self
                .services
                .with_app(|app| async move { app.repo_select(workspace, repo) });
        }
    }

    fn expand(&mut self, row: &Row, expanded: bool, cx: &mut Context<Self>) {
        if let Some(item) = self.tree.set_expanded(row, expanded) {
            self.load(item, cx).detach();
        }
        self.changed(cx);
    }

    fn toggle_row(&mut self, row: &Row, cx: &mut Context<Self>) {
        if let RowKind::Node {
            expandable: true,
            expanded,
            ..
        } = row.kind
        {
            self.expand(row, !expanded, cx);
        }
    }

    /// Selects row `index` as `how` says, and scrolls it into view.
    fn select_at(&mut self, index: usize, how: How, cx: &mut Context<Self>) {
        let rows = self.rows.clone();
        let Some(row) = rows.get(index).filter(|row| row.item().is_some()) else {
            return;
        };
        let moved = match how {
            How::Select => self.tree.select(row),
            How::Toggle => self.tree.toggle(row),
            How::Extend => self.tree.extend(&rows, row),
            How::Cursor => self.tree.move_cursor(row),
        };
        self.selected_repo(moved);
        self.scroll.scroll_to_item(index, ScrollStrategy::Nearest);
        self.changed(cx);
    }

    /// Selects the row of `repo` within the selection, as clicking its line
    /// of the summary strip does (UI-11).
    pub fn select_member(&mut self, repo: &RepoId, cx: &mut Context<Self>) {
        let moved = self.tree.select_member(repo);
        self.selected_repo(moved);
        self.changed(cx);
        if let Some(index) = self.selected_index() {
            self.scroll.scroll_to_item(index, ScrollStrategy::Nearest);
        }
    }

    fn selected_index(&self) -> Option<usize> {
        let selected = self.tree.selected()?;
        self.rows.iter().position(|row| row.id == selected)
    }

    /// The next node row from `from` in direction `step`; with `repos`, the
    /// next repo row.
    fn next_node(&self, from: Option<usize>, step: isize, repos: bool) -> Option<usize> {
        let count = self.rows.len() as isize;
        let mut index = from.map_or(if step > 0 { -1 } else { count }, |from| from as isize);
        loop {
            index += step;
            let row = self.rows.get(usize::try_from(index).ok()?)?;
            if row.item().is_some() && (!repos || row.is_repo()) {
                return Some(index as usize);
            }
        }
    }

    /// Up and Down, with Shift over repo rows (UI-6).
    fn step(&mut self, step: isize, how: How, cx: &mut Context<Self>) {
        let repos = how == How::Extend;
        let next = match self.selected_index() {
            Some(index) => self.next_node(Some(index), step, repos),
            None => self.next_node(None, 1, repos),
        };
        if let Some(next) = next {
            self.select_at(next, how, cx);
        }
    }

    fn select_next(&mut self, _: &SelectNext, _: &mut Window, cx: &mut Context<Self>) {
        self.step(1, How::Select, cx);
    }

    fn select_previous(&mut self, _: &SelectPrevious, _: &mut Window, cx: &mut Context<Self>) {
        self.step(-1, How::Select, cx);
    }

    fn extend_next(&mut self, _: &ExtendNext, _: &mut Window, cx: &mut Context<Self>) {
        self.step(1, How::Extend, cx);
    }

    fn extend_previous(&mut self, _: &ExtendPrevious, _: &mut Window, cx: &mut Context<Self>) {
        self.step(-1, How::Extend, cx);
    }

    fn cursor_next(&mut self, _: &CursorNext, _: &mut Window, cx: &mut Context<Self>) {
        self.step(1, How::Cursor, cx);
    }

    fn cursor_previous(&mut self, _: &CursorPrevious, _: &mut Window, cx: &mut Context<Self>) {
        self.step(-1, How::Cursor, cx);
    }

    fn toggle_selected(&mut self, _: &ToggleSelected, _: &mut Window, cx: &mut Context<Self>) {
        if let Some(index) = self.selected_index() {
            self.select_at(index, How::Toggle, cx);
        }
    }

    fn expand_selected(&mut self, _: &Expand, _: &mut Window, cx: &mut Context<Self>) {
        let Some(index) = self.selected_index() else {
            return;
        };
        let row = self.rows[index].clone();
        match row.kind {
            RowKind::Node {
                expandable: true,
                expanded: false,
                ..
            } => self.expand(&row, true, cx),
            RowKind::Node { expanded: true, .. } => {
                let child = self
                    .next_node(Some(index), 1, false)
                    .filter(|child| self.rows[*child].depth == row.depth + 1);
                if let Some(child) = child {
                    self.select_at(child, How::Select, cx);
                }
            }
            _ => {}
        }
    }

    fn collapse_selected(&mut self, _: &Collapse, _: &mut Window, cx: &mut Context<Self>) {
        let Some(index) = self.selected_index() else {
            return;
        };
        let row = self.rows[index].clone();
        if let RowKind::Node { expanded: true, .. } = row.kind {
            self.expand(&row, false, cx);
        } else if let Some(parent) = row
            .parent
            .and_then(|parent| self.rows.iter().position(|row| row.id == parent))
        {
            self.select_at(parent, How::Select, cx);
        }
    }

    fn activate(&mut self, _: &Activate, window: &mut Window, cx: &mut Context<Self>) {
        if !self.focus.is_focused(window) {
            return;
        }
        if let Some(index) = self.selected_index() {
            let row = self.rows[index].clone();
            self.toggle_row(&row, cx);
        }
    }

    fn select_first(&mut self, _: &SelectFirst, _: &mut Window, cx: &mut Context<Self>) {
        if let Some(first) = self.next_node(None, 1, false) {
            self.select_at(first, How::Select, cx);
        }
    }

    fn select_last(&mut self, _: &SelectLast, _: &mut Window, cx: &mut Context<Self>) {
        if let Some(last) = self.next_node(None, -1, false) {
            self.select_at(last, How::Select, cx);
        }
    }

    /// UI-6: a click selects the row; with Ctrl/Cmd it toggles a repo row,
    /// with Shift it selects a range. A double-click opens or closes it.
    fn click(
        &mut self,
        index: usize,
        event: &ClickEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        window.focus(&self.focus, cx);
        let modifiers = event.modifiers();
        let how = if modifiers.secondary(cx) {
            How::Toggle
        } else if modifiers.shift {
            How::Extend
        } else {
            How::Select
        };
        self.select_at(index, how, cx);
        if event.click_count() == 2
            && how == How::Select
            && let Some(row) = self.rows.get(index).cloned()
        {
            if let Some(gitcroc_core::TreeItem::Branch {
                repo_id,
                name,
                current,
                ..
            }) = row.item()
            {
                if !current && crate::row_context::blocked(self.tree.summary(repo_id)).is_none() {
                    cx.emit(AskBranch {
                        workspace: self.workspace_id.clone(),
                        repo: repo_id.clone(),
                        repo_name: self
                            .tree
                            .repo_names()
                            .get(repo_id)
                            .cloned()
                            .unwrap_or_default(),
                        mode: BranchMode::Run {
                            op: BranchOp::Checkout { name: name.clone() },
                        },
                        immediate: true,
                    });
                }
            } else {
                self.toggle_row(&row, cx);
            }
        }
    }

    pub fn operation_error(&mut self, error: Option<SharedString>, cx: &mut Context<Self>) {
        self.operation_error = error;
        cx.notify();
    }
    fn keyboard_row_menu(&mut self, _: &OpenRowMenu, window: &mut Window, cx: &mut Context<Self>) {
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
            self.row_menu(index, at, window, cx);
        }
    }
    fn row_menu(
        &mut self,
        index: usize,
        at: Point<Pixels>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.select_at(index, How::Select, cx);
        window.focus(&self.focus, cx);
        let Some(item) = self.rows.get(index).and_then(|row| row.item()) else {
            return;
        };
        let repo = repo_of(item).cloned();
        let view = repo.as_ref().and_then(|repo| self.tree.summary(repo));
        let entries = row_menus::tree_menu(
            item,
            crate::row_context::blocked(view),
            view.is_some_and(|v| v.missing),
        );
        let revision = match item {
            TreeItem::Branch { name, .. }
            | TreeItem::RemoteBranch { name, .. }
            | TreeItem::Tag { name, .. } => Some(name.clone()),
            _ => None,
        };
        let name = revision.clone().unwrap_or_default();
        let remote = match item {
            TreeItem::RemoteBranch { remote, branch, .. } => Some(RemoteBranchRef {
                remote: remote.clone(),
                branch: branch.clone(),
            }),
            _ => None,
        };
        let items = entries
            .into_iter()
            .map(|entry| {
                let mode = match entry.action {
                    A::Create => Some(BranchMode::Create {
                        start: revision
                            .clone()
                            .map(|revision| StartPoint::Revision { revision })
                            .unwrap_or(StartPoint::Head),
                    }),
                    A::Rename => Some(BranchMode::Rename { name: name.clone() }),
                    A::Delete => Some(BranchMode::Delete {
                        name: name.clone(),
                        tracked: matches!(
                            item,
                            TreeItem::Branch {
                                upstream: Some(_),
                                ..
                            }
                        ),
                    }),
                    A::SetUpstream => Some(BranchMode::Upstream {
                        name: name.clone(),
                        upstream: match item {
                            TreeItem::Branch { upstream, .. } => upstream.clone(),
                            _ => None,
                        },
                    }),
                    A::UnsetUpstream => Some(BranchMode::Run {
                        op: BranchOp::SetUpstream {
                            name: name.clone(),
                            upstream: None,
                        },
                    }),
                    A::Checkout => Some(BranchMode::Run {
                        op: BranchOp::Checkout { name: name.clone() },
                    }),
                    A::CheckoutRemote => remote.clone().map(|branch| BranchMode::Run {
                        op: BranchOp::CheckoutRemote { branch },
                    }),
                    A::DeleteRemote => remote.clone().map(|branch| BranchMode::Run {
                        op: BranchOp::DeleteRemote { branch },
                    }),
                    A::CheckoutDetached => Some(BranchMode::Run {
                        op: BranchOp::CheckoutDetached {
                            revision: name.clone(),
                        },
                    }),
                    _ => None,
                };
                let choice = mode.map(RowChoice::Branch).or_else(|| {
                    matches!(
                        entry.action,
                        A::OpenFileManager | A::OpenTerminal | A::RemoveRepo
                    )
                    .then_some(RowChoice::Repo(entry.action))
                });
                (entry.item, choice)
            })
            .collect::<Vec<_>>();
        if items.is_empty() {
            return;
        }
        let repo_name = repo
            .as_ref()
            .and_then(|repo| self.tree.repo_names().get(repo).cloned())
            .unwrap_or_default();
        cx.emit(AskRowMenu {
            label: strings::branches::TREE_ACTIONS,
            workspace: self.workspace_id.clone(),
            repo,
            repo_name,
            items,
            at,
            focus: self.focus.clone(),
        });
    }

    fn click_chevron(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        window.focus(&self.focus, cx);
        self.select_at(index, How::Select, cx);
        if let Some(row) = self.rows.get(index).cloned() {
            self.toggle_row(&row, cx);
        }
    }
}

impl Focusable for TreePane {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus.clone()
    }
}

/// A mark on a row, named for screen readers.
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

fn icon(path: &'static str, color: impl Into<gpui_kit::Hsla>) -> gpui_kit::Svg {
    svg().path(path).size(ICON).text_color(color)
}

impl TreePane {
    /// Ahead and behind as `↑1 ↓2`; zero counts are left out.
    fn ahead_behind(
        &self,
        id: &str,
        ahead: Option<u32>,
        behind: Option<u32>,
        cx: &App,
    ) -> Vec<AnyElement> {
        let color = cx.tokens().text_secondary;
        type Say = fn(u32) -> String;
        let count = |what: &str, count: Option<u32>, label: Say, shown: Say| {
            count.filter(|count| *count > 0).map(|count| {
                mark(format!("{id}-{what}"), label(count))
                    .text_color(color)
                    .child(shown(count))
                    .into_any_element()
            })
        };
        [
            count("ahead", ahead, text::ahead, text::ahead_mark),
            count("behind", behind, text::behind, text::behind_mark),
        ]
        .into_iter()
        .flatten()
        .collect()
    }

    /// What a repo row shows (UI-3).
    fn repo_content(&self, id: &str, label: &str, repo: &RepoId, cx: &App) -> Vec<AnyElement> {
        let tokens = cx.tokens();
        let view = self.tree.summary(repo);
        let summary = view.and_then(|view| view.summary.as_ref());
        // A summary from the cache is dimmed until read again (PERF-3).
        let stale = view.is_none_or(|view| view.stale);
        let mut content = vec![
            div()
                .flex_shrink_0()
                .text_color(if stale {
                    tokens.text_secondary
                } else {
                    tokens.text_primary
                })
                .child(label.to_owned())
                .into_any_element(),
        ];
        if let Some(summary) = summary {
            let head = match &summary.head {
                Head::Branch { name, .. } => name.clone(),
                Head::Detached { hash } => hash.chars().take(7).collect(),
            };
            // A long branch name is cut, not the repo's name.
            content.push(
                div()
                    .id(ElementId::Name(format!("{id}-head").into()))
                    .test_support()
                    .role(Role::Label)
                    .aria_label(head.clone())
                    .min_w_0()
                    .truncate()
                    .text_color(tokens.text_secondary)
                    .child(head)
                    .into_any_element(),
            );
            if let Some(upstream) = &summary.upstream {
                content.extend(self.ahead_behind(
                    id,
                    Some(upstream.ahead),
                    Some(upstream.behind),
                    cx,
                ));
            }
            if summary.dirty {
                content.push(
                    mark(format!("{id}-dirty"), text::UNCOMMITTED)
                        .child(div().size(px(7.)).rounded_full().bg(tokens.accent))
                        .into_any_element(),
                );
            }
        }
        let Some(view) = view else {
            return content;
        };
        if view.conflict {
            content.push(
                mark(format!("{id}-conflict"), text::CONFLICT)
                    .child(icon(icons::CONFLICT, tokens.danger))
                    .into_any_element(),
            );
        }
        if view.missing {
            content.push(
                mark(format!("{id}-missing"), text::FOLDER_MISSING)
                    .child(icon(icons::WARNING, tokens.warning))
                    .into_any_element(),
            );
        }
        if let Some(error) = &view.error {
            content.push(
                mark(format!("{id}-error"), text::cannot_read_repo(error))
                    .child(icon(icons::WARNING, tokens.warning))
                    .into_any_element(),
            );
        }
        if view.refreshing {
            content.push(
                mark(format!("{id}-refreshing"), text::REFRESHING)
                    .child(icon(icons::SPINNER, tokens.text_secondary).with_animation(
                        ElementId::Name(format!("{id}-spin").into()),
                        Animation::new(Duration::from_secs(1)).repeat(),
                        |spinner, delta| {
                            spinner.with_transformation(Transformation::rotate(percentage(delta)))
                        },
                    ))
                    .into_any_element(),
            );
        }
        content
    }

    /// What a group row shows (UI-4): name, member count, the branch the
    /// core derives (GRP-4), and the dirty and conflict totals when not 0.
    fn group_content(
        &self,
        id: &str,
        label: &str,
        summary: &GroupSummary,
        cx: &App,
    ) -> Vec<AnyElement> {
        let tokens = cx.tokens();
        let summary = self.tree.group_summary(summary);
        let mut content = vec![
            div()
                .flex_shrink_0()
                .font_weight(FontWeight::SEMIBOLD)
                .child(label.to_owned())
                .into_any_element(),
            mark(format!("{id}-members"), text::members(summary.members))
                .text_color(tokens.text_secondary)
                .child(summary.members.to_string())
                .into_any_element(),
        ];
        let head = match &summary.branch {
            GroupBranch::Branch { name } => Some(name.clone()),
            GroupBranch::Detached => Some(text::DETACHED.to_owned()),
            GroupBranch::Mixed { count } => Some(text::mixed(*count)),
            GroupBranch::None => None,
        };
        if let Some(head) = head {
            let mixed = matches!(summary.branch, GroupBranch::Mixed { .. });
            content.push(
                div()
                    .id(ElementId::Name(format!("{id}-branch").into()))
                    .test_support()
                    .role(Role::Label)
                    .aria_label(head.clone())
                    .when(mixed, |head| head.aria_description(text::MIXED_TITLE))
                    .min_w_0()
                    .truncate()
                    .text_color(if mixed {
                        tokens.warning
                    } else {
                        tokens.text_secondary
                    })
                    .child(head)
                    .into_any_element(),
            );
        }
        if summary.dirty > 0 {
            content.push(
                mark(
                    format!("{id}-dirty"),
                    text::members_with_changes(summary.dirty),
                )
                .gap_0p5()
                .child(div().size(px(7.)).rounded_full().bg(tokens.accent))
                .child(summary.dirty.to_string())
                .into_any_element(),
            );
        }
        if summary.conflicts > 0 {
            content.push(
                mark(
                    format!("{id}-conflict"),
                    text::members_in_conflict(summary.conflicts),
                )
                .gap_0p5()
                .child(icon(icons::CONFLICT, tokens.danger))
                .child(summary.conflicts.to_string())
                .into_any_element(),
            );
        }
        content
    }

    /// What a branch, a group's branch name or another row shows.
    fn ref_content(&self, id: &str, label: &str, item: &TreeItem, cx: &App) -> Vec<AnyElement> {
        let tokens = cx.tokens();
        match item {
            TreeItem::Branch {
                current,
                ahead,
                behind,
                gone,
                upstream,
                ..
            } => {
                let mut content = vec![
                    div()
                        .min_w_0()
                        .truncate()
                        .when(*current, |name| name.font_weight(FontWeight::SEMIBOLD))
                        .child(label.to_owned())
                        .into_any_element(),
                ];
                if *current {
                    content.push(
                        mark(format!("{id}-current"), text::CURRENT_BRANCH)
                            .child(icon(icons::CHECK, tokens.success))
                            .into_any_element(),
                    );
                }
                content.extend(self.ahead_behind(id, *ahead, *behind, cx));
                if *gone {
                    content.push(
                        mark(format!("{id}-gone"), text::UPSTREAM_GONE)
                            .when_some(upstream.clone(), |gone, upstream| {
                                gone.aria_description(upstream)
                            })
                            .text_color(tokens.warning)
                            .child(text::GONE)
                            .into_any_element(),
                    );
                }
                content
            }
            TreeItem::GroupBranch {
                present, members, ..
            } => vec![
                div()
                    .min_w_0()
                    .truncate()
                    .child(label.to_owned())
                    .into_any_element(),
                mark(
                    format!("{id}-present"),
                    text::in_members(*present, *members),
                )
                .text_color(tokens.text_secondary)
                .child(text::present_mark(*present, *members))
                .into_any_element(),
            ],
            _ => vec![
                div()
                    .min_w_0()
                    .truncate()
                    .child(label.to_owned())
                    .into_any_element(),
            ],
        }
    }

    fn render_row(&self, index: usize, window: &Window, cx: &mut Context<Self>) -> AnyElement {
        let row = &self.rows[index];
        let tokens = *cx.tokens();
        let id = format!("tree-row-{}", row.id);
        let indent = px(4. + row.depth as f32 * INDENT);
        let base = div()
            .relative()
            .id(ElementId::Name(id.clone().into()))
            .test_support()
            .h(ROW_HEIGHT)
            .w_full()
            .flex()
            .flex_row()
            .items_center()
            .gap_1()
            .pl(indent)
            .pr_2()
            .overflow_hidden()
            .whitespace_nowrap();
        let (label, item, expandable, expanded) = match &row.kind {
            RowKind::Message { text, error } => {
                return base
                    .role(if *error { Role::Alert } else { Role::Label })
                    .aria_label(text.clone())
                    .pl(indent + px(16.))
                    .text_color(if *error {
                        tokens.danger
                    } else {
                        tokens.text_secondary
                    })
                    .child(text.clone())
                    .into_any_element();
            }
            RowKind::Node {
                label,
                item,
                expandable,
                expanded,
            } => (label, item, *expandable, *expanded),
        };
        let selected = self.tree.is_selected(&row.id);
        let cursor = self.tree.selected() == Some(row.id.as_str());
        let focused = self.focus.is_focused(window);
        let chevron = div()
            .id(ElementId::Name(format!("{id}-chevron").into()))
            .test_support()
            .flex_shrink_0()
            .size(px(16.))
            .flex()
            .items_center()
            .justify_center()
            .when(expandable, |chevron| {
                chevron
                    .child(icon(
                        if expanded {
                            icons::CHEVRON_DOWN
                        } else {
                            icons::CHEVRON_RIGHT
                        },
                        tokens.text_secondary,
                    ))
                    .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                        cx.stop_propagation();
                        this.click_chevron(index, window, cx);
                    }))
            });
        let content = match item {
            TreeItem::Repo { repo_id, .. } => self.repo_content(&id, label, repo_id, cx),
            TreeItem::Group { summary, .. } => self.group_content(&id, label, summary, cx),
            item => self.ref_content(&id, label, item, cx),
        };
        let stale = repo_of(item)
            .filter(|_| row.is_repo())
            .and_then(|repo| self.tree.summary(repo))
            .is_none_or(|view| view.stale);
        base.role(Role::TreeItem)
            .aria_label(label.clone())
            .aria_level(row.depth + 1)
            .aria_selected(selected)
            .when(expandable, |row| row.aria_expanded(expanded))
            .when(row.is_repo() && stale, |row| {
                row.aria_description(text::STALE)
            })
            .when(cursor, |row| row.aria_active_descendant())
            .when(selected, |row| row.bg(tokens.selection_bg))
            // The row the keyboard is on, while the tree has the focus.
            .when(cursor && focused, |row| {
                row.border_1().border_color(tokens.focus_ring)
            })
            .hover(|row| row.bg(tokens.bg_3))
            .on_mouse_down(
                MouseButton::Right,
                cx.listener(move |this, event: &MouseDownEvent, window, cx| {
                    cx.stop_propagation();
                    this.row_menu(index, event.position, window, cx);
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
                this.click(index, event, window, cx);
            }))
            .child(chevron)
            .children(content)
            .into_any_element()
    }

    fn render_rows(
        &mut self,
        range: Range<usize>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Vec<AnyElement> {
        range
            .filter(|index| *index < self.rows.len())
            .map(|index| self.render_row(index, window, cx))
            .collect()
    }
}

impl Render for TreePane {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let tokens = *cx.tokens();
        let error = self.tree.error().map(|error| {
            let error = text::cannot_read(error);
            div()
                .id("tree-error")
                .test_support()
                .role(Role::Alert)
                .aria_label(error.clone())
                .px_2()
                .py_1()
                .text_color(tokens.danger)
                .child(error)
        });
        let body = if self.tree.is_empty() {
            div()
                .id("tree-empty")
                .test_support()
                .role(Role::Label)
                .aria_label(text::EMPTY)
                .p_3()
                .text_color(tokens.text_secondary)
                .child(text::EMPTY)
                .into_any_element()
        } else {
            uniform_list(
                "tree-rows",
                self.rows.len(),
                cx.processor(Self::render_rows),
            )
            .track_scroll(&self.scroll)
            .size_full()
            .into_any_element()
        };
        div()
            .id("tree")
            .test_support()
            .role(Role::Group)
            .aria_label(strings::regions::TREE)
            .size_full()
            .flex()
            .flex_col()
            .text_sm()
            .child(
                div().flex_shrink_0().p_1().child(
                    Input::new(&self.filter)
                        .id("tree-filter")
                        .aria_label(text::FILTER)
                        .cleanable(true),
                ),
            )
            .children(error)
            .children(crate::dialog::refusal(&self.operation_error, cx))
            .child(
                div()
                    .id("tree-list")
                    .test_support()
                    .track_focus(&self.focus)
                    .key_context(CONTEXT)
                    .role(Role::Tree)
                    .aria_label(text::LABEL)
                    .on_action(cx.listener(Self::select_next))
                    .on_action(cx.listener(Self::select_previous))
                    .on_action(cx.listener(Self::extend_next))
                    .on_action(cx.listener(Self::extend_previous))
                    .on_action(cx.listener(Self::cursor_next))
                    .on_action(cx.listener(Self::cursor_previous))
                    .on_action(cx.listener(Self::toggle_selected))
                    .on_action(cx.listener(Self::expand_selected))
                    .on_action(cx.listener(Self::collapse_selected))
                    .on_action(cx.listener(Self::activate))
                    .on_action(cx.listener(Self::keyboard_row_menu))
                    .on_action(cx.listener(Self::select_first))
                    .on_action(cx.listener(Self::select_last))
                    .flex_1()
                    .min_h_0()
                    .child(body),
            )
    }
}
