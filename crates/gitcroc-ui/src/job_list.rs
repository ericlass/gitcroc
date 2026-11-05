//! The job list of UI-21, a port of `ui/src/jobs/JobList.tsx` and
//! `ui/src/jobs/GroupReport.tsx`: every job of this run across all
//! workspaces, newest first, in a `uniform_list` (PERF-1). The selected job
//! shows git's raw output as text (SEC-3), or, for a group operation, its
//! sub-jobs while it runs and its result report when it ends (section 5,
//! step 4): one line per repo of the target set in target-set order, each
//! with its outcome (GRP-7) and git's reason, expanding to git's output.
//! Retry failed (GRP-11) starts a new preflight of the failed repos.
//! While it has not ended, Cancel (GRP-10) is there. Opening the list marks
//! every ended job seen; Escape, Close or a click beside it closes it.

use crate::scrollbars::ScrollAreaExt as _;
use gpui_component::scroll::ScrollableElement as _;
use std::collections::{HashMap, HashSet};
use std::ops::Range;
use std::rc::Rc;
use std::time::Duration;

use gitcroc_core::{CancelResult, JobId, JobState, JobView, Outcome, RepoId, WorkspaceId};
use gpui_component::Disableable;
use gpui_component::FocusTrapElement;
use gpui_component::button::Button;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::{
    Animation, AnimationExt, AnyElement, App, ClickEvent, Context, ElementId, Entity, EventEmitter,
    FocusHandle, Focusable, InteractiveElement, IntoElement, KeyBinding,
    KeyBindingContextPredicate, ParentElement, Pixels, Render, Role, ScrollHandle, ScrollStrategy,
    SharedString, StatefulInteractiveElement, Styled, Subscription, TestSupportExt, Transformation,
    UniformListScrollHandle, Window, actions, div, percentage, px, svg, uniform_list,
};

use crate::dialog::{self, Cancel, Closed, Modal};
use crate::icons;
use crate::jobs::{self, Jobs};
use crate::outline::Outlined;
use crate::status_bar::outcome_label;
use crate::strings::{self, jobs as text};
use crate::theme::{ActiveTokens, AppTheme, tokens::Tokens};

actions!(
    job_list,
    [
        SelectNext,
        SelectPrevious,
        SelectFirst,
        SelectLast,
        /// The toggle column: the next line's output toggle (UI-22).
        ReportToggleNext,
        ReportTogglePrevious,
        ReportToggleFirst,
        ReportToggleLast,
        /// The name column: the next line's repo name (UI-22).
        ReportNameNext,
        ReportNamePrevious,
        ReportNameFirst,
        ReportNameLast,
        /// Right from a toggle, Left from a name: the same line's other
        /// control, so a column is kept while moving.
        ReportToName,
        ReportToToggle,
        /// A job's output, once Tab gives it the focus: the keys scroll it
        /// (UI-21, UI-22).
        OutputDown,
        OutputUp,
        OutputPageDown,
        OutputPageUp,
        OutputTop,
        OutputBottom,
    ]
);

