//! The job engine (ARCH-7): every repo write runs as a job, with progress
//! events, one outcome of GRP-7's vocabulary and git's output, and the
//! repos it touched read again when it ends (OPS-7).
//!
//! At most one write runs per repo (ARCH-4): a write on a busy repo waits
//! in that repo's queue, in the order writes were started. Reads never wait
//! for a write. Running jobs and the background refresh share one pool of
//! workers, jobs first (PERF-4).

mod group;
mod pool;

use std::collections::{HashMap, VecDeque};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, OnceLock};
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use gitcroc_git::{
    Askpass, GitError, GitRepo, GitRunner, Kill, Output, RefSnapshot, StatusRead, StatusReader,
    Write,
};
use serde::{Deserialize, Serialize};
use tokio::sync::oneshot;

use crate::lock;
use crate::{RepoId, Summaries, SummaryView, WorkspaceError, Workspaces};

pub use group::{GroupRun, SubJobSpec};
pub use pool::{POOL_SIZE, Pool, Priority, Worker};

/// Identifies a workspace. All session state is keyed by it (ARCH-9).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, specta::Type)]
pub struct WorkspaceId(pub String);

/// Identifies a job, unique while the app runs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, specta::Type)]
pub struct JobId(pub u32);

/// Hands out job ids for the whole app, starting at 1.
#[derive(Debug, Default)]
pub struct JobIds(AtomicU32);

impl JobIds {
    pub fn next(&self) -> JobId {
        JobId(self.0.fetch_add(1, Ordering::Relaxed).wrapping_add(1))
    }
}

/// A step of adding repos: `done` of `total` steps. A `total` of 0 means
/// the number of steps is not known yet.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
pub struct JobProgress {
    pub job_id: JobId,
    pub workspace_id: WorkspaceId,
    pub label: String,
    pub done: u32,
    pub total: u32,
}

/// How a job ended (GRP-7). *Conflict* is not a failure: git left the repo
/// in conflict state (OPS-1).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Outcome {
    Succeeded,
    NothingToDo,
    Skipped { reason: SkipReason },
    Failed,
    Conflict,
}

/// Why a job, or a repo of a group operation, was skipped.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum SkipReason {
    /// Cancelled before it started, or killed while it ran (GRP-10).
    Cancelled,
    /// The preflight classed the repo *will skip* for the operation.
    Preflight { reason: String },
    /// The repo was blocked (GRP-8), at the preflight or when its sub-job
    /// was to start.
    Blocked { reason: String },
}

/// Where a job is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "snake_case")]
pub enum JobState {
    /// Waiting for its repos or for a worker.
    Queued,
    Running,
    Ended,
}

/// A job as the status bar and the job list show it (UI-21).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
pub struct JobView {
    pub job_id: JobId,
    pub workspace_id: WorkspaceId,
    /// The workspace's name when the job was started.
    pub workspace_name: String,
    pub label: String,
    pub repos: Vec<RepoId>,
    /// The names of `repos`, in their order, when the job was started.
    pub repo_names: Vec<String>,
    pub state: JobState,
    /// How far it has come: `done` of `total` steps. A `total` of 0 means
    /// the number of steps is not known yet.
    pub done: u32,
    pub total: u32,
    /// The phase `done` and `total` count, as git names it, such as
    /// `Receiving objects`; `None` when the job reports no phases.
    pub step: Option<String>,
    /// When it began to run, in milliseconds since the Unix epoch.
    #[specta(type = Option<f64>)]
    pub started_at: Option<u64>,
    /// How long it ran, in milliseconds; set when it ended.
    pub duration_ms: Option<u32>,
    /// Set when the job has ended.
    pub outcome: Option<Outcome>,
    /// What git printed, stdout then stderr, progress lines folded to their
    /// last state; at most [`MAX_OUTPUT`] bytes, the end kept.
    pub output: String,
    /// Why a failed or skipped job did not succeed.
    pub message: Option<String>,
    /// A group operation's sub-jobs, one per repo it runs on, in target-set
    /// order, as they are now; empty for any other job.
    pub sub_jobs: Vec<SubJob>,
    /// A group operation's result report, set when it ended: every repo of
    /// its target set, in target-set order, with one outcome (GRP-7).
    pub report: Option<Vec<RepoResult>>,
}

/// A sub-job of a group operation: its repo and where it is.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
pub struct SubJob {
    pub repo_id: RepoId,
    pub repo_name: String,
    pub state: JobState,
    /// Set when it ended.
    pub outcome: Option<Outcome>,
}

