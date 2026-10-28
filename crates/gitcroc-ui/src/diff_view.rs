//! The selected file's diff in the changes pane (UI-17), a port of rc.1's
//! `changes/DiffView.tsx` and the diff part of `stores/changes.ts`. It reads
//! one file's diff at a time (ARCH-8) with the options of the pane header,
//! builds its rows off the UI thread (ARCH-7), and draws them in one
//! virtualized list (PERF-1): inline, or side by side with unchanged runs
//! folded. Without wrapping, long lines scroll sideways together; with it,
//! the list measures each row. F7 and Shift+F7 move between hunks (UI-22).
//! Each tab's changes pane has one, so each keeps its diff (UI-29).

use std::ops::Range;

use gitcroc_core::{DiffMode, DiffRequest, FileDiff, LineKind, WorkspaceId};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::{
    AnyElement, App, AppContext, ClickEvent, Context, CursorStyle, Div, ElementId, FocusHandle,
    Hsla, InteractiveElement, IntoElement, ListAlignment, ListOffset, ListState, ParentElement,
    Pixels, Render, Role, ScrollEvent, ScrollStrategy, SharedString, StatefulInteractiveElement,
    Styled, Subscription, TestSupportExt, UniformListScrollHandle, Window, div, font, list, point,
    px, uniform_list,
};

use crate::changes::DiffTarget;
use crate::changes_pane::ChangesPrefs;
use crate::diff::{Cell, DiffDoc, DiffRow, Shown};
use crate::services::Services;
use crate::strings::changes as text;
use crate::theme::code_themes::CodeThemeColors;
use crate::theme::{ActiveTokens, AppTheme};

/// Rows drawn past the edges of the wrapped list, so scrolling shows no gap.
const OVERDRAW: Pixels = px(400.);

/// What the diff of a file is, built from the core's answer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Content {
    Text {
        doc: DiffDoc,
        /// Each side's last line has no line terminator.
        missing_newline: (bool, bool),
    },
    /// A diff that is not text; M6-13b shows it. Until then, its kind.
    Other(&'static str),
}

impl Content {
    /// Lays the core's diff out for `mode`; side by side waits for both
    /// whole files and shows the diff inline until they come.
    fn build(diff: FileDiff, request: DiffRequest) -> Self {
        match diff {
            FileDiff::Text(diff) => {
                let side = request
                    .full_text
                    .then(|| DiffDoc::side_by_side(&diff, request.context))
                    .flatten();
                Self::Text {
                    doc: side.unwrap_or_else(|| DiffDoc::inline(&diff)),
                    missing_newline: (diff.old_missing_newline, diff.new_missing_newline),
                }
            }
            FileDiff::Binary { .. } => Self::Other(text::BINARY),
            FileDiff::Image { .. } => Self::Other(text::IMAGE),
            FileDiff::TooLarge { .. } => Self::Other(text::TOO_LARGE),
            FileDiff::Directory => Self::Other(text::FOLDER),
            FileDiff::Submodule { .. } => Self::Other(text::SUBMODULE),
        }
    }
}

/// The request the pane's options make (UI-17): side by side asks for
/// both whole files too.
pub fn request(prefs: &gitcroc_core::ChangesSettings) -> DiffRequest {
    DiffRequest {
        ignore_whitespace: prefs.ignore_whitespace,
        context: prefs.context,
        load_anyway: false,
        full_text: prefs.diff_mode == DiffMode::SideBySide,
    }
}

/// Sizes of the code font, measured each frame.
#[derive(Debug, Clone)]
struct Metrics {
    family: SharedString,
    size: Pixels,
    line_height: Pixels,
    /// One column of the monospace font.
    advance: Pixels,
    /// A line number's column.
    number: Pixels,
    /// The +/− column.
    sign: Pixels,
}

