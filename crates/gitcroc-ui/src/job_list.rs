//! The job list of UI-21, a port of `ui/src/jobs/JobList.tsx`: every job of
//! this run across all workspaces, newest first, in a `uniform_list`
//! (PERF-1). The selected job shows git's raw output as text (SEC-3) and,
//! while it has not ended, Cancel (GRP-10). Opening it marks every ended
//! job seen; Escape, Close or a click beside it closes it.

use std::collections::HashMap;
use std::ops::Range;
use std::rc::Rc;

use gitcroc_core::{CancelResult, JobId, JobState, JobView};
use gpui_component::FocusTrapElement;
use gpui_component::button::Button;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::{
    AnyElement, App, ClickEvent, Context, ElementId, Entity, EventEmitter, FocusHandle, Focusable,
    InteractiveElement, IntoElement, KeyBinding, KeyBindingContextPredicate, ParentElement, Pixels,
    Render, Role, ScrollStrategy, SharedString, StatefulInteractiveElement, Styled, Subscription,
    TestSupportExt, UniformListScrollHandle, Window, actions, div, px, uniform_list,
};

use crate::dialog::{self, Cancel, Closed, Modal};
use crate::jobs::{self, Jobs};
use crate::status_bar::outcome_label;
use crate::strings::jobs as text;
use crate::theme::{ActiveTokens, AppTheme};

actions!(
    job_list,
    [SelectNext, SelectPrevious, SelectFirst, SelectLast]
);

/// The key context of the list of jobs.
const CONTEXT: &str = "GitcrocJobList";

/// One row's height; `uniform_list` lays out only those on screen.
pub const ROW_HEIGHT: Pixels = px(24.);
const LIST_HEIGHT: Pixels = px(240.);
const OUTPUT_HEIGHT: Pixels = px(220.);
const WIDTH: Pixels = px(860.);

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

pub struct JobList {
    jobs: Entity<Jobs>,
    selected: Option<JobId>,
    /// What a cancel did, by job, as the user reads it.
    cancelled: HashMap<JobId, SharedString>,
    scroll: UniformListScrollHandle,
    focus: FocusHandle,
    list_focus: FocusHandle,
    _jobs_changed: Subscription,
}

impl EventEmitter<Closed> for JobList {}

impl JobList {
    /// The list of `jobs`, the newest selected; every ended job is seen.
    pub fn new(jobs: Entity<Jobs>, cx: &mut Context<Self>) -> Self {
        jobs.update(cx, |jobs, cx| jobs.see_all(cx));
        let selected = jobs.read(cx).jobs().next().map(|job| job.job_id);
        let jobs_changed = cx.observe(&jobs, |_, _, cx| cx.notify());
        Self {
            jobs,
            selected,
            cancelled: HashMap::new(),
            scroll: UniformListScrollHandle::new(),
            focus: cx.focus_handle(),
            list_focus: cx.focus_handle(),
            _jobs_changed: jobs_changed,
        }
    }

    fn selected_index(&self, cx: &App) -> Option<usize> {
        let selected = self.selected?;
        self.jobs
            .read(cx)
            .jobs()
            .position(|job| job.job_id == selected)
    }

    fn select(&mut self, index: usize, cx: &mut Context<Self>) {
        let id = self.jobs.read(cx).jobs().nth(index).map(|job| job.job_id);
        if let Some(id) = id {
            self.selected = Some(id);
            self.scroll.scroll_to_item(index, ScrollStrategy::Nearest);
            cx.notify();
        }
    }

    fn step(&mut self, step: isize, cx: &mut Context<Self>) {
        let last = self.jobs.read(cx).len().saturating_sub(1);
        let index = self
            .selected_index(cx)
            .map_or(0, |at| at.saturating_add_signed(step).min(last));
        self.select(index, cx);
    }

    fn select_next(&mut self, _: &SelectNext, _: &mut Window, cx: &mut Context<Self>) {
        self.step(1, cx);
    }

    fn select_previous(&mut self, _: &SelectPrevious, _: &mut Window, cx: &mut Context<Self>) {
        self.step(-1, cx);
    }

    fn select_first(&mut self, _: &SelectFirst, _: &mut Window, cx: &mut Context<Self>) {
        self.select(0, cx);
    }

    fn select_last(&mut self, _: &SelectLast, _: &mut Window, cx: &mut Context<Self>) {
        let last = self.jobs.read(cx).len().saturating_sub(1);
        self.select(last, cx);
    }

