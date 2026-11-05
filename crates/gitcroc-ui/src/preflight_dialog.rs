//! The preflight dialog of section 5, step 2, a port of rc.1's
//! `ui/src/groups/PreflightDialog.tsx`: a progress line while the core
//! classes the repos, then one row per repo with its class and reason,
//! *needs attention* and *blocked* first, and the counts. A destructive
//! operation names what cannot be undone and every repo it runs in (GRP-9).
//! Repos cannot be left out here: the target set is built only in the tree
//! (GRP-5). Escape cancels; the focus starts on Cancel while the core works
//! and where something cannot be undone, on Run elsewhere.

use crate::scrollbars::ScrollAreaExt as _;
use std::time::Duration;

use gitcroc_core::{Class, PreflightId, PreflightReady, RepoClass};
use gpui_component::Disableable;
use gpui_component::FocusTrapElement;
use gpui_component::button::{Button, ButtonVariants};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::{
    Animation, AnimationExt, App, Context, Entity, EventEmitter, FocusHandle, Focusable,
    InteractiveElement, IntoElement, ParentElement, Render, Role, SharedString,
    StatefulInteractiveElement, Styled, Subscription, TestSupportExt, Transformation, Window, div,
    percentage, px, svg,
};

use crate::dialog::{self, Cancel, Closed, Modal};
use crate::group_ops::{GroupOps, PendingPreflight};
use crate::icons;
use crate::strings::group_ops as text;
use crate::theme::ActiveTokens;

/// Rows first by how much they need a look; within a class, target-set
/// order.
fn order(class: &Class) -> u8 {
    match class {
        Class::NeedsAttention { .. } => 0,
        Class::Blocked { .. } => 1,
        Class::WillSkip { .. } => 2,
        Class::WillRun => 3,
    }
}

/// Whether the repo runs: *will run* and *needs attention* do.
fn runs(repo: &RepoClass) -> bool {
    matches!(repo.class, Class::WillRun | Class::NeedsAttention { .. })
}

/// Such as `api: Will skip: no remote`.
/// What a row says after its class: the reason, and for a commit how many
/// files the repo commits (GRP-15).
fn row_detail(repo: &RepoClass) -> String {
    match (&repo.class, repo.staged_files) {
        (Class::WillRun, Some(files)) => text::preflight::staged_files(files),
        (class, Some(files)) => {
            let reason = text::reason(class).unwrap_or_default();
            format!("{reason}; {}", text::preflight::staged_files(files))
        }
        (class, None) => text::reason(class).unwrap_or_default().to_owned(),
    }
}

fn row_label(repo: &RepoClass) -> String {
    let class = text::class(&repo.class);
    match row_detail(repo) {
        detail if detail.is_empty() => format!("{}: {class}", repo.repo_name),
        detail => format!("{}: {class}: {detail}", repo.repo_name),
    }
}

pub struct PreflightDialog {
    group_ops: Entity<GroupOps>,
    focus: FocusHandle,
    cancel_focus: FocusHandle,
    run_focus: FocusHandle,
    /// The preflight state the focus was last placed for.
    focused: Option<(PreflightId, bool)>,
    _changed: Subscription,
}

impl EventEmitter<Closed> for PreflightDialog {}

impl PreflightDialog {
    pub fn new(group_ops: Entity<GroupOps>, cx: &mut Context<Self>) -> Self {
        let changed = cx.observe(&group_ops, |_, _, cx| cx.notify());
        Self {
            group_ops,
            focus: cx.focus_handle(),
            cancel_focus: cx.focus_handle(),
            run_focus: cx.focus_handle(),
            focused: None,
            _changed: changed,
        }
    }

    fn pending(&self, cx: &App) -> Option<PendingPreflight> {
        self.group_ops.read(cx).pending().cloned()
    }

    fn rows(ready: &PreflightReady) -> Vec<RepoClass> {
        let mut rows = ready.repos.clone();
        // Stable, so repos of a class keep target-set order.
        rows.sort_by_key(|repo| order(&repo.class));
        rows
    }

    fn running(ready: &PreflightReady) -> usize {
        ready.repos.iter().filter(|repo| runs(repo)).count()
    }

    fn destructive(ready: &PreflightReady) -> bool {
        !ready.destructive.is_empty()
    }

    /// The run button's text.
    fn submit(ready: &PreflightReady) -> String {
        let count = u32::try_from(Self::running(ready)).unwrap_or(u32::MAX);
        if Self::destructive(ready) {
            text::preflight::action_in(&ready.action, count)
        } else {
            text::preflight::run_on(count)
        }
    }

