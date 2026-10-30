//! Branch dialogs ported from rc.1's tree/BranchDialogs.tsx and
//! stores/branch.ts. The core owns validation, preflight and confirmation.
use crate::{
    dialog::{self, Cancel, Closed, Modal},
    services::Services,
    strings::{self, branches as text},
};
use gitcroc_core::{
    BranchChoices, BranchOp, BranchRequest, BranchStart, RepoId, StartPoint, WorkspaceId,
};
use gpui_component::button::{Button, ButtonVariants};
use gpui_component::checkbox::Checkbox;
use gpui_component::input::{Input, InputEvent, InputState};
use gpui_component::{Disableable, FocusTrapElement, Selectable};
use gpui_kit::{
    App, AppContext, ClickEvent, Context, Entity, EventEmitter, FocusHandle, Focusable,
    InteractiveElement, IntoElement, ParentElement, Render, Role, SharedString,
    StatefulInteractiveElement, Styled, Subscription, TestSupportExt, Window, div, px,
};

#[derive(Debug, Clone)]
pub enum BranchMode {
    Create {
        start: StartPoint,
    },
    Rename {
        name: String,
    },
    Delete {
        name: String,
        tracked: bool,
    },
    Upstream {
        name: String,
        upstream: Option<String>,
    },
    Run {
        op: BranchOp,
    },
}

/// Emitted by either pane, consumed by the window's open_dialog path.
#[derive(Clone)]
pub struct AskBranch {
    pub workspace: WorkspaceId,
    pub repo: RepoId,
    pub repo_name: String,
    pub mode: BranchMode,
    /// Direct operations run as soon as the modal exists.
    pub immediate: bool,
}

