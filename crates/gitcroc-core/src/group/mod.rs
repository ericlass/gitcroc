//! Groups: what a group shows (GRP-4, UI-4), and group operations (section
//! 5 "Lifecycle of a group operation"): a read-only preflight classes every
//! repo of the target set, the core decides whether the user must confirm,
//! and the run is one job with one sub-job per repo, ending with a result
//! report (D8).

mod op;
mod summary;

use std::collections::VecDeque;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};

use gitcroc_git::{GitRepo, GitRunner, StatusRead, StatusReader};
use serde::{Deserialize, Serialize};

pub use op::{Class, GroupOp, GroupOperation, MissingBranch, RunFuture};
pub use summary::{GroupBranch, GroupSummary, group_summaries};

use crate::unblocked::{BUSY, IN_CONFLICT, MISSING};
use crate::{
    GroupRun, GroupView, JobError, JobId, Jobs, Priority, RepoId, RepoView, SettingsStore,
    SkipReason, Summaries, TargetSet, WorkspaceError, WorkspaceId, Workspaces, lock, target_repos,
};

/// Identifies a preflight, unique while the app runs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, specta::Type)]
pub struct PreflightId(pub u32);

/// A repo of a preflight and its class.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
pub struct RepoClass {
    pub repo_id: RepoId,
    pub repo_name: String,
    pub class: Class,
}

/// A preflight's result, sent as event `preflight_ready`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
pub struct PreflightReady {
    pub workspace_id: WorkspaceId,
    pub preflight_id: PreflightId,
    /// The job's label, such as `Pull · Backend (5 repos)`.
    pub label: String,
    /// The operation is destructive (GRP-9).
    pub destructive: bool,
    /// Every repo of the target set, in target-set order.
    pub repos: Vec<RepoClass>,
    /// The user must confirm before it runs: a repo is not *will run*, the
    /// operation is destructive, or the settings always show the dialog
    /// (section 5, step 2).
    pub needs_confirmation: bool,
}

/// Why a group operation did not start.
#[derive(Debug, thiserror::Error)]
pub enum GroupError {
    #[error(transparent)]
    Workspace(#[from] WorkspaceError),
    #[error(transparent)]
    Job(#[from] JobError),
    #[error("{0} on a group is not available yet")]
    NotAvailable(String),
    #[error("the target set has no repo")]
    NoRepo,
    #[error("no such preflight, or it was used already")]
    UnknownPreflight,
    #[error("the preflight has not finished")]
    NotReady,
    #[error("the operation needs the user's confirmation")]
    NeedsConfirmation,
    #[error("the workspace changed since the preflight; run it again")]
    WorkspaceChanged,
}

/// The most preflights kept; the oldest is dropped past it.
const MAX_PREFLIGHTS: usize = 32;

/// A preflight kept for its start.
struct Preflight {
    id: PreflightId,
    workspace_id: WorkspaceId,
    op: Arc<dyn GroupOperation>,
    label: String,
    repos: Vec<RepoId>,
    /// The workspace's repos and groups when it ran.
    seen: (Vec<RepoView>, Vec<GroupView>),
    /// Set when it finished.
    ready: Option<PreflightReady>,
}

type ReadyFn = dyn Fn(&PreflightReady) + Send + Sync;

/// Runs group operations for the whole app.
pub struct GroupOps {
    workspaces: Arc<Workspaces>,
    summaries: Arc<Summaries>,
    settings: Arc<SettingsStore>,
    jobs: Arc<Jobs>,
    ready: Box<ReadyFn>,
    ids: AtomicU32,
    preflights: Mutex<VecDeque<Preflight>>,
}

impl std::fmt::Debug for GroupOps {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("GroupOps").finish_non_exhaustive()
    }
}

impl GroupOps {
    /// The engine; `ready` gets each preflight's result.
    pub fn new(
        workspaces: Arc<Workspaces>,
        summaries: Arc<Summaries>,
        settings: Arc<SettingsStore>,
        jobs: Arc<Jobs>,
        ready: impl Fn(&PreflightReady) + Send + Sync + 'static,
    ) -> Arc<Self> {
        Arc::new(Self {
            workspaces,
            summaries,
            settings,
            jobs,
            ready: Box::new(ready),
            ids: AtomicU32::new(0),
            preflights: Mutex::new(VecDeque::new()),
        })
    }