    /// Puts the focus where the dialog starts: Cancel while the core works
    /// and where something cannot be undone, Run elsewhere.
    fn place_focus(&self, window: &mut Window, cx: &mut App) {
        let pending = self.pending(cx);
        let target = pending.as_ref().and_then(|pending| {
            pending.ready.as_ref().map(|ready| {
                let run =
                    !Self::destructive(ready) && !pending.starting && Self::running(ready) > 0;
                if run {
                    &self.run_focus
                } else {
                    &self.cancel_focus
                }
            })
        });
        window.focus(target.unwrap_or(&self.cancel_focus), cx);
    }

    /// Follows the preflight: the focus moves to Run when it becomes ready,
    /// and back to Cancel for a destructive one.
    pub fn reflect_focus(&mut self, window: &mut Window, cx: &mut App) {
        let pending = self.pending(cx);
        let key = pending.as_ref().and_then(|pending| {
            pending
                .ready
                .as_ref()
                .map(|ready| (ready.preflight_id, true))
                .or(pending.preflight_id.map(|id| (id, false)))
        });
        if key != self.focused {
            self.focused = key;
            self.place_focus(window, cx);
        }
    }

    fn confirm(&mut self, _: &gpui_kit::ClickEvent, _: &mut Window, cx: &mut Context<Self>) {
        self.group_ops.update(cx, |ops, cx| ops.confirm(cx));
    }

    fn cancel(&mut self, _: &gpui_kit::ClickEvent, _: &mut Window, cx: &mut Context<Self>) {
        self.group_ops.update(cx, |ops, cx| ops.cancel(cx));
    }

    fn on_cancel(&mut self, _: &Cancel, _: &mut Window, cx: &mut Context<Self>) {
        self.group_ops.update(cx, |ops, cx| ops.cancel(cx));
    }
}

impl Modal for PreflightDialog {
    /// Cancel has the focus while the core works.
    fn focus_first(&self, window: &mut Window, cx: &mut App) {
        self.place_focus(window, cx);
    }
}

impl Focusable for PreflightDialog {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus.clone()
    }
}

fn spinner(cx: &App) -> impl IntoElement {
    let color = cx.tokens().text_secondary;
    svg()
        .path(icons::SPINNER)
        .size(px(12.))
        .text_color(color)
        .with_animation(
            "preflight-spinner",
            Animation::new(Duration::from_secs(1)).repeat(),
            |spinner, delta| spinner.with_transformation(Transformation::rotate(percentage(delta))),
        )
}

