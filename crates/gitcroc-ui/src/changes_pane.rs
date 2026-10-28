//! The changes pane of one workspace (UI-14, UI-16): what the history's
//! selected row is. A commit shows its header, with a parent selector for a
//! merge commit, and its changed files; "Uncommitted changes" shows the
//! Unmerged, Staged and Unstaged lists, sectioned by repo for a group or
//! several repos (UI-34). The lists are flat or a folder tree, virtualized
//! (PERF-1), and one file is selected; its diff is the [`DiffView`] below
//! them, with the diff options and the dock button in the pane header
//! (UI-17, UI-19). The window keeps one per open workspace, so each tab
//! keeps its own (UI-29). Everything shown comes from the core; this lays
//! it out and forwards input.

use std::ops::Range;
use std::rc::Rc;

use gitcroc_app::app::AppEvent;
use gitcroc_core::{
    ChangesContent, ChangesSettings, CommitDetails, Context as DiffContext, DiffMode, Dock,
    FileListMode, FileStatus, Person, UnmergedKind, WorkspaceId,
};
use gpui_component::Selectable;
use gpui_component::button::{Button, ButtonVariants};
use gpui_component::checkbox::Checkbox;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::{
    AnyElement, App, AppContext, ClickEvent, Context, CursorStyle, ElementId, Entity, EventEmitter,
    FocusHandle, Focusable, Global, Hsla, InteractiveElement, IntoElement, KeyBinding,
    KeyBindingContextPredicate, ParentElement, Pixels, Render, Role, ScrollStrategy, SharedString,
    StatefulInteractiveElement, Styled, Subscription, TestSupportExt, UniformListScrollHandle,
    Window, actions, div, point, px, svg, uniform_list,
};

use crate::changes::{Changes, Entry, FileKey, FileRow, List, Read};
use crate::diff_view::DiffView;
use crate::history::{self, Selection};
use crate::history_pane::HistoryPane;
use crate::icons;
use crate::services::Services;
use crate::strings::{self, changes as text};
use crate::theme::{ActiveTokens, tokens::Tokens};

actions!(
    changes_pane,
    [SelectNext, SelectPrevious, SelectFirst, SelectLast]
);

/// The key context of the file list.
const CONTEXT: &str = "GitcrocChanges";

/// One file row's height; `uniform_list` lays out only those on screen.
pub const ROW_HEIGHT: Pixels = px(22.);
const INDENT: f32 = 14.;
const ICON: Pixels = px(12.);
/// The commit message scrolls past this height.
const MESSAGE_HEIGHT: Pixels = px(160.);
/// The width of a header line's name.
const FIELD_WIDTH: Pixels = px(84.);
/// A parent's short hash, as rc.1 shows it.
const SHORT_HASH: usize = 10;

/// The keys of the file list (UI-22).
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
    ];
    cx.bind_keys(bindings.into_iter().flatten());
}

/// The changes pane's view settings, the same for every workspace (UI-14,
/// UI-17): read from the app when the window opens, and saved with
/// `settings_set_changes` when one changes.
#[derive(Debug, Clone, Copy, Default)]
pub struct ChangesPrefs(pub ChangesSettings);

impl Global for ChangesPrefs {}

impl ChangesPrefs {
    pub fn get(cx: &App) -> ChangesSettings {
        cx.try_global::<Self>()
            .map(|prefs| prefs.0)
            .unwrap_or_default()
    }
}

/// The dock button was pressed (UI-19); the panes move the pane.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ToggleDock;

/// The changes of one workspace.
pub struct ChangesPane {
    services: Services,
    workspace_id: WorkspaceId,
    changes: Changes,
    history: Entity<HistoryPane>,
    diff: Entity<DiffView>,
    focus: FocusHandle,
    scroll: UniformListScrollHandle,
    /// Where the pane is docked, for the dock button's label.
    dock: Dock,
    /// The latest read of the view; older answers are dropped.
    request: u64,
    _history_watch: Subscription,
    _prefs_watch: Subscription,
}