    fn lock(&self) -> MutexGuard<'_, VecDeque<Preflight>> {
        // Each change is a push, a removal or a field set.
        lock(&self.preflights)
    }

    /// Starts the preflight of `op` on `target` in workspace `id` and
    /// returns its id at once (ARCH-7); the result goes to the engine's
    /// `ready`. Repos are read in parallel in workers of the shared pool, at
    /// the priority of jobs (PERF-4). Needs a tokio runtime.
    pub fn preflight(
        self: &Arc<Self>,
        git: GitRunner,
        id: &WorkspaceId,
        target: &TargetSet,
        op: Arc<dyn GroupOperation>,
    ) -> Result<PreflightId, GroupError> {
        let repos = target_repos(&self.workspaces, id, target)?;
        if repos.is_empty() {
            return Err(GroupError::NoRepo);
        }
        let view = self.workspaces.view(id)?;
        let paths = repos
            .iter()
            .map(|repo| self.workspaces.repo_path(id, repo))
            .collect::<Result<Vec<_>, _>>()?;
        let names: Vec<String> = repos
            .iter()
            .map(|repo| name_of(&view.repos, repo))
            .collect();
        let label = format!(
            "{} · {}",
            op.name(),
            target_label(target, &view.groups, &names)
        );
        let preflight_id = PreflightId(self.ids.fetch_add(1, Ordering::Relaxed).wrapping_add(1));
        {
            let mut preflights = self.lock();
            if preflights.len() >= MAX_PREFLIGHTS {
                preflights.pop_front();
            }
            preflights.push_back(Preflight {
                id: preflight_id,
                workspace_id: id.clone(),
                op: op.clone(),
                label: label.clone(),
                repos: repos.clone(),
                seen: (view.repos, view.groups),
                ready: None,
            });
        }
        let engine = self.clone();
        let workspace = id.clone();
        tokio::spawn(async move {
            let classes: Vec<_> = repos
                .iter()
                .zip(paths)
                .map(|(repo, path)| {
                    let (summaries, workspace, repo, op, git) = (
                        engine.summaries.clone(),
                        workspace.clone(),
                        repo.clone(),
                        op.clone(),
                        git.clone(),
                    );
                    tokio::spawn(async move {
                        classify(&summaries, &workspace, &repo, path, op, git).await
                    })
                })
                .collect();
            let mut classed = Vec::with_capacity(classes.len());
            for ((repo, name), class) in repos.into_iter().zip(names).zip(classes) {
                let class = class.await.unwrap_or_else(|error| Class::Blocked {
                    reason: format!("the read did not finish: {error}"),
                });
                classed.push(RepoClass {
                    repo_id: repo,
                    repo_name: name,
                    class,
                });
            }
            let always = engine.settings.view().settings.groups.always_show_preflight;
            let needs_confirmation = always
                || op.destructive()
                || classed.iter().any(|repo| repo.class != Class::WillRun);
            let ready = PreflightReady {
                workspace_id: workspace,
                preflight_id,
                label,
                destructive: op.destructive(),
                repos: classed,
                needs_confirmation,
            };
            let kept = {
                let mut preflights = engine.lock();
                match preflights.iter_mut().find(|kept| kept.id == preflight_id) {
                    Some(kept) => {
                        kept.ready = Some(ready.clone());
                        true
                    }
                    None => false,
                }
            };
            if kept {
                (engine.ready)(&ready);
            }
        });
        Ok(preflight_id)
    }