/// The diff of one workspace's selected file.
pub struct DiffView {
    services: Services,
    workspace_id: WorkspaceId,
    target: Option<DiffTarget>,
    /// Reads sent so far; an answer to an older one is dropped.
    reads: u64,
    /// What the shown diff was read with.
    asked: Option<DiffRequest>,
    loading: bool,
    error: Option<String>,
    content: Option<Content>,
    wrap: bool,
    scroll: UniformListScrollHandle,
    wrapped: ListState,
    /// How far the lines are scrolled sideways.
    offset: Pixels,
    /// The width of the widest line and of one text column, from the last
    /// frame, to clamp `offset`.
    widest: Pixels,
    columns: usize,
    gutter: Pixels,
    /// The hunk F7 last went to.
    hunk: Option<usize>,
    hunk_focus: FocusHandle,
    _prefs_watch: Subscription,
}

impl DiffView {
    pub fn new(services: Services, workspace_id: WorkspaceId, cx: &mut Context<Self>) -> Self {
        let prefs_watch = cx.observe_global::<ChangesPrefs>(|this, cx| this.prefs_changed(cx));
        Self {
            services,
            workspace_id,
            target: None,
            reads: 0,
            asked: None,
            loading: false,
            error: None,
            content: None,
            wrap: ChangesPrefs::get(cx).wrap_lines,
            scroll: UniformListScrollHandle::new(),
            wrapped: ListState::new(0, ListAlignment::Top, OVERDRAW),
            offset: Pixels::ZERO,
            widest: Pixels::ZERO,
            columns: 1,
            gutter: Pixels::ZERO,
            hunk: None,
            hunk_focus: cx.focus_handle(),
            _prefs_watch: prefs_watch,
        }
    }

    pub fn target(&self) -> Option<&DiffTarget> {
        self.target.as_ref()
    }

    pub fn content(&self) -> Option<&Content> {
        self.content.as_ref()
    }

    /// The text diff's rows, when it is one.
    pub fn doc(&self) -> Option<&DiffDoc> {
        match self.content.as_ref()? {
            Content::Text { doc, .. } => Some(doc),
            Content::Other(_) => None,
        }
    }

    pub fn loading(&self) -> bool {
        self.loading
    }

    pub fn error(&self) -> Option<&str> {
        self.error.as_deref()
    }

    /// What the shown diff was last asked for.
    pub fn asked(&self) -> Option<DiffRequest> {
        self.asked
    }

    /// How many diffs were read.
    pub fn reads(&self) -> u64 {
        self.reads
    }

    pub fn wrapping(&self) -> bool {
        self.wrap
    }

    /// How far the lines are scrolled sideways.
    pub fn offset(&self) -> Pixels {
        self.offset
    }

    /// The hunk F7 last went to.
    pub fn hunk(&self) -> Option<usize> {
        self.hunk
    }

    /// The first row on screen.
    pub fn top_row(&self) -> usize {
        if self.wrap {
            return self.wrapped.logical_scroll_top().item_ix;
        }
        let state = self.scroll.0.borrow();
        if let Some(deferred) = &state.deferred_scroll_to_item {
            return deferred.item_index;
        }
        let height = state
            .last_item_size
            .map_or(Pixels::ZERO, |size| size.item.height);
        if height <= Pixels::ZERO {
            return 0;
        }
        (-state.base_handle.offset().y / height).floor().max(0.) as usize
    }

    /// Shows the diff of `target`. Another file starts at its top; the same
    /// file in a view read anew keeps its place and open folds (OPS-7).
    pub fn show(&mut self, target: Option<DiffTarget>, cx: &mut Context<Self>) {
        if self.target == target {
            return;
        }
        let same_file = matches!(
            (&self.target, &target),
            (Some(old), Some(new)) if old.same_file(new)
        );
        self.target = target;
        if !same_file {
            self.content = None;
            self.hunk = None;
            self.offset = Pixels::ZERO;
            self.scroll_to_top();
        }
        if self.target.is_some() {
            self.load(cx);
        } else {
            // A read under way answers nothing any more.
            self.reads += 1;
            self.loading = false;
            self.error = None;
            self.content = None;
            self.asked = None;
        }
        cx.notify();
    }

