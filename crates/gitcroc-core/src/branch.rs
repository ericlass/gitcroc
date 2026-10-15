//! Branch operations of one repo (section 6 "Branches", UI-7): checkout of
//! a local branch, a remote branch or a commit, create, rename, delete and
//! set upstream, each one job (ARCH-7). Every name is checked against git's
//! ref format rules before anything runs (SEC-4). A repo that is busy,
//! missing or in conflict state is refused (GRP-8). Force delete and
//! delete on a remote are destructive (GRP-9): without the user's
//! confirmation the core answers with what they would do instead.

use std::sync::Arc;

use gitcroc_git::{GitError, GitRepo, Output, TrackedBranch, ops};
use serde::{Deserialize, Serialize};

use crate::ref_name::{RefNameError, check_branch_name};
use crate::unblocked::{from_blocked, open_unblocked, repo_name};
use crate::{
    GitRunner, JobError, JobId, Jobs, NothingToDo, OpenError, RepoId, StatusReadError, Summaries,
    WorkspaceError, WorkspaceId, Workspaces, WriteJob,
};

/// A branch of a remote, such as `feature/x` of `origin`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
pub struct RemoteBranchRef {
    pub remote: String,
    /// Without the remote.
    pub branch: String,
}

/// Where a new branch starts.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum StartPoint {
    /// The commit checked out now.
    Head,
    /// The default branch (glossary).
    DefaultBranch,
    /// A ref or a commit hash.
    Revision { revision: String },
}

/// A branch operation (section 6 "Branches").
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum BranchOp {
    /// Checks out local branch `name`.
    Checkout {
        name: String,
    },
    /// Checks out a remote branch: the local branch of the same name when
    /// there is one, else a new one that tracks it.
    CheckoutRemote {
        branch: RemoteBranchRef,
    },
    /// Checks out the commit `revision` names, a hash or a tag, with a
    /// detached `HEAD`.
    CheckoutDetached {
        revision: String,
    },
    /// Creates branch `name` at `start`, checked out when `switch`.
    Create {
        name: String,
        start: StartPoint,
        switch: bool,
    },
    Rename {
        name: String,
        new_name: String,
    },
    /// Deletes local branch `name`. `force` deletes it when not merged;
    /// `on_remote` also deletes the branch it tracks on its remote. Both
    /// are destructive (GRP-9).
    Delete {
        name: String,
        force: bool,
        on_remote: bool,
    },
    /// Deletes a branch on its remote. Destructive (GRP-9).
    DeleteRemote {
        branch: RemoteBranchRef,
    },
    /// Makes branch `name` track `upstream`, or nothing when `None`.
    SetUpstream {
        name: String,
        upstream: Option<RemoteBranchRef>,
    },
}

/// A branch operation on one repo.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
pub struct BranchRequest {
    pub repo_id: RepoId,
    pub op: BranchOp,
    /// The user confirmed a destructive operation (GRP-9).
    pub confirmed: bool,
}

/// What a request did.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, specta::Type)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum BranchStart {
    /// The job runs; its end re-reads the repo (OPS-7).
    Started { job_id: JobId },
    /// The operation is destructive (GRP-9): ask again with `confirmed`
    /// once the user agreed. `actions` says what cannot be undone.
    Confirm { repo: String, actions: Vec<String> },
}

/// What the branch dialogs offer for one repo.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, specta::Type)]
pub struct BranchChoices {
    /// Local branches, remote branches with their remote, and tags, each
    /// group sorted: start points to pick from.
    pub start_points: Vec<String>,
    /// The default branch as git shortens it, such as `origin/main`.
    pub default_branch: Option<String>,
    /// Every remote branch: upstreams to pick from.
    pub remote_branches: Vec<RemoteBranchRef>,
}

