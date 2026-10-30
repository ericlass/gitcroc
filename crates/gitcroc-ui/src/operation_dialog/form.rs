use super::*;
use crate::{
    dialog::{self, Cancel},
    strings,
};
use gpui_component::{
    Disableable, FocusTrapElement, Selectable,
    button::{Button, ButtonVariants},
    checkbox::Checkbox,
    input::{Input, Textarea},
};
use gpui_kit::{
    AnyElement, InteractiveElement, IntoElement, ParentElement, Render, Role,
    StatefulInteractiveElement, Styled, TestSupportExt, div, px,
};
impl OperationDialog {
    fn title(&self) -> String {
        if let Some((_, repo, _)) = &self.confirm {
            return text::title(text::CONFIRM, repo);
        }
        if self.remotes.is_some() {
            return text::title(text::REMOTE, &self.repo_name);
        }
        if self.parents.is_some() {
            return text::title(text::PARENT, &self.repo_name);
        }
        if let OperationMode::Reset { commit, .. } = &self.mode {
            return text::reset_title(
                self.head_name.as_deref().unwrap_or("HEAD"),
                commit,
                &self.repo_name,
            );
        }
        if matches!(self.mode, OperationMode::Merge) {
            return text::merge_title(self.head_name.as_deref().unwrap_or("HEAD"), &self.repo_name);
        }
        let action = match self.mode {
            OperationMode::Fetch => text::FETCH,
            OperationMode::Pull => text::PULL,
            OperationMode::Push => text::PUSH,
            OperationMode::Merge => text::MERGE,
            OperationMode::StashPush => text::STASH,
            OperationMode::TagCreate { .. } => text::CREATE_TAG,
            OperationMode::TagDelete { .. } => text::DELETE_TAG,
            OperationMode::RemoteAdd => text::ADD_REMOTE,
            OperationMode::RemoteRename { .. } => text::RENAME_REMOTE,
            OperationMode::RemoteUrl { .. } => text::EDIT_URL,
            _ => text::RUN,
        };
        let detail = match &self.mode {
            OperationMode::TagCreate {
                commit: Some(commit),
            } => format!("{} at {commit}", self.repo_name),
            OperationMode::TagDelete { name }
            | OperationMode::RemoteRename { name }
            | OperationMode::RemoteUrl { name } => format!("{} — {name}", self.repo_name),
            _ => self.repo_name.clone(),
        };
        text::title(action, &detail)
    }
    fn field(
        &self,
        id: &'static str,
        label: &'static str,
        input: &Entity<InputState>,
    ) -> AnyElement {
        div()
            .flex()
            .flex_col()
            .gap_1()
            .child(label)
            .child(Input::new(input).id(id).aria_label(label))
            .into_any_element()
    }
    fn flag(
        &self,
        id: &'static str,
        label: &'static str,
        secondary: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        Checkbox::new(id)
            .label(label)
            .checked(if secondary { self.secondary } else { self.flag })
            .on_click(cx.listener(move |this, value: &bool, _, cx| {
                if secondary {
                    this.secondary = *value;
                } else {
                    this.flag = *value;
                }
                cx.notify();
            }))
            .into_any_element()
    }
    fn choices(
        &self,
        labels: &[&'static str],
        disabled: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        div()
            .flex()
            .flex_col()
            .gap_1()
            .children(labels.iter().enumerate().map(|(at, label)| {
                Button::new(gpui_kit::ElementId::Name(format!("op-choice-{at}").into()))
                    .label(*label)
                    .selected(self.choice == at)
                    .disabled(disabled)
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.choice = at;
                        cx.notify();
                    }))
            }))
            .into_any_element()
    }
}
impl Render for OperationDialog {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let title: SharedString = self.title().into();
        let mut body = div()
            .id("op-body")
            .flex()
            .flex_col()
            .gap_3()
            .max_h(px(550.))
            .overflow_y_scroll();
        if let Some((_, _, actions)) = &self.confirm {
            let actions: SharedString = actions.join("\n").into();
            body = body.child(
                div()
                    .id("op-confirm-actions")
                    .test_support()
                    .role(Role::Paragraph)
                    .aria_label(actions.clone())
                    .child(actions),
            );
        } else if let Some((_, remotes)) = &self.remotes {
            body = body.child(text::REMOTE);
            for (at, remote) in remotes.iter().enumerate() {
                body = body.child(
                    Button::new(gpui_kit::ElementId::Name(format!("op-remote-{at}").into()))
                        .label(remote.clone())
                        .selected(self.picked == Some(at))
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.picked = Some(at);
                            cx.notify();
                        })),
                );
            }
        } else if let Some((_, parents)) = &self.parents {
            body = body.child(text::PARENT);
            for (at, parent) in parents.iter().enumerate() {
                body = body.child(
                    Button::new(gpui_kit::ElementId::Name(format!("op-parent-{at}").into()))
                        .label(text::parent(at + 1, parent))
                        .selected(self.choice == at)
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.choice = at;
                            cx.notify();
                        })),
                );
            }
        } else {
            if self.checks_name() {
                body = body.child(self.field(
                    "op-name",
                    if matches!(self.mode, OperationMode::RemoteRename { .. }) {
                        text::NEW_NAME
                    } else {
                        text::NAME
                    },
                    &self.name,
                ));
                if let Some(error) = &self.name_error {
                    body = body.child(
                        div()
                            .id("op-name-error")
                            .test_support()
                            .role(Role::Alert)
                            .aria_label(error.clone())
                            .child(error.clone()),
                    );
                }
            }
            match &self.mode {
                OperationMode::Fetch => {
                    body = body.child(self.choices(
                        &[text::CONFIG, text::PRUNE, text::NO_PRUNE],
                        false,
                        cx,
                    ))
                }
                OperationMode::Pull => {
                    body = body.child(self.choices(
                        &[
                            text::CONFIG,
                            text::FF_ONLY,
                            text::MERGE_PULL,
                            text::REBASE_PULL,
                        ],
                        false,
                        cx,
                    ))
                }
                OperationMode::Push => {
                    if let Some(choices) = &self.push_choices {
                        if let Some(upstream) = &choices.upstream {
                            body = body.child(text::pushes(
                                choices.branch.as_deref().unwrap_or("HEAD"),
                                upstream,
                            ));
                        } else {
                            body = body.child(text::no_upstream(
                                choices.branch.as_deref().unwrap_or("HEAD"),
                            ));
                            for (at, remote) in choices.remotes.iter().enumerate() {
                                body = body.child(
                                    Button::new(gpui_kit::ElementId::Name(
                                        format!("op-remote-{at}").into(),
                                    ))
                                    .label(remote.clone())
                                    .selected(
                                        self.picked == Some(at)
                                            || self.picked.is_none()
                                                && choices.remote.as_ref() == Some(remote),
                                    )
                                    .on_click(cx.listener(
                                        move |this, _, _, cx| {
                                            this.picked = Some(at);
                                            cx.notify();
                                        },
                                    )),
                                );
                            }
                            body =
                                body.child(self.flag("op-upstream", text::SET_UPSTREAM, false, cx));
                        }
                    } else if self.read_error.is_none() {
                        body = body.child(text::LOADING);
                    }
                    body = body.child(self.flag("op-force", text::FORCE, true, cx));
                }
                OperationMode::Merge => {
                    body = body.child(self.field("op-revision", text::REVISION, &self.revision));
                    let mut targets = div()
                        .id("op-targets")
                        .flex()
                        .flex_col()
                        .gap_1()
                        .max_h(px(150.))
                        .overflow_y_scroll();
                    for (at, target) in self.targets.iter().enumerate() {
                        let target = target.clone();
                        targets = targets.child(
                            Button::new(gpui_kit::ElementId::Name(
                                format!("op-target-{at}").into(),
                            ))
                            .label(target.clone())
                            .on_click(cx.listener(
                                move |this, _, window, cx| {
                                    this.revision.update(cx, |input, cx| {
                                        input.set_value(target.clone(), window, cx)
                                    });
                                    this.filter(cx);
                                },
                            )),
                        );
                    }
                    if self.targets.is_empty() && self.read_error.is_none() {
                        targets = targets.child(text::NO_REFS);
                    }
                    body = body
                        .child(targets)
                        .child(self.choices(
                            &[text::CONFIG, text::FF_ONLY, text::NO_FF],
                            self.flag,
                            cx,
                        ))
                        .child(self.flag("op-rebase", text::REBASE, false, cx));
                }
                OperationMode::StashPush => {
                    body = body
                        .child(
                            div()
                                .id("op-message")
                                .test_support()
                                .child(text::MESSAGE)
                                .child(Textarea::new(&self.message).aria_label(text::MESSAGE)),
                        )
                        .child(self.flag("op-untracked", text::UNTRACKED, false, cx));
                }
                OperationMode::TagCreate { .. } => {
                    body = body.child(self.flag("op-annotated", text::ANNOTATED, false, cx));
                    if self.flag {
                        body = body.child(
                            div()
                                .id("op-message")
                                .test_support()
                                .child(text::MESSAGE)
                                .child(Textarea::new(&self.message).aria_label(text::MESSAGE)),
                        );
                    }
                    body = body.child(self.flag("op-push", text::PUSH_TAG, true, cx));
                }
                OperationMode::TagDelete { .. } => {
                    body = body.child(self.flag("op-on-remote", text::ON_REMOTE, false, cx))
                }
                OperationMode::Reset { .. } => {
                    body =
                        body.child(self.choices(&[text::SOFT, text::MIXED, text::HARD], false, cx))
                }
                OperationMode::RemoteAdd => {
                    body = body
                        .child(self.field("op-url", text::URL, &self.url))
                        .child(self.flag("op-fetch", text::FETCH_AFTER, false, cx))
                }
                OperationMode::RemoteUrl { .. } => {
                    if self.urls_loaded {
                        body = body.child(self.field("op-url", text::URL, &self.url));
                        if self.has_push_url {
                            body = body.child(self.field(
                                "op-push-url",
                                text::PUSH_URL,
                                &self.push_url,
                            ));
                        }
                    } else if self.read_error.is_none() {
                        body = body.child(text::LOADING);
                    }
                }
                _ => {}
            }
            if let Some(error) = &self.read_error {
                body = body.child(
                    div()
                        .id("op-read-error")
                        .test_support()
                        .role(Role::Alert)
                        .aria_label(error.clone())
                        .child(error.clone()),
                );
            }
        }
        let danger = self.confirm.is_some()
            || matches!(self.mode, OperationMode::TagDelete { .. })
            || matches!(self.mode, OperationMode::Reset { .. }) && self.choice == 2;
        let submit = Button::new("dialog-submit")
            .label(if self.confirm.is_some() {
                text::CONFIRM
            } else {
                match self.mode {
                    OperationMode::Fetch => text::FETCH,
                    OperationMode::Pull => text::PULL,
                    OperationMode::Push => text::PUSH,
                    OperationMode::Merge if self.flag => text::REBASE_PULL,
                    OperationMode::Merge => text::MERGE,
                    OperationMode::StashPush => text::STASH,
                    OperationMode::TagCreate { .. } => text::CREATE_TAG,
                    OperationMode::TagDelete { .. } => text::DELETE_TAG,
                    OperationMode::Reset { .. } => text::RESET,
                    OperationMode::RemoteAdd => text::ADD_REMOTE,
                    OperationMode::RemoteRename { .. } => text::RENAME_REMOTE,
                    OperationMode::RemoteUrl { .. } => text::EDIT_URL,
                    _ => text::RUN,
                }
            })
            .disabled(self.operation(cx).is_none())
            .on_click(cx.listener(|this, _, window, cx| this.submit(window, cx)));
        dialog::frame(
            "operation-frame",
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
        .on_action(cx.listener(|_, _: &Cancel, _, cx| cx.emit(Closed)))
        .child(body)
        .children(dialog::refusal(&self.error, cx))
        .child(
            dialog::actions_row()
                .child(
                    Button::new("dialog-cancel")
                        .label(strings::common::CANCEL)
                        .track_focus(&self.cancel_focus)
                        .on_click(cx.listener(|_, _, _, cx| cx.emit(Closed))),
                )
                .child(if danger {
                    submit.danger()
                } else {
                    submit.primary()
                }),
        )
        .focus_trap("operation-dialog", &self.focus)
    }
}
