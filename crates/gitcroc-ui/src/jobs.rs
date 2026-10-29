//! This run's jobs as the app reports them (ARCH-7, UI-21), a port of
//! `ui/src/stores/jobs.ts`: read with `jobs_get` at start and after events
//! were dropped, then kept current by `JobProgress` and `JobEnded`. One
//! model serves the whole window, as the job list covers every tab. It
//! decides nothing about a job (ARCH-6); besides the mirror it keeps only
//! what the user saw: the job that ended last in each workspace (ARCH-9),
//! which ended jobs the job list has shown, and which the status bar of
//! their own tab has shown. The tab marks only count (UI-32).

use std::collections::{BTreeMap, HashMap, HashSet};
use std::time::Duration;

use chrono::{Local, TimeZone as _, Timelike as _};
use gitcroc_app::app::AppEvent;
use gitcroc_core::{CancelResult, JobId, JobState, JobView, Outcome, WorkspaceId};
use gpui_kit::{Context, Task};

use crate::services::{Call, Services};
use crate::strings::jobs as text;

/// How long the outcome of the job that ended last stays in the status
/// bar.
pub const OUTCOME_SHOWN: Duration = Duration::from_secs(10);

/// What the status bar shows for a workspace.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Status<'a> {
    /// Its running jobs, newest first.
    Running(Vec<&'a JobView>),
    /// The job that ended last, for [`OUTCOME_SHOWN`]; then the newest
    /// failed or conflicted job the job list has not shown.
    Ended(&'a JobView),
    Idle,
}

/// What a workspace's tab shows (UI-32).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Marks {
    /// Its jobs that run or wait.
    pub active: u32,
    /// One of its jobs ended failed or in conflict, and has been shown
    /// neither in the job list nor in the status bar of its tab.
    pub unseen: bool,
}

/// The jobs and what the user saw of them, without the app.
#[derive(Debug, Default)]
pub struct Seen {
    /// Every job by its id's number; ids grow with each job the app
    /// starts, so the last is the newest.
    by_id: BTreeMap<u32, JobView>,
    /// The job whose `JobEnded` arrived last in each workspace, until
    /// [`OUTCOME_SHOWN`] has passed.
    recent: HashMap<WorkspaceId, JobId>,
    /// Ended jobs the job list has shown.
    seen: HashSet<JobId>,
    /// Ended jobs the status bar of their own tab has shown.
    shown: HashSet<JobId>,
}

impl Seen {
    /// A job's progress or end.
    pub fn record(&mut self, view: &JobView) {
        self.by_id.insert(view.job_id.0, view.clone());
    }

    /// A job the app listed; one already heard is as new or newer.
    pub fn listed(&mut self, view: JobView) {
        self.by_id.entry(view.job_id.0).or_insert(view);
    }

    /// `view` ended: its outcome shows in its tab's status bar until
    /// [`Self::forget_recent`].
    pub fn ended(&mut self, view: &JobView) {
        self.record(view);
        self.recent.insert(view.workspace_id.clone(), view.job_id);
    }

    /// [`OUTCOME_SHOWN`] has passed since `job` ended; a newer end stays.
    pub fn forget_recent(&mut self, workspace: &WorkspaceId, job: JobId) {
        if self.recent.get(workspace) == Some(&job) {
            self.recent.remove(workspace);
        }
    }

    /// Every job, newest first.
    pub fn jobs(&self) -> impl DoubleEndedIterator<Item = &JobView> + ExactSizeIterator {
        self.by_id.values().rev()
    }

    pub fn get(&self, id: JobId) -> Option<&JobView> {
        self.by_id.get(&id.0)
    }

