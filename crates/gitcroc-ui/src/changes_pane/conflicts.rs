//! What the changes pane shows for an unmerged file (OPS-3, OPS-5, OPS-9),
//! a port of rc.1's `ConflictView.tsx` and the non-AI parts of
//! `stores/conflicts.ts`: the core reads the file with `conflict_get`, and
//! M6-19a/b's editor shows it in place of the diff. Each file's state is
//! kept by workspace, repo and path for as long as the tab is open, whatever
//! is selected, so its buffer, cursor, scroll, undo and cards survive
//! switching (UI-29). Requests name the file by the current view's handle
//! and id, found again by path each time, so a view read anew never sends
//! an old id for a new file (ARCH-9). Nothing is written before Mark
//! resolved, which is a job (ARCH-7); a fresh list without the file drops
//! its state.
use std::collections::{HashMap, VecDeque};

use gitcroc_app::app::AppEvent;
use gitcroc_core::{
    ChangesContent, ChooseRequest, Conflict, ConflictChoice, ConflictContent, ConflictRead,
    ConflictStart, JobId, JobView, Outcome, ResolveRequest,
};
use gpui_component::button::{Button, ButtonVariants};
use gpui_component::{Disableable, Sizable};
use gpui_kit::{
    AnyElement, App, AppContext, Context, Entity, FocusHandle, Focusable, InteractiveElement,
    IntoElement, ParentElement, Role, SharedString, StatefulInteractiveElement, Styled,
    Subscription, TestSupportExt, Window, div,
};

use super::ChangesPane;
use super::conflict_ai::FileAi;
use crate::changes::List;
use crate::conflict::{Buffer, ConflictEditor, EditorKey, MarkResolved};
use crate::history::Selection;
use crate::strings::conflict as text;
use crate::theme::ActiveTokens;

/// The most ended jobs remembered, for a resolution whose job ends before
/// its request has answered.
const ENDED: usize = 32;

/// What the editor starts from (OPS-8).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Start {
    /// The conflict's hunks from the index stages.
    Conflict,
    /// The file as it is on disk.
    Disk,
}

/// One unmerged file's kept state.
pub struct KeptFile {
    pub conflict: Conflict,
    /// `None` while the edited-outside question waits for its answer.
    pub start: Option<Start>,
    /// Built off the UI thread; the next frame makes the editor from it.
    buffer: Option<Buffer>,
    pub editor: Option<Entity<ConflictEditor>>,
    unsaved: bool,
    /// Mark resolved was refused at this marker line (OPS-5).
    markers: Option<u32>,
    /// A request is on its way, or its job runs.
    sending: bool,
    job: Option<JobId>,
    error: Option<String>,
    /// Resolve with AI's state for the file (AI-3, AI-11).
    pub(super) ai: FileAi,
    _watch: Option<Subscription>,
}

impl KeptFile {
    fn new(conflict: Conflict) -> Self {
        let text = matches!(conflict.content, ConflictContent::Text { .. });
        let start = (!(conflict.edited_outside && text)).then_some(Start::Conflict);
        Self {
            conflict,
            start,
            buffer: None,
            editor: None,
            unsaved: false,
            markers: None,
            sending: false,
            job: None,
            error: None,
            ai: FileAi::opened(),
            _watch: None,
        }
    }
}

/// The kept conflict files of one workspace's changes pane (ARCH-9).
#[derive(Default)]
pub struct ConflictFiles {
    pub(super) files: HashMap<EditorKey, KeptFile>,
    /// The file being read, and its request; older answers are dropped.
    reading: Option<(EditorKey, u64)>,
    request: u64,
    read_error: Option<(EditorKey, String)>,
    /// Jobs that ended, newest last, with what went wrong.
    pub(super) ended: VecDeque<(JobId, Option<Outcome>, Option<String>)>,
    /// Which view takes the focus once it can (UI-22).
    refocus: Refocus,
}

/// Which conflict view takes the focus once it is shown and built.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
enum Refocus {
    #[default]
    None,
    /// The shown file's, as Enter on its row or an answer asked.
    Shown,
    /// The next file's after this one, which had the focus when it was
    /// resolved, so files are resolved one after another.
    After(EditorKey),
}

impl ConflictFiles {
    pub fn get(&self, key: &EditorKey) -> Option<&KeptFile> {
        self.files.get(key)
    }

