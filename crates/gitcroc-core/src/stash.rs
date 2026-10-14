//! Stash, apply, pop and drop in one repo (section 6 "Stash"), each one job
//! (ARCH-7); stash is single-repo only. A repo that is busy, missing or in
//! conflict state is refused (GRP-8). An entry is named by its index and
//! the hash read with the tree, so a stash list that changed since is
//! refused instead of acting on another entry. Drop cannot be undone from
//! the app: without the user's confirmation the core answers with what it
//! would do instead. A stash applied with conflicts ends the job
//! *conflict* (GRP-7); that conflict state is resolved like any other.

use std::path::Path;
use std::sync::Arc;

use gitcroc_git::{GitError, GitRepo, Output, StashEntry, Write, ops};
use serde::{Deserialize, Serialize};

use crate::unblocked::{BlockedError, open_unblocked, repo_name};
use crate::{
    GitRunner, JobError, JobId, Jobs, NothingToDo, OnCancel, OpenError, RepoId, StatusReadError,
    Summaries, WorkspaceError, WorkspaceId, Workspaces, WriteJob,
};

/// A stash entry as the tree showed it: `stash@{index}` with this commit.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
pub struct StashRef {
    pub index: u32,
    /// The stash commit's full hash.
    pub hash: String,
}

/// A stash operation (section 6 "Stash").
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum StashOp {
    /// Stashes the working copy's changes, with `message` unless it is
    /// blank, and its untracked files too when `include_untracked`.
    Push {
        message: Option<String>,
        include_untracked: bool,
    },
    /// Applies the entry to the working copy and keeps it.
    Apply { stash: StashRef },
    /// Applies the entry and drops it; git keeps it when the apply
    /// conflicts.
    Pop { stash: StashRef },
    /// Drops the entry. Cannot be undone from the app.
    Drop { stash: StashRef },
}

/// A stash operation on one repo.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
pub struct StashRequest {
    pub repo_id: RepoId,
    pub op: StashOp,
    /// The user confirmed an operation that cannot be undone.
    pub confirmed: bool,
}

/// What a request did.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, specta::Type)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum StashStart {
    /// The job runs; its end re-reads the repo (OPS-7).
    Started { job_id: JobId },
    /// The operation cannot be undone: ask again with `confirmed` once the
    /// user agreed. `actions` says what is lost.
    Confirm { repo: String, actions: Vec<String> },
}

/// Why a stash operation did not start.
#[derive(Debug, thiserror::Error)]
pub enum StashError {
    #[error(transparent)]
    Workspace(#[from] WorkspaceError),
    #[error(transparent)]
    Job(#[from] JobError),
    #[error(transparent)]
    Open(#[from] OpenError),
    #[error(transparent)]
    Status(#[from] StatusReadError),
    #[error("{0}")]
    Refused(String),
    #[error("the read did not finish: {0}")]
    Stopped(#[from] tokio::task::JoinError),
}

impl From<BlockedError> for StashError {
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

fn refused(why: impl Into<String>) -> StashError {
    StashError::Refused(why.into())
}

/// What a job runs.
#[derive(Debug, Clone)]
enum Plan {
    Push {
        message: Option<String>,
        untracked: bool,
    },
    /// `entry` as the stash list has it now.
    Apply {
        entry: StashEntry,
        pop: bool,
    },
    Drop {
        entry: StashEntry,
    },
}

/// `stash@{n}` and its message, as labels and confirmations name it.
fn named(entry: &StashEntry) -> String {
    format!("stash@{{{}}} ({})", entry.index, entry.message)
}

impl Plan {
    /// The job's label.
    fn label(&self) -> String {
        match self {
            Self::Push {
                message: Some(message),
                ..
            } => format!("Stash changes as \"{message}\""),
            Self::Push { message: None, .. } => "Stash changes".to_owned(),
            Self::Apply { entry, pop: false } => format!("Apply {}", named(entry)),
            Self::Apply { entry, pop: true } => format!("Pop {}", named(entry)),
            Self::Drop { entry } => format!("Drop {}", named(entry)),
        }
    }

    /// What cannot be undone from the app.
    fn destructive(&self) -> Vec<String> {
        match self {
            Self::Drop { entry } => vec![format!(
                "Drop {}; its changes are lost unless they were applied",
                named(entry)
            )],
            _ => Vec::new(),
        }
    }

    /// A push with nothing to stash changes no ref (GRP-7).
    fn nothing_to_do(&self) -> NothingToDo {
        match self {
            Self::Push { .. } => NothingToDo::WhenRefsUnchanged,
            _ => NothingToDo::Never,
        }
    }

    /// An apply or pop may leave unmerged paths.
    fn may_conflict(&self) -> bool {
        matches!(self, Self::Apply { .. })
    }
}

/// The entry `stash` names, refused when the stash list changed since it
/// was read.
fn entry_of(repo: &GitRepo, stash: &StashRef) -> Result<StashEntry, StashError> {
    repo.stashes()
        .into_iter()
        .find(|entry| entry.index == stash.index && entry.hash == stash.hash)
        .ok_or_else(|| {
            refused(format!(
                "the stash list changed since it was read: stash@{{{}}} is no longer that stash",
                stash.index
            ))
        })
}

/// What `op` runs in `repo`, or why it is refused. Reads the repo: call it
/// from a thread that may block.
fn plan(repo: &GitRepo, op: StashOp) -> Result<Plan, StashError> {
    match op {
        StashOp::Push {
            message,
            include_untracked,
        } => {
            let message = message
                .map(|message| message.trim().to_owned())
                .filter(|message| !message.is_empty());
            if message
                .as_ref()
                .is_some_and(|message| message.contains('\0'))
            {
                return Err(refused("the message cannot contain a NUL character"));
            }
            Ok(Plan::Push {
                message,
                untracked: include_untracked,
            })
        }
        StashOp::Apply { stash } => Ok(Plan::Apply {
            entry: entry_of(repo, &stash)?,
            pop: false,
        }),
        StashOp::Pop { stash } => Ok(Plan::Apply {
            entry: entry_of(repo, &stash)?,
            pop: true,
        }),
        StashOp::Drop { stash } => Ok(Plan::Drop {
            entry: entry_of(repo, &stash)?,
        }),
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
        Plan::Push { message, untracked } => {
            ops::stash_push(git, path, message.as_deref(), *untracked, write).await
        }
        Plan::Apply { entry, pop } => ops::stash_apply(git, path, entry.index, *pop, write).await,
        Plan::Drop { entry } => ops::stash_drop(git, path, entry.index, write).await,
    }
}

/// Runs stash operation `request` on its repo of workspace `id` as a job
/// (section 6 "Stash"). Refused for a blocked repo (GRP-8) or an entry that
/// is no longer the one read; a drop answers what it would do until
/// `confirmed`.
pub async fn op_stash(
    workspaces: Arc<Workspaces>,
    summaries: Arc<Summaries>,
    jobs: Arc<Jobs>,
    git: GitRunner,
    id: WorkspaceId,
    request: StashRequest,
) -> Result<StashStart, StashError> {
    let StashRequest {
        repo_id,
        op,
        confirmed,
    } = request;
    let (path, repo) = open_unblocked(&workspaces, &summaries, &git, &id, &repo_id, false).await?;
    let plan = tokio::task::spawn_blocking(move || plan(&repo, op)).await??;
    let destructive = plan.destructive();
    if !destructive.is_empty() && !confirmed {
        return Ok(StashStart::Confirm {
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
    Ok(StashStart::Started {
        job_id: handle.job_id,
    })
}