/// One repo's line of a result report (section 3 "Result report").
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
pub struct RepoResult {
    pub repo_id: RepoId,
    pub repo_name: String,
    pub outcome: Outcome,
    /// What git printed, as [`JobView::output`].
    pub output: String,
    /// Why it failed, as [`JobView::message`].
    pub message: Option<String>,
}

/// The most output a job keeps, in bytes.
pub const MAX_OUTPUT: usize = 64 * 1024;

/// The most ended jobs kept for the job list; older ones are dropped.
pub const MAX_ENDED_JOBS: usize = 500;

/// What happens to a running job when it is cancelled (GRP-10).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OnCancel {
    /// Its git process is killed: fetch and clone.
    Kill,
    /// It runs to its end.
    Finish,
}

/// When a write that succeeded did nothing, decided by the operation from
/// the refs and `HEAD` before and after, never from git's wording.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NothingToDo {
    /// It always did something when git succeeded.
    Never,
    /// It did nothing when no ref and not `HEAD` changed.
    WhenRefsUnchanged,
}

/// A write to run as a job.
#[derive(Debug, Clone)]
pub struct WriteJob {
    pub workspace_id: WorkspaceId,
    /// The repos it writes to, each waiting its turn (ARCH-4).
    pub repos: Vec<RepoId>,
    pub label: String,
    pub on_cancel: OnCancel,
    /// Git may stop in conflict state, which then ends the job as
    /// *conflict* (OPS-1).
    pub may_conflict: bool,
    pub nothing_to_do: NothingToDo,
    /// A network write (fetch, pull, push, clone, remote branch delete,
    /// tag push): git gets the askpass helper for its prompts (D10).
    pub network: bool,
}

impl WriteJob {
    /// A local write to `repo` that runs to its end when cancelled, cannot
    /// stop in conflict state and always does something when git succeeds.
    pub fn single(workspace_id: WorkspaceId, repo: RepoId, label: String) -> Self {
        Self {
            workspace_id,
            repos: vec![repo],
            label,
            on_cancel: OnCancel::Finish,
            may_conflict: false,
            nothing_to_do: NothingToDo::Never,
            network: false,
        }
    }
}

/// Lets the prompts of one network job reach the user (D10): the app's
/// askpass helper, ready for that job.
pub trait AskpassIssuer: Send + Sync {
    /// Askpass for job `job`, valid until the grant is dropped; `None` when
    /// prompts cannot be asked, and git then fails as it would without.
    fn issue(&self, job: &JobView) -> Option<AskpassGrant>;
}

/// Askpass for one job. Dropped, it is revoked: the helper's prompts for
/// the job get no answer from then on.
pub struct AskpassGrant {
    askpass: Askpass,
    revoke: Option<Box<dyn FnOnce() + Send + Sync>>,
}

impl AskpassGrant {
    pub fn new(askpass: Askpass, revoke: impl FnOnce() + Send + Sync + 'static) -> Self {
        Self {
            askpass,
            revoke: Some(Box::new(revoke)),
        }
    }

    pub fn askpass(&self) -> &Askpass {
        &self.askpass
    }
}

impl std::fmt::Debug for AskpassGrant {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AskpassGrant")
            .field("askpass", &self.askpass)
            .finish_non_exhaustive()
    }
}

impl Drop for AskpassGrant {
    fn drop(&mut self) {
        if let Some(revoke) = self.revoke.take() {
            revoke();
        }
    }
}

type ProgressFn = dyn Fn(Option<&str>, u32, u32) + Send + Sync;

/// What a running job's write gets: its repos' paths, in the order of
/// [`WriteJob::repos`], git, the kill request and progress.
#[derive(Clone)]
pub struct JobRun {
    pub job_id: JobId,
    pub paths: Vec<PathBuf>,
    pub git: GitRunner,
    kill: Kill,
    /// Set for a network write while it runs (D10).
    askpass: Option<Askpass>,
    progress: Arc<ProgressFn>,
}

impl std::fmt::Debug for JobRun {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("JobRun")
            .field("job_id", &self.job_id)
            .field("paths", &self.paths)
            .finish_non_exhaustive()
    }
}

