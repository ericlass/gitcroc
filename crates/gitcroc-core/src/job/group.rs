//! A group operation's run and report (section 5 "Lifecycle of a group
//! operation", steps 3 and 4): one job with one sub-job per repo that runs.
//! Sub-jobs start in target-set order, each when it gets a worker of the
//! shared pool, so the pool's size bounds how many run at once. Each holds
//! its repo's queue from the job's start (ARCH-4), checks GRP-8 again when it
//! is to start, and its repo is read again as soon as it ends (OPS-7). One
//! failing never stops the others (GRP-6). When all have ended, the report
//! lists every repo of the target set with one outcome (GRP-7).

use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Instant, SystemTime};

use gitcroc_git::{GitError, GitRepo, GitRunner, Kill, Output, StatusRead, StatusReader};

use super::{
    Ending, JobError, JobHandle, JobId, JobRun, JobState, JobView, Jobs, NothingToDo, OnCancel,
    Outcome, ProgressFn, RepoResult, SkipReason, SubJob, Worker, cancelled, ended_view, epoch_ms,
    finish, judge, output_text, release, see,
};
use crate::unblocked::{BUSY, IN_CONFLICT, MISSING};
use crate::{Priority, RepoId, WorkspaceId};

/// How each sub-job of a group operation runs, as [`super::WriteJob`] says
/// for a single write.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SubJobSpec {
    pub on_cancel: OnCancel,
    pub may_conflict: bool,
    pub nothing_to_do: NothingToDo,
    pub network: bool,
    /// The operation may run on a repo in conflict state: fetch (GRP-8).
    pub conflict_allowed: bool,
}

/// Why a sub-job's run did not succeed.
#[derive(Debug, thiserror::Error)]
pub enum SubJobError {
    /// git failed, or was killed (GRP-10).
    #[error(transparent)]
    Git(#[from] GitError),
    /// The repo no longer fits the operation, as when `HEAD` was detached
    /// since the preflight: *skipped* with the operation's reason, as the
    /// preflight would have said.
    #[error("{0}")]
    Skip(String),
    /// The repo could not be read before git ran.
    #[error("{0}")]
    Failed(String),
    /// A first step was made and a later one failed, as a push after a
    /// group commit: *failed* with git's output of both and `message`,
    /// which says what was made.
    #[error("{message}")]
    Partial { output: Output, message: String },
}

/// A group operation to run, as its preflight left it.
#[derive(Debug, Clone)]
pub struct GroupRun {
    pub workspace_id: WorkspaceId,
    pub label: String,
    pub spec: SubJobSpec,
    /// Every repo of the target set, in target-set order, with why it is
    /// skipped, or `None` when its sub-job runs.
    pub repos: Vec<(RepoId, Option<SkipReason>)>,
}

/// A sub-job that runs.
#[derive(Debug, Clone)]
struct Sub {
    /// Its repo's place in the target set.
    index: usize,
    /// Its place in [`JobView::sub_jobs`].
    position: usize,
    repo: RepoId,
    path: PathBuf,
}

impl Jobs {
    /// Starts group operation `group`: `run` runs once per repo that is not
    /// skipped, with that repo's path, through [`gitcroc_git::ops`] with
    /// [`JobRun::write`]. Returns at once; the job's view carries the
    /// sub-jobs while it runs and the report when it ended. Needs a tokio
    /// runtime.
    pub fn start_group<F, Fut>(
        self: &Arc<Self>,
        git: GitRunner,
        group: GroupRun,
        run: F,
    ) -> Result<JobHandle, JobError>
    where
        F: Fn(JobRun, PathBuf) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = Result<Output, SubJobError>> + Send + 'static,
    {
        if group.repos.is_empty() {
            return Err(JobError::NoRepo);
        }
        let workspace = &group.workspace_id;
        let all: Vec<RepoId> = group.repos.iter().map(|(repo, _)| repo.clone()).collect();
        let id = self.ids.next();
        let mut view = self.new_view(id, workspace, &group.label, &all)?;
        let mut subs = Vec::new();
        for (index, (repo, skip)) in group.repos.iter().enumerate() {
            if skip.is_none() {
                subs.push(Sub {
                    index,
                    position: subs.len(),
                    repo: repo.clone(),
                    path: self.workspaces.repo_path(workspace, repo)?,
                });
            }
        }
        view.sub_jobs = subs
            .iter()
            .map(|sub| SubJob {
                repo_id: sub.repo.clone(),
                repo_name: view.repo_names[sub.index].clone(),
                state: JobState::Queued,
                outcome: None,
            })
            .collect();
        view.total = u32::try_from(subs.len()).unwrap_or(u32::MAX);
        view.state = JobState::Running;
        view.started_at = Some(epoch_ms(SystemTime::now()));
        let (kill, stop) = (Kill::new(), Kill::new());
        {
            let mut state = self.lock();
            // Held from now, so another write waits behind it and GRP-8
            // sees the repo busy (ARCH-4).
            for sub in &subs {
                state
                    .queues
                    .entry((workspace.clone(), sub.repo.clone()))
                    .or_default()
                    .push_back(id);
            }
            state.jobs.push_back(super::Entry {
                view: view.clone(),
                began: Some(Instant::now()),
                kill: kill.clone(),
                on_cancel: group.spec.on_cancel,
                turn: None,
                stop: Some(stop.clone()),
            });
        }
        self.events.progress(&view);
        for sub in &subs {
            let summary = self.summaries.set_busy(workspace, &sub.repo, true);
            self.events.summary(summary, false);
        }
        let (ended, receiver) = tokio::sync::oneshot::channel();
        let engine = self.clone();
        tokio::spawn(async move {
            let view = engine
                .drive_group(id, group, subs, git, kill, stop, Arc::new(run))
                .await;
            // The caller may not wait for the end.
            let _ = ended.send(view);
        });
        Ok(JobHandle {
            job_id: id,
            ended: receiver,
        })
    }