    fn cancel_job(&mut self, id: JobId, cx: &mut Context<Self>) {
        let call = self.jobs.read(cx).cancel(id);
        cx.spawn(async move |this, cx| {
            let Some(result) = call.await else {
                return;
            };
            let note = match result {
                CancelResult::Killing => text::STOPPING,
                CancelResult::Finishing => text::CANNOT_STOP,
                CancelResult::Cancelled | CancelResult::Ended => return,
            };
            let _ = this.update(cx, |this, cx| {
                this.cancelled.insert(id, note.into());
                cx.notify();
            });
        })
        .detach();
    }

    fn close(&mut self, _: &Cancel, _: &mut Window, cx: &mut Context<Self>) {
        cx.emit(Closed);
    }

    fn on_close(&mut self, _: &ClickEvent, _: &mut Window, cx: &mut Context<Self>) {
        cx.emit(Closed);
    }

    fn render_rows(
        &mut self,
        range: Range<usize>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Vec<AnyElement> {
        let tokens = *cx.tokens();
        let focused = self.list_focus.is_focused(window);
        let selected = self.selected;
        let jobs = self.jobs.read(cx);
        let rows: Vec<(usize, JobView)> = jobs
            .jobs()
            .enumerate()
            .skip(range.start)
            .take(range.len())
            .map(|(index, job)| (index, job.clone()))
            .collect();
        rows.into_iter()
            .map(|(index, job)| {
                let is_selected = selected == Some(job.job_id);
                let cell = |width: f32, content: AnyElement| {
                    div().w(px(width)).flex_shrink_0().truncate().child(content)
                };
                div()
                    .id(ElementId::Name(format!("job-row-{index}").into()))
                    .test_support()
                    .role(Role::ListBoxOption)
                    .aria_label(row_label(&job))
                    .aria_selected(is_selected)
                    .when(is_selected && focused, |row| row.aria_active_descendant())
                    .h(ROW_HEIGHT)
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap_2()
                    .px_2()
                    .when(is_selected, |row| row.bg(tokens.selection_bg))
                    .when(is_selected && focused, |row| {
                        row.border_1().border_color(tokens.focus_ring)
                    })
                    .when(!is_selected, |row| row.hover(|row| row.bg(tokens.bg_3)))
                    .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                        window.focus(&this.list_focus, cx);
                        this.select(index, cx);
                    }))
                    .child(cell(240., job.label.clone().into_any_element()))
                    .child(cell(130., job.workspace_name.clone().into_any_element()))
                    .child(cell(130., job.repo_names.join(", ").into_any_element()))
                    .child(cell(150., state_element(&job, &tokens)))
                    .child(cell(
                        70.,
                        jobs::format_start(job.started_at).into_any_element(),
                    ))
                    .child(cell(
                        80.,
                        jobs::format_duration(job.duration_ms).into_any_element(),
                    ))
                    .into_any_element()
            })
            .collect()
    }

    fn detail(&self, cx: &mut Context<Self>) -> AnyElement {
        let tokens = *cx.tokens();
        let job = self
            .selected
            .and_then(|id| self.jobs.read(cx).get(id).cloned());
        let Some(job) = job else {
            return div()
                .id("job-select")
                .test_support()
                .role(Role::Label)
                .aria_label(text::SELECT_JOB)
                .text_color(tokens.text_secondary)
                .child(text::SELECT_JOB)
                .into_any_element();
        };
        let id = job.job_id;
        let code_font = AppTheme::global(cx).code_font().clone();
        let output = if job.output.is_empty() {
            div()
                .id("job-no-output")
                .test_support()
                .role(Role::Label)
                .aria_label(text::NO_OUTPUT)
                .text_color(tokens.text_secondary)
                .child(text::NO_OUTPUT)
                .into_any_element()
        } else {
            div()
                .id("job-output")
                .test_support()
                .role(Role::Document)
                .aria_label(job.output.clone())
                .max_h(OUTPUT_HEIGHT)
                .overflow_y_scroll()
                .p_2()
                .bg(tokens.bg_2)
                .border_1()
                .border_color(tokens.border)
                .rounded(px(4.))
                .font_family(code_font)
                .text_sm()
                .child(job.output.clone())
                .into_any_element()
        };
        div()
            .id("job-detail")
            .test_support()
            .role(Role::Group)
            .aria_label(text::OUTPUT)
            .flex()
            .flex_col()
            .gap_2()
            .child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap_3()
                    .child(
                        div()
                            .font_weight(gpui_kit::FontWeight::SEMIBOLD)
                            .child(job.label.clone()),
                    )
                    .child(state_element(&job, &tokens))
                    .when(job.state != JobState::Ended, |row| {
                        row.child(Button::new("job-cancel").label(text::CANCEL).on_click(
                            cx.listener(move |this, _, _, cx| {
                                this.cancel_job(id, cx);
                            }),
                        ))
                    }),
            )
            .children(self.cancelled.get(&id).map(|note| {
                div()
                    .id("job-cancel-note")
                    .test_support()
                    .role(Role::Status)
                    .aria_label(note.clone())
                    .child(note.clone())
            }))
            .children(job.message.clone().map(|message| {
                div()
                    .id("job-message")
                    .test_support()
                    .role(Role::Paragraph)
                    .aria_label(message.clone())
                    .child(message)
            }))
            .child(output)
            .into_any_element()
    }
}