    /// What workspace `id`'s status bar shows.
    pub fn status(&self, id: &WorkspaceId) -> Status<'_> {
        let running: Vec<&JobView> = self
            .jobs()
            .filter(|job| &job.workspace_id == id && job.state == JobState::Running)
            .collect();
        if !running.is_empty() {
            return Status::Running(running);
        }
        let recent = self.recent.get(id).and_then(|job| self.by_id.get(&job.0));
        let unseen = || {
            self.jobs().find(|job| {
                &job.workspace_id == id && stops(job) && !self.seen.contains(&job.job_id)
            })
        };
        recent.or_else(unseen).map_or(Status::Idle, Status::Ended)
    }

    /// The status bar of `id`'s tab is on screen: the ended job it shows is
    /// shown, for the tab's marker.
    pub fn show_status(&mut self, id: &WorkspaceId) {
        if let Status::Ended(job) = self.status(id) {
            let job = job.job_id;
            self.shown.insert(job);
        }
    }

    /// The job list was opened: every job that has ended is seen.
    pub fn see_all(&mut self) {
        let ended: Vec<JobId> = self
            .jobs()
            .filter(|job| job.state == JobState::Ended)
            .map(|job| job.job_id)
            .collect();
        self.seen.extend(ended);
    }

    /// What workspace `id`'s tab shows.
    pub fn marks(&self, id: &WorkspaceId) -> Marks {
        let mine = || self.jobs().filter(|job| &job.workspace_id == id);
        Marks {
            active: mine()
                .filter(|job| job.state != JobState::Ended)
                .count()
                .try_into()
                .unwrap_or(u32::MAX),
            unseen: mine().any(|job| {
                stops(job) && !self.seen.contains(&job.job_id) && !self.shown.contains(&job.job_id)
            }),
        }
    }
}

/// A job that ended *failed* or in *conflict*: it stays until seen.
fn stops(job: &JobView) -> bool {
    matches!(job.outcome, Some(Outcome::Failed | Outcome::Conflict))
}

/// The window's jobs.
pub struct Jobs {
    services: Services,
    seen: Seen,
    /// Each workspace's wait to forget its recent job; a newer end replaces
    /// it.
    timers: HashMap<WorkspaceId, Task<()>>,
    /// The tab whose status bar is on screen.
    active: Option<WorkspaceId>,
}

impl Jobs {
    pub fn new(services: Services, cx: &mut Context<Self>) -> Self {
        let mut this = Self {
            services,
            seen: Seen::default(),
            timers: HashMap::new(),
            active: None,
        };
        this.read_again(cx);
        this
    }

    /// Reads this run's jobs. An event that arrived meanwhile is as new as
    /// the answer or newer, so a job already known is kept.
    pub fn read_again(&mut self, cx: &mut Context<Self>) {
        let call = self.services.with_app(|app| async move { app.jobs_get() });
        cx.spawn(async move |this, cx| {
            if let Some(views) = call.await {
                let _ = this.update(cx, |this, cx| {
                    for view in views {
                        this.seen.listed(view);
                    }
                    this.show_status();
                    cx.notify();
                });
            }
        })
        .detach();
    }

    pub fn on_event(&mut self, event: &AppEvent, cx: &mut Context<Self>) {
        match event {
            AppEvent::JobProgress(view) => self.seen.record(view),
            AppEvent::JobEnded(view) => {
                self.seen.ended(view);
                self.forget_later(view.workspace_id.clone(), view.job_id, cx);
            }
            _ => return,
        }
        self.show_status();
        cx.notify();
    }

    /// Forgets `job` as `workspace`'s recent end once [`OUTCOME_SHOWN`] has
    /// passed.
    fn forget_later(&mut self, workspace: WorkspaceId, job: JobId, cx: &mut Context<Self>) {
        let timer = cx.background_executor().timer(OUTCOME_SHOWN);
        let wait = cx.spawn({
            let workspace = workspace.clone();
            async move |this, cx| {
                timer.await;
                let _ = this.update(cx, |this, cx| {
                    this.seen.forget_recent(&workspace, job);
                    this.timers.remove(&workspace);
                    this.show_status();
                    cx.notify();
                });
            }
        });
        self.timers.insert(workspace, wait);
    }

    /// The tab `active` is shown, with its status bar.
    pub fn set_active(&mut self, active: Option<WorkspaceId>, cx: &mut Context<Self>) {
        if self.active != active {
            self.active = active;
            self.show_status();
            cx.notify();
        }
    }

    fn show_status(&mut self) {
        if let Some(active) = &self.active {
            self.seen.show_status(active);
        }
    }

    /// The job list was opened.
    pub fn see_all(&mut self, cx: &mut Context<Self>) {
        self.seen.see_all();
        cx.notify();
    }

    pub fn active(&self) -> Option<&WorkspaceId> {
        self.active.as_ref()
    }

    /// Every job, newest first.
    pub fn jobs(&self) -> impl DoubleEndedIterator<Item = &JobView> + ExactSizeIterator {
        self.seen.jobs()
    }