impl JobRun {
    /// How the runner runs this job's git: killed on cancel when the job
    /// says so.
    pub fn write(&self) -> Write<'_> {
        Write {
            kill: Some(&self.kill),
            askpass: self.askpass.as_ref(),
            ..Write::default()
        }
    }

    /// Reports `done` of `total` steps.
    pub fn progress(&self, done: u32, total: u32) {
        (self.progress)(None, done, total);
    }

    /// Reports `done` of `total` in phase `step`, such as a transfer's
    /// `Receiving objects`; a `total` of 0 when the phase gives none.
    pub fn step(&self, step: &str, done: u32, total: u32) {
        (self.progress)(Some(step), done, total);
    }
}

/// What the engine tells the app.
pub trait JobEvents: Send + Sync {
    /// A job was queued, began to run, or made progress.
    fn progress(&self, job: &JobView);
    /// A job ended; sent after its repos were read again.
    fn ended(&self, job: &JobView);
    /// A repo's summary changed: read again (`read`), or only its busy flag.
    fn summary(&self, view: SummaryView, read: bool);
    /// A write starts (`held`) or ended on a repo. Its re-read replaces the
    /// watcher's refresh for the same change (OPS-7).
    fn hold_watch(&self, workspace: &WorkspaceId, repo: &RepoId, held: bool) {
        let _ = (workspace, repo, held);
    }
}

/// Why a job could not start.
#[derive(Debug, thiserror::Error)]
pub enum JobError {
    #[error(transparent)]
    Workspace(#[from] WorkspaceError),
    #[error("a job needs a repo")]
    NoRepo,
}

/// A started job: its id, and its view once it ended.
#[derive(Debug)]
pub struct JobHandle {
    pub job_id: JobId,
    pub ended: oneshot::Receiver<JobView>,
}

/// What [`Jobs::cancel`] did.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "snake_case")]
pub enum CancelResult {
    /// It had not started and never will.
    Cancelled,
    /// Its git process is being killed.
    Killing,
    /// It runs to its end (GRP-10).
    Finishing,
    /// It has ended, or never existed.
    Ended,
}

/// One job's place in the engine.
struct Entry {
    view: JobView,
    /// When it began to run.
    began: Option<Instant>,
    kill: Kill,
    on_cancel: OnCancel,
    /// Wakes the job when it is first in all its repos' queues.
    turn: Option<oneshot::Sender<()>>,
    /// A group operation's request to start no more sub-jobs (GRP-10).
    stop: Option<Kill>,
}

#[derive(Default)]
struct State {
    /// This run's jobs, oldest first.
    jobs: VecDeque<Entry>,
    /// Each repo's writes, the running one first (ARCH-4).
    queues: HashMap<(WorkspaceId, RepoId), VecDeque<JobId>>,
}

impl State {
    fn entry(&mut self, id: JobId) -> Option<&mut Entry> {
        self.jobs.iter_mut().find(|entry| entry.view.job_id == id)
    }

    /// Whether job `id` on `repos` is first in all their queues.
    fn first(&self, workspace: &WorkspaceId, repos: &[RepoId], id: JobId) -> bool {
        repos.iter().all(|repo| {
            self.queues
                .get(&(workspace.clone(), repo.clone()))
                .and_then(VecDeque::front)
                == Some(&id)
        })
    }
}

/// The engine of the whole app.
pub struct Jobs {
    ids: JobIds,
    workspaces: Arc<Workspaces>,
    summaries: Arc<Summaries>,
    events: Arc<dyn JobEvents>,
    askpass: OnceLock<Arc<dyn AskpassIssuer>>,
    state: Mutex<State>,
}

impl std::fmt::Debug for Jobs {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Jobs").finish_non_exhaustive()
    }
}

/// What a repo looked like before or after a write.
#[derive(Debug, Default)]
struct Seen {
    refs: Option<RefSnapshot>,
    conflict: bool,
}

impl Jobs {
    /// The engine. Jobs run in the workers of `summaries`' pool, which the
    /// background refresh shares.
    pub fn new(
        workspaces: Arc<Workspaces>,
        summaries: Arc<Summaries>,
        events: Arc<dyn JobEvents>,
    ) -> Arc<Self> {
        Arc::new(Self {
            ids: JobIds::default(),
            workspaces,
            summaries,
            events,
            askpass: OnceLock::new(),
            state: Mutex::new(State::default()),
        })
    }

    /// Network writes from now on send git's prompts through `issuer`
    /// (D10). Set once; returns `false` when it was set already.
    pub fn use_askpass(&self, issuer: Arc<dyn AskpassIssuer>) -> bool {
        self.askpass.set(issuer).is_ok()
    }

    /// A new job id, for jobs that are not writes, such as adding repos.
    pub fn next_id(&self) -> JobId {
        self.ids.next()
    }