/// A row as screen readers and the tests read it: its columns, those with
/// text.
fn row_label(job: &JobView) -> String {
    [
        job.label.clone(),
        job.workspace_name.clone(),
        job.repo_names.join(", "),
        jobs::state(job),
        jobs::format_start(job.started_at),
        jobs::format_duration(job.duration_ms),
    ]
    .into_iter()
    .filter(|part| !part.is_empty())
    .collect::<Vec<_>>()
    .join(", ")
}

/// A job's outcome in its colour, or where it is.
fn state_element(job: &JobView, tokens: &crate::theme::tokens::Tokens) -> AnyElement {
    match &job.outcome {
        Some(outcome) => outcome_label(tokens, outcome).into_any_element(),
        None => div()
            .text_color(tokens.text_secondary)
            .child(jobs::state(job))
            .into_any_element(),
    }
}

impl Modal for JobList {
    /// The list has the focus.
    fn focus_first(&self, window: &mut Window, cx: &mut App) {
        window.focus(&self.list_focus, cx);
    }

    fn closes_on_backdrop(&self) -> bool {
        true
    }
}

impl Focusable for JobList {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus.clone()
    }
}

impl Render for JobList {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let tokens = *cx.tokens();
        let title = SharedString::from(text::TITLE);
        let count = self.jobs.read(cx).len();
        let header = |name: &'static str, width: f32| {
            div().w(px(width)).flex_shrink_0().truncate().child(name)
        };
        let list = if count == 0 {
            div()
                .id("job-list-empty")
                .test_support()
                .role(Role::Label)
                .aria_label(text::NO_JOBS)
                .p_2()
                .text_color(tokens.text_secondary)
                .child(text::NO_JOBS)
                .into_any_element()
        } else {
            uniform_list("job-list-rows", count, cx.processor(Self::render_rows))
                .track_scroll(&self.scroll)
                .size_full()
                .into_any_element()
        };
        let detail = self.detail(cx);
        dialog::frame("job-list-frame", Role::Dialog, &title, cx)
            .test_support()
            .track_focus(&self.focus)
            .key_context(dialog::CONTEXT)
            .on_action(cx.listener(Self::close))
            .w(WIDTH)
            .text_sm()
            .child(
                div()
                    .flex()
                    .flex_row()
                    .gap_2()
                    .px_2()
                    .text_color(tokens.text_secondary)
                    .child(header(text::COLUMN_JOB, 240.))
                    .child(header(text::COLUMN_WORKSPACE, 130.))
                    .child(header(text::COLUMN_REPO, 130.))
                    .child(header(text::COLUMN_STATE, 150.))
                    .child(header(text::COLUMN_STARTED, 70.))
                    .child(header(text::COLUMN_DURATION, 80.)),
            )
            .child(
                div()
                    .id("job-rows")
                    .test_support()
                    .track_focus(&self.list_focus)
                    .key_context(CONTEXT)
                    .on_action(cx.listener(Self::select_next))
                    .on_action(cx.listener(Self::select_previous))
                    .on_action(cx.listener(Self::select_first))
                    .on_action(cx.listener(Self::select_last))
                    .role(Role::ListBox)
                    .aria_label(text::TITLE)
                    .h(LIST_HEIGHT)
                    .border_1()
                    .border_color(tokens.border)
                    .child(list),
            )
            .child(detail)
            .child(
                dialog::actions_row().child(
                    Button::new("job-list-close")
                        .label(text::CLOSE)
                        .on_click(cx.listener(Self::on_close)),
                ),
            )
            // The trap takes the place of the frame, under the dialog's id.
            .focus_trap("job-list", &self.focus)
    }
}
