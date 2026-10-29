//! The status bar of UI-21, a port of `ui/src/layout/StatusBar.tsx`: the
//! active tab's running jobs with their progress; with none running, the
//! outcome of the job that ended last for a while, then a failed or
//! conflicted job until the job list has shown it. It is a button: a click,
//! or Enter or Space while it has the focus, opens the job list. Its text
//! is no live region, so progress is not read out on every step.

use std::rc::Rc;

use gitcroc_core::{JobView, Outcome};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::{
    App, Context, Entity, EventEmitter, FocusHandle, Focusable, InteractiveElement, IntoElement,
    KeyBinding, KeyBindingContextPredicate, ParentElement, Render, Role, SharedString,
    StatefulInteractiveElement, Styled, Subscription, TestSupportExt, Window, actions, div, px,
};

use crate::jobs::{self, Jobs, Status};
use crate::strings::{jobs as job_text, status_bar as text};
use crate::theme::ActiveTokens;
use crate::theme::tokens::{Color, Tokens};

actions!(
    status_bar,
    [
        /// Opens the job list from the focused status bar.
        OpenJobs,
    ]
);

/// The key context of the focused status bar.
const CONTEXT: &str = "GitcrocStatusBar";

pub fn bind(cx: &mut App) {
    let context = KeyBindingContextPredicate::parse(CONTEXT).ok().map(Rc::new);
    let bindings = ["enter", "space"].into_iter().filter_map(|keys| {
        KeyBinding::load(keys, Box::new(OpenJobs), context.clone(), false, None, cx).ok()
    });
    cx.bind_keys(bindings.collect::<Vec<_>>());
}

/// The user asked for the job list.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ShowJobs;

pub struct StatusBar {
    jobs: Entity<Jobs>,
    focus: FocusHandle,
    _jobs_changed: Subscription,
}

impl EventEmitter<ShowJobs> for StatusBar {}

impl StatusBar {
    pub fn new(jobs: Entity<Jobs>, cx: &mut Context<Self>) -> Self {
        let jobs_changed = cx.observe(&jobs, |_, _, cx| cx.notify());
        Self {
            jobs,
            focus: cx.focus_handle(),
            _jobs_changed: jobs_changed,
        }
    }

    fn open(&mut self, _: &OpenJobs, _: &mut Window, cx: &mut Context<Self>) {
        cx.emit(ShowJobs);
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
        div()
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
            .min_w(px(0.))
    }
}