impl ChangesPane {
    pub fn new(
        services: Services,
        workspace_id: WorkspaceId,
        history: Entity<HistoryPane>,
        cx: &mut Context<Self>,
    ) -> Self {
        let history_watch = cx.observe(&history, |this, _, cx| this.follow_history(cx));
        let prefs_watch = cx.observe_global::<ChangesPrefs>(|this, cx| {
            let mode = ChangesPrefs::get(cx).file_list;
            if this.changes.set_mode(mode) {
                this.scroll_to_selected();
                cx.notify();
            }
        });
        let mut changes = Changes::default();
        changes.set_mode(ChangesPrefs::get(cx).file_list);
        let diff = cx.new(|cx| DiffView::new(services.clone(), workspace_id.clone(), cx));
        let mut this = Self {
            services,
            workspace_id,
            changes,
            history,
            diff,
            focus: cx.focus_handle(),
            scroll: UniformListScrollHandle::new(),
            dock: Dock::Right,
            request: 0,
            _history_watch: history_watch,
            _prefs_watch: prefs_watch,
        };
        this.follow_history(cx);
        this
    }

    pub fn changes(&self) -> &Changes {
        &self.changes
    }

    pub fn diff(&self) -> &Entity<DiffView> {
        &self.diff
    }

    /// Where the panes dock this pane (UI-19).
    pub fn set_dock(&mut self, dock: Dock, cx: &mut Context<Self>) {
        if self.dock != dock {
            self.dock = dock;
            cx.notify();
        }
    }

    /// F7 and Shift+F7 (UI-22).
    pub fn step_hunk(&mut self, step: isize, window: &mut Window, cx: &mut Context<Self>) {
        self.diff
            .update(cx, |diff, cx| diff.step_hunk(step, window, cx));
    }

    /// Shows the selected file's diff, or none.
    fn sync_diff(&mut self, cx: &mut Context<Self>) {
        let target = self.changes.diff_target();
        self.diff.update(cx, |diff, cx| diff.show(target, cx));
    }

    /// Follows the app's events about this workspace: a working copy shown
    /// that was read anew is read again (OPS-7).
    pub fn on_event(&mut self, event: &AppEvent, cx: &mut Context<Self>) {
        if let AppEvent::RepoSummary(view) = event
            && view.workspace_id == self.workspace_id
            && !view.refreshing
            && self.changes.watched().contains(&view.repo_id)
        {
            self.reload(cx);
        }
    }

    /// Reads the shown view again, after the app's events were missed.
    pub fn read_again(&mut self, cx: &mut Context<Self>) {
        self.reload(cx);
    }

    /// Shows what the history selects (UI-14, UI-15, UI-34).
    fn follow_history(&mut self, cx: &mut Context<Self>) {
        let selection = self.history.read(cx).selection();
        if self.changes.show(selection) {
            self.scroll
                .0
                .borrow()
                .base_handle
                .set_offset(point(px(0.), px(0.)));
            self.load(None, cx);
            self.sync_diff(cx);
            cx.notify();
        }
    }

    /// Reads the view again, keeping the selected file where it still
    /// exists (OPS-7).
    fn reload(&mut self, cx: &mut Context<Self>) {
        if self.changes.selection().is_none() {
            return;
        }
        let keep = self.changes.kept();
        self.load(keep, cx);
    }

    /// Reads the view of the shown selection (ARCH-7).
    fn load(&mut self, keep: Option<crate::changes::Kept>, cx: &mut Context<Self>) {
        let Some(read) = self.changes.read() else {
            return;
        };
        self.request = self.request.wrapping_add(1);
        let request = self.request;
        self.changes.start_load(keep.clone());
        cx.notify();
        let workspace = self.workspace_id.clone();
        let call = self.services.with_app(move |app| async move {
            match read {
                Read::Repo { repo, target } => app.changes_get(workspace, repo, target).await,
                Read::Merged(target) => app.changes_get_merged(workspace, target).await,
            }
        });
        cx.spawn(async move |this, cx| {
            let result = call.await;
            let _ = this.update(cx, |this, cx| {
                if this.request != request {
                    return;
                }
                match result {
                    Some(Ok(Some(view))) => {
                        this.changes.set_view(view);
                        this.scroll_to_selected();
                    }
                    // A read the core began later took over, yet this is
                    // the newest one sent: ask again, or the lists stay as
                    // they were (OPS-7).
                    Some(Ok(None)) => {
                        this.load(keep, cx);
                        return;
                    }
                    Some(Err(error)) => this.changes.set_error(error.message),
                    None => this.changes.set_loading(false),
                }
                this.sync_diff(cx);
                cx.notify();
            });
        })
        .detach();
    }