    /// Runs group job `id`'s sub-jobs, then ends it with its report.
    #[allow(clippy::too_many_arguments)]
    async fn drive_group<F, Fut>(
        self: &Arc<Self>,
        id: JobId,
        group: GroupRun,
        subs: Vec<Sub>,
        git: GitRunner,
        kill: Kill,
        stop: Kill,
        run: Arc<F>,
    ) -> JobView
    where
        F: Fn(JobRun, PathBuf) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = Result<Output, SubJobError>> + Send + 'static,
    {
        let workspace = &group.workspace_id;
        let mut endings: Vec<Option<Ending>> = group
            .repos
            .iter()
            .map(|(_, skip)| {
                skip.clone()
                    .map(|reason| (Outcome::Skipped { reason }, String::new(), None))
            })
            .collect();
        let mut running = Vec::new();
        let mut pending = subs.into_iter();
        while let Some(sub) = pending.next() {
            let worker = tokio::select! {
                biased;
                () = stop.requested() => None,
                worker = self.summaries.pool().worker(Priority::Job) => Some(worker),
            };
            let Some(worker) = worker.filter(|_| !stop.is_requested()) else {
                // GRP-10: the sub-jobs not started never will.
                for sub in std::iter::once(sub).chain(pending.by_ref()) {
                    let ending = (cancelled(), String::new(), Some("cancelled".to_owned()));
                    self.end_sub(id, workspace, &sub, &ending);
                    endings[sub.index] = Some(ending);
                }
                break;
            };
            let engine = self.clone();
            let (workspace, git, run) = (workspace.clone(), git.clone(), run.clone());
            let kill = match group.spec.on_cancel {
                OnCancel::Kill => kill.clone(),
                // Never requested: the sub-job runs to its end.
                OnCancel::Finish => Kill::new(),
            };
            let spec = group.spec;
            running.push((
                sub.index,
                tokio::spawn(async move {
                    engine
                        .run_sub(id, &workspace, sub, git, kill, spec, worker, run)
                        .await
                }),
            ));
        }
        for (index, task) in running {
            endings[index] = Some(task.await.unwrap_or_else(|_| {
                (
                    Outcome::Failed,
                    String::new(),
                    Some("the sub-job stopped before its end".to_owned()),
                )
            }));
        }
        let all: Vec<RepoId> = group.repos.iter().map(|(repo, _)| repo.clone()).collect();
        let names = self
            .lock()
            .entry(id)
            .map(|entry| entry.view.repo_names.clone());
        let report: Vec<RepoResult> = group
            .repos
            .iter()
            .zip(endings)
            .enumerate()
            .map(|(index, ((repo, _), ending))| {
                let (outcome, output, message) = ending
                    .unwrap_or_else(|| (cancelled(), String::new(), Some("cancelled".to_owned())));
                RepoResult {
                    repo_id: repo.clone(),
                    repo_name: names
                        .as_ref()
                        .and_then(|names| names.get(index).cloned())
                        .unwrap_or_else(|| repo.0.clone()),
                    outcome,
                    output,
                    message,
                }
            })
            .collect();
        let (outcome, message) = overall(&report);
        let view = {
            let mut state = self.lock();
            if let Some(entry) = state.entry(id) {
                entry.view.report = Some(report.clone());
            }
            finish(&mut state, id, outcome, String::new(), message)
        };
        let view = view.unwrap_or_else(|ending| JobView {
            report: Some(report),
            ..ended_view(id, workspace, &group.label, &all, ending)
        });
        self.events.ended(&view);
        view
    }