    /// Starts the job of preflight `id` (section 5, step 3). Refused when
    /// it needs confirmation and `confirmed` is not set, when it was used
    /// already, and when the workspace's repos or groups changed since it
    /// ran. Returns the job's id at once. Needs a tokio runtime.
    pub fn start(
        &self,
        git: GitRunner,
        id: PreflightId,
        confirmed: bool,
    ) -> Result<JobId, GroupError> {
        let preflight = {
            let mut preflights = self.lock();
            let index = preflights
                .iter()
                .position(|kept| kept.id == id)
                .ok_or(GroupError::UnknownPreflight)?;
            match preflights.get(index).and_then(|kept| kept.ready.as_ref()) {
                None => return Err(GroupError::NotReady),
                Some(ready) if ready.needs_confirmation && !confirmed => {
                    return Err(GroupError::NeedsConfirmation);
                }
                Some(_) => {}
            }
            preflights
                .remove(index)
                .ok_or(GroupError::UnknownPreflight)?
        };
        let view = self.workspaces.view(&preflight.workspace_id)?;
        if (view.repos, view.groups) != preflight.seen {
            return Err(GroupError::WorkspaceChanged);
        }
        let parallelism = self.settings.view().settings.groups.group_parallelism;
        self.summaries
            .pool()
            .resize(usize::try_from(parallelism).unwrap_or(crate::POOL_SIZE));
        let ready = preflight.ready.ok_or(GroupError::NotReady)?;
        let repos = preflight
            .repos
            .into_iter()
            .zip(ready.repos)
            .map(|(repo, classed)| {
                let skip = match classed.class {
                    Class::WillRun | Class::NeedsAttention { .. } => None,
                    Class::WillSkip { reason } => Some(SkipReason::Preflight { reason }),
                    Class::Blocked { reason } => Some(SkipReason::Blocked { reason }),
                };
                (repo, skip)
            })
            .collect();
        let op = preflight.op;
        let group = GroupRun {
            workspace_id: preflight.workspace_id,
            label: preflight.label,
            spec: op.spec(),
            repos,
        };
        let handle = self
            .jobs
            .start_group(git, group, move |job, path| op.run(job, path))?;
        Ok(handle.job_id)
    }
}

/// Repo `repo`'s name in `repos`; its id when it is not listed.
fn name_of(repos: &[RepoView], repo: &RepoId) -> String {
    repos
        .iter()
        .find(|known| known.id == *repo)
        .map_or_else(|| repo.0.clone(), |known| known.name.clone())
}

/// How a job's label names `target`: the repo's name, the group's name with
/// its number of repos, or the number of repos selected.
fn target_label(target: &TargetSet, groups: &[GroupView], names: &[String]) -> String {
    let count = match names.len() {
        1 => "1 repo".to_owned(),
        n => format!("{n} repos"),
    };
    match target {
        TargetSet::Repo { .. } => names.first().cloned().unwrap_or(count),
        TargetSet::Group { group_id } => match groups.iter().find(|group| group.id == *group_id) {
            Some(group) => format!("{} ({count})", group.name),
            None => count,
        },
        TargetSet::Repos { .. } => count,
    }
}

/// The class of repo `repo` at `path` for `op`: GRP-8 first, for every
/// operation since all of them write, then the operation's own rule. Holds
/// a worker of the shared pool while it reads.
async fn classify(
    summaries: &Summaries,
    workspace: &WorkspaceId,
    repo: &RepoId,
    path: PathBuf,
    op: Arc<dyn GroupOperation>,
    git: GitRunner,
) -> Class {
    let blocked = |reason: &str| Class::Blocked {
        reason: reason.to_owned(),
    };
    let _worker = summaries.pool().worker(Priority::Job).await;
    if summaries.busy(workspace, repo) {
        return blocked(BUSY);
    }
    tokio::task::spawn_blocking(move || {
        if !path.exists() {
            return blocked(MISSING);
        }
        let repo = match GitRepo::open(&path) {
            Ok(repo) => repo,
            Err(error) => return blocked(&format!("cannot open the repo: {error}")),
        };
        if !op.spec().conflict_allowed {
            match StatusReader::new(repo.clone(), git.clone()).repo_state() {
                Ok(state) if state.in_conflict() => return blocked(IN_CONFLICT),
                Ok(_) => {}
                Err(error) => return blocked(&format!("cannot read the repo: {error}")),
            }
        }
        op.classify(&repo, &git)
    })
    .await
    .unwrap_or_else(|error| blocked(&format!("the read did not finish: {error}")))
}