    fn lock(&self) -> MutexGuard<'_, State> {
        // Each change is a push, a removal or a field set, so a panic
        // leaves no half-changed state.
        lock(&self.state)
    }

    /// Starts `job`: it waits for its repos (ARCH-4) and a worker, then runs
    /// `run`, which must run git through [`gitcroc_git::ops`] with
    /// [`JobRun::write`]. Returns at once. Needs a tokio runtime.
    pub fn start<F, Fut>(
        self: &Arc<Self>,
        git: GitRunner,
        job: WriteJob,
        run: F,
    ) -> Result<JobHandle, JobError>
    where
        F: FnOnce(JobRun) -> Fut + Send + 'static,
        Fut: Future<Output = Result<Output, GitError>> + Send + 'static,
    {
        if job.repos.is_empty() {
            return Err(JobError::NoRepo);
        }
        let paths = job
            .repos
            .iter()
            .map(|repo| self.workspaces.repo_path(&job.workspace_id, repo))
            .collect::<Result<Vec<_>, _>>()?;
        let id = self.ids.next();
        let view = self.new_view(id, &job.workspace_id, &job.label, &job.repos)?;
        let kill = Kill::new();
        let (turn, my_turn) = oneshot::channel();
        let first = {
            let mut state = self.lock();
            for repo in &job.repos {
                state
                    .queues
                    .entry((job.workspace_id.clone(), repo.clone()))
                    .or_default()
                    .push_back(id);
            }
            let first = state.first(&job.workspace_id, &job.repos, id);
            state.jobs.push_back(Entry {
                view: view.clone(),
                began: None,
                kill: kill.clone(),
                on_cancel: job.on_cancel,
                turn: (!first).then_some(turn),
                stop: None,
            });
            first
        };
        self.events.progress(&view);
        for repo in &job.repos {
            let view = self.summaries.set_busy(&job.workspace_id, repo, true);
            self.events.summary(view, false);
        }
        let (ended, receiver) = oneshot::channel();
        let engine = self.clone();
        tokio::spawn(async move {
            let my_turn = (!first).then_some(my_turn);
            let view = engine.drive(id, job, paths, git, kill, my_turn, run).await;
            // The caller may not wait for the end.
            let _ = ended.send(view);
        });
        Ok(JobHandle {
            job_id: id,
            ended: receiver,
        })
    }

    /// A queued job's view: the workspace's and repos' names as they are
    /// now, so the job list keeps them when a repo is removed.
    fn new_view(
        &self,
        id: JobId,
        workspace: &WorkspaceId,
        label: &str,
        repos: &[RepoId],
    ) -> Result<JobView, WorkspaceError> {
        let current = self.workspaces.view(workspace)?;
        let repo_names = repos
            .iter()
            .map(|repo| {
                current
                    .repos
                    .iter()
                    .find(|known| known.id == *repo)
                    .map_or_else(|| repo.0.clone(), |known| known.name.clone())
            })
            .collect();
        Ok(JobView {
            job_id: id,
            workspace_id: workspace.clone(),
            workspace_name: current.name,
            label: label.to_owned(),
            repos: repos.to_owned(),
            repo_names,
            state: JobState::Queued,
            done: 0,
            total: 0,
            step: None,
            started_at: None,
            duration_ms: None,
            outcome: None,
            output: String::new(),
            message: None,
            sub_jobs: Vec::new(),
            report: None,
        })
    }

    /// Tracks a job that is not a repo write, such as adding repos: it runs
    /// at once, outside the repo queues, and ends with [`TrackedJob::end`].
    pub fn track(
        self: &Arc<Self>,
        workspace: &WorkspaceId,
        label: &str,
    ) -> Result<TrackedJob, JobError> {
        self.track_as(workspace, label, OnCancel::Finish, false)
    }

    /// Tracks a network write that has no repo yet, a clone: like
    /// [`Self::track`], but cancel kills its git process (GRP-10), and git
    /// gets the askpass helper for its prompts (D10). Its git runs with
    /// [`TrackedJob::write`].
    pub fn track_network(
        self: &Arc<Self>,
        workspace: &WorkspaceId,
        label: &str,
    ) -> Result<TrackedJob, JobError> {
        self.track_as(workspace, label, OnCancel::Kill, true)
    }

    fn track_as(
        self: &Arc<Self>,
        workspace: &WorkspaceId,
        label: &str,
        on_cancel: OnCancel,
        network: bool,
    ) -> Result<TrackedJob, JobError> {
        let id = self.ids.next();
        let view = self.new_view(id, workspace, label, &[])?;
        let kill = Kill::new();
        self.lock().jobs.push_back(Entry {
            view,
            began: None,
            kill: kill.clone(),
            on_cancel,
            turn: None,
            stop: None,
        });
        let begun = self.begin(id);
        if let Some(view) = &begun {
            self.events.progress(view);
        }
        let grant = begun
            .filter(|_| network)
            .and_then(|view| self.askpass.get()?.issue(&view));
        Ok(TrackedJob {
            jobs: self.clone(),
            id,
            kill,
            grant,
            ended: false,
        })
    }

    /// Sets job `id`'s progress and sends its view when it changed.
    fn set_progress(&self, id: JobId, step: Option<&str>, done: u32, total: u32) {
        let view = {
            let mut state = self.lock();
            let Some(entry) = state.entry(id) else {
                return;
            };
            let unchanged = entry.view.done == done
                && entry.view.total == total
                && entry.view.step.as_deref() == step;
            if entry.view.state != JobState::Running || unchanged {
                return;
            }
            entry.view.done = done;
            entry.view.total = total;
            entry.view.step = step.map(str::to_owned);
            entry.view.clone()
        };
        self.events.progress(&view);
    }

    /// This run's jobs of every workspace, newest first, for the job list
    /// (UI-21).
    pub fn all_views(&self) -> Vec<JobView> {
        self.lock()
            .jobs
            .iter()
            .rev()
            .map(|entry| entry.view.clone())
            .collect()
    }

    /// This run's jobs of workspace `workspace`, newest first.
    pub fn views(&self, workspace: &WorkspaceId) -> Vec<JobView> {
        self.lock()
            .jobs
            .iter()
            .rev()
            .filter(|entry| entry.view.workspace_id == *workspace)
            .map(|entry| entry.view.clone())
            .collect()
    }

    /// Cancels job `id`: a job that has not started never will; a running
    /// fetch or clone is killed; any other running job runs to its end
    /// (GRP-10).
    pub fn cancel(&self, id: JobId) -> CancelResult {
        let mut state = self.lock();
        let Some(entry) = state.entry(id) else {
            return CancelResult::Ended;
        };
        if let Some(stop) = &entry.stop
            && entry.view.state != JobState::Ended
        {
            stop.kill();
        }
        match (entry.view.state, entry.on_cancel) {
            (JobState::Ended, _) => CancelResult::Ended,
            (JobState::Queued, _) => {
                entry.kill.kill();
                CancelResult::Cancelled
            }
            (JobState::Running, OnCancel::Kill) => {
                entry.kill.kill();
                CancelResult::Killing
            }
            (JobState::Running, OnCancel::Finish) => CancelResult::Finishing,
        }
    }

    /// Runs job `id` from its turn to its end.
    #[allow(clippy::too_many_arguments)]
    async fn drive<F, Fut>(
        self: &Arc<Self>,
        id: JobId,
        job: WriteJob,
        paths: Vec<PathBuf>,
        git: GitRunner,
        kill: Kill,
        my_turn: Option<oneshot::Receiver<()>>,
        run: F,
    ) -> JobView
    where
        F: FnOnce(JobRun) -> Fut,
        Fut: Future<Output = Result<Output, GitError>>,
    {
        let ready = async {
            if let Some(my_turn) = my_turn {
                // The sender goes only with the job's entry, after its end.
                let _ = my_turn.await;
            }
            self.summaries.pool().worker(Priority::Job).await
        };
        let worker = tokio::select! {
            biased;
            () = kill.requested() => None,
            worker = ready => Some(worker),
        };
        let begun = worker.and_then(|worker| self.begin(id).map(|view| (worker, view)));
        let Some((_worker, view)) = begun else {
            return self.end(
                &job,
                id,
                cancelled(),
                String::new(),
                Some("cancelled".to_owned()),
            );
        };
        self.events.progress(&view);
        for repo in &job.repos {
            self.events.hold_watch(&job.workspace_id, repo, true);
        }
        let before = see(&paths, &git, job.may_conflict).await;
        let grant = job
            .network
            .then(|| self.askpass.get())
            .flatten()
            .and_then(|issuer| issuer.issue(&view));
        let progress = {
            let engine = self.clone();
            Arc::new(move |step: Option<&str>, done, total| {
                engine.set_progress(id, step, done, total);
            })
        };
        let result = run(JobRun {
            job_id: id,
            paths: paths.clone(),
            git: git.clone(),
            kill: match job.on_cancel {
                OnCancel::Kill => kill,
                // Never requested: the job runs to its end.
                OnCancel::Finish => Kill::new(),
            },
            askpass: grant.as_ref().map(|grant| grant.askpass().clone()),
            progress,
        })
        .await;
        // Revoked: a prompt still open for the job gets no answer.
        drop(grant);
        let after = see(&paths, &git, job.may_conflict).await;
        let (outcome, output, message) = judge(
            job.may_conflict,
            job.nothing_to_do,
            &result,
            &before,
            &after,
        );
        // OPS-7: the repos it touched, read again at once.
        for (index, repo) in job.repos.iter().enumerate() {
            self.reread(
                &job.workspace_id,
                repo,
                &git,
                before.get(index),
                after.get(index),
            )
            .await;
        }
        self.end(&job, id, outcome, output, message)
    }

    /// Reads `repo` again at once after a write that saw it as `before` and
    /// `after` (OPS-7), sends its summary, and lets the watcher refresh it
    /// again.
    async fn reread(
        &self,
        workspace: &WorkspaceId,
        repo: &RepoId,
        git: &GitRunner,
        before: Option<&Seen>,
        after: Option<&Seen>,
    ) {
        let refs_changed = match (before, after) {
            (
                Some(Seen {
                    refs: Some(old), ..
                }),
                Some(Seen {
                    refs: Some(new), ..
                }),
            ) => old != new,
            _ => true,
        };
        let (summaries, workspaces, workspace_id, repo_id, git) = (
            self.summaries.clone(),
            self.workspaces.clone(),
            workspace.clone(),
            repo.clone(),
            git.clone(),
        );
        let view = tokio::task::spawn_blocking(move || {
            summaries.reread(&workspaces, &workspace_id, &repo_id, &git, refs_changed)
        })
        .await;
        if let Ok(Some(view)) = view {
            self.events.summary(view, true);
        }
        self.events.hold_watch(workspace, repo, false);
    }

    /// Marks job `id` running, unless it was cancelled meanwhile, and
    /// returns its view.
    fn begin(&self, id: JobId) -> Option<JobView> {
        let mut state = self.lock();
        let entry = state.entry(id)?;
        if entry.kill.is_requested() {
            return None;
        }
        entry.view.state = JobState::Running;
        entry.view.started_at = Some(epoch_ms(SystemTime::now()));
        entry.began = Some(Instant::now());
        Some(entry.view.clone())
    }

    /// Ends job `id`: takes it out of its repos' queues, wakes the jobs now
    /// first, and sends its view.
    fn end(
        &self,
        job: &WriteJob,
        id: JobId,
        outcome: Outcome,
        output: String,
        message: Option<String>,
    ) -> JobView {
        let view = {
            let mut state = self.lock();
            release(&mut state, &job.workspace_id, &job.repos, id);
            finish(&mut state, id, outcome, output, message)
        };
        for repo in &job.repos {
            let summary = self.summaries.set_busy(&job.workspace_id, repo, false);
            self.events.summary(summary, false);
        }
        let view = view.unwrap_or_else(|ending| {
            ended_view(id, &job.workspace_id, &job.label, &job.repos, ending)
        });
        self.events.ended(&view);
        view
    }
}