    /// Selects a parent in the history, as its link asks (UI-14).
    fn select_parent(&mut self, hash: String, cx: &mut Context<Self>) {
        let Some(Selection::Commit { repo, .. }) = self.changes.selection().cloned() else {
            return;
        };
        self.history
            .update(cx, |history, cx| history.select_commit(&repo, &hash, cx));
    }

    /// Compares a merge commit with another parent (UI-16).
    fn compare_with(&mut self, parent: u32, cx: &mut Context<Self>) {
        if self.changes.set_parent(parent) {
            self.load(None, cx);
        }
    }

    /// Lists the files flat or as a tree, in every workspace (UI-14).
    fn set_mode(&mut self, mode: FileListMode, cx: &mut Context<Self>) {
        self.set_prefs(|prefs| prefs.file_list = mode, cx);
    }

    /// Changes the pane's view settings for every workspace, and keeps
    /// them (UI-14, UI-17).
    fn set_prefs(&mut self, change: impl FnOnce(&mut ChangesSettings), cx: &mut Context<Self>) {
        let mut prefs = ChangesPrefs::get(cx);
        let before = prefs;
        change(&mut prefs);
        if prefs == before {
            return;
        }
        cx.set_global(ChangesPrefs(prefs));
        let call = self
            .services
            .with_app(move |app| async move { app.settings_set_changes(prefs).await });
        // A failed save keeps the choice for this session.
        cx.spawn(async move |_, _| {
            let _ = call.await;
        })
        .detach();
    }

    fn scroll_to_selected(&self) {
        if let Some(index) = self.changes.selected_index() {
            self.scroll.scroll_to_item(index, ScrollStrategy::Nearest);
        }
    }

    fn select_row(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        window.focus(&self.focus, cx);
        if self.changes.select_index(index) {
            self.scroll.scroll_to_item(index, ScrollStrategy::Nearest);
            self.sync_diff(cx);
            cx.notify();
        }
    }

    fn click(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(FileRow::Folder { key, .. }) = self.changes.rows().get(index) {
            let key = key.clone();
            window.focus(&self.focus, cx);
            self.changes.toggle_folder(&key);
            cx.notify();
            return;
        }
        self.select_row(index, window, cx);
    }

    fn select_next(&mut self, _: &SelectNext, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(index) = self.changes.step(1) {
            self.select_row(index, window, cx);
        }
    }

    fn select_previous(&mut self, _: &SelectPrevious, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(index) = self.changes.step(-1) {
            self.select_row(index, window, cx);
        }
    }

    fn select_first(&mut self, _: &SelectFirst, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(index) = self.changes.end(false) {
            self.select_row(index, window, cx);
        }
    }

    fn select_last(&mut self, _: &SelectLast, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(index) = self.changes.end(true) {
            self.select_row(index, window, cx);
        }
    }
}

/// A file status's letter, name and colour.
fn status(entry: Entry, tokens: &Tokens) -> (&'static str, &'static str, Hsla) {
    match entry {
        Entry::Unmerged(_) => (text::CONFLICT_LETTER, text::CONFLICT, tokens.danger.into()),
        Entry::Changed(file) => match file.status {
            FileStatus::Added => (text::ADDED_LETTER, text::ADDED, tokens.success.into()),
            FileStatus::Modified => (text::MODIFIED_LETTER, text::MODIFIED, tokens.warning.into()),
            FileStatus::Deleted => (text::DELETED_LETTER, text::DELETED, tokens.danger.into()),
            FileStatus::Renamed => (text::RENAMED_LETTER, text::RENAMED, tokens.accent.into()),
            FileStatus::TypeChanged => (
                text::TYPE_CHANGED_LETTER,
                text::TYPE_CHANGED,
                tokens.accent.into(),
            ),
            FileStatus::Untracked => (
                text::UNTRACKED_LETTER,
                text::UNTRACKED,
                tokens.text_secondary.into(),
            ),
        },
    }
}

