//! The missing repos dialog (DATA-2), a port of rc.1's
//! `ui/src/clone/MissingReposDialog.tsx` and `ui/src/stores/share.ts`.
//! After an import, or from a missing repo's menu, it lists the
//! workspace's repos whose folder is missing, as the core reports them,
//! with their remote. Each row has a check box, checked when the repo has a
//! remote (or only the repo the dialog was opened for), and its target as
//! parent folder and name, which the core checks as they change; a parent
//! that does not exist yet is created by the clone. A repo with no remote
//! can't be checked, and says so. Clone selected starts one clone job per
//! checked row, all at once; each row then shows that it is cloning and how
//! its clone ended, with git's message when it failed. A refused start
//! shows on its row. The dialog stays open until closed. Names, paths and
//! URLs come from a shared file and are text only (SEC-3).

use crate::scrollbars::ScrollAreaExt as _;
use std::collections::HashMap;
use std::path::PathBuf;

use gitcroc_app::app::AppEvent;
use gitcroc_core::{CloneReport, CloneRequest, JobId, MissingRepo, RepoId, WorkspaceId};
use gpui_component::button::{Button, ButtonVariants};
use gpui_component::checkbox::Checkbox;
use gpui_component::input::{Input, InputEvent, InputState};
use gpui_component::{Disableable, FocusTrapElement};
use gpui_kit::{
    App, AppContext, ClickEvent, Context, ElementId, Entity, EventEmitter, FocusHandle, Focusable,
    InteractiveElement, IntoElement, ParentElement, Render, Role, SharedString,
    StatefulInteractiveElement, Styled, Subscription, Task, TestSupportExt, Window, div, px,
};
use tokio::sync::broadcast::error::RecvError;

use crate::dialog::{self, Cancel, Closed, Modal};
use crate::services::Services;
use crate::strings::{clone as clone_text, missing_repos as text};
use crate::theme::ActiveTokens;

/// Where a row's clone is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RowState {
    Idle,
    Cloning,
    Cloned,
    Failed,
}

/// The core's verdict on a row's target.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Verdict {
    parent: PathBuf,
    name: String,
    error: Option<String>,
}

struct Row {
    repo: MissingRepo,
    checked: bool,
    parent: PathBuf,
    name: Entity<InputState>,
    verdict: Option<Verdict>,
    state: RowState,
    job: Option<JobId>,
    /// Why the clone failed.
    message: Option<String>,
    _name_events: Subscription,
}

impl Row {
    fn busy(&self) -> bool {
        matches!(self.state, RowState::Cloning | RowState::Cloned)
    }
}

/// The key of a row's folder picks, which comes back with the pick.
fn pick_key(repo: &RepoId) -> String {
    format!("missing:{}", repo.0)
}

pub struct MissingReposDialog {
    services: Services,
    workspace: WorkspaceId,
    /// The repo the dialog was opened for, checked alone; `None` for all.
    focus_repo: Option<RepoId>,
    /// `None` while the core is asked.
    rows: Option<Vec<Row>>,
    error: Option<SharedString>,
    /// Reports that arrived before their job's id did.
    early: HashMap<JobId, CloneReport>,
    focus: FocusHandle,
    close: FocusHandle,
    _events: Task<()>,
}

impl EventEmitter<Closed> for MissingReposDialog {}

impl MissingReposDialog {
    pub fn new(
        services: Services,
        workspace: WorkspaceId,
        focus_repo: Option<RepoId>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let mut events = services.app().subscribe();
        let listening = cx.spawn(async move |this, cx| {
            loop {
                let event = match events.recv().await {
                    Ok(event) => event,
                    Err(RecvError::Lagged(_)) => continue,
                    Err(RecvError::Closed) => break,
                };
                if this
                    .update(cx, |this, cx| this.on_event(event, cx))
                    .is_err()
                {
                    break;
                }
            }
        });
        let asked = workspace.clone();
        let call = services.with_app(|app| async move { app.workspace_missing_repos(asked).await });
        cx.spawn_in(window, async move |this, cx| {
            let result = call.await;
            let _ = this.update_in(cx, |this, window, cx| match result {
                Some(Ok(missing)) => this.listed(missing, window, cx),
                Some(Err(error)) => {
                    this.error = Some(error.message.into());
                    cx.notify();
                }
                None => {
                    this.error = Some(clone_text::STOPPED.into());
                    cx.notify();
                }
            });
        })
        .detach();
        Self {
            services,
            workspace,
            focus_repo,
            rows: None,
            error: None,
            early: HashMap::new(),
            focus: cx.focus_handle(),
            close: cx.focus_handle(),
            _events: listening,
        }
    }