/// A job that is not a repo write, from [`Jobs::track`]. Dropped before it
/// ended, as when its task panics, it ends as *failed*.
pub struct TrackedJob {
    jobs: Arc<Jobs>,
    id: JobId,
    /// Requested on cancel only when the job was tracked as one to kill.
    kill: Kill,
    /// Revoked with the job, as a write job's is when it ends (D10).
    grant: Option<AskpassGrant>,
    ended: bool,
}

impl std::fmt::Debug for TrackedJob {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TrackedJob")
            .field("id", &self.id)
            .finish_non_exhaustive()
    }
}

impl TrackedJob {
    pub fn job_id(&self) -> JobId {
        self.id
    }

    /// Reports `done` of `total` steps.
    pub fn progress(&self, done: u32, total: u32) {
        self.jobs.set_progress(self.id, None, done, total);
    }

    /// Reports `done` of `total` in phase `step`, as [`JobRun::step`] does.
    pub fn step(&self, step: &str, done: u32, total: u32) {
        self.jobs.set_progress(self.id, Some(step), done, total);
    }

    /// How the runner runs this job's git: killed on cancel, with askpass
    /// for a network job, as [`JobRun::write`].
    pub fn write(&self) -> Write<'_> {
        Write {
            kill: Some(&self.kill),
            askpass: self.grant.as_ref().map(AskpassGrant::askpass),
            ..Write::default()
        }
    }

    /// Reports progress like [`Self::progress`], from another thread.
    pub fn reporter(&self) -> impl Fn(u32, u32) + Send + Sync + 'static {
        let (jobs, id) = (self.jobs.clone(), self.id);
        move |done, total| jobs.set_progress(id, None, done, total)
    }

    /// Ends the job and sends its view.
    pub fn end(mut self, outcome: Outcome, output: String, message: Option<String>) {
        self.ended = true;
        self.finish(outcome, output, message);
    }

    fn finish(&self, outcome: Outcome, output: String, message: Option<String>) {
        let view = finish(&mut self.jobs.lock(), self.id, outcome, output, message);
        if let Ok(view) = view {
            self.jobs.events.ended(&view);
        }
    }
}

