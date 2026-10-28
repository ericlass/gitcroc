//! The history pane of one workspace (UI-9): the commit list, virtualized
//! (PERF-1) and paged by the core (ARCH-8), with the graph painter from the
//! spike. The window keeps one per open workspace, so each tab keeps its own
//! source, rows, selection and scroll position (UI-29). Everything shown
//! comes from the core; this lays it out and forwards input.

use std::collections::HashMap;
use std::ops::Range;
use std::rc::Rc;

use gitcroc_app::app::AppEvent;
use gitcroc_core::{GraphRow, RefKind, RepoId, TargetSet, WorkspaceId};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::{
    AnyElement, App, Bounds, ClickEvent, ContentMask, Context, ElementId, Entity, FocusHandle,
    Focusable, Hsla, InteractiveElement, IntoElement, KeyBinding, KeyBindingContextPredicate,
    ParentElement, PathBuilder, Pixels, Render, Role, ScrollStrategy, SharedString,
    StatefulInteractiveElement, Styled, Subscription, TestSupportExt, UniformListScrollHandle,
    Window, actions, canvas, div, fill, point, px, size, uniform_list,
};

use crate::history::{self, History, Restore, Row, Selection, Show};
use crate::services::Services;
use crate::strings::{self, history as text};
use crate::theme::{ActiveTokens, tokens::Tokens};
use crate::tree_pane::TreePane;

actions!(
    history_pane,
    [
        SelectNext,
        SelectPrevious,
        PageNext,
        PagePrevious,
        SelectFirst,
        SelectLast,
    ]
);

/// The key context of the history list.
const CONTEXT: &str = "GitcrocHistory";

/// One row's height; `uniform_list` lays out only those on screen.
pub const ROW_HEIGHT: Pixels = px(24.);
const GRAPH_WIDTH: Pixels = px(80.);
const AUTHOR_WIDTH: Pixels = px(140.);
const DATE_WIDTH: Pixels = px(128.);
const HASH_WIDTH: Pixels = px(72.);
const LANE_WIDTH: f32 = 12.;
/// The next page is asked for when this few loaded rows are left below.
const PREFETCH_ROWS: usize = 100;
/// Placeholder rows below the loaded ones while a page follows.
const PLACEHOLDER_ROWS: usize = 500;

/// The keys of the history list (UI-22).
pub fn bind(cx: &mut App) {
    let context = KeyBindingContextPredicate::parse(CONTEXT).ok().map(Rc::new);
    let bind = |keys: &str, action: Box<dyn gpui_kit::Action>, cx: &mut App| {
        KeyBinding::load(keys, action, context.clone(), false, None, cx).ok()
    };
    let bindings = [
        bind("down", Box::new(SelectNext), cx),
        bind("up", Box::new(SelectPrevious), cx),
        bind("pagedown", Box::new(PageNext), cx),
        bind("pageup", Box::new(PagePrevious), cx),
        bind("home", Box::new(SelectFirst), cx),
        bind("end", Box::new(SelectLast), cx),
    ];
    cx.bind_keys(bindings.into_iter().flatten());
}

/// The history of one workspace.
pub struct HistoryPane {
    services: Services,
    workspace_id: WorkspaceId,
    history: History,
    tree: Entity<TreePane>,
    focus: FocusHandle,
    scroll: UniformListScrollHandle,
    /// The latest first read; older answers are dropped.
    request: u64,
    /// The last `refs_version` heard of each repo (OPS-7).
    refs_versions: HashMap<RepoId, u32>,
    _tree_watch: Subscription,
}

impl HistoryPane {
    pub fn new(
        services: Services,
        workspace_id: WorkspaceId,
        tree: Entity<TreePane>,
        cx: &mut Context<Self>,
    ) -> Self {
        let tree_watch = cx.observe(&tree, |this, _, cx| this.follow_tree(cx));
        let mut this = Self {
            services,
            workspace_id,
            history: History::default(),
            tree,
            focus: cx.focus_handle(),
            scroll: UniformListScrollHandle::new(),
            request: 0,
            refs_versions: HashMap::new(),
            _tree_watch: tree_watch,
        };
        this.follow_tree(cx);
        this
    }

    pub fn history(&self) -> &History {
        &self.history
    }

    /// What the selected row shows in the changes pane (M6-12).
    pub fn selection(&self) -> Option<Selection> {
        self.history.selection()
    }