    /// The rows' repos and their states, in order.
    pub fn rows(&self) -> Vec<(RepoId, bool, RowState)> {
        self.rows
            .iter()
            .flatten()
            .map(|row| (row.repo.repo_id.clone(), row.checked, row.state))
            .collect()
    }

    fn listed(&mut self, missing: Vec<MissingRepo>, window: &mut Window, cx: &mut Context<Self>) {
        let rows = missing
            .into_iter()
            .enumerate()
            .map(|(at, repo)| {
                let name =
                    cx.new(|cx| InputState::new(window, cx).default_value(repo.folder.clone()));
                let name_events = cx.subscribe(&name, move |this, _, event, cx| {
                    if let InputEvent::Change = event {
                        this.check(at, cx);
                    }
                });
                Row {
                    checked: repo.remote.is_some()
                        && self
                            .focus_repo
                            .as_ref()
                            .is_none_or(|focus| *focus == repo.repo_id),
                    parent: repo.parent.clone(),
                    repo,
                    name,
                    verdict: None,
                    state: RowState::Idle,
                    job: None,
                    message: None,
                    _name_events: name_events,
                }
            })
            .collect::<Vec<_>>();
        let count = rows.len();
        self.rows = Some(rows);
        for at in 0..count {
            self.check(at, cx);
        }
        cx.notify();
    }

    fn row(&self, at: usize) -> Option<&Row> {
        self.rows.as_ref()?.get(at)
    }

    fn row_mut(&mut self, at: usize) -> Option<&mut Row> {
        self.rows.as_mut()?.get_mut(at)
    }

    /// Row `at`'s target as it is now.
    fn target(&self, at: usize, cx: &App) -> Option<(PathBuf, String)> {
        let row = self.row(at)?;
        Some((row.parent.clone(), row.name.read(cx).value().to_string()))
    }

    /// Asks the core about row `at`'s target; only the answer for the
    /// target still shown counts.
    fn check(&mut self, at: usize, cx: &mut Context<Self>) {
        cx.notify();
        let Some((parent, name)) = self.target(at, cx) else {
            return;
        };
        let (asked_parent, asked_name) = (parent.clone(), name.clone());
        let call = self.services.with_app(|app| async move {
            app.clone_target_check(asked_parent, asked_name, true).await
        });
        cx.spawn(async move |this, cx| {
            let error = call
                .await
                .unwrap_or_else(|| Some(clone_text::STOPPED.to_owned()));
            let _ = this.update(cx, |this, cx| {
                if this.target(at, cx) == Some((parent.clone(), name.clone()))
                    && let Some(row) = this.row_mut(at)
                {
                    row.verdict = Some(Verdict {
                        parent,
                        name,
                        error,
                    });
                    cx.notify();
                }
            });
        })
        .detach();
    }

    /// The core's verdict on row `at`'s target as shown, once it answered.
    fn verdict(&self, at: usize, cx: &App) -> Option<&Verdict> {
        let (parent, name) = self.target(at, cx)?;
        self.row(at)?
            .verdict
            .as_ref()
            .filter(|verdict| verdict.parent == parent && verdict.name == name)
    }

    /// Whether Clone selected clones row `at`.
    fn clonable(&self, at: usize, cx: &App) -> bool {
        self.row(at).is_some_and(|row| {
            row.checked
                && row.repo.remote.is_some()
                && !row.busy()
                && self
                    .verdict(at, cx)
                    .is_some_and(|verdict| verdict.error.is_none())
        })
    }

    fn on_event(&mut self, event: AppEvent, cx: &mut Context<Self>) {
        match event {
            AppEvent::CloneParentPicked {
                workspace_id,
                path,
                key: Some(key),
            } if workspace_id == self.workspace => {
                let at = self
                    .rows
                    .iter()
                    .flatten()
                    .position(|row| pick_key(&row.repo.repo_id) == key);
                if let Some(at) = at
                    && let Some(row) = self.row_mut(at)
                {
                    row.parent = path;
                    self.check(at, cx);
                }
            }
            AppEvent::CloneReport(report) => {
                let at = self
                    .rows
                    .iter()
                    .flatten()
                    .position(|row| row.job == Some(report.job_id));
                match at {
                    Some(at) => self.ended(at, &report, cx),
                    None if report.workspace_id == self.workspace => {
                        self.early.insert(report.job_id, report);
                    }
                    None => {}
                }
            }
            _ => {}
        }
    }