fn conflict_name(kind: UnmergedKind) -> &'static str {
    match kind {
        UnmergedKind::BothDeleted => text::BOTH_DELETED,
        UnmergedKind::AddedByUs => text::ADDED_BY_US,
        UnmergedKind::DeletedByThem => text::DELETED_BY_THEM,
        UnmergedKind::AddedByThem => text::ADDED_BY_THEM,
        UnmergedKind::DeletedByUs => text::DELETED_BY_US,
        UnmergedKind::BothAdded => text::BOTH_ADDED,
        UnmergedKind::BothModified => text::BOTH_MODIFIED,
    }
}

fn section_name(list: List) -> &'static str {
    match list {
        List::Unmerged => text::UNMERGED,
        List::Staged => text::STAGED,
        List::Unstaged | List::Commit => text::UNSTAGED,
    }
}

fn row_id(index: usize, part: &str) -> ElementId {
    if part.is_empty() {
        ElementId::Name(format!("changes-row-{index}").into())
    } else {
        ElementId::Name(format!("changes-row-{index}-{part}").into())
    }
}

/// A file list row, before its role and content.
fn row_base(
    index: usize,
) -> impl ParentElement + Styled + StatefulInteractiveElement + IntoElement {
    div()
        .id(row_id(index, ""))
        .test_support()
        .h(ROW_HEIGHT)
        .w_full()
        .flex()
        .flex_row()
        .items_center()
        .gap_1()
        .pr_2()
        .overflow_hidden()
        .whitespace_nowrap()
        .text_size(px(13.))
}

/// A part of a row or of the header, named for screen readers.
fn part(
    id: ElementId,
    role: Role,
    label: impl Into<SharedString>,
) -> impl ParentElement + Styled + StatefulInteractiveElement + IntoElement {
    div()
        .id(id)
        .test_support()
        .role(role)
        .aria_label(label)
        .flex_shrink_0()
}