    fn scroll_to_top(&mut self) {
        self.scroll
            .0
            .borrow()
            .base_handle
            .set_offset(point(px(0.), px(0.)));
        self.wrapped.scroll_to(ListOffset::default());
    }

    /// Reads the diff of the target with the pane's options (ARCH-7).
    fn load(&mut self, cx: &mut Context<Self>) {
        let Some(target) = self.target.clone() else {
            return;
        };
        let request = request(&ChangesPrefs::get(cx));
        self.reads += 1;
        let read = self.reads;
        self.asked = Some(request);
        self.loading = true;
        self.error = None;
        let workspace = self.workspace_id.clone();
        let call = self.services.with_app(move |app| async move {
            app.diff_get(workspace, target.handle, target.file, request)
                .await
        });
        cx.spawn(async move |this, cx| {
            let built = match call.await {
                Some(Ok(Some(diff))) => Some(Ok(cx
                    .background_spawn(async move { Content::build(diff, request) })
                    .await)),
                Some(Err(error)) => Some(Err(error.message)),
                // A newer view of the workspace took over; the pane reads
                // it and shows its file.
                Some(Ok(None)) | None => None,
            };
            let _ = this.update(cx, |this, cx| {
                if this.reads != read {
                    return;
                }
                this.loading = false;
                match built {
                    Some(Ok(content)) => this.set_content(content),
                    Some(Err(error)) => {
                        this.content = None;
                        this.error = Some(error);
                    }
                    None => {}
                }
                cx.notify();
            });
        })
        .detach();
    }

    /// Shows a diff read anew, with the folds the same file had open.
    fn set_content(&mut self, mut content: Content) {
        if let (Some(Content::Text { doc: old, .. }), Content::Text { doc, .. }) =
            (&self.content, &mut content)
            && old.side_by_side_mode() == doc.side_by_side_mode()
        {
            doc.set_unfolded(&old.unfolded().clone());
        }
        let len = match &content {
            Content::Text { doc, .. } => doc.len(),
            Content::Other(_) => 0,
        };
        let top = self.wrapped.logical_scroll_top();
        self.wrapped.reset(len);
        self.wrapped.scroll_to(top);
        if let Content::Text { doc, .. } = &content
            && self.hunk.is_some_and(|hunk| hunk >= doc.hunk_count())
        {
            self.hunk = None;
        }
        self.content = Some(content);
    }

    /// An option of the pane header changed (UI-17): one the core applies
    /// reads the diff again; wrapping only lays it out again.
    fn prefs_changed(&mut self, cx: &mut Context<Self>) {
        let prefs = ChangesPrefs::get(cx);
        if prefs.wrap_lines != self.wrap {
            let top = self.top_row();
            self.wrap = prefs.wrap_lines;
            if self.wrap {
                self.wrapped.scroll_to(ListOffset {
                    item_ix: top,
                    offset_in_item: Pixels::ZERO,
                });
            } else {
                self.scroll.scroll_to_item(top, ScrollStrategy::Top);
            }
            cx.notify();
        }
        if self.target.is_some() && self.asked.is_some_and(|asked| asked != request(&prefs)) {
            self.load(cx);
            cx.notify();
        }
    }

    fn unfold(&mut self, fold: usize, cx: &mut Context<Self>) {
        if let Some(Content::Text { doc, .. }) = &mut self.content {
            doc.unfold(fold);
            let top = self.wrapped.logical_scroll_top();
            self.wrapped.reset(doc.len());
            self.wrapped.scroll_to(top);
            cx.notify();
        }
    }