    /// Row `at`'s clone ended as `report` says.
    fn ended(&mut self, at: usize, report: &CloneReport, cx: &mut Context<Self>) {
        if let Some(row) = self.row_mut(at) {
            let cloned = report.repo.is_some();
            row.state = if cloned {
                RowState::Cloned
            } else {
                RowState::Failed
            };
            row.message = (!cloned).then(|| {
                report
                    .message
                    .clone()
                    .unwrap_or_else(|| text::CLONE_FAILED.to_owned())
            });
            row.checked = !cloned && row.checked;
            cx.notify();
        }
    }

    /// Starts one clone per checked row, all at once.
    fn clone_selected(&mut self, cx: &mut Context<Self>) {
        self.error = None;
        let count = self.rows.as_ref().map_or(0, Vec::len);
        for at in 0..count {
            if !self.clonable(at, cx) {
                continue;
            }
            let Some((parent, name)) = self.target(at, cx) else {
                continue;
            };
            let Some(row) = self.row_mut(at) else {
                continue;
            };
            let Some(url) = row.repo.remote.clone() else {
                continue;
            };
            row.state = RowState::Cloning;
            row.message = None;
            let request = CloneRequest {
                url,
                parent,
                name,
                repo: Some(row.repo.repo_id.clone()),
                group: None,
            };
            let workspace = self.workspace.clone();
            let call = self
                .services
                .with_app(|app| async move { app.clone_start(workspace, request).await });
            cx.spawn(async move |this, cx| {
                let result = call.await;
                let _ = this.update(cx, |this, cx| {
                    match result {
                        Some(Ok(job)) => {
                            if let Some(row) = this.row_mut(at) {
                                row.job = Some(job);
                            }
                            if let Some(report) = this.early.remove(&job) {
                                this.ended(at, &report, cx);
                            }
                        }
                        Some(Err(error)) => this.refused(at, error.message, cx),
                        None => this.refused(at, clone_text::STOPPED.to_owned(), cx),
                    }
                    cx.notify();
                });
            })
            .detach();
        }
        cx.notify();
    }

    fn refused(&mut self, at: usize, why: String, cx: &mut Context<Self>) {
        if let Some(row) = self.row_mut(at) {
            row.state = RowState::Failed;
            row.message = Some(why);
            cx.notify();
        }
    }

    /// Asks the core for the folder dialog for row `at`; the pick arrives
    /// as an event with the row's key.
    fn choose(&mut self, at: usize, cx: &mut Context<Self>) {
        let Some(row) = self.row(at) else {
            return;
        };
        let key = pick_key(&row.repo.repo_id);
        let workspace = self.workspace.clone();
        let call = self
            .services
            .with_app(|app| async move { app.clone_pick_parent(workspace, Some(key)) });
        cx.spawn(async move |this, cx| {
            if let Some(Err(error)) = call.await {
                let _ = this.update(cx, |this, cx| {
                    this.error = Some(error.message.into());
                    cx.notify();
                });
            }
        })
        .detach();
    }

    fn on_submit(&mut self, _: &ClickEvent, _: &mut Window, cx: &mut Context<Self>) {
        self.clone_selected(cx);
    }

    fn cancel(&mut self, _: &Cancel, _: &mut Window, cx: &mut Context<Self>) {
        cx.emit(Closed);
    }

    fn on_close(&mut self, _: &ClickEvent, _: &mut Window, cx: &mut Context<Self>) {
        cx.emit(Closed);
    }

