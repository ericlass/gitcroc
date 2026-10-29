//! The status bar of UI-21, a port of `ui/src/layout/StatusBar.tsx`: the
//! active tab's running jobs with their progress; with none running, the
//! outcome of the job that ended last for a while, then a failed or
//! conflicted job until the job list has shown it. It is a button: a click,
//! or Enter or Space while it has the focus, opens the job list. Its text
//! is no live region, so progress is not read out on every step. At its
//! right, while a newer version is on offer, the update notice opens the
//! update dialog (PLAT-4).

use std::rc::Rc;

use gitcroc_core::{JobView, Outcome};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::{
    App, Context, Entity, EventEmitter, FocusHandle, Focusable, InteractiveElement, IntoElement,
    KeyBinding, KeyBindingContextPredicate, ParentElement, Render, Role, SharedString,
    StatefulInteractiveElement, Styled, Subscription, TestSupportExt, Window, actions, div, px,
};

use crate::jobs::{self, Jobs, Status};
use crate::strings::{jobs as job_text, status_bar as text, update as update_text};
use crate::theme::ActiveTokens;
use crate::theme::tokens::{Color, Tokens};
use crate::updates::Updates;

actions!(
    status_bar,
    [
        /// Opens the job list from the focused status bar.
        OpenJobs,
        /// Opens the update dialog from the focused notice.
        OpenUpdate,
    ]
);

/// The key context of the focused status bar.
const CONTEXT: &str = "GitcrocStatusBar";
/// The key context of the focused update notice.
const NOTICE_CONTEXT: &str = "GitcrocUpdateNotice";

pub fn bind(cx: &mut App) {
    let bind = |context: &str, action: &dyn gpui_kit::Action, cx: &mut App| {
        let context = KeyBindingContextPredicate::parse(context).ok().map(Rc::new);
        let bindings: Vec<KeyBinding> = ["enter", "space"]
            .into_iter()
            .filter_map(|keys| {
                KeyBinding::load(keys, action.boxed_clone(), context.clone(), false, None, cx).ok()
            })
            .collect();
        cx.bind_keys(bindings);
    };
    bind(CONTEXT, &OpenJobs, cx);
    bind(NOTICE_CONTEXT, &OpenUpdate, cx);
}

/// The user asked for the job list.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ShowJobs;

/// The user asked for the update dialog.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ShowUpdate;

pub struct StatusBar {
    jobs: Entity<Jobs>,
    updates: Entity<Updates>,
    focus: FocusHandle,
    notice_focus: FocusHandle,
    _changed: [Subscription; 2],
}

impl EventEmitter<ShowJobs> for StatusBar {}
impl EventEmitter<ShowUpdate> for StatusBar {}

impl StatusBar {
    pub fn new(jobs: Entity<Jobs>, updates: Entity<Updates>, cx: &mut Context<Self>) -> Self {
        let changed = [
            cx.observe(&jobs, |_, _, cx| cx.notify()),
            cx.observe(&updates, |_, _, cx| cx.notify()),
        ];
        Self {
            jobs,
            updates,
            focus: cx.focus_handle(),
            notice_focus: cx.focus_handle(),
            _changed: changed,
        }
    }

    fn open(&mut self, _: &OpenJobs, _: &mut Window, cx: &mut Context<Self>) {
        cx.emit(ShowJobs);
    }

    fn open_update(&mut self, _: &OpenUpdate, _: &mut Window, cx: &mut Context<Self>) {
        cx.emit(ShowUpdate);
    }

    /// The notice of a newer version, while one is on offer.
    fn notice(&self, window: &Window, cx: &mut Context<Self>) -> Option<impl IntoElement + use<>> {
        let tokens = *cx.tokens();
        let notice: SharedString = update_text::notice(self.updates.read(cx).notice()?).into();
        let focused = self.notice_focus.is_focused(window);
        Some(
            div()
                .id("update-notice")
                .test_support()
                .track_focus(&self.notice_focus.clone().tab_stop(true))
                .key_context(NOTICE_CONTEXT)
                .on_action(cx.listener(Self::open_update))
                .role(Role::Button)
                .aria_label(notice.clone())
                .relative()
                .flex()
                .flex_shrink_0()
                .items_center()
                .h_full()
                .px_2()
                .text_sm()
                .text_color(tokens.accent)
                .hover(move |style| style.bg(tokens.bg_3))
                .when(focused, |this| {
                    this.child(
                        div()
                            .absolute()
                            .inset_0()
                            .border_2()
                            .border_color(tokens.focus_ring),
                    )
                })
                .on_click(cx.listener(|_, _, _, cx| cx.emit(ShowUpdate)))
                .child(notice),
        )
    }
}