    /// Follows the app's events about this workspace.
    pub fn on_event(&mut self, event: &AppEvent, cx: &mut Context<Self>) {
        match event {
            AppEvent::RepoSummary(view) if view.workspace_id == self.workspace_id => {
                if view.refreshing {
                    return;
                }
                let shown = self.history.source().and_then(|source| source.repo.clone());
                if shown.as_ref() != Some(&view.repo_id) {
                    return;
                }
                let seen = self.refs_versions.entry(view.repo_id.clone()).or_insert(0);
                if view.refs_version <= *seen {
                    return;
                }
                *seen = view.refs_version;
                self.reload(cx);
            }
            AppEvent::WorkspaceChanged(view) if view.id == self.workspace_id => {
                self.follow_tree(cx);
            }
            _ => {}
        }
    }

    /// Reads the shown history again, after the app's events were missed.
    pub fn read_again(&mut self, cx: &mut Context<Self>) {
        if self.history.source().is_some() {
            self.reload(cx);
        } else {
            self.follow_tree(cx);
        }
    }

    /// Shows what the tree selects (section 8 "History pane").
    fn follow_tree(&mut self, cx: &mut Context<Self>) {
        let source = {
            let tree = self.tree.read(cx);
            match tree.target() {
                Some(TargetSet::Repo { .. }) => tree.selected_item().and_then(history::source_of),
                _ => None,
            }
        };
        match self.history.show(source) {
            Show::Kept => {}
            Show::Empty => {
                self.request = self.request.wrapping_add(1);
                cx.notify();
            }
            Show::Load(restore) => self.read_first(restore, cx),
        }
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
        let call = self.services.with_app(move |app| async move {
            history::read(app, workspace, source, String::new(), restore.rows).await
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
        let call = self.services.with_app(move |app| async move {
            history::read(app, workspace, source, String::new(), rows).await
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

    /// Selects loaded row `index`, and scrolls it into view.
    fn select(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        let Some(row_id) = self.history.row_id_at(index) else {
            return;
        };
        window.focus(&self.focus, cx);
        self.history.select(Some(row_id));
        self.scroll.scroll_to_item(index, ScrollStrategy::Nearest);
        cx.notify();
    }

    fn move_to(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        let count = self.history.rows().len();
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

    fn select_next(&mut self, _: &SelectNext, window: &mut Window, cx: &mut Context<Self>) {
        self.step(1, window, cx);
    }

    fn select_previous(&mut self, _: &SelectPrevious, window: &mut Window, cx: &mut Context<Self>) {
        self.step(-1, window, cx);
    }

    fn page_next(&mut self, _: &PageNext, window: &mut Window, cx: &mut Context<Self>) {
        let step = self.page() as isize;
        self.step(step, window, cx);
    }

    fn page_previous(&mut self, _: &PagePrevious, window: &mut Window, cx: &mut Context<Self>) {
        let step = self.page() as isize;
        self.step(-step, window, cx);
    }

    fn select_first(&mut self, _: &SelectFirst, window: &mut Window, cx: &mut Context<Self>) {
        self.move_to(0, window, cx);
    }

    fn select_last(&mut self, _: &SelectLast, window: &mut Window, cx: &mut Context<Self>) {
        let count = self.history.rows().len();
        if count > 0 {
            self.move_to(count - 1, window, cx);
        }
    }

    fn click(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        self.select(index, window, cx);
    }

    fn header(&self, tokens: &Tokens) -> AnyElement {
        div()
            .id("history-header")
            .test_support()
            .role(Role::Toolbar)
            .aria_label(text::COLUMNS_LABEL)
            .h(px(28.))
            .flex_shrink_0()
            .flex()
            .flex_row()
            .items_center()
            .gap_2()
            .px_2()
            .border_b_1()
            .border_color(tokens.border)
            .bg(tokens.bg_2)
            .text_size(px(12.))
            .text_color(tokens.text_secondary)
            .child(header_cell(
                "history-column-graph",
                text::GRAPH,
                Some(GRAPH_WIDTH),
            ))
            .child(header_cell("history-column-subject", text::SUBJECT, None))
            .child(header_cell(
                "history-column-author",
                text::AUTHOR,
                Some(AUTHOR_WIDTH),
            ))
            .child(header_cell(
                "history-column-date",
                text::DATE,
                Some(DATE_WIDTH),
            ))
            .child(header_cell(
                "history-column-hash",
                text::HASH,
                Some(HASH_WIDTH),
            ))
            .into_any_element()
    }

    fn render_row(&self, index: usize, window: &Window, cx: &mut Context<Self>) -> AnyElement {
        let tokens = *cx.tokens();
        if let Some(row) = self.history.rows().get(index) {
            return self.commit_row(index, row, window, cx);
        }
        div()
            .id(ElementId::Name(format!("history-row-{index}").into()))
            .test_support()
            .role(Role::ListBoxOption)
            .aria_label(text::LOADING)
            .h(ROW_HEIGHT)
            .w_full()
            .flex()
            .flex_row()
            .items_center()
            .gap_3()
            .px_2()
            .child(
                div()
                    .h(px(10.))
                    .w(GRAPH_WIDTH)
                    .flex_none()
                    .rounded(px(3.))
                    .bg(tokens.bg_3),
            )
            .child(div().h(px(10.)).w(px(240.)).rounded(px(3.)).bg(tokens.bg_3))
            .into_any_element()
    }

    fn commit_row(
        &self,
        index: usize,
        row: &Row,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let tokens = *cx.tokens();
        let id = format!("history-row-{index}");
        let row_id = self.history.row_id(row);
        let selected = self.history.selected() == Some(row_id.as_str());
        let focused = self.focus.is_focused(window);
        let graph = row.graph.clone();
        let colors = tokens.graph.map(Hsla::from);
        let graph_cell = div()
            .id(ElementId::Name(format!("{id}-graph").into()))
            .test_support()
            .w(GRAPH_WIDTH)
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
            );
        let mut subject = div()
            .flex()
            .flex_row()
            .items_center()
            .flex_1()
            .min_w_0()
            .gap_1()
            .overflow_hidden();
        for (at, (kind, name)) in row.refs.iter().enumerate() {
            subject = subject.child(self.ref_label(&id, at, *kind, name.clone(), &tokens));
        }
        subject = subject.child(
            div()
                .id(ElementId::Name(format!("{id}-subject").into()))
                .test_support()
                .role(Role::Label)
                .aria_label(row.subject.clone())
                .truncate()
                .text_color(tokens.text_primary)
                .child(row.subject.clone()),
        );
        div()
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
            .gap_3()
            .px_2()
            .overflow_hidden()
            .whitespace_nowrap()
            .text_size(px(13.))
            .when(selected, |row| row.bg(tokens.selection_bg))
            .when(!selected, |row| row.hover(|style| style.bg(tokens.bg_3)))
            .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                this.click(index, window, cx);
            }))
            .child(graph_cell)
            .child(subject)
            .child(
                div()
                    .id(ElementId::Name(format!("{id}-author").into()))
                    .test_support()
                    .role(Role::Label)
                    .aria_label(row.author.clone())
                    .w(AUTHOR_WIDTH)
                    .flex_none()
                    .truncate()
                    .text_color(tokens.text_secondary)
                    .child(row.author.clone()),
            )
            .child(
                div()
                    .id(ElementId::Name(format!("{id}-date").into()))
                    .test_support()
                    .role(Role::Label)
                    .aria_label(row.date.clone())
                    .aria_description(row.dates.clone())
                    .w(DATE_WIDTH)
                    .flex_none()
                    .whitespace_nowrap()
                    .text_color(tokens.text_secondary)
                    .child(row.date.clone()),
            )
            .child(
                div()
                    .id(ElementId::Name(format!("{id}-hash").into()))
                    .test_support()
                    .role(Role::Label)
                    .aria_label(row.short_hash.clone())
                    .w(HASH_WIDTH)
                    .flex_none()
                    .whitespace_nowrap()
                    .text_size(px(12.))
                    .text_color(tokens.text_secondary)
                    .child(row.short_hash.clone()),
            )
            .into_any_element()
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

    fn render_rows(
        &mut self,
        range: Range<usize>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Vec<AnyElement> {
        if !self.history.done() && range.end + PREFETCH_ROWS >= self.history.rows().len() {
            self.load_more(cx);
        }
        range
            .map(|index| self.render_row(index, window, cx))
            .collect()
    }
}

impl Focusable for HistoryPane {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus.clone()
    }
}

fn header_cell(id: &'static str, label: &'static str, width: Option<Pixels>) -> AnyElement {
    div()
        .id(id)
        .test_support()
        .role(Role::ColumnHeader)
        .aria_label(label)
        .when_some(width, |cell, width| cell.w(width).flex_none())
        .when(width.is_none(), |cell| cell.flex_1())
        .truncate()
        .child(label)
        .into_any_element()
}

impl Render for HistoryPane {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.remember_scroll();
        let tokens = *cx.tokens();
        let error = self.history.error().map(|error| {
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
        } else if self.history.rows().is_empty()
            && self.history.done()
            && self.history.error().is_none()
        {
            message("history-empty", text::NO_COMMITS, tokens.text_secondary)
        } else {
            let count = self.history.rows().len()
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
            .when(self.history.source().is_some(), |pane| {
                pane.child(self.header(&tokens))
            })
            .children(error)
            .child(body)
    }
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