impl ChangesPane {
    fn render_rows(
        &mut self,
        range: Range<usize>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Vec<AnyElement> {
        range
            .filter(|index| *index < self.changes.rows().len())
            .map(|index| self.render_row(index, window, cx))
            .collect()
    }

    fn render_row(&self, index: usize, window: &Window, cx: &mut Context<Self>) -> AnyElement {
        let tokens = *cx.tokens();
        let base = row_base(index);
        let indent = |depth: usize| px(8. + INDENT * depth as f32);
        let Some(row) = self.changes.rows().get(index) else {
            return base.into_any_element();
        };
        match row {
            FileRow::Section { list, count } => {
                let label = text::counted(section_name(*list), *count);
                base.role(Role::Heading)
                    .aria_level(1)
                    .aria_label(label.clone())
                    .pl(indent(0))
                    .bg(tokens.bg_2)
                    .font_weight(gpui_kit::FontWeight::SEMIBOLD)
                    .text_color(tokens.text_secondary)
                    .child(label)
                    .into_any_element()
            }
            FileRow::Repo {
                name,
                count,
                blocked,
                ..
            } => base
                .role(Role::Heading)
                .aria_level(2)
                .aria_label(name.clone())
                .pl(indent(0))
                .text_color(tokens.text_primary)
                .font_weight(gpui_kit::FontWeight::SEMIBOLD)
                .child(
                    div()
                        .min_w_0()
                        .truncate()
                        .child(text::counted(name, *count)),
                )
                .children(blocked.as_ref().map(|reason| {
                    part(row_id(index, "blocked"), Role::Label, text::BLOCKED)
                        .aria_description(reason.clone())
                        .text_color(tokens.warning)
                        .child(text::BLOCKED)
                }))
                .into_any_element(),
            FileRow::Folder {
                name,
                depth,
                collapsed,
                ..
            } => base
                .role(Role::TreeItem)
                .aria_label(name.clone())
                .aria_level(depth + 1)
                .aria_expanded(!collapsed)
                .pl(indent(*depth))
                .cursor(CursorStyle::PointingHand)
                .hover(|row| row.bg(tokens.bg_3))
                .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                    this.click(index, window, cx);
                }))
                .child(
                    svg()
                        .path(if *collapsed {
                            icons::CHEVRON_RIGHT
                        } else {
                            icons::CHEVRON_DOWN
                        })
                        .size(ICON)
                        .flex_shrink_0()
                        .text_color(tokens.text_secondary),
                )
                .child(
                    div()
                        .min_w_0()
                        .truncate()
                        .text_color(tokens.text_primary)
                        .child(name.clone()),
                )
                .into_any_element(),
            FileRow::File { file, name, depth } => {
                self.file_row(index, file, name, *depth, window, cx)
            }
        }
    }

    fn file_row(
        &self,
        index: usize,
        file: &FileKey,
        name: &str,
        depth: usize,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let tokens = *cx.tokens();
        let selected = self.changes.file() == Some(file);
        let focused = self.focus.is_focused(window);
        let name = SharedString::from(name.to_owned());
        let mut row = row_base(index)
            .role(Role::TreeItem)
            .aria_label(name.clone())
            .aria_level(depth + 1)
            .aria_selected(selected)
            .when(selected && focused, |row| row.aria_active_descendant())
            // Room for a folder's chevron, so names line up in a tree.
            .pl(px(8. + INDENT * depth as f32 + f32::from(ICON) + 4.))
            .when(selected, |row| row.bg(tokens.selection_bg))
            .when(selected && focused, |row| {
                row.border_1().border_color(tokens.focus_ring)
            })
            .when(!selected, |row| row.hover(|row| row.bg(tokens.bg_3)))
            .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                this.click(index, window, cx);
            }));
        let Some(entry) = self.changes.entry_of(file) else {
            return row.child(name).into_any_element();
        };
        let (letter, status_name, color) = status(entry, &tokens);
        row = row
            .aria_description(status_name)
            .child(
                part(row_id(index, "status"), Role::Image, status_name)
                    .w(px(12.))
                    .text_color(color)
                    .font_weight(gpui_kit::FontWeight::SEMIBOLD)
                    .child(letter),
            )
            .child(
                div()
                    .min_w_0()
                    .truncate()
                    .text_color(tokens.text_primary)
                    .child(name),
            );
        match entry {
            Entry::Changed(changed) => {
                if let Some(old) = &changed.old_path {
                    row = row.child(
                        part(row_id(index, "old"), Role::Label, text::renamed_from(old))
                            .min_w_0()
                            .flex_shrink(1.)
                            .truncate()
                            .text_color(tokens.text_secondary)
                            .child(text::old_path(old)),
                    );
                }
                if changed.added.is_some() || changed.removed.is_some() {
                    let (added, removed) = (
                        changed.added.unwrap_or_default(),
                        changed.removed.unwrap_or_default(),
                    );
                    row = row.child(
                        part(
                            row_id(index, "counts"),
                            Role::Label,
                            text::counts(added, removed),
                        )
                        .ml_auto()
                        .pl_2()
                        .text_color(tokens.text_secondary)
                        .child(text::counts_mark(added, removed)),
                    );
                }
            }
            Entry::Unmerged(unmerged) => {
                let kind = conflict_name(unmerged.conflict);
                row = row.child(
                    part(
                        row_id(index, "conflict"),
                        Role::Label,
                        text::conflict_kind(kind),
                    )
                    .text_color(tokens.danger)
                    .child(text::conflict_mark(kind)),
                );
            }
        }
        row.into_any_element()
    }

    /// One line of the commit header: its name, then its value.
    fn field(
        id: &'static str,
        name: &'static str,
        value: SharedString,
        tokens: &Tokens,
    ) -> AnyElement {
        div()
            .id(id)
            .test_support()
            .role(Role::Label)
            .aria_label(text::field(name, &value))
            .flex()
            .flex_row()
            .gap_2()
            .child(
                div()
                    .flex_shrink_0()
                    .w(FIELD_WIDTH)
                    .text_color(tokens.text_secondary)
                    .child(name),
            )
            .child(div().min_w_0().text_color(tokens.text_primary).child(value))
            .into_any_element()
    }

    fn person(person: &Person) -> SharedString {
        format!(
            "{}  {}",
            text::person(&person.name, &person.email),
            history::format_full_date(person.time)
        )
        .into()
    }

    /// A commit's header (UI-14) and, for a merge commit, its parent
    /// selector (UI-16). All of it is repo text, rendered as text (SEC-3).
    fn commit_header(
        &self,
        details: &CommitDetails,
        parent: u32,
        tokens: &Tokens,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let parents = div()
            .id("changes-parents")
            .test_support()
            .role(Role::Group)
            .aria_label(text::parents(details.parents.len()))
            .flex()
            .flex_row()
            .items_center()
            .gap_2()
            .child(
                div()
                    .flex_shrink_0()
                    .w(FIELD_WIDTH)
                    .text_color(tokens.text_secondary)
                    .child(text::parents(details.parents.len())),
            )
            .when(details.parents.is_empty(), |line| {
                line.child(
                    div()
                        .text_color(tokens.text_secondary)
                        .child(text::NO_PARENTS),
                )
            })
            .children(details.parents.iter().enumerate().map(|(at, hash)| {
                let short: String = hash.chars().take(SHORT_HASH).collect();
                let hash = hash.clone();
                Button::new(ElementId::Name(format!("changes-parent-{at}").into()))
                    .link()
                    .compact()
                    .label(short)
                    .accessibility_label(text::select(&hash))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.select_parent(hash.clone(), cx);
                    }))
            }));
        let compare = (details.parents.len() > 1).then(|| {
            div()
                .id("changes-compare")
                .test_support()
                .role(Role::Group)
                .aria_label(text::COMPARE_WITH_PARENT)
                .flex()
                .flex_row()
                .flex_wrap()
                .items_center()
                .gap_1()
                .child(
                    div()
                        .flex_shrink_0()
                        .w(FIELD_WIDTH)
                        .text_color(tokens.text_secondary)
                        .child(text::COMPARE_WITH),
                )
                .children(details.parents.iter().enumerate().map(|(at, hash)| {
                    let name = if at == 0 {
                        text::FIRST_PARENT.to_owned()
                    } else {
                        text::parent(at + 1)
                    };
                    let short: String = hash.chars().take(SHORT_HASH).collect();
                    let chosen = parent as usize == at;
                    let at = u32::try_from(at).unwrap_or(u32::MAX);
                    Button::new(ElementId::Name(format!("changes-compare-{at}").into()))
                        .compact()
                        .outline()
                        .label(text::parent_choice(&name, &short))
                        .selected(chosen)
                        .toggled(chosen)
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.compare_with(at, cx);
                        }))
                }))
        });
        let message = SharedString::from(details.message.trim_end().to_owned());
        div()
            .id("changes-commit")
            .test_support()
            .role(Role::Group)
            .aria_label(text::COMMIT)
            .flex_shrink_0()
            .flex()
            .flex_col()
            .gap_1()
            .p_2()
            .border_b_1()
            .border_color(tokens.border)
            .child(Self::field(
                "changes-commit-hash",
                text::COMMIT,
                details.hash.clone().into(),
                tokens,
            ))
            .child(Self::field(
                "changes-author",
                text::AUTHOR,
                Self::person(&details.author),
                tokens,
            ))
            .child(Self::field(
                "changes-committer",
                text::COMMITTER,
                Self::person(&details.committer),
                tokens,
            ))
            .child(parents)
            .children(compare)
            .child(
                div()
                    .id("changes-message")
                    .test_support()
                    .role(Role::Label)
                    .aria_label(message.clone())
                    .mt_1()
                    .max_h(MESSAGE_HEIGHT)
                    .overflow_y_scroll()
                    .text_color(tokens.text_primary)
                    .child(message),
            )
            .into_any_element()
    }

    /// The files header: their number and the Flat and Tree toggle.
    fn files_header(&self, tokens: &Tokens, cx: &mut Context<Self>) -> AnyElement {
        let mode = self.changes.mode();
        let count = text::files(self.changes.file_count());
        let toggle = |id: &'static str, label: &'static str, shown: FileListMode| {
            Button::new(id)
                .compact()
                .ghost()
                .label(label)
                .selected(mode == shown)
                .toggled(mode == shown)
                .on_click(cx.listener(move |this, _, _, cx| this.set_mode(shown, cx)))
        };
        div()
            .id("changes-files-header")
            .test_support()
            .flex_shrink_0()
            .flex()
            .flex_row()
            .items_center()
            .justify_between()
            .px_2()
            .py_1()
            .border_b_1()
            .border_color(tokens.border)
            .child(
                part("changes-file-count".into(), Role::Label, count.clone())
                    .text_color(tokens.text_secondary)
                    .child(count),
            )
            .child(
                div()
                    .id("changes-file-list-mode")
                    .test_support()
                    .role(Role::Group)
                    .aria_label(text::FILE_LIST)
                    .flex()
                    .flex_row()
                    .gap_1()
                    .child(toggle("changes-flat", text::FLAT, FileListMode::Flat))
                    .child(toggle("changes-tree", text::TREE, FileListMode::Tree)),
            )
            .into_any_element()
    }

    /// The pane header: the diff mode and options (UI-17), for every
    /// workspace, and the dock button (UI-19).
    fn options(&self, tokens: &Tokens, cx: &mut Context<Self>) -> AnyElement {
        let prefs = ChangesPrefs::get(cx);
        let toggle = |id: &'static str, label: &'static str, on: bool| {
            Button::new(id)
                .compact()
                .ghost()
                .label(label)
                .selected(on)
                .toggled(on)
        };
        let mode =
            |id, label, mode: DiffMode| {
                toggle(id, label, prefs.diff_mode == mode).on_click(cx.listener(
                    move |this, _, _, cx| this.set_prefs(|prefs| prefs.diff_mode = mode, cx),
                ))
            };
        let context = |id, label, context: DiffContext| {
            toggle(id, label, prefs.context == context).on_click(cx.listener(
                move |this, _, _, cx| this.set_prefs(|prefs| prefs.context = context, cx),
            ))
        };
        let group = |id: &'static str, label: &'static str| {
            div()
                .id(id)
                .test_support()
                .role(Role::Group)
                .aria_label(label)
                .flex()
                .flex_row()
                .items_center()
                .gap_1()
        };
        div()
            .id("changes-options")
            .test_support()
            .role(Role::Toolbar)
            .aria_label(text::DIFF_OPTIONS)
            .flex_shrink_0()
            .flex()
            .flex_row()
            .flex_wrap()
            .items_center()
            .gap_x_3()
            .gap_y_1()
            .px_2()
            .py_1()
            .border_b_1()
            .border_color(tokens.border)
            .bg(tokens.bg_2)
            .child(
                group("changes-diff-mode", text::DIFF_MODE)
                    .child(mode("changes-inline", text::INLINE, DiffMode::Inline))
                    .child(mode(
                        "changes-side-by-side",
                        text::SIDE_BY_SIDE,
                        DiffMode::SideBySide,
                    )),
            )
            .child(
                Checkbox::new("changes-ignore-whitespace")
                    .label(text::IGNORE_WHITESPACE)
                    .checked(prefs.ignore_whitespace)
                    .on_click(cx.listener(|this, checked: &bool, _, cx| {
                        let checked = *checked;
                        this.set_prefs(|prefs| prefs.ignore_whitespace = checked, cx);
                    })),
            )
            .child(
                Checkbox::new("changes-wrap-lines")
                    .label(text::WRAP_LINES)
                    .checked(prefs.wrap_lines)
                    .on_click(cx.listener(|this, checked: &bool, _, cx| {
                        let checked = *checked;
                        this.set_prefs(|prefs| prefs.wrap_lines = checked, cx);
                    })),
            )
            .child(
                group("changes-context", text::CONTEXT)
                    .child(div().text_color(tokens.text_secondary).child(text::CONTEXT))
                    .child(context(
                        "changes-context-three",
                        text::CONTEXT_THREE,
                        DiffContext::Three,
                    ))
                    .child(context(
                        "changes-context-ten",
                        text::CONTEXT_TEN,
                        DiffContext::Ten,
                    ))
                    .child(context(
                        "changes-context-whole",
                        text::WHOLE_FILE,
                        DiffContext::WholeFile,
                    )),
            )
            .child(
                div().ml_auto().child(
                    Button::new("changes-dock")
                        .compact()
                        .ghost()
                        .label(match self.dock {
                            Dock::Right => text::DOCK_BOTTOM,
                            Dock::Bottom => text::DOCK_RIGHT,
                        })
                        .on_click(cx.listener(|_, _, _, cx| cx.emit(ToggleDock))),
                ),
            )
            .into_any_element()
    }

    /// The selected file's header and its diff.
    fn diff_area(&self, tokens: &Tokens) -> AnyElement {
        let area = div()
            .id("changes-diff")
            .test_support()
            .role(Role::Group)
            .aria_label(text::DIFF)
            .flex_1()
            .min_h_0()
            .flex()
            .flex_col()
            .border_t_1()
            .border_color(tokens.border);
        let Some(entry) = self.changes.entry() else {
            return area
                .child(message(
                    "changes-no-file",
                    text::NO_FILE,
                    tokens.text_secondary,
                ))
                .into_any_element();
        };
        let (path, mode) = match entry {
            Entry::Changed(file) => (
                match &file.old_path {
                    Some(old) => text::renamed(old, &file.path),
                    None => file.path.clone(),
                },
                file.mode_change
                    .as_ref()
                    .map(|change| text::mode(&change.old, &change.new)),
            ),
            Entry::Unmerged(file) => (file.path.clone(), None),
        };
        let diff = matches!(entry, Entry::Changed(_)).then(|| self.diff.clone());
        area.child(
            div()
                .id("changes-file")
                .test_support()
                .role(Role::Heading)
                .aria_level(2)
                .aria_label(path.clone())
                .flex_shrink_0()
                .flex()
                .flex_row()
                .gap_2()
                .px_2()
                .py_1()
                .bg(tokens.bg_2)
                .text_color(tokens.text_primary)
                .child(div().min_w_0().truncate().child(path))
                .children(mode.map(|mode| {
                    part("changes-file-mode".into(), Role::Label, mode.clone())
                        .text_color(tokens.text_secondary)
                        .child(mode)
                })),
        )
        .children(diff)
        .into_any_element()
    }
}