impl Focusable for StatusBar {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus.clone()
    }
}

/// Each outcome's colour token (UI-23).
pub fn outcome_color(tokens: &Tokens, outcome: &Outcome) -> Color {
    match outcome {
        Outcome::Succeeded => tokens.outcome_succeeded,
        Outcome::NothingToDo => tokens.outcome_nothing_to_do,
        Outcome::Skipped { .. } => tokens.outcome_skipped,
        Outcome::Failed => tokens.outcome_failed,
        Outcome::Conflict => tokens.outcome_conflict,
    }
}

/// An outcome's icon and text, in its colour.
pub fn outcome_label(tokens: &Tokens, outcome: &Outcome) -> impl IntoElement + use<> {
    div()
        .flex()
        .flex_row()
        .items_center()
        .gap_1()
        .text_color(outcome_color(tokens, outcome))
        .child(job_text::icon(outcome))
        .child(job_text::outcome(outcome))
}

/// What the status bar says, as screen readers and the tests read it.
fn label(status: &Status<'_>) -> String {
    match status {
        Status::Running(running) => running
            .iter()
            .map(|job| format!("{} {}", job.label, jobs::progress(job)))
            .collect::<Vec<_>>()
            .join(", "),
        Status::Ended(job) => match &job.outcome {
            Some(outcome) => format!("{}: {}", job.label, job_text::outcome(outcome)),
            None => job.label.clone(),
        },
        Status::Idle => text::NO_JOBS.to_owned(),
    }
}

fn running_job(job: &JobView, tokens: &Tokens) -> impl IntoElement + use<> {
    div()
        .flex()
        .flex_row()
        .gap_1()
        .min_w_0()
        .child(div().truncate().child(job.label.clone()))
        .child(
            div()
                .flex_shrink_0()
                .text_color(tokens.text_secondary)
                .child(jobs::progress(job)),
        )
}

impl Render for StatusBar {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let tokens = *cx.tokens();
        let jobs = self.jobs.read(cx);
        let status = jobs
            .active()
            .map_or(Status::Idle, |active| jobs.status(active));
        let label = SharedString::from(label(&status));
        let content = match &status {
            Status::Running(running) => div()
                .flex()
                .flex_row()
                .gap_3()
                .min_w_0()
                .children(running.iter().map(|job| running_job(job, &tokens)))
                .into_any_element(),
            Status::Ended(job) => div()
                .flex()
                .flex_row()
                .gap_1()
                .min_w_0()
                .child(div().truncate().child(format!("{}:", job.label)))
                .children(
                    job.outcome
                        .as_ref()
                        .map(|outcome| outcome_label(&tokens, outcome)),
                )
                .into_any_element(),
            Status::Idle => div()
                .text_color(tokens.text_secondary)
                .child(text::NO_JOBS)
                .into_any_element(),
        };
        let focused = self.focus.is_focused(window);
        let notice = self.notice(window, cx);
        let jobs = div()
            .id("status-jobs")
            .test_support()
            .track_focus(&self.focus)
            .key_context(CONTEXT)
            .on_action(cx.listener(Self::open))
            .role(Role::Button)
            .aria_label(label)
            .aria_description(text::SHOW_JOBS)
            .relative()
            .flex()
            .flex_row()
            .items_center()
            .h_full()
            .max_w_full()
            .px_2()
            .text_sm()
            .text_color(tokens.text_primary)
            .hover(move |style| style.bg(tokens.bg_3))
            .when(focused, |this| {
                this.child(
                    div()
                        .absolute()
                        .inset_0()
                        .border_2()
                        .border_color(tokens.focus_ring),
                )
            })
            .on_click(cx.listener(|_, _, _, cx| cx.emit(ShowJobs)))
            .child(content)
            .min_w(px(0.));
        div()
            .flex()
            .flex_row()
            .justify_between()
            .size_full()
            .min_w_0()
            .child(jobs)
            .children(notice)
    }
}