    /// Runs one sub-job in `worker`: checks GRP-8 again, runs git, judges
    /// the outcome as a single write's, and reads the repo again (OPS-7).
    #[allow(clippy::too_many_arguments)]
    async fn run_sub<F, Fut>(
        self: &Arc<Self>,
        id: JobId,
        workspace: &WorkspaceId,
        sub: Sub,
        git: GitRunner,
        kill: Kill,
        spec: SubJobSpec,
        _worker: Worker,
        run: Arc<F>,
    ) -> Ending
    where
        F: Fn(JobRun, PathBuf) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = Result<Output, SubJobError>> + Send + 'static,
    {
        if let Some(reason) = self.blocked(id, workspace, &sub, &git, spec).await {
            let ending = (
                Outcome::Skipped {
                    reason: SkipReason::Blocked {
                        reason: reason.clone(),
                    },
                },
                String::new(),
                Some(reason),
            );
            self.end_sub(id, workspace, &sub, &ending);
            return ending;
        }
        let Some(view) = self.set_sub(id, sub.position, JobState::Running, None) else {
            let ending = (cancelled(), String::new(), Some("cancelled".to_owned()));
            self.end_sub(id, workspace, &sub, &ending);
            return ending;
        };
        self.events.progress(&view);
        self.events.hold_watch(workspace, &sub.repo, true);
        let paths = vec![sub.path.clone()];
        let before = see(&paths, &git, spec.may_conflict).await;
        // A prompt names the sub-job's repo, not the whole target set (D10).
        let asking = JobView {
            repo_names: view
                .repo_names
                .get(sub.index)
                .cloned()
                .into_iter()
                .collect(),
            repos: vec![sub.repo.clone()],
            ..view.clone()
        };
        let grant = spec
            .network
            .then(|| self.askpass.get())
            .flatten()
            .and_then(|issuer| issuer.issue(&asking));
        let progress: Arc<ProgressFn> = Arc::new(|_: Option<&str>, _: u32, _: u32| {});
        let result = run(
            JobRun {
                job_id: id,
                paths: paths.clone(),
                git: git.clone(),
                kill,
                askpass: grant.as_ref().map(|grant| grant.askpass().clone()),
                progress,
            },
            sub.path.clone(),
        )
        .await;
        // Revoked: a prompt still open for the sub-job gets no answer.
        drop(grant);
        let after = see(&paths, &git, spec.may_conflict).await;
        let judged = |result: &Result<Output, GitError>| {
            judge(
                spec.may_conflict,
                spec.nothing_to_do,
                result,
                &before,
                &after,
            )
        };
        let ending = match result {
            // Each repo in conflict is resolved on its own (OPS-2).
            Ok(output) => in_repo(judged(&Ok(output))),
            Err(SubJobError::Git(error)) => in_repo(judged(&Err(error))),
            Err(SubJobError::Skip(reason)) => (
                Outcome::Skipped {
                    reason: SkipReason::Preflight {
                        reason: reason.clone(),
                    },
                },
                String::new(),
                Some(reason),
            ),
            Err(SubJobError::Failed(reason)) => (Outcome::Failed, String::new(), Some(reason)),
            Err(SubJobError::Partial { output, message }) => (
                Outcome::Failed,
                output_text(&output.stdout, &output.stderr),
                Some(message),
            ),
        };
        self.reread(workspace, &sub.repo, &git, before.first(), after.first())
            .await;
        self.end_sub(id, workspace, &sub, &ending);
        ending
    }