    /// Scrolls to the next (`step` 1) or previous (-1) hunk, wrapping
    /// around, and gives its header the focus (UI-22).
    pub fn step_hunk(&mut self, step: isize, window: &mut Window, cx: &mut Context<Self>) {
        let Some(doc) = self.doc() else {
            return;
        };
        let count = doc.hunk_count();
        if count == 0 {
            return;
        }
        let next = match self.hunk {
            None if step > 0 => 0,
            None => count - 1,
            Some(at) => (at as isize + step).rem_euclid(count as isize) as usize,
        };
        let Some(row) = doc.hunk_row(next) else {
            return;
        };
        self.hunk = Some(next);
        if self.wrap {
            self.wrapped.scroll_to_reveal_item(row);
        } else {
            self.scroll.scroll_to_item(row, ScrollStrategy::Center);
        }
        window.focus(&self.hunk_focus, cx);
        cx.notify();
    }

    /// Scrolls the lines sideways with the wheel or the trackpad; the
    /// list itself scrolls only up and down.
    fn scroll_sideways(
        &mut self,
        event: &ScrollEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.wrap {
            return;
        }
        let delta = event.delta.pixel_delta(window.line_height()).x;
        if delta == Pixels::ZERO {
            return;
        }
        let viewport = self.scroll.0.borrow().base_handle.bounds().size.width;
        let column = viewport / self.columns as f32 - self.gutter;
        let max = (self.widest - column).max(Pixels::ZERO);
        let offset = (self.offset - delta).clamp(Pixels::ZERO, max);
        if offset != self.offset {
            self.offset = offset;
            cx.notify();
        }
    }

    fn metrics(window: &Window, cx: &App, digits: usize) -> Metrics {
        let theme = AppTheme::global(cx);
        let family = theme.code_font().clone();
        let size = theme.code_font_size();
        let font_id = window.text_system().resolve_font(&font(family.clone()));
        let advance = window
            .text_system()
            .em_advance(font_id, size)
            .unwrap_or(size * 0.6);
        Metrics {
            family,
            size,
            line_height: px((f32::from(size) * 1.5).round()),
            advance,
            number: advance * (digits as f32 + 2.),
            sign: advance * 2.,
        }
    }
}

fn id(name: String) -> ElementId {
    ElementId::Name(name.into())
}

/// The text of a line: one row without wrapping, scrolled sideways by
/// `offset`; wrapped at the column's edge with it.
fn line_text(text: SharedString, offset: Pixels, wrap: bool) -> Div {
    let cell = div().flex_1().min_w_0().pr_2();
    if wrap {
        cell.child(text)
    } else {
        cell.overflow_hidden().child(
            div()
                .relative()
                .left(-offset)
                .whitespace_nowrap()
                .child(text),
        )
    }
}

fn kind_name(kind: LineKind) -> &'static str {
    match kind {
        LineKind::Added => text::LINE_ADDED,
        LineKind::Removed => text::LINE_REMOVED,
        LineKind::Context => text::LINE_UNCHANGED,
    }
}

/// A line's background and sign.
fn kind_look(kind: LineKind, colors: &CodeThemeColors) -> (Option<Hsla>, &'static str) {
    match kind {
        LineKind::Added => (Some(colors.added.into()), text::SIGN_ADDED),
        LineKind::Removed => (Some(colors.removed.into()), text::SIGN_REMOVED),
        LineKind::Context => (None, ""),
    }
}