/// The key context of the list of jobs.
const CONTEXT: &str = "GitcrocJobList";
/// The key contexts of a report's two controls.
const TOGGLE_CONTEXT: &str = "GitcrocReportToggle";
const NAME_CONTEXT: &str = "GitcrocReportName";
/// The key context of a job's output.
const OUTPUT_CONTEXT: &str = "GitcrocJobOutput";
/// How far an arrow key scrolls the output.
const OUTPUT_STEP: Pixels = px(18.);

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
    let report = |keys: &str, action: Box<dyn gpui_kit::Action>, context: &str, cx: &mut App| {
        let context = KeyBindingContextPredicate::parse(context).ok().map(Rc::new);
        KeyBinding::load(keys, action, context, false, None, cx).ok()
    };
    let toggle = [
        report("down", Box::new(ReportToggleNext), TOGGLE_CONTEXT, cx),
        report("up", Box::new(ReportTogglePrevious), TOGGLE_CONTEXT, cx),
        report("home", Box::new(ReportToggleFirst), TOGGLE_CONTEXT, cx),
        report("end", Box::new(ReportToggleLast), TOGGLE_CONTEXT, cx),
        report("right", Box::new(ReportToName), TOGGLE_CONTEXT, cx),
    ];
    let name = [
        report("down", Box::new(ReportNameNext), NAME_CONTEXT, cx),
        report("up", Box::new(ReportNamePrevious), NAME_CONTEXT, cx),
        report("home", Box::new(ReportNameFirst), NAME_CONTEXT, cx),
        report("end", Box::new(ReportNameLast), NAME_CONTEXT, cx),
        report("left", Box::new(ReportToToggle), NAME_CONTEXT, cx),
    ];
    let output = [
        report("down", Box::new(OutputDown), OUTPUT_CONTEXT, cx),
        report("up", Box::new(OutputUp), OUTPUT_CONTEXT, cx),
        report("pagedown", Box::new(OutputPageDown), OUTPUT_CONTEXT, cx),
        report("pageup", Box::new(OutputPageUp), OUTPUT_CONTEXT, cx),
        report("home", Box::new(OutputTop), OUTPUT_CONTEXT, cx),
        report("end", Box::new(OutputBottom), OUTPUT_CONTEXT, cx),
    ];
    cx.bind_keys(toggle.into_iter().flatten());
    cx.bind_keys(name.into_iter().flatten());
    cx.bind_keys(output.into_iter().flatten());
}

/// What a report line asked the window to do (UI-21).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum JobListEvent {
    /// Select the repo in its workspace's tree, from its report line.
    SelectRepo {
        workspace: WorkspaceId,
        repo: RepoId,
    },
    /// Retry failed (GRP-11) of the group job: its failed repos only.
    Retry { workspace: WorkspaceId, job: JobId },
}

/// The two controls of a report line, the toggle and the repo's name.
const TOGGLE: usize = 0;
const NAME: usize = 1;

pub struct JobList {
    jobs: Entity<Jobs>,
    selected: Option<JobId>,
    /// What a cancel did, by job, as the user reads it.
    cancelled: HashMap<JobId, SharedString>,
    scroll: UniformListScrollHandle,
    focus: FocusHandle,
    list_focus: FocusHandle,
    /// The shown job's output, a stop in the Tab order, and its scroll.
    output_focus: FocusHandle,
    output_scroll: ScrollHandle,
    /// The lines a report shows and, for each, its two focus handles.
    report_handles: Vec<[FocusHandle; 2]>,
    /// The job and line count the handles are for.
    report_key: Option<(JobId, usize)>,
    /// The line the report's focus is on, and its expanded ones.
    report_line: usize,
    expanded: HashSet<RepoId>,
    _jobs_changed: Subscription,
}

impl EventEmitter<Closed> for JobList {}
impl EventEmitter<JobListEvent> for JobList {}