impl Drop for TrackedJob {
    fn drop(&mut self) {
        if !self.ended {
            self.finish(
                Outcome::Failed,
                String::new(),
                Some("the job stopped before its end".to_owned()),
            );
        }
    }
}

/// Takes job `id` out of the queues of `repos` and wakes the jobs now first
/// in all their repos' queues.
fn release(state: &mut State, workspace: &WorkspaceId, repos: &[RepoId], id: JobId) {
    for repo in repos {
        let key = (workspace.clone(), repo.clone());
        if let Some(queue) = state.queues.get_mut(&key) {
            queue.retain(|queued| *queued != id);
            if queue.is_empty() {
                state.queues.remove(&key);
            }
        }
    }
    let waiting: Vec<(JobId, Vec<RepoId>)> = state
        .jobs
        .iter()
        .filter(|entry| entry.turn.is_some() && entry.view.workspace_id == *workspace)
        .map(|entry| (entry.view.job_id, entry.view.repos.clone()))
        .collect();
    for (next, repos) in waiting {
        if state.first(workspace, &repos, next)
            && let Some(turn) = state.entry(next).and_then(|entry| entry.turn.take())
        {
            // A job cancelled meanwhile no longer waits.
            let _ = turn.send(());
        }
    }
}

/// How a job ended: its outcome, output and message.
type Ending = (Outcome, String, Option<String>);