    /// Unsaved files of the workspace, by repo and path.
    pub fn unsaved(&self) -> Vec<&EditorKey> {
        let mut keys: Vec<&EditorKey> = self
            .files
            .iter()
            .filter(|(_, file)| file.unsaved)
            .map(|(key, _)| key)
            .collect();
        keys.sort_by(|a, b| (&a.repo.0, &a.path).cmp(&(&b.repo.0, &b.path)));
        keys
    }

    pub fn forget_repo(&mut self, repo: &gitcroc_core::RepoId) {
        self.files.retain(|key, _| &key.repo != repo);
        if self
            .reading
            .as_ref()
            .is_some_and(|(key, _)| &key.repo == repo)
        {
            self.reading = None;
        }
    }

    pub(super) fn forget(&mut self, key: &EditorKey) {
        self.files.remove(key);
    }
}

pub(super) fn stopped() -> String {
    crate::strings::operations::STOPPED.to_owned()
}

impl ChangesPane {
    /// The selected unmerged file's key, if one is selected.
    fn selected_conflict(&self) -> Option<EditorKey> {
        let file = self.changes.file()?;
        if file.list != List::Unmerged {
            return None;
        }
        let path = self.changes.entry_of(file)?.path().to_owned();
        let repo = match (&file.repo, self.changes.selection()?) {
            (Some(repo), _) => repo.clone(),
            (None, Selection::Uncommitted { repo }) => repo.clone(),
            (None, _) => return None,
        };
        Some(EditorKey {
            workspace: self.workspace_id.clone(),
            repo,
            path,
        })
    }

    /// The handle and id that name `key`'s file in the view shown now.
    pub(super) fn conflict_target(&self, key: &EditorKey) -> Option<(u32, u32)> {
        let view = self.changes.view()?;
        let files = match &view.content {
            ChangesContent::Uncommitted { unmerged, .. } => match self.changes.selection()? {
                Selection::Uncommitted { repo } if repo == &key.repo => unmerged,
                _ => return None,
            },
            ChangesContent::Merged { repos, .. } => {
                &repos
                    .iter()
                    .find(|shown| shown.repo_id == key.repo)?
                    .unmerged
            }
            ChangesContent::Commit { .. } => return None,
        };
        let id = files.iter().find(|file| file.path == key.path)?.id;
        let handle = self.changes.handle_of(Some(&key.repo))?;
        Some((handle, u32::try_from(id).ok()?))
    }

    /// The selected unmerged file's editor, as the pane shows it.
    pub fn shown_conflict(&self, _cx: &App) -> Option<Entity<ConflictEditor>> {
        let key = self.selected_conflict()?;
        self.conflicts.get(&key)?.editor.clone()
    }

    /// The kept editor of `path` in `repo`, shown or not.
    pub fn kept_conflict(
        &self,
        repo: &gitcroc_core::RepoId,
        path: &str,
    ) -> Option<Entity<ConflictEditor>> {
        let key = EditorKey {
            workspace: self.workspace_id.clone(),
            repo: repo.clone(),
            path: path.to_owned(),
        };
        self.conflicts.get(&key)?.editor.clone()
    }

    /// Mark resolved's focus, or Mark resolved anyway's.
    pub fn mark_focus(&self, anyway: bool) -> FocusHandle {
        if anyway {
            self.anyway_focus.clone()
        } else {
            self.mark_focus.clone()
        }
    }

    /// The kept files, for the questions before closing and restarting.
    pub fn conflicts(&self) -> &ConflictFiles {
        &self.conflicts
    }

    /// Whether unmerged `path` of `repo` has unsaved edits, for its row.
    pub(super) fn conflict_unsaved(&self, repo: Option<&gitcroc_core::RepoId>, path: &str) -> bool {
        let repo = match (repo, self.changes.selection()) {
            (Some(repo), _) => repo.clone(),
            (None, Some(Selection::Uncommitted { repo })) => repo.clone(),
            _ => return false,
        };
        let key = EditorKey {
            workspace: self.workspace_id.clone(),
            repo,
            path: path.to_owned(),
        };
        self.conflicts.get(&key).is_some_and(|file| file.unsaved)
    }