impl DiffView {
    fn render_rows(
        &mut self,
        range: Range<usize>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Vec<AnyElement> {
        let digits = self.doc().map_or(1, DiffDoc::number_digits);
        let metrics = Self::metrics(window, cx, digits);
        range
            .map(|index| self.render_row(index, &metrics, cx))
            .collect()
    }

    fn render_wrapped(
        &mut self,
        index: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let digits = self.doc().map_or(1, DiffDoc::number_digits);
        let metrics = Self::metrics(window, cx, digits);
        self.render_row(index, &metrics, cx)
    }

    fn render_row(&self, index: usize, metrics: &Metrics, cx: &mut Context<Self>) -> AnyElement {
        let colors = AppTheme::global(cx).code_theme().colors;
        let Some(shown) = self.doc().and_then(|doc| doc.get(index)) else {
            return div().id(id(format!("diff-row-{index}"))).into_any_element();
        };
        let wrap = self.wrap;
        let base = |name: String| {
            div()
                .id(id(name))
                .test_support()
                .w_full()
                .flex()
                .flex_row()
                .when(wrap, |row| row.min_h(metrics.line_height))
                .when(!wrap, |row| row.h(metrics.line_height).overflow_hidden())
                .line_height(metrics.line_height)
        };
        let number = |value: Option<u32>| {
            div()
                .flex_shrink_0()
                .w(metrics.number)
                .pr_2()
                .text_right()
                .text_color(colors.gutter_text)
                .bg(colors.gutter_background)
                .children(value.map(|value| SharedString::from(value.to_string())))
        };
        let sign = |sign: &'static str| {
            div()
                .flex_shrink_0()
                .w(metrics.sign)
                .text_center()
                .text_color(colors.gutter_text)
                .child(sign)
        };
        match shown {
            Shown::Row(DiffRow::Hunk {
                index: hunk,
                header,
            }) => {
                let label = text::hunk(hunk + 1, header);
                base(format!("diff-hunk-{hunk}"))
                    .role(Role::Heading)
                    .aria_level(3)
                    .aria_label(label)
                    .when(self.hunk == Some(*hunk), |row| {
                        row.track_focus(&self.hunk_focus)
                    })
                    .bg(colors.hunk_background)
                    .text_color(colors.hunk_text)
                    .child(line_text(header.clone(), self.offset, wrap).pl_2())
                    .into_any_element()
            }
            Shown::Row(DiffRow::Line {
                kind,
                old,
                new,
                text: line,
            }) => {
                let (background, mark) = kind_look(*kind, &colors);
                base(format!("diff-row-{index}"))
                    .role(Role::ListItem)
                    .aria_label(text::line(kind_name(*kind), *old, *new, line))
                    .when_some(background, |row, background| row.bg(background))
                    .child(number(*old))
                    .child(number(*new))
                    .child(sign(mark))
                    .child(line_text(line.clone(), self.offset, wrap))
                    .into_any_element()
            }
            Shown::Row(DiffRow::Pair { old, new }) => {
                let half = |cell: &Option<Cell>, side: &str| {
                    let half = div()
                        .id(id(format!("diff-row-{index}-{side}")))
                        .flex_1()
                        .min_w_0()
                        .flex()
                        .flex_row();
                    match cell {
                        None => half.bg(colors.gutter_background),
                        Some(cell) => {
                            let (background, mark) = kind_look(cell.kind, &colors);
                            half.when_some(background, |half, background| half.bg(background))
                                .child(number(Some(cell.number)))
                                .child(sign(mark))
                                .child(line_text(cell.text.clone(), self.offset, wrap))
                        }
                    }
                };
                let label = |cell: &Option<Cell>, old: bool| {
                    cell.as_ref().map(|cell| {
                        let (old_number, new_number) = if old {
                            (Some(cell.number), None)
                        } else {
                            (None, Some(cell.number))
                        };
                        text::line(kind_name(cell.kind), old_number, new_number, &cell.text)
                    })
                };
                let label = text::pair(label(old, true).as_deref(), label(new, false).as_deref());
                base(format!("diff-row-{index}"))
                    .role(Role::ListItem)
                    .aria_label(label)
                    .child(half(old, "old"))
                    .child(div().flex_shrink_0().w(px(1.)).bg(colors.gutter_text))
                    .child(half(new, "new"))
                    .into_any_element()
            }
            Shown::Fold { fold, lines } => {
                let label = text::unfold(lines);
                base(format!("diff-fold-{fold}"))
                    .role(Role::Button)
                    .aria_label(label)
                    .cursor(CursorStyle::PointingHand)
                    .justify_center()
                    .bg(colors.hunk_background)
                    .text_color(colors.hunk_text)
                    .child(text::folded(lines))
                    .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                        this.unfold(fold, cx);
                    }))
                    .into_any_element()
            }
        }
    }
}