impl EventEmitter<ToggleDock> for ChangesPane {}

impl Focusable for ChangesPane {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus.clone()
    }
}

impl Render for ChangesPane {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let tokens = *cx.tokens();
        let root = div()
            .id("changes")
            .test_support()
            .track_focus(&self.focus)
            .key_context(CONTEXT)
            .role(Role::Group)
            .aria_label(strings::regions::CHANGES)
            .on_action(cx.listener(Self::select_next))
            .on_action(cx.listener(Self::select_previous))
            .on_action(cx.listener(Self::select_first))
            .on_action(cx.listener(Self::select_last))
            .size_full()
            .flex()
            .flex_col()
            .text_sm()
            .child(self.options(&tokens, cx));
        if self.changes.selection().is_none() {
            return root.child(message(
                "changes-empty",
                text::NO_SELECTION,
                tokens.text_secondary,
            ));
        }
        let error = self.changes.error().map(|error| {
            let error = text::cannot_read(error);
            div()
                .id("changes-error")
                .test_support()
                .role(Role::Alert)
                .aria_label(error.clone())
                .px_2()
                .py_1()
                .text_color(tokens.danger)
                .child(error)
        });
        let Some(view) = self.changes.view() else {
            return root.children(error);
        };
        let header = match &view.content {
            ChangesContent::Commit {
                details, parent, ..
            } => {
                let details = details.clone();
                self.commit_header(&details, *parent, &tokens, cx)
            }
            _ => div()
                .id("changes-title")
                .test_support()
                .role(Role::Heading)
                .aria_level(1)
                .aria_label(text::UNCOMMITTED)
                .flex_shrink_0()
                .px_2()
                .py_1()
                .border_b_1()
                .border_color(tokens.border)
                .font_weight(gpui_kit::FontWeight::SEMIBOLD)
                .text_color(tokens.text_primary)
                .child(text::UNCOMMITTED)
                .into_any_element(),
        };
        let files_header = self.files_header(&tokens, cx);
        let list = div()
            .id("changes-files")
            .test_support()
            .role(Role::Tree)
            .aria_label(text::CHANGED_FILES)
            .flex_1()
            .min_h_0()
            .child(
                uniform_list(
                    "changes-rows",
                    self.changes.rows().len(),
                    cx.processor(Self::render_rows),
                )
                .track_scroll(&self.scroll)
                .size_full(),
            );
        let diff = self.diff_area(&tokens);
        root.children(error)
            .child(
                div()
                    .flex_1()
                    .min_h_0()
                    .flex()
                    .flex_col()
                    .child(header)
                    .child(files_header)
                    .child(list),
            )
            .child(diff)
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