pub struct BranchDialog {
    services: Services,
    workspace: WorkspaceId,
    repo: RepoId,
    repo_name: String,
    mode: BranchMode,
    name: Entity<InputState>,
    revision: Entity<InputState>,
    start: StartPoint,
    switch: bool,
    force: bool,
    on_remote: bool,
    picked: Option<usize>,
    choices: Option<BranchChoices>,
    choices_error: Option<SharedString>,
    name_error: Option<SharedString>,
    validating: bool,
    validation: u64,
    error: Option<SharedString>,
    sending: bool,
    confirm: Option<(BranchOp, String, Vec<String>)>,
    focus: FocusHandle,
    cancel_focus: FocusHandle,
    _name_events: Subscription,
    _revision_events: Subscription,
}
impl EventEmitter<Closed> for BranchDialog {}
impl BranchDialog {
    pub fn new(
        services: Services,
        workspace: WorkspaceId,
        repo: RepoId,
        repo_name: String,
        mode: BranchMode,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let initial = match &mode {
            BranchMode::Rename { name } => name.as_str(),
            _ => "",
        };
        let start = match &mode {
            BranchMode::Create { start } => start.clone(),
            _ => StartPoint::Head,
        };
        let rev = match &start {
            StartPoint::Revision { revision } => revision.clone(),
            _ => String::new(),
        };
        let name = cx.new(|cx| InputState::new(window, cx).default_value(initial.to_owned()));
        let revision = cx.new(|cx| InputState::new(window, cx).default_value(rev));
        let name_events =
            cx.subscribe_in(&name, window, |this, _, event, window, cx| match event {
                InputEvent::Change => this.check_name(cx),
                InputEvent::PressEnter { .. } if this.confirm.is_none() => {
                    this.submit(window, cx);
                }
                _ => {}
            });
        let revision_events = cx.subscribe(&revision, |this, _, event, cx| {
            if let InputEvent::Change = event {
                this.start = StartPoint::Revision {
                    revision: this.revision.read(cx).value().to_string(),
                };
                cx.notify();
            }
        });
        let mut this = Self {
            services,
            workspace,
            repo,
            repo_name,
            mode,
            name,
            revision,
            start,
            switch: true,
            force: false,
            on_remote: false,
            picked: None,
            choices: None,
            choices_error: None,
            name_error: None,
            validating: false,
            validation: 0,
            error: None,
            sending: false,
            confirm: None,
            focus: cx.focus_handle(),
            cancel_focus: cx.focus_handle(),
            _name_events: name_events,
            _revision_events: revision_events,
        };
        this.check_name(cx);
        let workspace = this.workspace.clone();
        let repo = this.repo.clone();
        let call = this
            .services
            .with_app(move |app| async move { app.branch_choices(workspace, repo).await });
        cx.spawn(async move |this, cx| {
            let result = call.await;
            let _ = this.update(cx, |this, cx| {
                match result {
                    Some(Ok(choices)) => {
                        if let BranchMode::Upstream {
                            upstream: Some(upstream),
                            ..
                        } = &this.mode
                        {
                            this.picked = choices
                                .remote_branches
                                .iter()
                                .position(|b| format!("{}/{}", b.remote, b.branch) == *upstream);
                        }
                        this.choices = Some(choices);
                    }
                    Some(Err(error)) => {
                        this.choices_error = Some(text::cannot_read(&error.to_string()).into())
                    }
                    None => this.choices_error = Some(text::STOPPED.into()),
                };
                cx.notify();
            });
        })
        .detach();
        this
    }
    fn check_name(&mut self, cx: &mut Context<Self>) {
        if !matches!(
            self.mode,
            BranchMode::Create { .. } | BranchMode::Rename { .. }
        ) {
            return;
        }
        self.validation += 1;
        let generation = self.validation;
        self.validating = true;
        let value = self.name.read(cx).value().to_string();
        let call = self
            .services
            .with_app(move |app| async move { app.branch_name_check(&value) });
        cx.spawn(async move |this, cx| {
            let result = call.await;
            let _ = this.update(cx, |this, cx| {
                if this.validation == generation {
                    this.name_error = match result {
                        Some(error) => error.map(Into::into),
                        None => Some(text::STOPPED.into()),
                    };
                    this.validating = false;
                    cx.notify();
                }
            });
        })
        .detach();
        cx.notify();
    }
    fn operation(&self, cx: &App) -> Option<BranchOp> {
        if self.sending {
            return None;
        }
        if let Some((op, _, _)) = &self.confirm {
            return Some(op.clone());
        }
        let name = self.name.read(cx).value().to_string();
        if matches!(
            self.mode,
            BranchMode::Create { .. } | BranchMode::Rename { .. }
        ) && (self.validating || self.name_error.is_some() || name.is_empty())
        {
            return None;
        }
        Some(match &self.mode {
            BranchMode::Create { .. } => {
                let start = match &self.start {
                    StartPoint::Revision { .. } => {
                        let revision = self.revision.read(cx).value().trim().to_owned();
                        if revision.is_empty() {
                            return None;
                        }
                        StartPoint::Revision { revision }
                    }
                    StartPoint::DefaultBranch => {
                        self.choices.as_ref()?.default_branch.as_ref()?;
                        StartPoint::DefaultBranch
                    }
                    StartPoint::Head => StartPoint::Head,
                };
                BranchOp::Create {
                    name,
                    start,
                    switch: self.switch,
                }
            }
            BranchMode::Rename { name: old } => {
                if *old == name {
                    return None;
                }
                BranchOp::Rename {
                    name: old.clone(),
                    new_name: name,
                }
            }
            BranchMode::Delete { name, .. } => BranchOp::Delete {
                name: name.clone(),
                force: self.force,
                on_remote: self.on_remote,
            },
            BranchMode::Upstream { name, .. } => BranchOp::SetUpstream {
                name: name.clone(),
                upstream: Some(
                    self.choices
                        .as_ref()?
                        .remote_branches
                        .get(self.picked?)?
                        .clone(),
                ),
            },
            BranchMode::Run { op } => op.clone(),
        })
    }
    pub fn submit(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(op) = self.operation(cx) else {
            return;
        };
        let confirmed = self.confirm.is_some();
        self.sending = true;
        self.error = None;
        let request = BranchRequest {
            repo_id: self.repo.clone(),
            op: op.clone(),
            confirmed,
        };
        let workspace = self.workspace.clone();
        let call = self
            .services
            .with_app(move |app| async move { app.op_branch(workspace, request).await });
        cx.spawn_in(window, async move |this, cx| {
            let result = call.await;
            let _ = this.update_in(cx, |this, window, cx| {
                this.sending = false;
                match result {
                    Some(Ok(BranchStart::Started { .. })) => cx.emit(Closed),
                    Some(Ok(BranchStart::Confirm { repo, actions })) => {
                        this.confirm = Some((op, repo, actions));
                        window.focus(&this.cancel_focus, cx);
                    }
                    Some(Err(error)) => this.error = Some(error.to_string().into()),
                    None => this.error = Some(text::STOPPED.into()),
                }
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }
    fn on_submit(&mut self, _: &ClickEvent, window: &mut Window, cx: &mut Context<Self>) {
        self.submit(window, cx);
    }
    fn cancel(&mut self, _: &Cancel, _: &mut Window, cx: &mut Context<Self>) {
        cx.emit(Closed);
    }
    fn on_cancel(&mut self, _: &ClickEvent, _: &mut Window, cx: &mut Context<Self>) {
        cx.emit(Closed);
    }
    fn title(&self) -> String {
        if let Some((_, repo, _)) = &self.confirm {
            return text::confirm(repo);
        }
        match &self.mode {
            BranchMode::Create { .. } => text::create_dialog(&self.repo_name),
            BranchMode::Rename { name } => text::rename_dialog(name, &self.repo_name),
            BranchMode::Delete { name, .. } => text::delete_dialog(name, &self.repo_name),
            BranchMode::Upstream { name, .. } => text::upstream_dialog(name, &self.repo_name),
            BranchMode::Run { .. } => text::confirm(&self.repo_name),
        }
    }
}
impl Modal for BranchDialog {
    fn focus_first(&self, window: &mut Window, cx: &mut App) {
        if matches!(
            self.mode,
            BranchMode::Create { .. } | BranchMode::Rename { .. }
        ) {
            self.name.update(cx, |input, cx| input.focus(window, cx));
        } else {
            window.focus(&self.cancel_focus, cx);
        }
    }
}
impl Focusable for BranchDialog {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus.clone()
    }
}
impl Render for BranchDialog {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let title: SharedString = self.title().into();
        let mut frame = dialog::frame(
            "branch-frame",
            if self.confirm.is_some() {
                Role::AlertDialog
            } else {
                Role::Dialog
            },
            &title,
            cx,
        )
        .test_support()
        .track_focus(&self.focus)
        .key_context(dialog::CONTEXT)
        .on_action(cx.listener(Self::cancel));
        if let Some((_, _, actions)) = &self.confirm {
            let actions: SharedString = actions.join("\n").into();
            frame = frame.child(
                div()
                    .id("branch-confirm-actions")
                    .test_support()
                    .role(Role::Paragraph)
                    .aria_label(actions.clone())
                    .child(actions),
            );
        } else {
            if matches!(
                self.mode,
                BranchMode::Create { .. } | BranchMode::Rename { .. }
            ) {
                let label = if matches!(self.mode, BranchMode::Rename { .. }) {
                    text::NEW_NAME
                } else {
                    text::NAME
                };
                frame = frame.child(
                    div()
                        .child(label)
                        .child(Input::new(&self.name).id("branch-name").aria_label(label)),
                );
                if let Some(error) = &self.name_error {
                    frame = frame.child(
                        div()
                            .id("branch-name-error")
                            .test_support()
                            .role(Role::Alert)
                            .aria_label(error.clone())
                            .child(error.clone()),
                    );
                }
            }
            match &self.mode {
                BranchMode::Create { .. } => {
                    frame = frame
                        .child(
                            div()
                                .child(text::START_POINT)
                                .child(
                                    Button::new("branch-start-head")
                                        .label(text::CURRENT_HEAD)
                                        .selected(matches!(self.start, StartPoint::Head))
                                        .on_click(cx.listener(|this, _, _, cx| {
                                            this.start = StartPoint::Head;
                                            cx.notify();
                                        })),
                                )
                                .child(
                                    Button::new("branch-start-default")
                                        .label(text::DEFAULT_BRANCH)
                                        .disabled(
                                            self.choices
                                                .as_ref()
                                                .and_then(|c| c.default_branch.as_ref())
                                                .is_none(),
                                        )
                                        .selected(matches!(self.start, StartPoint::DefaultBranch))
                                        .on_click(cx.listener(|this, _, _, cx| {
                                            this.start = StartPoint::DefaultBranch;
                                            cx.notify();
                                        })),
                                )
                                .child(
                                    Button::new("branch-start-revision")
                                        .label(text::REVISION)
                                        .selected(matches!(self.start, StartPoint::Revision { .. }))
                                        .on_click(cx.listener(|this, _, window, cx| {
                                            this.start = StartPoint::Revision {
                                                revision: this
                                                    .revision
                                                    .read(cx)
                                                    .value()
                                                    .to_string(),
                                            };
                                            this.revision
                                                .update(cx, |input, cx| input.focus(window, cx));
                                            cx.notify();
                                        })),
                                )
                                .child(
                                    Input::new(&self.revision)
                                        .id("branch-revision")
                                        .aria_label(text::REVISION_LABEL),
                                ),
                        )
                        .child(
                            Checkbox::new("branch-switch")
                                .label(text::CHECK_OUT_NEW)
                                .checked(self.switch)
                                .on_click(cx.listener(|this, value: &bool, _, cx| {
                                    this.switch = *value;
                                    cx.notify();
                                })),
                        );
                }
                BranchMode::Delete { tracked, .. } => {
                    frame = frame
                        .child(
                            Checkbox::new("branch-force")
                                .label(text::FORCE)
                                .checked(self.force)
                                .on_click(cx.listener(|this, value: &bool, _, cx| {
                                    this.force = *value;
                                    cx.notify();
                                })),
                        )
                        .child(
                            Checkbox::new("branch-on-remote")
                                .label(text::ON_REMOTE)
                                .checked(self.on_remote)
                                .disabled(!tracked)
                                .on_click(cx.listener(|this, value: &bool, _, cx| {
                                    this.on_remote = *value;
                                    cx.notify();
                                })),
                        );
                }
                BranchMode::Upstream { .. } => {
                    frame = frame.child(text::REMOTE_BRANCH);
                    if let Some(choices) = &self.choices {
                        if choices.remote_branches.is_empty() {
                            frame = frame.child(text::NO_REMOTE_BRANCHES);
                        }
                        let mut list = div()
                            .id("branch-upstreams")
                            .flex()
                            .flex_col()
                            .gap_1()
                            .max_h(px(240.))
                            .overflow_y_scroll();
                        for (at, b) in choices.remote_branches.iter().enumerate() {
                            list = list.child(
                                Button::new(gpui_kit::ElementId::Name(
                                    format!("branch-upstream-{at}").into(),
                                ))
                                .label(format!("{}/{}", b.remote, b.branch))
                                .selected(self.picked == Some(at))
                                .on_click(cx.listener(
                                    move |this, _, _, cx| {
                                        this.picked = Some(at);
                                        cx.notify();
                                    },
                                )),
                            );
                        }
                        frame = frame.child(list);
                    } else if self.choices_error.is_none() {
                        frame = frame.child(text::LOADING);
                    }
                }
                _ => {}
            }
            if let Some(error) = &self.choices_error {
                frame = frame.child(
                    div()
                        .id("branch-choices-error")
                        .test_support()
                        .role(Role::Alert)
                        .aria_label(error.clone())
                        .child(error.clone()),
                );
            }
        }
        let submit = if self.confirm.is_some() {
            text::DELETE
        } else {
            match self.mode {
                BranchMode::Create { .. } => text::CREATE,
                BranchMode::Rename { .. } => text::RENAME,
                BranchMode::Delete { .. } => text::DELETE,
                BranchMode::Upstream { .. } => text::SET_UPSTREAM,
                BranchMode::Run { .. } => text::RUN,
            }
        };
        let submit_button = Button::new("dialog-submit")
            .label(submit)
            .disabled(self.operation(cx).is_none())
            .on_click(cx.listener(Self::on_submit));
        let submit_button =
            if self.confirm.is_some() || matches!(self.mode, BranchMode::Delete { .. }) {
                submit_button.danger()
            } else {
                submit_button.primary()
            };
        frame
            .children(dialog::refusal(&self.error, cx))
            .child(
                dialog::actions_row()
                    .child(
                        Button::new("dialog-cancel")
                            .label(strings::common::CANCEL)
                            .track_focus(&self.cancel_focus)
                            .on_click(cx.listener(Self::on_cancel)),
                    )
                    .child(submit_button),
            )
            .focus_trap("branch-dialog", &self.focus)
    }
}