    /// Reads the selected unmerged file unless its state is kept (OPS-9).
    pub(super) fn sync_conflict(&mut self, cx: &mut Context<Self>) {
        let Some(key) = self.selected_conflict() else {
            return;
        };
        if self.conflicts.files.contains_key(&key)
            || self
                .conflicts
                .reading
                .as_ref()
                .is_some_and(|(reading, _)| reading == &key)
        {
            return;
        }
        let Some((handle, file)) = self.conflict_target(&key) else {
            return;
        };
        self.conflicts.request = self.conflicts.request.wrapping_add(1);
        let request = self.conflicts.request;
        self.conflicts.reading = Some((key.clone(), request));
        self.conflicts.read_error = None;
        let workspace = self.workspace_id.clone();
        let call = self
            .services
            .with_app(move |app| async move { app.conflict_get(workspace, handle, file).await });
        cx.spawn(async move |this, cx| {
            let result = call.await;
            let _ = this.update(cx, |this, cx| {
                if this.conflicts.reading.as_ref() != Some(&(key.clone(), request)) {
                    return;
                }
                this.conflicts.reading = None;
                match result {
                    Some(Ok(Some(ConflictRead::Conflict { conflict }))) => {
                        this.conflicts
                            .files
                            .insert(key.clone(), KeptFile::new(*conflict));
                        this.build_buffer(&key, cx);
                    }
                    // Resolved meanwhile, or a newer view: read the lists.
                    Some(Ok(Some(ConflictRead::AlreadyResolved) | None)) => this.reload(cx),
                    Some(Err(error)) => this.conflicts.read_error = Some((key, error.to_string())),
                    None => this.conflicts.read_error = Some((key, stopped())),
                }
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }

    /// Builds the editor's buffer of `key` off the UI thread (ARCH-7), from
    /// the conflict's hunks or the text on disk, as its start says.
    pub(super) fn build_buffer(&mut self, key: &EditorKey, cx: &mut Context<Self>) {
        let Some(file) = self.conflicts.files.get(&key.clone()) else {
            return;
        };
        let labels = file.conflict.labels.clone();
        let source = match (file.start, &file.conflict.content) {
            (Some(Start::Conflict), ConflictContent::Text { chunks }) => Ok(chunks.clone()),
            (Some(Start::Disk), _) => match &file.conflict.on_disk {
                Some(disk) => Err(disk.clone()),
                None => return,
            },
            _ => return,
        };
        let key = key.clone();
        cx.spawn(async move |this, cx| {
            let buffer = cx
                .background_spawn(async move {
                    match source {
                        Ok(chunks) => Buffer::build(&chunks, &labels),
                        // Plain text: no hunks, whatever it looks like.
                        Err(disk) => Buffer::plain(&disk, &labels),
                    }
                })
                .await;
            let _ = this.update(cx, |this, cx| {
                if let Some(file) = this.conflicts.files.get_mut(&key)
                    && file.editor.is_none()
                {
                    file.buffer = Some(buffer);
                    cx.notify();
                }
            });
        })
        .detach();
    }

    /// Makes the shown file's editor from its built buffer; it needs the
    /// window, so it is made as the pane is drawn.
    fn install_editor(&mut self, key: &EditorKey, window: &mut Window, cx: &mut Context<Self>) {
        let Some(file) = self.conflicts.files.get_mut(key) else {
            return;
        };
        let Some(buffer) = file.buffer.take() else {
            return;
        };
        let path = key.path.clone();
        let editor = cx.new(|cx| {
            let mut editor = ConflictEditor::new(buffer, window, cx);
            editor.set_language(&path, cx);
            editor
        });
        let watched = key.clone();
        // The row's unsaved mark follows the buffer.
        file._watch = Some(cx.observe(&editor, move |this, editor, cx| {
            let unsaved = editor.read(cx).unsaved(cx);
            if let Some(file) = this.conflicts.files.get_mut(&watched)
                && file.unsaved != unsaved
            {
                file.unsaved = unsaved;
                cx.notify();
            }
        }));
        file.editor = Some(editor.clone());
    }

    /// The resolved file's view had the focus and no unmerged file is left
    /// to take it: the file list takes it, so the keys go on working.
    pub(super) fn refocus_list(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !matches!(self.conflicts.refocus, Refocus::After(_))
            || self.selected_conflict().is_some()
            || self.changes.loading()
        {
            return;
        }
        self.conflicts.refocus = Refocus::None;
        let list = self.focus.clone();
        window.defer(cx, move |window, cx| window.focus(&list, cx));
    }

    /// The resolved file's view had the focus: the next file's view takes
    /// it once it can, its editor or its first button.
    fn take_refocus(&mut self, key: &EditorKey, window: &mut Window, cx: &mut Context<Self>) {
        match &self.conflicts.refocus {
            Refocus::None => return,
            Refocus::After(resolved) if resolved == key => {
                // Refused or failed: it stays, and so does the focus.
                if self
                    .conflicts
                    .get(key)
                    .is_some_and(|file| file.error.is_some() || file.markers.is_some())
                {
                    self.conflicts.refocus = Refocus::None;
                }
                return;
            }
            _ => {}
        }
        let Some(file) = self.conflicts.get(key) else {
            return;
        };
        let target = if let Some(editor) = &file.editor {
            editor.read(cx).editor.read(cx).focus_handle(cx)
        } else if matches!(file.conflict.content, ConflictContent::Choice { .. })
            || file.start.is_none()
        {
            if file.start.is_none() && file.conflict.on_disk.is_none() {
                self.second_focus.clone()
            } else {
                self.first_focus.clone()
            }
        } else {
            return;
        };
        self.conflicts.refocus = Refocus::None;
        window.defer(cx, move |window, cx| window.focus(&target, cx));
    }

    /// What the conflict editor's Mark resolved, hunk keys and AI buttons
    /// do now, for the palette (UI-22); `None` while no editor is shown.
    pub(super) fn editor_command(
        &self,
        pane: &Entity<Self>,
        op: gitcroc_core::PaletteOp,
        cx: &App,
    ) -> Option<crate::palette::Command> {
        use crate::conflict::Take;
        use crate::palette::Command;
        use gitcroc_core::PaletteOp;
        let key = self.selected_conflict()?;
        let file = self.conflicts.get(&key)?;
        let editor = file.editor.clone()?;
        let side = match op {
            PaletteOp::MarkResolved => {
                if file.sending {
                    return Some(Command::cannot(crate::strings::palette::CANNOT_APPLY));
                }
                let pane = pane.clone();
                return Some(Command::run(move |_, window, cx| {
                    pane.update(cx, |pane, cx| pane.mark_resolved(false, window, cx))
                }));
            }
            PaletteOp::ResolveWithAi | PaletteOp::AcceptHighConfidence => {
                return self.ai_command(pane, &key, &editor, op, cx);
            }
            PaletteOp::TakeOurs => Take::Ours,
            PaletteOp::TakeTheirs => Take::Theirs,
            PaletteOp::TakeBoth => Take::Both,
            _ => return None,
        };
        if editor.read(cx).hunk_at_cursor(cx).is_none() {
            return Some(Command::cannot(crate::strings::palette::NO_HUNK));
        }
        Some(Command::run(move |_, window, cx| {
            editor.update(cx, |editor, cx| editor.take_at_cursor(side, window, cx))
        }))
    }

    /// Keep ours or Keep theirs: the choice's button, for a file that has
    /// the choice (OPS-6).
    pub(super) fn choice_command(
        &self,
        pane: &Entity<Self>,
        op: gitcroc_core::PaletteOp,
    ) -> Option<crate::palette::Command> {
        use crate::palette::Command;
        let key = self.selected_conflict()?;
        let file = self.conflicts.get(&key)?;
        if !matches!(file.conflict.content, ConflictContent::Choice { .. }) {
            return Some(Command::cannot(crate::strings::palette::NOT_CHOICE));
        }
        if file.sending {
            return Some(Command::cannot(crate::strings::palette::CANNOT_APPLY));
        }
        let side = if op == gitcroc_core::PaletteOp::KeepOurs {
            ConflictChoice::Ours
        } else {
            ConflictChoice::Theirs
        };
        let pane = pane.clone();
        Some(Command::run(move |_, window, cx| {
            pane.update(cx, |pane, cx| pane.choose(&key, side, window, cx))
        }))
    }

    fn on_mark_resolved(&mut self, _: &MarkResolved, window: &mut Window, cx: &mut Context<Self>) {
        self.mark_resolved(false, window, cx);
    }

    /// Mark resolved (OPS-5): the whole buffer goes to `conflict_resolve`,
    /// which writes and stages it as a job. `override_markers` resolves
    /// although marker lines remain.
    fn mark_resolved(
        &mut self,
        override_markers: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(key) = self.selected_conflict() else {
            return;
        };
        let Some(file) = self.conflicts.files.get(&key) else {
            return;
        };
        if file.sending {
            return;
        }
        let Some(editor) = file.editor.clone() else {
            return;
        };
        let Some((handle, id)) = self.conflict_target(&key) else {
            self.reload(cx);
            return;
        };
        let focused = editor
            .read(cx)
            .editor
            .read(cx)
            .focus_handle(cx)
            .contains_focused(window, cx)
            || self.mark_focus.is_focused(window)
            || self.anyway_focus.is_focused(window);
        if focused {
            self.conflicts.refocus = Refocus::After(key.clone());
        }
        let text = editor.read(cx).value(cx);
        if let Some(file) = self.conflicts.files.get_mut(&key) {
            file.sending = true;
            file.markers = None;
            file.error = None;
        }
        let workspace = self.workspace_id.clone();
        let request = ResolveRequest {
            handle,
            file: id,
            text,
            override_markers,
        };
        let call = self
            .services
            .with_app(move |app| async move { app.conflict_resolve(workspace, request).await });
        self.settle(key, call, cx);
        cx.notify();
    }

    /// What a resolution's answer does: a job, a refusal naming a marker
    /// line, or a file resolved meanwhile or a view replaced, which reads
    /// the lists again.
    pub(super) fn settle<E: std::fmt::Display + Send + 'static>(
        &mut self,
        key: EditorKey,
        call: crate::services::Call<Result<Option<ConflictStart>, E>>,
        cx: &mut Context<Self>,
    ) {
        cx.spawn(async move |this, cx| {
            let result = call.await;
            let _ = this.update(cx, |this, cx| {
                let reload = {
                    let ended = &this.conflicts.ended;
                    let Some(file) = this.conflicts.files.get_mut(&key) else {
                        return;
                    };
                    match result {
                        Some(Ok(Some(ConflictStart::Started { job_id }))) => {
                            match ended.iter().find(|(job, _, _)| *job == job_id) {
                                Some((_, outcome, message)) => {
                                    file.sending = false;
                                    file.error = failure(outcome.clone(), message.as_deref());
                                }
                                None => file.job = Some(job_id),
                            }
                            false
                        }
                        Some(Ok(Some(ConflictStart::MarkersRemain { line }))) => {
                            file.sending = false;
                            file.markers = Some(line);
                            false
                        }
                        Some(Ok(Some(ConflictStart::AlreadyResolved))) => {
                            this.conflicts.forget(&key);
                            true
                        }
                        Some(Ok(None)) => {
                            file.sending = false;
                            true
                        }
                        Some(Err(error)) => {
                            file.sending = false;
                            file.error = Some(error.to_string());
                            false
                        }
                        None => {
                            file.sending = false;
                            file.error = Some(stopped());
                            false
                        }
                    }
                };
                if reload {
                    this.reload(cx);
                }
                cx.notify();
            });
        })
        .detach();
    }

    /// A job ended: a resolution's failure is said by its file, which
    /// keeps its buffer (OPS-9).
    pub(super) fn conflict_event(&mut self, event: &AppEvent, cx: &mut Context<Self>) {
        match event {
            AppEvent::JobEnded(job) if job.workspace_id == self.workspace_id => {
                self.job_ended(job);
                cx.notify();
            }
            // A fresh, loaded summary out of conflict state: the conflict
            // ended, here or in a terminal, and its edits go with it. A
            // refreshing, stale or unread summary proves nothing (OPS-9).
            AppEvent::RepoSummary(view)
                if view.workspace_id == self.workspace_id
                    && !view.refreshing
                    && !view.stale
                    && view.summary.is_some()
                    && !view.conflict =>
            {
                self.forget_conflict_repo(&view.repo_id, cx);
            }
            _ => {}
        }
    }

    /// Drops the kept files of `repo`: its conflict state ended (OPS-9).
    pub fn forget_conflict_repo(&mut self, repo: &gitcroc_core::RepoId, cx: &mut Context<Self>) {
        let before = self.conflicts.files.len();
        self.conflicts.forget_repo(repo);
        if self.conflicts.files.len() != before {
            cx.notify();
        }
    }

    /// Enter or Space on an unmerged row: the focus moves into its view,
    /// the editor or the first button (UI-22).
    pub(super) fn focus_conflict(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(key) = self.selected_conflict() else {
            return;
        };
        match self.conflicts.get(&key) {
            Some(file) if file.editor.is_some() => {
                if let Some(editor) = &file.editor {
                    let state = editor.read(cx).editor.clone();
                    state.update(cx, |state, cx| state.focus(window, cx));
                }
            }
            Some(file)
                if file.start.is_none()
                    || matches!(file.conflict.content, ConflictContent::Choice { .. }) =>
            {
                let first = if file.start.is_none() && file.conflict.on_disk.is_none() {
                    self.second_focus.clone()
                } else {
                    self.first_focus.clone()
                };
                window.focus(&first, cx);
            }
            // Not read or built yet: it takes the focus when it is.
            _ => {
                self.conflicts.refocus = Refocus::Shown;
                cx.notify();
            }
        }
    }

    /// F7 and Shift+F7 with an unmerged file shown: its next or previous
    /// hunk's controls (UI-22). `false` when no conflict editor is shown.
    pub(super) fn step_conflict(
        &mut self,
        step: isize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(editor) = self.shown_conflict(cx) else {
            return false;
        };
        editor.update(cx, |editor, cx| editor.navigate(step > 0, window, cx));
        true
    }

    fn job_ended(&mut self, job: &JobView) {
        let ended = &mut self.conflicts.ended;
        ended.push_back((job.job_id, job.outcome.clone(), job.message.clone()));
        while ended.len() > ENDED {
            ended.pop_front();
        }
        for file in self.conflicts.files.values_mut() {
            if file.job == Some(job.job_id) {
                file.job = None;
                file.sending = false;
                file.error = failure(job.outcome.clone(), job.message.as_deref());
            }
        }
        self.ai_job_ended(job);
    }

    /// Drops the kept files of the view's repos that its fresh lists no
    /// longer have: resolved, here or elsewhere (OPS-9).
    pub(super) fn prune_conflicts(&mut self) {
        let Some(view) = self.changes.view() else {
            return;
        };
        let lists: Vec<(gitcroc_core::RepoId, Vec<String>)> = match &view.content {
            ChangesContent::Uncommitted { unmerged, .. } => match self.changes.selection() {
                Some(Selection::Uncommitted { repo }) => vec![(
                    repo.clone(),
                    unmerged.iter().map(|file| file.path.clone()).collect(),
                )],
                _ => Vec::new(),
            },
            ChangesContent::Merged { repos, .. } => repos
                .iter()
                .map(|shown| {
                    (
                        shown.repo_id.clone(),
                        shown
                            .unmerged
                            .iter()
                            .map(|file| file.path.clone())
                            .collect(),
                    )
                })
                .collect(),
            ChangesContent::Commit { .. } => Vec::new(),
        };
        for (repo, paths) in lists {
            self.conflicts
                .files
                .retain(|key, _| key.repo != repo || paths.contains(&key.path));
        }
    }

    /// What the diff area shows for the selected unmerged file.
    pub(super) fn conflict_area(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let tokens = *cx.tokens();
        let Some(key) = self.selected_conflict() else {
            return div().into_any_element();
        };
        self.install_editor(&key, window, cx);
        self.take_refocus(&key, window, cx);
        self.sync_ai(&key, cx);
        let alert = |id: &'static str, message: String| {
            let message: SharedString = message.into();
            div()
                .id(id)
                .test_support()
                .role(Role::Alert)
                .aria_label(message.clone())
                .px_2()
                .py_1()
                .text_color(tokens.danger)
                .child(message)
        };
        let panel = div()
            .id("conflict-panel")
            .test_support()
            .key_context("GitcrocConflictPanel")
            .on_action(cx.listener(Self::on_mark_resolved))
            .flex_1()
            .min_h_0()
            .flex()
            .flex_col();
        let Some(file) = self.conflicts.get(&key) else {
            let shown = match &self.conflicts.read_error {
                Some((failed, error)) if failed == &key => {
                    alert("conflict-read-error", text::cannot_read(error)).into_any_element()
                }
                _ => div()
                    .id("conflict-reading")
                    .test_support()
                    .role(Role::Status)
                    .aria_label(text::READING)
                    .p_3()
                    .text_color(tokens.text_secondary)
                    .child(text::READING)
                    .into_any_element(),
            };
            return panel.child(shown).into_any_element();
        };
        if let Some(chosen) = self.special_conflict(&key, file, cx) {
            return panel.child(chosen).into_any_element();
        }
        let sending = file.sending;
        let markers = file.markers;
        let error = file.error.clone();
        let editor = file.editor.clone();
        let toolbar = div()
            .id("conflict-toolbar")
            .test_support()
            .role(Role::Toolbar)
            .aria_label(text::TOOLBAR)
            .flex_shrink_0()
            .flex()
            .flex_row()
            .flex_wrap()
            .items_center()
            .gap_2()
            .px_2()
            .py_1()
            .bg(tokens.bg_2)
            .child(
                Button::new("conflict-mark-resolved")
                    .small()
                    .primary()
                    .label(text::MARK_RESOLVED)
                    .tooltip(text::MARK_RESOLVED_TITLE)
                    .track_focus(&self.mark_focus)
                    .disabled(sending || editor.is_none())
                    .on_click(
                        cx.listener(|this, _, window, cx| this.mark_resolved(false, window, cx)),
                    ),
            )
            .children(self.ai_header(&key, cx));
        let markers = markers.map(|line| {
            div()
                .flex_shrink_0()
                .flex()
                .flex_row()
                .flex_wrap()
                .items_center()
                .gap_2()
                .child(alert("conflict-markers", text::marker_line(line)))
                .child(
                    Button::new("conflict-mark-anyway")
                        .small()
                        .label(text::MARK_RESOLVED_ANYWAY)
                        .track_focus(&self.anyway_focus)
                        .disabled(sending)
                        .on_click(
                            cx.listener(|this, _, window, cx| this.mark_resolved(true, window, cx)),
                        ),
                )
        });
        let error = error.map(|error| alert("conflict-error", text::cannot_mark_resolved(&error)));
        panel
            .child(toolbar)
            .children(markers)
            .children(error)
            .child(div().flex_1().min_h_0().children(editor))
            .into_any_element()
    }

    /// Answers the edited-outside question (OPS-8): the editor starts from
    /// the file on disk or from the conflict. Nothing is written.
    fn answer(&mut self, key: &EditorKey, start: Start, window: &Window, cx: &mut Context<Self>) {
        let Some(file) = self.conflicts.files.get_mut(key) else {
            return;
        };
        if start == Start::Disk && file.conflict.on_disk.is_none() {
            return;
        }
        file.start = Some(start);
        // The answer was given from the keyboard or the pointer; the
        // editor takes the focus either way, where the question had it.
        if self.first_focus.contains_focused(window, cx)
            || self.second_focus.contains_focused(window, cx)
            || window.focused(cx).is_none()
        {
            self.conflicts.refocus = Refocus::Shown;
        }
        self.build_buffer(key, cx);
        cx.notify();
    }

    /// Use this version (OPS-6): resolves with `side`'s stage, as a job.
    fn choose(
        &mut self,
        key: &EditorKey,
        side: ConflictChoice,
        window: &Window,
        cx: &mut Context<Self>,
    ) {
        let Some(file) = self.conflicts.files.get_mut(key) else {
            return;
        };
        if file.sending {
            return;
        }
        let Some((handle, id)) = self.conflict_target(key) else {
            self.reload(cx);
            return;
        };
        if self.first_focus.is_focused(window) || self.second_focus.is_focused(window) {
            self.conflicts.refocus = Refocus::After(key.clone());
        }
        if let Some(file) = self.conflicts.files.get_mut(key) {
            file.sending = true;
            file.error = None;
        }
        let workspace = self.workspace_id.clone();
        let request = ChooseRequest {
            handle,
            file: id,
            side,
        };
        let call = self
            .services
            .with_app(move |app| async move { app.conflict_choose(workspace, request).await });
        self.settle(key.clone(), call, cx);
        cx.notify();
    }

    /// The edited-outside question or a choice conflict, in place of the
    /// editor; `None` for an editor.
    fn special_conflict(
        &self,
        key: &EditorKey,
        file: &KeptFile,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let tokens = *cx.tokens();
        let paragraph = |id: &'static str, said: String| {
            let said: SharedString = said.into();
            div()
                .id(id)
                .test_support()
                .role(Role::Paragraph)
                .aria_label(said.clone())
                .child(said)
        };
        if let ConflictContent::Choice {
            reason,
            ours,
            theirs,
        } = &file.conflict.content
        {
            let labels = &file.conflict.labels;
            let side = |which: ConflictChoice,
                        label: String,
                        holds: &gitcroc_core::SideHolds,
                        focus: &FocusHandle,
                        cx: &mut Context<Self>| {
                let (id, holds_id, use_id) = match which {
                    ConflictChoice::Ours => (
                        "conflict-side-ours",
                        "conflict-holds-ours",
                        "conflict-use-ours",
                    ),
                    ConflictChoice::Theirs => (
                        "conflict-side-theirs",
                        "conflict-holds-theirs",
                        "conflict-use-theirs",
                    ),
                };
                let chosen = key.clone();
                let label: SharedString = label.into();
                div()
                    .id(id)
                    .test_support()
                    .role(Role::Region)
                    .aria_label(label.clone())
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .flex_col()
                    .gap_1()
                    .p_2()
                    .border_1()
                    .border_color(tokens.border)
                    .child(
                        div()
                            .id(gpui_kit::ElementId::Name(format!("{id}-heading").into()))
                            .role(Role::Heading)
                            .aria_level(3)
                            .aria_label(label.clone())
                            .font_weight(gpui_kit::FontWeight::SEMIBOLD)
                            .child(label.clone()),
                    )
                    .child(paragraph(holds_id, text::holds(holds)))
                    .child(
                        Button::new(use_id)
                            .small()
                            .label(text::USE_THIS)
                            .accessibility_label(text::use_this(&label))
                            .track_focus(focus)
                            .disabled(file.sending)
                            .on_click(cx.listener(move |this, _, window, cx| {
                                this.choose(&chosen, which, window, cx)
                            })),
                    )
            };
            let ours = side(
                ConflictChoice::Ours,
                text::ours(&labels.ours),
                ours,
                &self.first_focus,
                cx,
            );
            let theirs = side(
                ConflictChoice::Theirs,
                text::theirs(&labels.theirs),
                theirs,
                &self.second_focus,
                cx,
            );
            let error = file.error.as_ref().map(|error| {
                let error: SharedString = text::cannot_resolve(error).into();
                div()
                    .id("conflict-error")
                    .test_support()
                    .role(Role::Alert)
                    .aria_label(error.clone())
                    .text_color(tokens.danger)
                    .child(error)
            });
            return Some(
                div()
                    .id("conflict-choice")
                    .test_support()
                    .flex()
                    .flex_col()
                    .gap_2()
                    .p_2()
                    .child(paragraph(
                        "conflict-choice-reason",
                        text::choose(text::reason(*reason)),
                    ))
                    .child(
                        div()
                            .flex()
                            .flex_row()
                            .flex_wrap()
                            .gap_2()
                            .child(ours)
                            .child(theirs),
                    )
                    .children(error)
                    .into_any_element(),
            );
        }
        if file.start.is_some() {
            return None;
        }
        let no_disk = file.conflict.on_disk.is_none();
        let keep = key.clone();
        let over = key.clone();
        Some(
            div()
                .id("conflict-outside")
                .test_support()
                .role(Role::Group)
                .aria_label(text::EDITED_OUTSIDE)
                .flex()
                .flex_col()
                .gap_2()
                .p_2()
                .child(paragraph(
                    "conflict-outside-text",
                    text::changed_outside(&key.path),
                ))
                .children(
                    no_disk
                        .then(|| paragraph("conflict-no-disk", text::CANNOT_SHOW_DISK.to_owned())),
                )
                .child(
                    div()
                        .flex()
                        .flex_row()
                        .flex_wrap()
                        .gap_2()
                        .child(
                            Button::new("conflict-keep-disk")
                                .small()
                                .label(text::KEEP_DISK)
                                .track_focus(&self.first_focus)
                                .disabled(no_disk)
                                .on_click(cx.listener(move |this, _, window, cx| {
                                    this.answer(&keep, Start::Disk, window, cx)
                                })),
                        )
                        .child(
                            Button::new("conflict-start-over")
                                .small()
                                .label(text::START_OVER)
                                .track_focus(&self.second_focus)
                                .on_click(cx.listener(move |this, _, window, cx| {
                                    this.answer(&over, Start::Conflict, window, cx)
                                })),
                        ),
                )
                .into_any_element(),
        )
    }
}

/// What a resolution's job said when it did not succeed.
fn failure(outcome: Option<Outcome>, message: Option<&str>) -> Option<String> {
    match outcome {
        Some(Outcome::Succeeded | Outcome::NothingToDo) => None,
        _ => Some(
            message
                .unwrap_or(crate::strings::operations::STOPPED)
                .to_owned(),
        ),
    }
}