impl JobList {
    /// The list of `jobs`, on `initial` or the newest; every ended job is
    /// seen.
    pub fn new(jobs: Entity<Jobs>, initial: Option<JobId>, cx: &mut Context<Self>) -> Self {
        jobs.update(cx, |jobs, cx| jobs.see_all(cx));
        let selected = initial.or_else(|| jobs.read(cx).jobs().next().map(|job| job.job_id));
        let jobs_changed = cx.observe(&jobs, |_, _, cx| cx.notify());
        let mut this = Self {
            jobs,
            selected,
            cancelled: HashMap::new(),
            scroll: UniformListScrollHandle::new(),
            focus: cx.focus_handle(),
            list_focus: cx.focus_handle(),
            output_focus: cx.focus_handle().tab_stop(true),
            output_scroll: ScrollHandle::new(),
            report_handles: Vec::new(),
            report_key: None,
            report_line: 0,
            expanded: HashSet::new(),
            _jobs_changed: jobs_changed,
        };
        this.sync_report(cx);
        this
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
            self.expanded.clear();
            self.report_key = None;
            self.report_line = 0;
            self.sync_report(cx);
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

    /// The job the detail shows.
    fn job(&self, cx: &App) -> Option<JobView> {
        self.selected
            .and_then(|id| self.jobs.read(cx).get(id).cloned())
    }

    /// The lines a job's report shows: its sub-jobs while it runs, its
    /// result lines when it ended.
    fn line_count(job: &JobView) -> usize {
        if job.state == JobState::Ended {
            job.report.as_ref().map_or(0, Vec::len)
        } else {
            0
        }
    }

    /// Builds the report's focus handles for the selected job, when its
    /// lines changed.
    fn sync_report(&mut self, cx: &mut Context<Self>) {
        let job = self.job(cx);
        let key = job
            .as_ref()
            .map(|job| (job.job_id, Self::line_count(job)))
            .filter(|(_, count)| *count > 0);
        if key == self.report_key {
            return;
        }
        if let Some((id, _)) = &key
            && Some(*id) != self.report_key.map(|(id, _)| id)
        {
            self.expanded.clear();
            self.report_line = 0;
        }
        self.report_key = key;
        self.report_handles = (0..key.map_or(0, |(_, count)| count))
            .map(|_| [cx.focus_handle(), cx.focus_handle()])
            .collect();
    }

    /// Gives the report a line's control the focus.
    fn focus_line(
        &mut self,
        index: usize,
        column: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.sync_report(cx);
        let Some(handles) = self.report_handles.get(index) else {
            return;
        };
        self.report_line = index;
        window.focus(&handles[column], cx);
    }

    fn report_step(
        &mut self,
        step: isize,
        column: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.sync_report(cx);
        let last = self.report_handles.len().saturating_sub(1);
        let index = self.report_line.saturating_add_signed(step).min(last);
        self.focus_line(index, column, window, cx);
    }

    fn report_first(&mut self, column: usize, window: &mut Window, cx: &mut Context<Self>) {
        self.focus_line(0, column, window, cx);
    }

    fn report_last(&mut self, column: usize, window: &mut Window, cx: &mut Context<Self>) {
        let last = self.report_handles.len().saturating_sub(1);
        self.focus_line(last, column, window, cx);
    }

    fn toggle_next(&mut self, _: &ReportToggleNext, window: &mut Window, cx: &mut Context<Self>) {
        self.report_step(1, TOGGLE, window, cx);
    }

    fn toggle_previous(
        &mut self,
        _: &ReportTogglePrevious,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.report_step(-1, TOGGLE, window, cx);
    }

    fn toggle_first(&mut self, _: &ReportToggleFirst, window: &mut Window, cx: &mut Context<Self>) {
        self.report_first(TOGGLE, window, cx);
    }

    fn toggle_last(&mut self, _: &ReportToggleLast, window: &mut Window, cx: &mut Context<Self>) {
        self.report_last(TOGGLE, window, cx);
    }

    fn name_next(&mut self, _: &ReportNameNext, window: &mut Window, cx: &mut Context<Self>) {
        self.report_step(1, NAME, window, cx);
    }

    fn name_previous(
        &mut self,
        _: &ReportNamePrevious,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.report_step(-1, NAME, window, cx);
    }

    fn name_first(&mut self, _: &ReportNameFirst, window: &mut Window, cx: &mut Context<Self>) {
        self.report_first(NAME, window, cx);
    }

    fn name_last(&mut self, _: &ReportNameLast, window: &mut Window, cx: &mut Context<Self>) {
        self.report_last(NAME, window, cx);
    }

    fn report_name_control(
        &mut self,
        _: &ReportToName,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let index = self.report_line;
        self.focus_line(index, NAME, window, cx);
    }

    fn report_toggle_control(
        &mut self,
        _: &ReportToToggle,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let index = self.report_line;
        self.focus_line(index, TOGGLE, window, cx);
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
        let count = jobs.jobs().len();
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
                    .aria_description(strings::a11y::position(index, count))
                    .aria_selected(is_selected)
                    .when(is_selected && focused, |row| row.aria_active_descendant())
                    .h(ROW_HEIGHT)
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap_2()
                    .px_2()
                    .relative()
                    .when(is_selected, |row| row.bg(tokens.selection_bg))
                    .selection_mark(is_selected, &tokens)
                    .focus_ring(is_selected && focused, &tokens)
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

    /// Job `job`'s output, which Tab reaches and the keys scroll.
    fn output(
        &self,
        job: &JobView,
        tokens: &Tokens,
        code_font: &SharedString,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        if job.output.is_empty() {
            return div()
                .id("job-no-output")
                .test_support()
                .role(Role::Label)
                .aria_label(text::NO_OUTPUT)
                .text_color(tokens.text_secondary)
                .child(text::NO_OUTPUT)
                .into_any_element();
        }
        let tokens = *tokens;
        let output =
            div()
                .id("job-output")
                .test_support()
                .track_focus(&self.output_focus)
                .track_scroll(&self.output_scroll)
                .key_context(OUTPUT_CONTEXT)
                .on_action(cx.listener(|this, _: &OutputDown, _, cx| {
                    this.scroll_output(|at, _, _| at + OUTPUT_STEP, cx)
                }))
                .on_action(cx.listener(|this, _: &OutputUp, _, cx| {
                    this.scroll_output(|at, _, _| at - OUTPUT_STEP, cx)
                }))
                .on_action(cx.listener(|this, _: &OutputPageDown, _, cx| {
                    this.scroll_output(|at, page, _| at + page, cx)
                }))
                .on_action(cx.listener(|this, _: &OutputPageUp, _, cx| {
                    this.scroll_output(|at, page, _| at - page, cx)
                }))
                .on_action(cx.listener(|this, _: &OutputTop, _, cx| {
                    this.scroll_output(|_, _, _| px(0.), cx)
                }))
                .on_action(cx.listener(|this, _: &OutputBottom, _, cx| {
                    this.scroll_output(|_, _, end| end, cx)
                }))
                .role(Role::Document)
                .aria_label(job.output.clone())
                .max_h(OUTPUT_HEIGHT)
                .overflow_y_scroll()
                .p_2()
                .bg(tokens.bg_2)
                .border_1()
                .border_color(tokens.border)
                .rounded(px(4.))
                // It scrolls, so its own border is its ring.
                .focus(move |style| style.border_color(tokens.focus_ring))
                .font_family(code_font.clone())
                .text_sm()
                .child(job.output.clone());
        div()
            .relative()
            .child(output)
            .vertical_scrollbar(&self.output_scroll)
            .into_any_element()
    }

    /// How far down the output is scrolled.
    pub fn output_offset(&self) -> Pixels {
        -self.output_scroll.offset().y
    }

    /// Scrolls the output to where `to` puts it, from how far down it is,
    /// a page and the end; it stays between the top and the end.
    fn scroll_output(
        &mut self,
        to: impl FnOnce(Pixels, Pixels, Pixels) -> Pixels,
        cx: &mut Context<Self>,
    ) {
        let end = self.output_scroll.max_offset().y;
        let page = (self.output_scroll.bounds().size.height - OUTPUT_STEP).max(OUTPUT_STEP);
        let at = -self.output_scroll.offset().y;
        let next = to(at, page, end).clamp(px(0.), end);
        let x = self.output_scroll.offset().x;
        self.output_scroll.set_offset(gpui_kit::point(x, -next));
        cx.notify();
    }

    fn detail(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let tokens = *cx.tokens();
        let Some(job) = self.job(cx) else {
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
        let report = job.state == JobState::Ended && job.report.is_some();
        let running_group = job.state != JobState::Ended && !job.sub_jobs.is_empty();
        let body = if report || running_group {
            self.sync_report(cx);
            self.report(&job, &tokens, cx)
        } else if !job.ai_files.is_empty() {
            // Resolve all with AI: each file and where it is (AI-13).
            div()
                .flex()
                .flex_col()
                .gap_2()
                .child(crate::resolve_all::file_list(
                    AI_FILE_IDS,
                    &job,
                    None,
                    &tokens,
                    cx,
                ))
                .child(self.output(&job, &tokens, &code_font, cx))
                .into_any_element()
        } else {
            self.output(&job, &tokens, &code_font, cx)
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
            .when(!report && !running_group, |panel| {
                panel.children(job.message.clone().map(|message| {
                    div()
                        .id("job-message")
                        .test_support()
                        .role(Role::Paragraph)
                        .aria_label(message.clone())
                        .child(message)
                }))
            })
            .child(body)
            .into_any_element()
    }

    /// A group job's sub-jobs while it runs, or its result report (section
    /// 5, step 4; UI-21).
    fn report(&mut self, job: &JobView, tokens: &Tokens, cx: &mut Context<Self>) -> AnyElement {
        if job.state == JobState::Ended {
            self.result(job, tokens, cx)
        } else {
            self.sub_jobs(job, tokens, cx).into_any_element()
        }
    }

    fn sub_jobs(&self, job: &JobView, tokens: &Tokens, cx: &App) -> impl IntoElement {
        let rows = job
            .sub_jobs
            .iter()
            .enumerate()
            .map(|(at, sub)| {
                let state = match &sub.outcome {
                    Some(outcome) => text::outcome(outcome),
                    None if sub.state == JobState::Queued => text::WAITING.to_owned(),
                    None => text::RUNNING.to_owned(),
                };
                let mut row = div()
                    .id(ElementId::Name(format!("sub-job-{at}").into()))
                    .test_support()
                    .role(Role::ListItem)
                    .aria_label(text::repo_state(&sub.repo_name, &state))
                    .flex()
                    .flex_row()
                    .gap_2()
                    .child(div().child(sub.repo_name.clone()));
                row = row.child(match (&sub.outcome, sub.state) {
                    (None, JobState::Queued) => div()
                        .text_color(tokens.text_secondary)
                        .child(text::WAITING)
                        .into_any_element(),
                    (None, _) => div()
                        .id(ElementId::Name(format!("sub-job-{at}-spinner").into()))
                        .test_support()
                        .role(Role::Image)
                        .aria_label(text::RUNNING)
                        .text_color(tokens.text_secondary)
                        .child(spinner(cx))
                        .into_any_element(),
                    (Some(_), _) => div()
                        .text_color(tokens.text_secondary)
                        .child(state)
                        .into_any_element(),
                });
                row
            })
            .collect::<Vec<_>>();
        div()
            .id("group-report")
            .test_support()
            .flex()
            .flex_col()
            .gap_2()
            .child(
                div()
                    .id("report-counts")
                    .test_support()
                    .role(Role::Label)
                    .aria_label(jobs::progress(job))
                    .child(jobs::progress(job)),
            )
            .child(
                div()
                    .id("sub-jobs")
                    .role(Role::List)
                    .aria_label(text::REPOS_LABEL)
                    .flex()
                    .flex_col()
                    .children(rows),
            )
    }

    fn result(&mut self, job: &JobView, tokens: &Tokens, cx: &mut Context<Self>) -> AnyElement {
        let report = job.report.clone().unwrap_or_default();
        let failed = report.iter().any(|line| line.outcome == Outcome::Failed);
        let counts = outcome_counts(&report);
        let retry = Button::new("report-retry")
            .label(text::RETRY_FAILED)
            .disabled(!failed)
            .tooltip(if failed {
                text::RETRY_TITLE
            } else {
                text::NO_REPO_FAILED
            })
            .on_click(cx.listener({
                let workspace = job.workspace_id.clone();
                let job = job.job_id;
                move |_, _, _, cx| {
                    cx.emit(JobListEvent::Retry {
                        workspace: workspace.clone(),
                        job,
                    });
                }
            }));
        let lines = report
            .iter()
            .enumerate()
            .map(|(at, line)| {
                let expanded = self.expanded.contains(&line.repo_id);
                let outcome = text::outcome(&line.outcome);
                let reason = line
                    .message
                    .clone()
                    .filter(|_| !matches!(line.outcome, Outcome::Skipped { .. }));
                let handles = self.report_handles.get(at);
                let mut row = div()
                    .id(ElementId::Name(format!("report-line-{at}").into()))
                    .test_support()
                    .flex()
                    .flex_row()
                    .flex_wrap()
                    .items_center()
                    .gap_2()
                    .child(
                        Button::new(ElementId::Name(format!("report-toggle-{at}").into()))
                            .label(if expanded { "▾" } else { "▸" })
                            .accessibility_label(text::output_in(&line.repo_name))
                            .when_some(handles.map(|handles| &handles[TOGGLE]), |button, handle| {
                                button.track_focus(handle)
                            })
                            .key_context(TOGGLE_CONTEXT)
                            .on_click(cx.listener({
                                let repo = line.repo_id.clone();
                                move |this, _, _, cx| {
                                    if !this.expanded.insert(repo.clone()) {
                                        this.expanded.remove(&repo);
                                    }
                                    cx.notify();
                                }
                            })),
                    )
                    .child(
                        Button::new(ElementId::Name(format!("report-name-{at}").into()))
                            .label(line.repo_name.clone())
                            .accessibility_label(line.repo_name.clone())
                            .tooltip(text::SELECT_IN_TREE)
                            .when_some(handles.map(|handles| &handles[NAME]), |button, handle| {
                                button.track_focus(handle)
                            })
                            .key_context(NAME_CONTEXT)
                            .on_click(cx.listener({
                                let workspace = job.workspace_id.clone();
                                let repo = line.repo_id.clone();
                                move |_, _, _, cx| {
                                    cx.emit(JobListEvent::SelectRepo {
                                        workspace: workspace.clone(),
                                        repo: repo.clone(),
                                    });
                                }
                            })),
                    )
                    .child(
                        div()
                            .id(ElementId::Name(format!("report-outcome-{at}").into()))
                            .test_support()
                            .role(Role::Label)
                            .aria_label(outcome)
                            .child(outcome_label(tokens, &line.outcome)),
                    )
                    .when_some(reason, |row, reason| {
                        row.child(
                            div()
                                .id(ElementId::Name(format!("report-reason-{at}").into()))
                                .test_support()
                                .role(Role::Paragraph)
                                .aria_label(reason.clone())
                                .text_color(tokens.text_secondary)
                                .child(reason),
                        )
                    });
                if expanded {
                    row = row.child(match line.output.is_empty() {
                        true => div()
                            .id(ElementId::Name(format!("report-output-{at}").into()))
                            .test_support()
                            .role(Role::Label)
                            .aria_label(text::NO_OUTPUT)
                            .text_color(tokens.text_secondary)
                            .child(text::NO_OUTPUT)
                            .into_any_element(),
                        false => div()
                            .id(ElementId::Name(format!("report-output-{at}").into()))
                            .test_support()
                            .role(Role::Document)
                            .aria_label(line.output.clone())
                            .max_h(OUTPUT_HEIGHT)
                            .visible_y_scroll()
                            .child(line.output.clone())
                            .into_any_element(),
                    });
                }
                row.into_any_element()
            })
            .collect::<Vec<_>>();
        div()
            .id("group-report")
            .test_support()
            .on_action(cx.listener(Self::toggle_next))
            .on_action(cx.listener(Self::toggle_previous))
            .on_action(cx.listener(Self::toggle_first))
            .on_action(cx.listener(Self::toggle_last))
            .on_action(cx.listener(Self::name_next))
            .on_action(cx.listener(Self::name_previous))
            .on_action(cx.listener(Self::name_first))
            .on_action(cx.listener(Self::name_last))
            .on_action(cx.listener(Self::report_name_control))
            .on_action(cx.listener(Self::report_toggle_control))
            .flex()
            .flex_col()
            .gap_2()
            .child(
                div()
                    .id("report-header")
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap_3()
                    .child(
                        div()
                            .id("report-counts")
                            .test_support()
                            .role(Role::Label)
                            .aria_label(counts.clone())
                            .child(counts),
                    )
                    .child(retry),
            )
            .child(
                div()
                    .id("report-lines")
                    .test_support()
                    .role(Role::List)
                    .aria_label(text::RESULT_REPORT)
                    .flex()
                    .flex_col()
                    .children(lines),
            )
            .into_any_element()
    }
}

/// The list of a Resolve all with AI job's files.
const AI_FILE_IDS: crate::resolve_all::ListIds = crate::resolve_all::ListIds {
    list: "job-ai-files",
    row: "job-ai-file",
    file: "job-ai-file-name",
    state: "job-ai-file-state",
};

/// The spinner a running sub-job shows.
pub(crate) fn spinner(cx: &App) -> impl IntoElement {
    let color = cx.tokens().text_secondary;
    svg()
        .path(icons::SPINNER)
        .size(px(12.))
        .text_color(color)
        .with_animation(
            "sub-job-spinner",
            Animation::new(Duration::from_secs(1)).repeat(),
            |spinner, delta| spinner.with_transformation(Transformation::rotate(percentage(delta))),
        )
}

/// The job's raw output, or the note that there is none.
/// GRP-7's outcomes, in the order the counts name them.
const OUTCOMES: [u8; 5] = [0, 1, 2, 3, 4];

fn outcome_key(outcome: &Outcome) -> u8 {
    match outcome {
        Outcome::Failed => 0,
        Outcome::Conflict => 1,
        Outcome::Succeeded => 2,
        Outcome::NothingToDo => 3,
        Outcome::Skipped { .. } => 4,
    }
}

/// Counts per outcome, such as `2 failed, 3 succeeded` (section 5, step 4).
pub fn outcome_counts(report: &[gitcroc_core::RepoResult]) -> String {
    OUTCOMES
        .into_iter()
        .filter_map(|key| {
            let n = report
                .iter()
                .filter(|line| outcome_key(&line.outcome) == key)
                .count();
            (n > 0).then(|| format!("{n} {}", name(key)))
        })
        .collect::<Vec<_>>()
        .join(", ")
}

/// An outcome's name in the counts, without a skip's reason.
fn name(key: u8) -> &'static str {
    match key {
        0 => "failed",
        1 => "conflict",
        2 => "succeeded",
        3 => "nothing to do",
        _ => "skipped",
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
fn state_element(job: &JobView, tokens: &Tokens) -> AnyElement {
    match &job.outcome {
        Some(outcome) => outcome_label(tokens, outcome).into_any_element(),
        None => div()
            .text_color(tokens.text_secondary)
            .child(jobs::state(job))
            .into_any_element(),
    }
}

impl Modal for JobList {
    /// A report has the focus on its first line; otherwise the list does.
    fn focus_first(&self, window: &mut Window, cx: &mut App) {
        if let Some(handles) = self.report_handles.first() {
            window.focus(&handles[TOGGLE], cx);
        } else {
            window.focus(&self.list_focus, cx);
        }
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
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.sync_report(cx);
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
        let detail = self.detail(window, cx);
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
                    .child(list)
                    .relative()
                    .vertical_scrollbar(&self.scroll),
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