/// Marks job `id` ended, drops the oldest ended job past
/// [`MAX_ENDED_JOBS`], and returns its view, or the ending when it is not
/// listed.
fn finish(
    state: &mut State,
    id: JobId,
    outcome: Outcome,
    output: String,
    message: Option<String>,
) -> Result<JobView, Ending> {
    let view = match state.entry(id) {
        Some(entry) => {
            entry.view.state = JobState::Ended;
            entry.view.outcome = Some(outcome);
            entry.view.output = output;
            entry.view.message = message;
            entry.view.duration_ms = entry
                .began
                .map(|began| u32::try_from(began.elapsed().as_millis()).unwrap_or(u32::MAX));
            entry.turn = None;
            Ok(entry.view.clone())
        }
        None => Err((outcome, output, message)),
    };
    let ended = state
        .jobs
        .iter()
        .filter(|entry| entry.view.state == JobState::Ended)
        .count();
    if ended > MAX_ENDED_JOBS
        && let Some(oldest) = state
            .jobs
            .iter()
            .position(|entry| entry.view.state == JobState::Ended)
    {
        state.jobs.remove(oldest);
    }
    view
}

/// The view of a write job whose entry is gone, which a running job's
/// never is.
fn ended_view(
    id: JobId,
    workspace: &WorkspaceId,
    label: &str,
    repos: &[RepoId],
    (outcome, output, message): Ending,
) -> JobView {
    JobView {
        job_id: id,
        workspace_id: workspace.clone(),
        workspace_name: String::new(),
        label: label.to_owned(),
        repos: repos.to_owned(),
        repo_names: repos.iter().map(|repo| repo.0.clone()).collect(),
        state: JobState::Ended,
        done: 0,
        total: 0,
        step: None,
        started_at: None,
        duration_ms: None,
        outcome: Some(outcome),
        output,
        message,
        sub_jobs: Vec::new(),
        report: None,
    }
}

/// `time` in milliseconds since the Unix epoch.
fn epoch_ms(time: SystemTime) -> u64 {
    let since = time.duration_since(UNIX_EPOCH).unwrap_or_default();
    u64::try_from(since.as_millis()).unwrap_or(u64::MAX)
}

fn cancelled() -> Outcome {
    Outcome::Skipped {
        reason: SkipReason::Cancelled,
    }
}