    /// Why `sub`'s repo is blocked now (GRP-8): another write is ahead of
    /// job `id` in its queue, its folder is missing, or it is in conflict
    /// state when the operation does not allow that.
    async fn blocked(
        &self,
        id: JobId,
        workspace: &WorkspaceId,
        sub: &Sub,
        git: &GitRunner,
        spec: SubJobSpec,
    ) -> Option<String> {
        let first = self
            .lock()
            .first(workspace, std::slice::from_ref(&sub.repo), id);
        if !first {
            return Some(BUSY.to_owned());
        }
        if !sub.path.exists() {
            return Some(MISSING.to_owned());
        }
        if spec.conflict_allowed {
            return None;
        }
        let (path, git) = (sub.path.clone(), git.clone());
        tokio::task::spawn_blocking(move || {
            let repo = GitRepo::open(&path).map_err(|error| error.to_string())?;
            let state = StatusReader::new(repo, git)
                .repo_state()
                .map_err(|error| error.to_string())?;
            Ok::<_, String>(state.in_conflict().then(|| IN_CONFLICT.to_owned()))
        })
        .await
        .unwrap_or_else(|error| Err(error.to_string()))
        .unwrap_or_else(|error| Some(format!("cannot read the repo: {error}")))
    }

    /// Sets sub-job `position` of job `id` and returns the job's view;
    /// `None` when the job is gone.
    fn set_sub(
        &self,
        id: JobId,
        position: usize,
        state: JobState,
        outcome: Option<Outcome>,
    ) -> Option<JobView> {
        let mut jobs = self.lock();
        let entry = jobs.entry(id)?;
        let sub = entry.view.sub_jobs.get_mut(position)?;
        sub.state = state;
        sub.outcome = outcome;
        if state == JobState::Ended {
            entry.view.done = entry.view.done.saturating_add(1);
        }
        Some(entry.view.clone())
    }

    /// Ends `sub` of job `id` with `ending`: takes it out of its repo's
    /// queue and sends the job's view.
    fn end_sub(&self, id: JobId, workspace: &WorkspaceId, sub: &Sub, ending: &Ending) {
        release(
            &mut self.lock(),
            workspace,
            std::slice::from_ref(&sub.repo),
            id,
        );
        let summary = self.summaries.set_busy(workspace, &sub.repo, false);
        self.events.summary(summary, false);
        if let Some(view) = self.set_sub(id, sub.position, JobState::Ended, Some(ending.0.clone()))
        {
            self.events.progress(&view);
        }
    }
}

/// `ending`, its report line saying where a conflict is resolved: in that
/// repo, whose banner offers Abort and Continue (OPS-2).
fn in_repo((outcome, output, message): Ending) -> Ending {
    let message = match outcome {
        Outcome::Conflict => {
            message.or_else(|| Some("in conflict state; resolve it in this repo".to_owned()))
        }
        _ => message,
    };
    (outcome, output, message)
}

/// The group job's own outcome, which the status bar shows (UI-21): failed
/// when any repo failed, else conflict when any is in conflict, else
/// succeeded when any succeeded, else nothing to do when any had nothing to
/// do, else the first repo's skip.
fn overall(report: &[RepoResult]) -> (Outcome, Option<String>) {
    let count =
        |kind: fn(&Outcome) -> bool| report.iter().filter(|line| kind(&line.outcome)).count();
    let failed = count(|outcome| *outcome == Outcome::Failed);
    let conflicts = count(|outcome| *outcome == Outcome::Conflict);
    let of = report.len();
    let repos = |n: usize| if n == 1 { "repo" } else { "repos" };
    if failed > 0 {
        return (
            Outcome::Failed,
            Some(format!("{failed} of {of} {} failed", repos(of))),
        );
    }
    if conflicts > 0 {
        return (
            Outcome::Conflict,
            Some(format!("{conflicts} of {of} {} in conflict", repos(of))),
        );
    }
    if count(|outcome| *outcome == Outcome::Succeeded) > 0 {
        return (Outcome::Succeeded, None);
    }
    if count(|outcome| *outcome == Outcome::NothingToDo) > 0 {
        return (Outcome::NothingToDo, None);
    }
    report.first().map_or_else(
        || (cancelled(), None),
        |line| (line.outcome.clone(), line.message.clone()),
    )
}