    pub fn len(&self) -> usize {
        self.seen.by_id.len()
    }

    pub fn is_empty(&self) -> bool {
        self.seen.by_id.is_empty()
    }

    pub fn get(&self, id: JobId) -> Option<&JobView> {
        self.seen.get(id)
    }

    /// What workspace `id`'s status bar shows.
    pub fn status(&self, id: &WorkspaceId) -> Status<'_> {
        self.seen.status(id)
    }

    /// What workspace `id`'s tab shows (UI-32).
    pub fn marks(&self, id: &WorkspaceId) -> Marks {
        self.seen.marks(id)
    }

    /// Asks the app to cancel a job (GRP-10); its end arrives as
    /// `JobEnded`.
    pub fn cancel(&self, id: JobId) -> Call<CancelResult> {
        self.services
            .with_app(move |app| async move { app.job_cancel(id) })
    }
}

/// Running progress: the phase git names, then `done/total`, with what a
/// group job or Resolve all with AI counts; "Running" while the total is
/// not known.
pub fn progress(job: &JobView) -> String {
    let count = if job.total == 0 {
        text::RUNNING.to_owned()
    } else if !job.sub_jobs.is_empty() {
        format!("{}/{} {}", job.done, job.total, text::REPOS)
    } else if !job.ai_files.is_empty() {
        format!("{}/{} {}", job.done, job.total, text::FILES)
    } else {
        format!("{}/{}", job.done, job.total)
    };
    match &job.step {
        Some(step) => format!("{step} {count}"),
        None => count,
    }
}

/// Where a job is: its outcome, what it waits for, or its progress.
pub fn state(job: &JobView) -> String {
    match (&job.outcome, job.state) {
        (Some(outcome), _) => text::outcome(outcome),
        (None, JobState::Queued) => text::waiting_for(&job.repo_names.join(", ")),
        (None, _) => progress(job),
    }
}

/// A start time as the local time of day.
pub fn format_start(epoch_ms: Option<u64>) -> String {
    let local = epoch_ms
        .and_then(|ms| i64::try_from(ms).ok())
        .and_then(|ms| Local.timestamp_millis_opt(ms).single());
    local.map_or_else(String::new, |time| {
        format!(
            "{:02}:{:02}:{:02}",
            time.hour(),
            time.minute(),
            time.second()
        )
    })
}

/// A duration as `850 ms`, `4.2 s` or `3 min 5 s`.
pub fn format_duration(ms: Option<u32>) -> String {
    match ms {
        None => String::new(),
        Some(ms) if ms < 1_000 => text::milliseconds(ms),
        Some(ms) if ms < 60_000 => text::seconds(&format!("{:.1}", f64::from(ms) / 1_000.)),
        Some(ms) => {
            let seconds = (ms + 500) / 1_000;
            text::minutes(seconds / 60, seconds % 60)
        }
    }
}

#[cfg(test)]
mod tests {
    use gitcroc_core::{AiFileProgress, AiFileState, RepoId, SkipReason, SubJob};

    use super::*;

    fn job(id: u32, workspace: &str, state: JobState, outcome: Option<Outcome>) -> JobView {
        JobView {
            job_id: JobId(id),
            workspace_id: WorkspaceId(workspace.into()),
            workspace_name: workspace.into(),
            label: format!("Job {id}"),
            repos: Vec::new(),
            repo_names: vec!["api".into(), "web".into()],
            state,
            done: 0,
            total: 0,
            step: None,
            started_at: None,
            duration_ms: None,
            outcome,
            output: String::new(),
            message: None,
            sub_jobs: Vec::new(),
            report: None,
            ai_files: Vec::new(),
        }
    }

    fn ended(id: u32, workspace: &str, outcome: Outcome) -> JobView {
        job(id, workspace, JobState::Ended, Some(outcome))
    }

    fn ids(status: &Status<'_>) -> Vec<u32> {
        match status {
            Status::Running(jobs) => jobs.iter().map(|job| job.job_id.0).collect(),
            Status::Ended(job) => vec![job.job_id.0],
            Status::Idle => Vec::new(),
        }
    }