/// Reads each repo's refs, and its conflict state when `conflict`, on a
/// blocking thread. A repo that cannot be read is seen with no refs.
async fn see(paths: &[PathBuf], git: &GitRunner, conflict: bool) -> Vec<Seen> {
    let (paths, git) = (paths.to_owned(), git.clone());
    tokio::task::spawn_blocking(move || {
        paths
            .iter()
            .map(|path| {
                let Ok(repo) = GitRepo::open(path) else {
                    return Seen::default();
                };
                let refs = repo.ref_snapshot().ok();
                let conflict = conflict
                    && StatusReader::new(repo, git.clone())
                        .repo_state()
                        .is_ok_and(|state| state.in_conflict());
                Seen { refs, conflict }
            })
            .collect()
    })
    .await
    .unwrap_or_default()
}

/// The outcome of a write that gave `result`, its output and its message.
/// *Conflict* is read from the repo (OPS-1): a write that may stop in
/// conflict state and left a repo in it, when it was not before or git
/// failed, as when a continued rebase stops again.
fn judge(
    may_conflict: bool,
    nothing_to_do: NothingToDo,
    result: &Result<Output, GitError>,
    before: &[Seen],
    after: &[Seen],
) -> Ending {
    let output = git_output(result);
    let conflict = may_conflict
        && after.iter().enumerate().any(|(index, seen)| {
            seen.conflict && (result.is_err() || !before.get(index).is_some_and(|old| old.conflict))
        });
    let unchanged = before.len() == after.len()
        && before
            .iter()
            .zip(after)
            .all(|(old, new)| old.refs.is_some() && old.refs == new.refs);
    match result {
        Err(GitError::Killed) => (cancelled(), output, Some("cancelled".to_owned())),
        _ if conflict => (Outcome::Conflict, output, None),
        Err(error) => (Outcome::Failed, output, Some(error.to_string())),
        Ok(_) if nothing_to_do == NothingToDo::WhenRefsUnchanged && unchanged => {
            (Outcome::NothingToDo, output, None)
        }
        Ok(_) => (Outcome::Succeeded, output, None),
    }
}

/// git's output as a job shows it: stdout then stderr, as far as git ran.
fn git_output(result: &Result<Output, GitError>) -> String {
    match result {
        Ok(output) => output_text(&output.stdout, &output.stderr),
        Err(GitError::Failed { stdout, stderr, .. }) => output_text(stdout, stderr),
        Err(_) => String::new(),
    }
}

/// How a job that ran git once ends, as a write job does: *skipped
/// (cancelled)* when it was killed, *failed* with git's message, or
/// *succeeded*; with git's output.
pub(crate) fn git_ending(result: &Result<Output, GitError>) -> (Outcome, String, Option<String>) {
    let output = git_output(result);
    match result {
        Err(GitError::Killed) => (cancelled(), output, Some("cancelled".to_owned())),
        Err(error) => (Outcome::Failed, output, Some(error.to_string())),
        Ok(_) => (Outcome::Succeeded, output, None),
    }
}

/// stdout then stderr as text, each line of stderr folded to what follows
/// its last carriage return, as a terminal shows progress; at most
/// [`MAX_OUTPUT`] bytes, the end kept.
fn output_text(stdout: &[u8], stderr: &[u8]) -> String {
    let mut text = String::from_utf8_lossy(stdout).into_owned();
    for line in String::from_utf8_lossy(stderr).split_inclusive('\n') {
        let ends = line.ends_with('\n');
        let shown = line
            .trim_end_matches(['\n', '\r'])
            .rsplit('\r')
            .next()
            .unwrap_or_default();
        text.push_str(shown);
        if ends {
            text.push('\n');
        }
    }
    if text.len() > MAX_OUTPUT {
        let mut start = text.len() - MAX_OUTPUT;
        while !text.is_char_boundary(start) {
            start += 1;
        }
        text = format!("…{}", &text[start..]);
    }
    text
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn output_folds_progress_lines_and_keeps_the_end() {
        let text = output_text(b"out\n", b"Counting: 1%\rCounting: 100%, done.\r\nDone\n");
        assert_eq!(text, "out\nCounting: 100%, done.\nDone\n");
        let long = vec![b'a'; MAX_OUTPUT + 10];
        let text = output_text(&long, b"end\n");
        assert!(text.starts_with('…') && text.ends_with("end\n"));
        assert_eq!(text.len(), MAX_OUTPUT + '…'.len_utf8());
    }
}