fn note(id_name: &'static str, role: Role, label: String, color: impl Into<Hsla>) -> AnyElement {
    div()
        .id(id_name)
        .test_support()
        .role(role)
        .aria_label(label.clone())
        .flex_shrink_0()
        .px_2()
        .py_1()
        .text_color(color)
        .child(label)
        .into_any_element()
}

impl Render for DiffView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let tokens = *cx.tokens();
        let colors = AppTheme::global(cx).code_theme().colors;
        let root = div().id("diff-body").flex_1().min_h_0().flex().flex_col();
        let Some(target) = self.target.clone() else {
            return root;
        };
        let error = self.error.as_deref().map(|error| {
            note(
                "diff-error",
                Role::Alert,
                text::cannot_read_diff(error),
                tokens.danger,
            )
        });
        let whitespace = (target.uncommitted()
            && self.asked.is_some_and(|asked| asked.ignore_whitespace)
            && self.doc().is_some())
        .then(|| {
            note(
                "diff-whitespace-note",
                Role::Paragraph,
                text::HUNKS_IGNORE_WHITESPACE.to_owned(),
                tokens.text_secondary,
            )
        });
        let root = root.children(error).children(whitespace);
        let (doc, missing_newline) = match &self.content {
            None => return root,
            Some(Content::Other(kind)) => {
                return root.child(note(
                    "diff-other",
                    Role::Paragraph,
                    (*kind).to_owned(),
                    tokens.text_secondary,
                ));
            }
            Some(Content::Text {
                doc,
                missing_newline,
            }) => (doc, *missing_newline),
        };
        let newline = (missing_newline.0 || missing_newline.1).then(|| {
            note(
                "diff-no-newline",
                Role::Paragraph,
                text::no_newline(missing_newline.0, missing_newline.1),
                tokens.text_secondary,
            )
        });
        if doc.hunk_count() == 0 {
            return root
                .child(note(
                    "diff-no-lines",
                    Role::Paragraph,
                    text::NO_LINES.to_owned(),
                    tokens.text_secondary,
                ))
                .children(newline);
        }
        let metrics = Self::metrics(window, cx, doc.number_digits());
        self.columns = if doc.side_by_side_mode() { 2 } else { 1 };
        self.gutter = if doc.side_by_side_mode() {
            metrics.number + metrics.sign
        } else {
            metrics.number * 2. + metrics.sign
        } + px(8.);
        self.widest = metrics.advance * doc.widest() as f32;
        let len = doc.len();
        let lines = if self.wrap {
            list(
                self.wrapped.clone(),
                cx.processor(|this, index: usize, window, cx| {
                    this.render_wrapped(index, window, cx)
                }),
            )
            .size_full()
            .into_any_element()
        } else {
            let mut rows = uniform_list("diff-rows", len, cx.processor(Self::render_rows))
                .track_scroll(&self.scroll)
                .size_full();
            // Sideways input scrolls the lines, never up or down.
            rows.style().restrict_scroll_to_axis = Some(true);
            rows.into_any_element()
        };
        root.child(
            div()
                .id("diff-lines")
                .test_support()
                .role(Role::List)
                .aria_label(text::diff_of(&target.path))
                .flex_1()
                .min_h_0()
                .bg(colors.background)
                .text_color(colors.text)
                .font_family(metrics.family.clone())
                .text_size(metrics.size)
                .on_scroll(cx.listener(Self::scroll_sideways))
                .child(lines),
        )
        .children(newline)
    }
}