    #[test]
    fn ui_21_progress_and_state_text() {
        let mut running = job(1, "w", JobState::Running, None);
        assert_eq!(progress(&running), "Running");
        running.total = 40;
        running.done = 3;
        running.step = Some("Receiving objects".into());
        assert_eq!(progress(&running), "Receiving objects 3/40");
        running.step = None;
        running.sub_jobs = vec![SubJob {
            repo_id: RepoId("r1".into()),
            repo_name: "api".into(),
            state: JobState::Running,
            outcome: None,
        }];
        assert_eq!(progress(&running), "3/40 repos");
        running.sub_jobs.clear();
        running.ai_files = vec![AiFileProgress {
            path: "a.txt".into(),
            state: AiFileState::Waiting,
        }];
        assert_eq!(progress(&running), "3/40 files");

        let queued = job(2, "w", JobState::Queued, None);
        assert_eq!(state(&queued), "waiting for api, web");
        let skipped = ended(
            3,
            "w",
            Outcome::Skipped {
                reason: SkipReason::Cancelled,
            },
        );
        assert_eq!(state(&skipped), "Skipped (cancelled)");
        assert_eq!(state(&ended(4, "w", Outcome::Failed)), "Failed");
    }

    #[test]
    fn ui_21_start_and_duration_text() {
        assert_eq!(format_duration(None), "");
        assert_eq!(format_duration(Some(850)), "850 ms");
        assert_eq!(format_duration(Some(4_200)), "4.2 s");
        assert_eq!(format_duration(Some(185_000)), "3 min 5 s");
        assert_eq!(format_start(None), "");
        let start = format_start(Some(1_700_000_000_000));
        assert_eq!(start.len(), 8, "{start}");
        assert_eq!(start.matches(':').count(), 2, "{start}");
    }

    #[test]
    fn ui_21_which_job_the_status_bar_shows() {
        let w = WorkspaceId("w".into());
        let mut seen = Seen::default();
        assert_eq!(seen.status(&w), Status::Idle);
        // Listed at start, not heard ending: not shown.
        seen.listed(ended(1, "w", Outcome::Succeeded));
        assert_eq!(seen.status(&w), Status::Idle);

        seen.record(&job(2, "w", JobState::Running, None));
        seen.record(&job(3, "w", JobState::Running, None));
        seen.record(&job(4, "other", JobState::Running, None));
        seen.record(&job(5, "w", JobState::Queued, None));
        assert_eq!(ids(&seen.status(&w)), [3, 2]);

        seen.ended(&ended(2, "w", Outcome::Failed));
        seen.ended(&ended(3, "w", Outcome::Succeeded));
        assert_eq!(ids(&seen.status(&w)), [3]);
        // The time passed: the failure not seen is next.
        seen.forget_recent(&w, JobId(3));
        assert_eq!(ids(&seen.status(&w)), [2]);
        seen.see_all();
        assert_eq!(seen.status(&w), Status::Idle);

        // Only the newest end is forgotten when its time passes.
        seen.ended(&ended(6, "w", Outcome::Succeeded));
        seen.ended(&ended(7, "w", Outcome::NothingToDo));
        seen.forget_recent(&w, JobId(6));
        assert_eq!(ids(&seen.status(&w)), [7]);
    }

    #[test]
    fn ui_32_tab_marks() {
        let w = WorkspaceId("w".into());
        let mut seen = Seen::default();
        seen.record(&job(1, "w", JobState::Running, None));
        seen.record(&job(2, "w", JobState::Queued, None));
        seen.record(&job(3, "other", JobState::Running, None));
        assert_eq!(
            seen.marks(&w),
            Marks {
                active: 2,
                unseen: false
            }
        );
        seen.ended(&ended(1, "w", Outcome::Conflict));
        seen.ended(&ended(2, "w", Outcome::Succeeded));
        assert_eq!(
            seen.marks(&w),
            Marks {
                active: 0,
                unseen: true
            }
        );
        // Its own tab's status bar shows the newest end, then the conflict.
        seen.show_status(&w);
        assert!(seen.marks(&w).unseen);
        seen.forget_recent(&w, JobId(2));
        seen.show_status(&w);
        assert!(!seen.marks(&w).unseen);

        seen.ended(&ended(4, "w", Outcome::Failed));
        assert!(seen.marks(&w).unseen);
        seen.see_all();
        assert!(!seen.marks(&w).unseen);
    }
}
