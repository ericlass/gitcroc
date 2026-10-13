//! The job engine (ARCH-7): every repo write runs as a job, with progress
//! events, one outcome of GRP-7's vocabulary and git's output, and the
//! repos it touched read again when it ends (OPS-7).
//!
//! At most one write runs per repo (ARCH-4): a write on a busy repo waits
//! in that repo's queue, in the order writes were started. Reads never wait
//! for a write. Running jobs and the background refresh share one pool of
//! workers, jobs first (PERF-4).

mod pool;

use std::collections::{HashMap, VecDeque};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use gitcroc_git::{
    GitError, GitRepo, GitRunner, Kill, Output, RefSnapshot, StatusRead, StatusReader, Write,
};
use serde::{Deserialize, Serialize};
use tokio::sync::oneshot;

use crate::{RepoId, Summaries, SummaryView, WorkspaceError, Workspaces};

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

/// How far a job has come: `done` of `total` steps. A `total` of 0 means
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

/// Why a job was skipped.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "snake_case")]
pub enum SkipReason {
    /// Cancelled before it started, or killed while it ran (GRP-10).
    Cancelled,
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

/// A job as the job list shows it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
pub struct JobView {
    pub job_id: JobId,
    pub workspace_id: WorkspaceId,
    pub label: String,
    pub repos: Vec<RepoId>,
    pub state: JobState,
    /// Set when the job has ended.
    pub outcome: Option<Outcome>,
    /// What git printed, stdout then stderr, progress lines folded to their
    /// last state; at most [`MAX_OUTPUT`] bytes, the end kept.
    pub output: String,
    /// Why a failed or skipped job did not succeed.
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
}

type ProgressFn = dyn Fn(u32, u32) + Send + Sync;

/// What a running job's write gets: its repos' paths, in the order of
/// [`WriteJob::repos`], git, the kill request and progress.
#[derive(Clone)]
pub struct JobRun {
    pub job_id: JobId,
    pub paths: Vec<PathBuf>,
    pub git: GitRunner,
    kill: Kill,
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
            on_line: None,
        }
    }

    /// The job's kill request, never made when the job runs to its end.
    pub fn kill(&self) -> &Kill {
        &self.kill
    }

    /// Reports `done` of `total` steps.
    pub fn progress(&self, done: u32, total: u32) {
        (self.progress)(done, total);
    }
}

/// What the engine tells the app.
pub trait JobEvents: Send + Sync {
    fn progress(&self, progress: JobProgress);
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
    kill: Kill,
    on_cancel: OnCancel,
    /// Wakes the job when it is first in all its repos' queues.
    turn: Option<oneshot::Sender<()>>,
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
            state: Mutex::new(State::default()),
        })
    }

    /// A new job id, for jobs that are not writes, such as adding repos.
    pub fn next_id(&self) -> JobId {
        self.ids.next()
    }

    fn lock(&self) -> MutexGuard<'_, State> {
        // Each change is a push, a removal or a field set, so a panic
        // leaves no half-changed state.
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
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
                view: JobView {
                    job_id: id,
                    workspace_id: job.workspace_id.clone(),
                    label: job.label.clone(),
                    repos: job.repos.clone(),
                    state: JobState::Queued,
                    outcome: None,
                    output: String::new(),
                    message: None,
                },
                kill: kill.clone(),
                on_cancel: job.on_cancel,
                turn: (!first).then_some(turn),
            });
            first
        };
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
        let Some(_worker) = worker.filter(|_| self.begin(id)) else {
            return self.end(
                &job,
                id,
                cancelled(),
                String::new(),
                Some("cancelled".to_owned()),
            );
        };
        self.events.progress(JobProgress {
            job_id: id,
            workspace_id: job.workspace_id.clone(),
            label: job.label.clone(),
            done: 0,
            total: 0,
        });
        for repo in &job.repos {
            self.events.hold_watch(&job.workspace_id, repo, true);
        }
        let before = see(&paths, &git, job.may_conflict).await;
        let progress = {
            let (events, workspace, label) = (
                self.events.clone(),
                job.workspace_id.clone(),
                job.label.clone(),
            );
            Arc::new(move |done, total| {
                events.progress(JobProgress {
                    job_id: id,
                    workspace_id: workspace.clone(),
                    label: label.clone(),
                    done,
                    total,
                });
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
            progress,
        })
        .await;
        let after = see(&paths, &git, job.may_conflict).await;
        let (outcome, output, message) = judge(&job, &result, &before, &after);
        // OPS-7: the repos it touched, read again at once.
        for (index, repo) in job.repos.iter().enumerate() {
            let refs_changed = match (before.get(index), after.get(index)) {
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
            let (summaries, workspaces, workspace, repo_id, git) = (
                self.summaries.clone(),
                self.workspaces.clone(),
                job.workspace_id.clone(),
                repo.clone(),
                git.clone(),
            );
            let view = tokio::task::spawn_blocking(move || {
                summaries.reread(&workspaces, &workspace, &repo_id, &git, refs_changed)
            })
            .await;
            if let Ok(Some(view)) = view {
                self.events.summary(view, true);
            }
            self.events.hold_watch(&job.workspace_id, repo, false);
        }
        self.end(&job, id, outcome, output, message)
    }

    /// Marks job `id` running, unless it was cancelled meanwhile.
    fn begin(&self, id: JobId) -> bool {
        let mut state = self.lock();
        match state.entry(id) {
            Some(entry) if !entry.kill.is_requested() => {
                entry.view.state = JobState::Running;
                true
            }
            _ => false,
        }
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
            for repo in &job.repos {
                let key = (job.workspace_id.clone(), repo.clone());
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
                .filter(|entry| entry.turn.is_some() && entry.view.workspace_id == job.workspace_id)
                .map(|entry| (entry.view.job_id, entry.view.repos.clone()))
                .collect();
            for (next, repos) in waiting {
                if state.first(&job.workspace_id, &repos, next)
                    && let Some(turn) = state.entry(next).and_then(|entry| entry.turn.take())
                {
                    // A job cancelled meanwhile no longer waits.
                    let _ = turn.send(());
                }
            }
            let view = match state.entry(id) {
                Some(entry) => {
                    entry.view.state = JobState::Ended;
                    entry.view.outcome = Some(outcome);
                    entry.view.output = output;
                    entry.view.message = message;
                    entry.turn = None;
                    entry.view.clone()
                }
                None => JobView {
                    job_id: id,
                    workspace_id: job.workspace_id.clone(),
                    label: job.label.clone(),
                    repos: job.repos.clone(),
                    state: JobState::Ended,
                    outcome: Some(outcome),
                    output,
                    message,
                },
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
        };
        for repo in &job.repos {
            let summary = self.summaries.set_busy(&job.workspace_id, repo, false);
            self.events.summary(summary, false);
        }
        self.events.ended(&view);
        view
    }
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
    job: &WriteJob,
    result: &Result<Output, GitError>,
    before: &[Seen],
    after: &[Seen],
) -> (Outcome, String, Option<String>) {
    let output = match result {
        Ok(output) => output_text(&output.stdout, &output.stderr),
        Err(GitError::Failed { stdout, stderr, .. }) => output_text(stdout, stderr),
        Err(_) => String::new(),
    };
    let conflict = job.may_conflict
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
        Ok(_) if job.nothing_to_do == NothingToDo::WhenRefsUnchanged && unchanged => {
            (Outcome::NothingToDo, output, None)
        }
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
