//! Merge, rebase, cherry-pick and revert of one repo, and the abort and
//! continue of an operation stopped in conflict state (section 6
//! "Integrate", section 5 rows "Merge X into current" and "Rebase current
//! onto X", OPS-2), each one job (ARCH-7). A repo that is busy, missing or
//! in conflict state is refused (GRP-8); abort and continue are what
//! conflict state allows, and they act on the operation the repo's state
//! shows, never on what the app started (OPS-1). Nothing is stashed to make
//! one succeed (GRP-12). The abort of a stash apply cannot be undone from
//! the app: without the user's confirmation the core answers with what it
//! would do instead.

use std::path::Path;
use std::sync::Arc;

use gitcroc_git::ops::{FastForward, Resume};
use gitcroc_git::{
    ConflictDetails, GitError, GitRepo, Operation, Output, RefsRead, StatusRead, StatusReader,
    Write, ops,
};
use serde::{Deserialize, Serialize};

use crate::unblocked::{BlockedError, open_unblocked, repo_name};
use crate::{
    GitRunner, JobError, JobId, Jobs, NothingToDo, OnCancel, OpenError, RepoId, StatusReadError,
    Summaries, WorkspaceError, WorkspaceId, Workspaces, WriteJob,
};

/// The most names [`merge_targets`] answers with.
pub const MAX_MERGE_TARGETS: usize = 200;

/// Whether a merge may fast-forward (section 5 "Merge X into current").
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "snake_case")]
pub enum MergeFastForward {
    /// As `merge.ff` says; by default a fast-forward when possible, else a
    /// merge commit.
    Config,
    /// Fast-forward, or fail.
    FfOnly,
    /// Always a merge commit.
    NoFf,
}

/// An integrate operation (section 6 "Integrate"), or what to do with one
/// stopped in conflict state (OPS-2).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum IntegrateOp {
    /// Merges `revision`, a ref or a hash, into the current branch.
    Merge {
        revision: String,
        fast_forward: MergeFastForward,
    },
    /// Rebases the current branch onto `onto`, a ref or a hash.
    Rebase { onto: String },
    /// Applies `commits`, oldest first whatever their order here.
    CherryPick { commits: Vec<String> },
    /// Reverts `commit`. A merge commit needs `mainline`, the parent whose
    /// side is kept, counted from 1; without it the core asks.
    Revert {
        commit: String,
        mainline: Option<u32>,
    },
    /// Aborts the operation the repo is in.
    Abort,
    /// Continues the operation the repo is in once no path is unmerged.
    Continue,
}

/// An integrate operation on one repo.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
pub struct IntegrateRequest {
    pub repo_id: RepoId,
    pub op: IntegrateOp,
    /// The user confirmed an operation that cannot be undone.
    pub confirmed: bool,
}

/// What a request did.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, specta::Type)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum IntegrateStart {
    /// The job runs; its end re-reads the repo (OPS-7).
    Started { job_id: JobId },
    /// A revert of a merge commit: ask again with the parent whose side is
    /// kept. `commit` is its full hash, `parents` each parent as
    /// `1a2b3c4 Subject`, the first being mainline 1.
    ChooseMainline {
        commit: String,
        parents: Vec<String>,
    },
    /// The operation cannot be undone: ask again with `confirmed` once the
    /// user agreed. `actions` says what is lost.
    Confirm { repo: String, actions: Vec<String> },
}