/// Why a branch operation did not start.
#[derive(Debug, thiserror::Error)]
pub enum BranchError {
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
    #[error(transparent)]
    Name(#[from] RefNameError),
    #[error("{0}")]
    Refused(String),
    #[error("the read did not finish: {0}")]
    Stopped(#[from] tokio::task::JoinError),
}

from_blocked!(BranchError);

fn refused(why: impl Into<String>) -> BranchError {
    BranchError::Refused(why.into())
}

/// `name` checked as a branch name (SEC-4): why it is not one, or `None`.
pub fn branch_name_check(name: &str) -> Option<String> {
    check_branch_name(name).err().map(|error| error.to_string())
}

/// What a job runs.
#[derive(Debug, Clone)]
enum Plan {
    Switch(String),
    /// Creates a local branch tracking `remote/branch` and checks it out.
    SwitchTrack(RemoteBranchRef),
    /// The full hash, and what the user named it by.
    Detach {
        hash: String,
        named: String,
    },
    Create {
        name: String,
        start: Option<String>,
        switch: bool,
    },
    Rename {
        old: String,
        new: String,
    },
    Delete {
        name: String,
        force: bool,
        remote: Option<TrackedBranch>,
    },
    DeleteRemote(RemoteBranchRef),
    SetUpstream {
        name: String,
        upstream: Option<RemoteBranchRef>,
    },
}

impl Plan {
    /// The job's label.
    fn label(&self) -> String {
        match self {
            Self::Switch(name) => format!("Check out {name}"),
            Self::SwitchTrack(branch) => format!("Check out {}/{}", branch.remote, branch.branch),
            Self::Detach { named, .. } => format!("Check out {named} (detached)"),
            Self::Create { name, .. } => format!("Create branch {name}"),
            Self::Rename { old, new } => format!("Rename {old} to {new}"),
            Self::Delete {
                name,
                force,
                remote,
            } => {
                let verb = if *force { "Force delete" } else { "Delete" };
                match remote {
                    Some(remote) => format!(
                        "{verb} branch {name} and {}/{} on the remote",
                        remote.remote, remote.branch
                    ),
                    None => format!("{verb} branch {name}"),
                }
            }
            Self::DeleteRemote(branch) => {
                format!("Delete {}/{} on the remote", branch.remote, branch.branch)
            }
            Self::SetUpstream {
                name,
                upstream: Some(upstream),
            } => format!(
                "Set the upstream of {name} to {}/{}",
                upstream.remote, upstream.branch
            ),
            Self::SetUpstream {
                name,
                upstream: None,
            } => format!("Unset the upstream of {name}"),
        }
    }

    /// What cannot be undone, for the confirmation (GRP-9); empty when
    /// nothing is destructive.
    fn destructive(&self) -> Vec<String> {
        match self {
            Self::Delete {
                name,
                force,
                remote,
            } => {
                let mut actions = Vec::new();
                if *force {
                    actions.push(format!(
                        "Delete branch {name} even if it is not merged; its commits may be lost"
                    ));
                }
                if let Some(remote) = remote {
                    actions.push(format!(
                        "Delete {} on the remote {}",
                        remote.branch, remote.remote
                    ));
                }
                actions
            }
            Self::DeleteRemote(branch) => vec![format!(
                "Delete {} on the remote {}",
                branch.branch, branch.remote
            )],
            _ => Vec::new(),
        }
    }

    /// A checkout that may find the branch already checked out: *nothing
    /// to do* when no ref and not `HEAD` changed.
    fn nothing_to_do(&self) -> NothingToDo {
        match self {
            Self::Switch(_) | Self::SwitchTrack(_) | Self::Detach { .. } => {
                NothingToDo::WhenRefsUnchanged
            }
            _ => NothingToDo::Never,
        }
    }

    /// It contacts a remote, so git gets the askpass helper (D10).
    fn network(&self) -> bool {
        matches!(
            self,
            Self::DeleteRemote(_)
                | Self::Delete {
                    remote: Some(_),
                    ..
                }
        )
    }
}

/// Runs `plan` in `path`.
async fn run(
    git: &GitRunner,
    path: &std::path::Path,
    plan: &Plan,
    write: gitcroc_git::Write<'_>,
) -> Result<Output, GitError> {
    match plan {
        Plan::Switch(name) => ops::switch(git, path, name, write).await,
        Plan::SwitchTrack(branch) => {
            let tracked = format!("{}/{}", branch.remote, branch.branch);
            ops::switch_track(git, path, &tracked, write).await
        }
        Plan::Detach { hash, .. } => ops::switch_detach(git, path, hash, write).await,
        Plan::Create {
            name,
            start,
            switch,
        } => ops::branch_create(git, path, name, start.as_deref(), *switch, write).await,
        Plan::Rename { old, new } => ops::branch_rename(git, path, old, new, write).await,
        Plan::Delete {
            name,
            force,
            remote,
        } => {
            let mut all = ops::branch_delete(git, path, name, *force, write).await?;
            if let Some(remote) = remote {
                all.append(
                    ops::remote_branch_delete(git, path, &remote.remote, &remote.branch, write)
                        .await?,
                );
            }
            Ok(all)
        }
        Plan::DeleteRemote(branch) => {
            ops::remote_branch_delete(git, path, &branch.remote, &branch.branch, write).await
        }
        Plan::SetUpstream { name, upstream } => {
            let upstream = upstream
                .as_ref()
                .map(|upstream| format!("{}/{}", upstream.remote, upstream.branch));
            ops::branch_set_upstream(git, path, name, upstream.as_deref(), write).await
        }
    }
}

/// `branch` names a configured remote and a valid branch name.
fn check_remote_branch(repo: &GitRepo, branch: &RemoteBranchRef) -> Result<(), BranchError> {
    if !repo.remote_names().contains(&branch.remote) {
        return Err(refused(format!("there is no remote {}", branch.remote)));
    }
    check_branch_name(&branch.branch)?;
    Ok(())
}

/// `name` is a valid name of an existing local branch.
fn check_local(repo: &GitRepo, name: &str) -> Result<(), BranchError> {
    check_branch_name(name)?;
    if repo.has_local_branch(name) {
        Ok(())
    } else {
        Err(refused(format!("there is no local branch {name}")))
    }
}

/// What `op` runs in `repo`, or why it is refused. Reads the repo: call it
/// from a thread that may block.
fn plan(repo: &GitRepo, op: BranchOp) -> Result<Plan, BranchError> {
    match op {
        BranchOp::Checkout { name } => {
            check_local(repo, &name)?;
            Ok(Plan::Switch(name))
        }
        BranchOp::CheckoutRemote { branch } => {
            check_remote_branch(repo, &branch)?;
            if !repo.has_remote_branch(&branch.remote, &branch.branch) {
                return Err(refused(format!(
                    "there is no remote branch {}/{}",
                    branch.remote, branch.branch
                )));
            }
            if repo.has_local_branch(&branch.branch) {
                Ok(Plan::Switch(branch.branch))
            } else {
                Ok(Plan::SwitchTrack(branch))
            }
        }
        BranchOp::CheckoutDetached { revision } => {
            let hash = resolve(repo, &revision)?;
            Ok(Plan::Detach {
                hash,
                named: revision,
            })
        }
        BranchOp::Create {
            name,
            start,
            switch,
        } => {
            check_branch_name(&name)?;
            if repo.has_local_branch(&name) {
                return Err(refused(format!("a branch named {name} exists already")));
            }
            let start = match start {
                StartPoint::Head => None,
                StartPoint::DefaultBranch => Some(
                    repo.default_branch()
                        .ok_or_else(|| refused("the repo has no default branch"))?,
                ),
                StartPoint::Revision { revision } => {
                    resolve(repo, &revision)?;
                    Some(revision)
                }
            };
            Ok(Plan::Create {
                name,
                start,
                switch,
            })
        }
        BranchOp::Rename { name, new_name } => {
            check_local(repo, &name)?;
            check_branch_name(&new_name)?;
            Ok(Plan::Rename {
                old: name,
                new: new_name,
            })
        }
        BranchOp::Delete {
            name,
            force,
            on_remote,
        } => {
            check_local(repo, &name)?;
            if repo.current_branch().as_deref() == Some(name.as_str()) {
                return Err(refused("the current branch cannot be deleted"));
            }
            let remote = if on_remote {
                Some(
                    repo.tracked_branch(&name)
                        .ok_or_else(|| refused(format!("{name} tracks no remote branch")))?,
                )
            } else {
                None
            };
            Ok(Plan::Delete {
                name,
                force,
                remote,
            })
        }
        BranchOp::DeleteRemote { branch } => {
            check_remote_branch(repo, &branch)?;
            Ok(Plan::DeleteRemote(branch))
        }
        BranchOp::SetUpstream { name, upstream } => {
            check_local(repo, &name)?;
            if let Some(upstream) = &upstream {
                check_remote_branch(repo, upstream)?;
                if !repo.has_remote_branch(&upstream.remote, &upstream.branch) {
                    return Err(refused(format!(
                        "there is no remote branch {}/{}",
                        upstream.remote, upstream.branch
                    )));
                }
            }
            Ok(Plan::SetUpstream { name, upstream })
        }
    }
}

/// The full hash of the commit `revision` names.
fn resolve(repo: &GitRepo, revision: &str) -> Result<String, BranchError> {
    if revision.is_empty() || revision.starts_with('-') {
        return Err(refused(format!("'{revision}' names no commit")));
    }
    repo.resolve_commit(revision)
        .ok_or_else(|| refused(format!("'{revision}' names no commit")))
}

/// Runs branch operation `request` on its repo of workspace `id` as a job
/// (section 6 "Branches"). Refused for an invalid name (SEC-4), a blocked
/// repo (GRP-8) or what the operation cannot apply to; a destructive one
/// answers what it would do until `confirmed` (GRP-9).
pub async fn op_branch(
    workspaces: Arc<Workspaces>,
    summaries: Arc<Summaries>,
    jobs: Arc<Jobs>,
    git: GitRunner,
    id: WorkspaceId,
    request: BranchRequest,
) -> Result<BranchStart, BranchError> {
    let BranchRequest {
        repo_id,
        op,
        confirmed,
    } = request;
    let (path, repo) = open_unblocked(&workspaces, &summaries, &git, &id, &repo_id, false).await?;
    let plan = tokio::task::spawn_blocking(move || plan(&repo, op)).await??;
    let destructive = plan.destructive();
    if !destructive.is_empty() && !confirmed {
        return Ok(BranchStart::Confirm {
            repo: repo_name(&workspaces, &id, &repo_id)?,
            actions: destructive,
        });
    }
    let job = WriteJob {
        nothing_to_do: plan.nothing_to_do(),
        network: plan.network(),
        ..WriteJob::single(id, repo_id, plan.label())
    };
    let handle = jobs.start(git, job, move |job| async move {
        run(&job.git, &path, &plan, job.write()).await
    })?;
    Ok(BranchStart::Started {
        job_id: handle.job_id,
    })
}

/// What the branch dialogs of repo `repo` offer: start points, the default
/// branch and the remote branches to track.
pub async fn branch_choices(
    workspaces: Arc<Workspaces>,
    id: WorkspaceId,
    repo: RepoId,
) -> Result<BranchChoices, BranchError> {
    let path = workspaces.repo_path(&id, &repo)?;
    tokio::task::spawn_blocking(move || {
        let repo = GitRepo::open(&path)?;
        let refs = repo.refs(false)?;
        let remote_branches: Vec<RemoteBranchRef> = refs
            .remotes
            .iter()
            .flat_map(|remote| {
                remote.branches.iter().map(|branch| RemoteBranchRef {
                    remote: remote.remote.clone(),
                    branch: branch.clone(),
                })
            })
            .collect();
        let start_points = refs
            .local
            .iter()
            .map(|branch| branch.name.clone())
            .chain(
                remote_branches
                    .iter()
                    .map(|branch| format!("{}/{}", branch.remote, branch.branch)),
            )
            .chain(refs.tags.iter().map(|tag| tag.name.clone()))
            .collect();
        let default_branch = repo.default_branch().map(|full| {
            full.strip_prefix("refs/heads/")
                .or_else(|| full.strip_prefix("refs/remotes/"))
                .unwrap_or(&full)
                .to_owned()
        });
        Ok(BranchChoices {
            start_points,
            default_branch,
            remote_branches,
        })
    })
    .await?
}