impl Render for PreflightDialog {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let tokens = *cx.tokens();
        let pending = self.pending(cx);
        let ready = pending.as_ref().and_then(|pending| pending.ready.clone());
        let destructive = ready.as_ref().is_some_and(Self::destructive);
        let title: SharedString = ready
            .as_ref()
            .map_or_else(|| text::preflight::TITLE, |ready| ready.label.as_str())
            .into();
        let mut body = div().id("preflight-body").flex().flex_col().gap_2();
        // What a screen reader reads as the dialog opens (M6-25a).
        let mut said = SharedString::default();
        match &ready {
            None => {
                if pending
                    .as_ref()
                    .is_some_and(|pending| pending.error.is_none())
                {
                    said = text::preflight::CHECKING.into();
                    body = body.child(
                        div()
                            .id("preflight-progress")
                            .test_support()
                            .role(Role::Status)
                            .aria_label(text::preflight::CHECKING)
                            .flex()
                            .flex_row()
                            .items_center()
                            .gap_2()
                            .child(spinner(cx))
                            .child(text::preflight::CHECKING),
                    );
                }
            }
            Some(ready) => {
                let rows = Self::rows(ready);
                let counts = [
                    Class::WillRun,
                    Class::NeedsAttention {
                        reason: String::new(),
                    },
                    Class::WillSkip {
                        reason: String::new(),
                    },
                    Class::Blocked {
                        reason: String::new(),
                    },
                ]
                .into_iter()
                .filter_map(|class| {
                    let n = rows
                        .iter()
                        .filter(|repo| order(&repo.class) == order(&class))
                        .count();
                    (n > 0).then(|| format!("{n} {}", text::class(&class).to_lowercase()))
                })
                .collect::<Vec<_>>()
                .join(", ");
                said = counts.clone().into();
                body = body.child(
                    div()
                        .id("preflight-counts")
                        .test_support()
                        .role(Role::Label)
                        .aria_label(counts.clone())
                        .text_color(tokens.text_secondary)
                        .child(counts),
                );
                if destructive {
                    let actions = ready
                        .destructive
                        .iter()
                        .enumerate()
                        .map(|(at, action)| {
                            div()
                                .id(gpui_kit::ElementId::Name(
                                    format!("preflight-action-{at}").into(),
                                ))
                                .test_support()
                                .role(Role::ListItem)
                                .aria_label(action.clone())
                                .child(action.clone())
                        })
                        .collect::<Vec<_>>();
                    let affected = ready
                        .repos
                        .iter()
                        .filter(|repo| runs(repo))
                        .enumerate()
                        .map(|(at, repo)| {
                            div()
                                .id(gpui_kit::ElementId::Name(
                                    format!("preflight-affected-{at}").into(),
                                ))
                                .test_support()
                                .role(Role::ListItem)
                                .aria_label(repo.repo_name.clone())
                                .child(repo.repo_name.clone())
                        })
                        .collect::<Vec<_>>();
                    body = body.child(
                        div()
                            .id("preflight-destructive")
                            .test_support()
                            .role(Role::Alert)
                            .aria_label(text::preflight::CANNOT_BE_UNDONE_IN)
                            .flex()
                            .flex_col()
                            .gap_1()
                            .child(text::preflight::CANNOT_BE_UNDONE_IN.to_owned())
                            .child(
                                div()
                                    .id("preflight-actions")
                                    .test_support()
                                    .role(Role::List)
                                    .flex()
                                    .flex_col()
                                    .children(actions),
                            )
                            .child(text::preflight::IN_THESE_REPOS.to_owned())
                            .child(
                                div()
                                    .id("preflight-affected")
                                    .test_support()
                                    .role(Role::List)
                                    .flex()
                                    .flex_col()
                                    .children(affected),
                            ),
                    );
                }
                let row_elements = rows
                    .iter()
                    .enumerate()
                    .map(|(at, repo)| {
                        let detail = row_detail(repo);
                        div()
                            .id(gpui_kit::ElementId::Name(
                                format!("preflight-repo-{at}").into(),
                            ))
                            .test_support()
                            .role(Role::ListItem)
                            .aria_label(row_label(repo))
                            .flex()
                            .flex_row()
                            .gap_2()
                            .child(
                                div()
                                    .id(gpui_kit::ElementId::Name(
                                        format!("preflight-name-{at}").into(),
                                    ))
                                    .text_color(tokens.text_primary)
                                    .child(repo.repo_name.clone()),
                            )
                            .child(
                                div()
                                    .id(gpui_kit::ElementId::Name(
                                        format!("preflight-class-{at}").into(),
                                    ))
                                    .text_color(tokens.text_secondary)
                                    .child(text::class(&repo.class)),
                            )
                            .child(
                                div()
                                    .id(gpui_kit::ElementId::Name(
                                        format!("preflight-detail-{at}").into(),
                                    ))
                                    .text_color(tokens.text_secondary)
                                    .child(detail),
                            )
                    })
                    .collect::<Vec<_>>();
                body = body
                    .child(
                        div()
                            .id("preflight-repos")
                            .test_support()
                            .role(Role::List)
                            .aria_label(text::preflight::REPOS)
                            .flex()
                            .flex_col()
                            .max_h(px(240.))
                            .visible_y_scroll()
                            .children(row_elements),
                    )
                    .child(
                        div()
                            .id("preflight-hint")
                            .test_support()
                            .role(Role::Label)
                            .aria_label(text::preflight::LEAVE_OUT)
                            .text_color(tokens.text_secondary)
                            .child(text::preflight::LEAVE_OUT),
                    );
            }
        }
        let submit = ready.as_ref().map(Self::submit);
        let starting = pending.as_ref().is_some_and(|pending| pending.starting);
        let run_enabled = ready.as_ref().is_some_and(|ready| Self::running(ready) > 0) && !starting;
        let cancel = Button::new("preflight-cancel")
            .label(crate::strings::common::CANCEL)
            .track_focus(&self.cancel_focus)
            .on_click(cx.listener(Self::cancel));
        let run = Button::new("preflight-run")
            .label(submit.clone().unwrap_or_default())
            .track_focus(&self.run_focus)
            .disabled(!run_enabled)
            .when(destructive, |button| button.danger())
            .when(!destructive, |button| button.primary())
            .on_click(cx.listener(Self::confirm));
        let error = pending.as_ref().and_then(|pending| pending.error.clone());
        dialog::frame(
            "preflight-frame",
            if destructive {
                Role::AlertDialog
            } else {
                Role::Dialog
            },
            &title,
            cx,
        )
        .aria_description(said)
        .test_support()
        .track_focus(&self.focus)
        .key_context(dialog::CONTEXT)
        .on_action(cx.listener(Self::on_cancel))
        .child(body)
        .children(dialog::refusal(&error, cx))
        .child(
            dialog::actions_row()
                .child(cancel)
                .children(submit.map(|_| run)),
        )
        // The trap takes the place of the frame, under the dialog's id.
        .focus_trap("preflight", &self.focus)
    }
}