    fn render_row(&self, at: usize, row: &Row, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let tokens = *cx.tokens();
        let id = |part: &'static str| ElementId::NamedInteger(part.into(), at as u64);
        let label = |part: &'static str, text: SharedString| {
            div()
                .id(id(part))
                .test_support()
                .role(Role::Label)
                .aria_label(text.clone())
                .min_w_0()
                .truncate()
                .child(text)
        };
        let name: SharedString = row.repo.name.clone().into();
        let remote: SharedString = row
            .repo
            .remote
            .clone()
            .map_or_else(|| text::NO_REMOTE.into(), Into::into);
        let parent: SharedString = row.parent.display().to_string().into();
        let error = (row.state == RowState::Idle)
            .then(|| {
                self.verdict(at, cx)
                    .and_then(|verdict| verdict.error.clone())
            })
            .flatten()
            .map(|error| {
                let error: SharedString = error.into();
                div()
                    .id(id("missing-error"))
                    .test_support()
                    .role(Role::Alert)
                    .aria_label(error.clone())
                    .text_color(tokens.danger)
                    .child(error)
            });
        let state = match row.state {
            RowState::Idle => None,
            RowState::Cloning => Some(text::CLONING.to_owned()),
            RowState::Cloned => Some(text::CLONED.to_owned()),
            RowState::Failed => Some(text::failed(
                row.message.as_deref().unwrap_or(text::CLONE_FAILED),
            )),
        }
        .map(|state| {
            let state: SharedString = state.into();
            div()
                .id(id("missing-state"))
                .test_support()
                .role(Role::Status)
                .aria_label(state.clone())
                .text_color(if row.state == RowState::Failed {
                    tokens.danger
                } else {
                    tokens.text_secondary
                })
                .child(state)
        });
        let busy = row.busy();
        div()
            .id(id("missing-row"))
            .test_support()
            .role(Role::ListItem)
            .aria_label(name.clone())
            .flex()
            .flex_col()
            .gap_1()
            .py_1()
            .border_b_1()
            .border_color(tokens.border)
            .child(
                Checkbox::new(id("missing-check"))
                    .label(name.clone())
                    .checked(row.checked)
                    .disabled(row.repo.remote.is_none() || busy)
                    .on_click(cx.listener(move |this, checked: &bool, _, cx| {
                        if let Some(row) = this.row_mut(at) {
                            row.checked = *checked;
                        }
                        cx.notify();
                    })),
            )
            .child(label("missing-remote", remote).text_color(tokens.text_secondary))
            .child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap_2()
                    .child(label("missing-parent", parent).flex_1())
                    .child(
                        Button::new(id("missing-choose"))
                            .label(clone_text::CHOOSE)
                            .accessibility_label(text::choose_folder(&row.repo.name))
                            .disabled(busy)
                            .on_click(cx.listener(move |this, _, _, cx| this.choose(at, cx))),
                    )
                    .child(
                        div().w(px(160.)).flex_shrink_0().child(
                            Input::new(&row.name)
                                .id(id("missing-folder"))
                                .aria_label(text::folder_name(&row.repo.name))
                                .disabled(busy),
                        ),
                    ),
            )
            .children(error)
            .children(state)
    }
}

impl Modal for MissingReposDialog {
    /// Close has the focus.
    fn focus_first(&self, window: &mut Window, cx: &mut App) {
        window.focus(&self.close, cx);
    }
}

impl Focusable for MissingReposDialog {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus.clone()
    }
}

impl Render for MissingReposDialog {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let title: SharedString = text::TITLE.into();
        let paragraph = |id: &'static str, text: &'static str| {
            div()
                .id(id)
                .test_support()
                .role(Role::Paragraph)
                .aria_label(text)
                .child(text)
        };
        let body = match &self.rows {
            None if self.error.is_none() => {
                Some(paragraph("missing-looking", text::LOOKING).into_any_element())
            }
            None => None,
            Some(rows) if rows.is_empty() => {
                Some(paragraph("missing-none", text::NONE).into_any_element())
            }
            Some(rows) => {
                let rows: Vec<_> = rows
                    .iter()
                    .enumerate()
                    .map(|(at, row)| self.render_row(at, row, cx).into_any_element())
                    .collect();
                Some(
                    div()
                        .flex()
                        .flex_col()
                        .gap_2()
                        .child(paragraph("missing-intro", text::INTRO))
                        .child(
                            div()
                                .id("missing-list")
                                .test_support()
                                .role(Role::List)
                                .aria_label(text::TITLE)
                                .flex()
                                .flex_col()
                                .max_h(px(420.))
                                .visible_y_scroll()
                                .children(rows),
                        )
                        .into_any_element(),
                )
            }
        };
        let said = match &self.rows {
            None if self.error.is_none() => text::LOOKING,
            None => "",
            Some(rows) if rows.is_empty() => text::NONE,
            Some(_) => text::INTRO,
        };
        let count = self.rows.as_ref().map_or(0, Vec::len);
        let any = (0..count).any(|at| self.clonable(at, cx));
        dialog::frame("missing-frame", Role::Dialog, &title, cx)
            .aria_description(said)
            .w(px(640.))
            .test_support()
            .track_focus(&self.focus)
            .key_context(dialog::CONTEXT)
            .on_action(cx.listener(Self::cancel))
            .children(body)
            .children(dialog::refusal(&self.error, cx))
            .child(
                dialog::actions_row()
                    .child(
                        Button::new("dialog-cancel")
                            .label(text::CLOSE)
                            .track_focus(&self.close)
                            .on_click(cx.listener(Self::on_close)),
                    )
                    .child(
                        Button::new("dialog-submit")
                            .primary()
                            .label(text::CLONE_SELECTED)
                            .disabled(!any)
                            .on_click(cx.listener(Self::on_submit)),
                    ),
            )
            // The trap takes the place of the frame, under the dialog's id.
            .focus_trap("missing-dialog", &self.focus)
    }
}