/// Why an integrate operation did not start, or a read for it failed.
#[derive(Debug, thiserror::Error)]
pub enum IntegrateError {
    #[error(transparent)]
    Workspace(#[from] WorkspaceError),
    #[error(transparent)]
    Job(#[from] JobError),
    #[error(transparent)]
    Open(#[from] OpenError),
    #[error(transparent)]
    Status(#[from] StatusReadError),
    #[error(transparent)]
    Refs(#[from] gitcroc_git::RefsError),
    #[error("{0}")]
    Refused(String),
    #[error("the read did not finish: {0}")]
    Stopped(#[from] tokio::task::JoinError),
}

impl From<BlockedError> for IntegrateError {
    fn from(error: BlockedError) -> Self {
        match error {
            BlockedError::Workspace(error) => Self::Workspace(error),
            BlockedError::Open(error) => Self::Open(error),
            BlockedError::Status(error) => Self::Status(error),
            BlockedError::Stopped(error) => Self::Stopped(error),
            BlockedError::Blocked(why) => Self::Refused(why),
        }
    }
}

impl From<gitcroc_git::StatusError> for IntegrateError {
    fn from(error: gitcroc_git::StatusError) -> Self {
        Self::Status(StatusReadError::Read(error))
    }
}

fn refused(why: impl Into<String>) -> IntegrateError {
    IntegrateError::Refused(why.into())
}

/// What a job runs.
#[derive(Debug, Clone)]
enum Plan {
    Merge {
        branch: String,
        revision: String,
        ff: FastForward,
    },
    Rebase {
        branch: String,
        onto: String,
    },
    /// Full hashes, oldest first, and how the label names them.
    CherryPick {
        commits: Vec<String>,
        named: String,
    },
    Revert {
        commit: String,
        named: String,
        mainline: Option<u32>,
    },
    Resume {
        operation: Operation,
        resume: Resume,
    },
}

/// The operation as a label words it.
fn operation_name(operation: Operation) -> &'static str {
    match operation {
        Operation::Merge => "merge",
        Operation::Rebase => "rebase",
        Operation::ApplyMailbox => "patch application",
        Operation::CherryPick => "cherry-pick",
        Operation::Revert => "revert",
        Operation::StashApply => "stash apply",
    }
}

impl Plan {
    /// The job's label.
    fn label(&self) -> String {
        match self {
            Self::Merge {
                branch, revision, ..
            } => format!("Merge {revision} into {branch}"),
            Self::Rebase { branch, onto } => format!("Rebase {branch} onto {onto}"),
            Self::CherryPick { named, .. } => format!("Cherry-pick {named}"),
            Self::Revert { named, .. } => format!("Revert {named}"),
            Self::Resume { operation, resume } => {
                let verb = match resume {
                    Resume::Abort => "Abort",
                    Resume::Continue => "Continue",
                };
                format!("{verb} the {}", operation_name(*operation))
            }
        }
    }

    /// A merge or rebase that finds nothing to bring in changes no ref.
    fn nothing_to_do(&self) -> NothingToDo {
        match self {
            Self::Merge { .. } | Self::Rebase { .. } => NothingToDo::WhenRefsUnchanged,
            _ => NothingToDo::Never,
        }
    }

    /// What cannot be undone from the app: the abort of a stash apply,
    /// which git keeps no state to go back to.
    fn destructive(&self) -> Vec<String> {
        match self {
            Self::Resume {
                operation: Operation::StashApply,
                resume: Resume::Abort,
            } => vec![
                "Put the files the stash apply changed back to HEAD, resolutions included; the stash is kept and other local changes stay"
                    .to_owned(),
            ],
            _ => Vec::new(),
        }
    }

    /// Everything but an abort may stop in conflict state.
    fn may_conflict(&self) -> bool {
        !matches!(
            self,
            Self::Resume {
                resume: Resume::Abort,
                ..
            }
        )
    }

    /// Whether it runs on a repo in conflict state (GRP-8).
    fn in_conflict(op: &IntegrateOp) -> bool {
        matches!(op, IntegrateOp::Abort | IntegrateOp::Continue)
    }
}

/// What a request turns into.
enum Planned {
    Run(Plan),
    ChooseMainline {
        commit: String,
        parents: Vec<String>,
    },
}

/// The current branch, or why `what` needs one.
fn current_branch(repo: &GitRepo, what: &str) -> Result<String, IntegrateError> {
    repo.current_branch()
        .ok_or_else(|| refused(format!("HEAD is detached; check out a branch to {what}")))
}

/// The full hash of the commit `revision` names.
fn resolve(repo: &GitRepo, revision: &str) -> Result<String, IntegrateError> {
    if revision.is_empty() || revision.starts_with('-') {
        return Err(refused(format!("'{revision}' names no commit")));
    }
    repo.resolve_commit(revision)
        .ok_or_else(|| refused(format!("'{revision}' names no commit")))
}

/// How a label names commit `hash`: its short hash.
fn short(repo: &GitRepo, hash: &str) -> String {
    repo.commit_text(hash)
        .and_then(|text| text.split(' ').next().map(str::to_owned))
        .unwrap_or_else(|| hash.to_owned())
}

/// What `op` runs in `repo`, or why it is refused. Reads the repo: call it
/// from a thread that may block.
fn plan(repo: &GitRepo, git: &GitRunner, op: IntegrateOp) -> Result<Planned, IntegrateError> {
    match op {
        IntegrateOp::Merge {
            revision,
            fast_forward,
        } => {
            let branch = current_branch(repo, "merge into it")?;
            resolve(repo, &revision)?;
            let ff = match fast_forward {
                MergeFastForward::Config => FastForward::Config,
                MergeFastForward::FfOnly => FastForward::Only,
                MergeFastForward::NoFf => FastForward::Never,
            };
            Ok(Planned::Run(Plan::Merge {
                branch,
                revision,
                ff,
            }))
        }
        IntegrateOp::Rebase { onto } => {
            let branch = current_branch(repo, "rebase it")?;
            resolve(repo, &onto)?;
            Ok(Planned::Run(Plan::Rebase { branch, onto }))
        }
        IntegrateOp::CherryPick { commits } => {
            let mut resolved: Vec<String> = Vec::with_capacity(commits.len());
            for commit in &commits {
                let hash = resolve(repo, commit)?;
                if !resolved.contains(&hash) {
                    resolved.push(hash);
                }
            }
            let commits = repo.oldest_first(&resolved)?;
            let named = match commits.as_slice() {
                [] => return Err(refused("no commit to cherry-pick")),
                [one] => short(repo, one),
                many => format!("{} commits", many.len()),
            };
            Ok(Planned::Run(Plan::CherryPick { commits, named }))
        }
        IntegrateOp::Revert { commit, mainline } => {
            let hash = resolve(repo, &commit)?;
            let named = short(repo, &hash);
            let parents = repo.commit_parents(&hash).unwrap_or_default();
            match (parents.len(), mainline) {
                (0 | 1, None) => {}
                (0 | 1, Some(_)) => {
                    return Err(refused(format!("{named} is not a merge commit")));
                }
                (_, None) => {
                    return Ok(Planned::ChooseMainline {
                        parents: parents
                            .iter()
                            .map(|parent| {
                                repo.commit_text(parent).unwrap_or_else(|| parent.clone())
                            })
                            .collect(),
                        commit: hash,
                    });
                }
                (count, Some(parent)) => {
                    if parent == 0 || usize::try_from(parent).is_ok_and(|parent| parent > count) {
                        return Err(refused(format!("{named} has no parent {parent}")));
                    }
                }
            }
            Ok(Planned::Run(Plan::Revert {
                commit: hash,
                named,
                mainline,
            }))
        }
        IntegrateOp::Abort | IntegrateOp::Continue => {
            let continuing = op == IntegrateOp::Continue;
            let state = StatusReader::new(repo.clone(), git.clone()).repo_state()?;
            let what = if continuing { "continue" } else { "abort" };
            let operation = state
                .operation
                .ok_or_else(|| refused(format!("the repo has no operation to {what}")))?;
            if continuing && operation == Operation::StashApply {
                return Err(refused(
                    "a stash apply has nothing to continue: it ends when the last file is marked resolved",
                ));
            }
            if continuing && state.unmerged_paths > 0 {
                let paths = match state.unmerged_paths {
                    1 => "1 path is".to_owned(),
                    many => format!("{many} paths are"),
                };
                return Err(refused(format!(
                    "{paths} still unmerged; resolve and stage them first"
                )));
            }
            let resume = if continuing {
                Resume::Continue
            } else {
                Resume::Abort
            };
            Ok(Planned::Run(Plan::Resume { operation, resume }))
        }
    }
}

/// Runs `plan` in `path`.
async fn run(
    git: &GitRunner,
    path: &Path,
    plan: &Plan,
    write: Write<'_>,
) -> Result<Output, GitError> {
    match plan {
        Plan::Merge { revision, ff, .. } => ops::merge(git, path, revision, *ff, write).await,
        Plan::Rebase { onto, .. } => ops::rebase(git, path, onto, write).await,
        Plan::CherryPick { commits, .. } => ops::cherry_pick(git, path, commits, write).await,
        Plan::Revert {
            commit, mainline, ..
        } => ops::revert(git, path, commit, *mainline, write).await,
        Plan::Resume { operation, resume } => {
            ops::resume(git, path, *operation, *resume, write).await
        }
    }
}

/// Runs integrate operation `request` on its repo of workspace `id` as a
/// job (section 6 "Integrate"). Refused for a blocked repo (GRP-8), except
/// that abort and continue need conflict state, or for what the operation
/// cannot apply to; a revert of a merge commit asks for its mainline first,
/// and the abort of a stash apply answers what it would do until
/// `confirmed`.
pub async fn op_integrate(
    workspaces: Arc<Workspaces>,
    summaries: Arc<Summaries>,
    jobs: Arc<Jobs>,
    git: GitRunner,
    id: WorkspaceId,
    request: IntegrateRequest,
) -> Result<IntegrateStart, IntegrateError> {
    let IntegrateRequest {
        repo_id,
        op,
        confirmed,
    } = request;
    let in_conflict = Plan::in_conflict(&op);
    let (path, repo) =
        open_unblocked(&workspaces, &summaries, &git, &id, &repo_id, in_conflict).await?;
    let plan = {
        let git = git.clone();
        match tokio::task::spawn_blocking(move || plan(&repo, &git, op)).await?? {
            Planned::Run(plan) => plan,
            Planned::ChooseMainline { commit, parents } => {
                return Ok(IntegrateStart::ChooseMainline { commit, parents });
            }
        }
    };
    let destructive = plan.destructive();
    if !destructive.is_empty() && !confirmed {
        return Ok(IntegrateStart::Confirm {
            repo: repo_name(&workspaces, &id, &repo_id)?,
            actions: destructive,
        });
    }
    let job = WriteJob {
        workspace_id: id,
        repos: vec![repo_id],
        label: plan.label(),
        on_cancel: OnCancel::Finish,
        may_conflict: plan.may_conflict(),
        nothing_to_do: plan.nothing_to_do(),
        network: false,
    };
    let handle = jobs.start(git, job, move |job| async move {
        let path = job.paths.first().cloned().unwrap_or(path);
        run(&job.git, &path, &plan, job.write()).await
    })?;
    Ok(IntegrateStart::Started {
        job_id: handle.job_id,
    })
}

/// What the merge dialog offers in repo `repo` (UI-20): local branches but
/// the current one, remote branches as `origin/x` and tags, each group
/// sorted, those whose name contains `filter` ignoring case; at most
/// [`MAX_MERGE_TARGETS`].
pub async fn merge_targets(
    workspaces: Arc<Workspaces>,
    id: WorkspaceId,
    repo: RepoId,
    filter: String,
) -> Result<Vec<String>, IntegrateError> {
    let path = workspaces.repo_path(&id, &repo)?;
    tokio::task::spawn_blocking(move || {
        let repo = GitRepo::open(&path)?;
        let refs = repo.refs(false)?;
        let current = repo.current_branch();
        let filter = filter.to_lowercase();
        let local = refs
            .local
            .iter()
            .filter(|branch| Some(&branch.name) != current.as_ref())
            .map(|branch| branch.name.clone());
        let remote = refs.remotes.iter().flat_map(|remote| {
            remote
                .branches
                .iter()
                .filter(|branch| branch.as_str() != "HEAD")
                .map(|branch| format!("{}/{branch}", remote.remote))
        });
        let tags = refs.tags.iter().map(|tag| tag.name.clone());
        Ok(local
            .chain(remote)
            .chain(tags)
            .filter(|name| name.to_lowercase().contains(&filter))
            .take(MAX_MERGE_TARGETS)
            .collect())
    })
    .await?
}

/// What the conflict banner of repo `repo` names (OPS-2), read from the
/// repo (OPS-1).
pub async fn conflict_details(
    workspaces: Arc<Workspaces>,
    git: GitRunner,
    id: WorkspaceId,
    repo: RepoId,
) -> Result<ConflictDetails, IntegrateError> {
    let path = workspaces.repo_path(&id, &repo)?;
    tokio::task::spawn_blocking(move || {
        let repo = GitRepo::open(&path)?;
        Ok(StatusReader::new(repo, git).conflict_details()?)
    })
    .await?
}
