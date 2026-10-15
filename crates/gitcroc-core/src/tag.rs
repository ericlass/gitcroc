//! Tag operations of one repo (section 6 "Tags", section 5 "Create tag X"
//! for one repo, UI-7, UI-13): create, delete and push, each one job
//! (ARCH-7). Every name is checked against git's ref format rules before
//! anything runs (SEC-4). A repo that is busy, missing or in conflict state
//! is refused (GRP-8). A push goes to `origin`, else to the only remote,
//! else the core asks which. A delete on the remote cannot be undone from
//! the app: without the user's confirmation the core answers with what it
//! would do instead.

use std::path::Path;
use std::sync::Arc;

use gitcroc_git::{GitError, GitRepo, Output, Write, ops, parse_progress};
use serde::{Deserialize, Serialize};

use crate::ref_name::{RefNameError, check_tag_name};
use crate::sync::default_remote;
use crate::unblocked::{from_blocked, open_unblocked, repo_name};
use crate::{
    GitRunner, JobError, JobId, JobRun, Jobs, OpenError, RepoId, StatusReadError, Summaries,
    WorkspaceError, WorkspaceId, Workspaces, WriteJob,
};

/// A tag operation (section 6 "Tags"). `remote` picks where a push or a
/// delete on the remote goes; `None` lets the core pick `origin` or the
/// only remote.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum TagOp {
    /// Creates tag `name` at `commit`, a hash, or at `HEAD` when `None`:
    /// annotated with `message` when `annotated`, else lightweight. With
    /// `push`, the same job then pushes it.
    Create {
        name: String,
        commit: Option<String>,
        annotated: bool,
        message: String,
        push: bool,
        remote: Option<String>,
    },
    /// Deletes local tag `name`, and with `on_remote` first the tag of that
    /// name on the remote, which cannot be undone from the app.
    Delete {
        name: String,
        on_remote: bool,
        remote: Option<String>,
    },
    /// Pushes tag `name`.
    Push {
        name: String,
        remote: Option<String>,
    },
}

/// A tag operation on one repo.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
pub struct TagRequest {
    pub repo_id: RepoId,
    pub op: TagOp,
    /// The user confirmed an operation that cannot be undone.
    pub confirmed: bool,
}

/// What a request did.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, specta::Type)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum TagStart {
    /// The job runs; its end re-reads the repo (OPS-7).
    Started { job_id: JobId },
    /// The operation cannot be undone: ask again with `confirmed` once the
    /// user agreed. `actions` says what is lost.
    Confirm { repo: String, actions: Vec<String> },
    /// The repo has several remotes and no `origin`: ask again with one of
    /// `remotes`.
    ChooseRemote { remotes: Vec<String> },
}

/// Why a tag operation did not start.
#[derive(Debug, thiserror::Error)]
pub enum TagError {
    #[error(transparent)]
    Workspace(#[from] WorkspaceError),
    #[error(transparent)]
    Job(#[from] JobError),
    #[error(transparent)]
    Open(#[from] OpenError),
    #[error(transparent)]
    Status(#[from] StatusReadError),
    #[error(transparent)]
    Name(#[from] RefNameError),
    #[error("{0}")]
    Refused(String),
    #[error("the read did not finish: {0}")]
    Stopped(#[from] tokio::task::JoinError),
}

from_blocked!(TagError);

fn refused(why: impl Into<String>) -> TagError {
    TagError::Refused(why.into())
}

/// `name` checked as a tag name (SEC-4): why it is not one, or `None`.
pub fn tag_name_check(name: &str) -> Option<String> {
    check_tag_name(name).err().map(|error| error.to_string())
}

/// What a job runs.
#[derive(Debug, Clone)]
pub(crate) enum Plan {
    Create {
        name: String,
        /// The full hash.
        commit: String,
        /// `None` for a lightweight tag.
        message: Option<String>,
        /// Where it is pushed after it was created.
        push: Option<String>,
    },
    Delete {
        name: String,
        /// The remote it is deleted on first.
        remote: Option<String>,
    },
    Push {
        name: String,
        remote: String,
    },
}

impl Plan {
    /// The job's label.
    fn label(&self) -> String {
        match self {
            Self::Create {
                name,
                push: Some(remote),
                ..
            } => format!("Create tag {name} and push it to {remote}"),
            Self::Create { name, .. } => format!("Create tag {name}"),
            Self::Delete {
                name,
                remote: Some(remote),
            } => format!("Delete tag {name} here and on {remote}"),
            Self::Delete { name, remote: None } => format!("Delete tag {name}"),
            Self::Push { name, remote } => format!("Push tag {name} to {remote}"),
        }
    }

    /// What cannot be undone from the app; empty when nothing is.
    fn destructive(&self) -> Vec<String> {
        match self {
            Self::Delete {
                name,
                remote: Some(remote),
            } => vec![format!(
                "Delete tag {name} on the remote {remote}, for everyone who fetches from it"
            )],
            _ => Vec::new(),
        }
    }

    /// It contacts a remote, so git gets the askpass helper (D10).
    fn network(&self) -> bool {
        match self {
            Self::Create { push, .. } => push.is_some(),
            Self::Delete { remote, .. } => remote.is_some(),
            Self::Push { .. } => true,
        }
    }
}

/// What a request turns into.
enum Planned {
    Run(Plan),
    /// The remote must be picked from these.
    ChooseRemote(Vec<String>),
}

/// The remote a push or delete on the remote goes to: `picked` when it
/// exists, else `origin` or the only remote; `None` when the user must
/// pick one.
fn remote_of(
    repo: &GitRepo,
    picked: Option<String>,
) -> Result<Result<String, Vec<String>>, TagError> {
    let remotes = repo.remote_names();
    match picked {
        Some(remote) if remotes.contains(&remote) => Ok(Ok(remote)),
        Some(remote) => Err(refused(format!("there is no remote {remote}"))),
        None => match default_remote(&remotes) {
            Some(remote) => Ok(Ok(remote)),
            None if remotes.is_empty() => Err(refused("the repo has no remote")),
            None => Ok(Err(remotes)),
        },
    }
}

/// `name` is a valid name of an existing tag.
fn check_existing(repo: &GitRepo, name: &str) -> Result<(), TagError> {
    check_tag_name(name)?;
    if repo.has_tag(name) {
        Ok(())
    } else {
        Err(refused(format!("there is no tag {name}")))
    }
}

/// What `op` runs in `repo`, or why it is refused. Reads the repo: call it
/// from a thread that may block.
fn plan(repo: &GitRepo, op: TagOp) -> Result<Planned, TagError> {
    match op {
        TagOp::Create {
            name,
            commit,
            annotated,
            message,
            push,
            remote,
        } => {
            check_tag_name(&name)?;
            if repo.has_tag(&name) {
                return Err(refused(format!("a tag named {name} exists already")));
            }
            let commit = match commit {
                Some(commit) if commit.is_empty() || commit.starts_with('-') => {
                    return Err(refused(format!("'{commit}' names no commit")));
                }
                Some(commit) => repo
                    .resolve_commit(&commit)
                    .ok_or_else(|| refused(format!("'{commit}' names no commit")))?,
                None => repo
                    .resolve_commit("HEAD")
                    .ok_or_else(|| refused("the branch has no commit to tag yet"))?,
            };
            let message = if annotated {
                if message.trim().is_empty() {
                    return Err(refused("an annotated tag needs a message"));
                }
                if message.contains('\0') {
                    return Err(refused("the message cannot contain a NUL character"));
                }
                Some(message)
            } else {
                None
            };
            let push = if push {
                match remote_of(repo, remote)? {
                    Ok(remote) => Some(remote),
                    Err(remotes) => return Ok(Planned::ChooseRemote(remotes)),
                }
            } else {
                None
            };
            Ok(Planned::Run(Plan::Create {
                name,
                commit,
                message,
                push,
            }))
        }
        TagOp::Delete {
            name,
            on_remote,
            remote,
        } => {
            check_existing(repo, &name)?;
            let remote = if on_remote {
                match remote_of(repo, remote)? {
                    Ok(remote) => Some(remote),
                    Err(remotes) => return Ok(Planned::ChooseRemote(remotes)),
                }
            } else {
                None
            };
            Ok(Planned::Run(Plan::Delete { name, remote }))
        }
        TagOp::Push { name, remote } => {
            check_existing(repo, &name)?;
            match remote_of(repo, remote)? {
                Ok(remote) => Ok(Planned::Run(Plan::Push { name, remote })),
                Err(remotes) => Ok(Planned::ChooseRemote(remotes)),
            }
        }
    }
}

/// A tag created on a target set (section 5 "Create tag X"): `name` was
/// checked once (SEC-4), and an annotated tag's `message` too.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct GroupTag {
    pub(crate) name: String,
    /// `None` for a lightweight tag.
    pub(crate) message: Option<String>,
    pub(crate) push: bool,
}

/// What creating `tag` runs in `repo` as part of a group operation, at the
/// repo's own `HEAD` (GRP-14); or why the repo is skipped. A push goes
/// where a single repo's goes (`origin`, else the only remote); a repo
/// where the user would have to pick is skipped, as nobody is asked
/// during a group run. Reads the repo: call it from a thread that may
/// block.
pub(crate) fn group_plan(repo: &GitRepo, tag: &GroupTag) -> Result<Plan, String> {
    let name = &tag.name;
    if repo.has_tag(name) {
        return Err(format!("{name} already exists"));
    }
    let commit = repo
        .resolve_commit("HEAD")
        .ok_or_else(|| "no commit to tag yet".to_owned())?;
    let push = if tag.push {
        let remotes = repo.remote_names();
        match default_remote(&remotes) {
            Some(remote) => Some(remote),
            None if remotes.is_empty() => return Err("no remote to push the tag to".to_owned()),
            None => {
                return Err("several remotes and no origin; push the tag from the repo".to_owned());
            }
        }
    } else {
        None
    };
    Ok(Plan::Create {
        name: name.clone(),
        commit,
        message: tag.message.clone(),
        push,
    })
}

/// Runs `plan` in `path`, a push's progress lines reported as the job's
/// steps. A delete on the remote runs first, so the local tag stays when
/// it fails.
pub(crate) async fn run_plan(job: &JobRun, path: &Path, plan: &Plan) -> Result<Output, GitError> {
    let report = |line: &str| {
        if let Some(progress) = parse_progress(line) {
            job.step(&progress.step, progress.done, progress.total);
        }
    };
    let write = Write {
        on_line: Some(&report),
        ..job.write()
    };
    let git = &job.git;
    match plan {
        Plan::Create {
            name,
            commit,
            message,
            push,
        } => {
            let message = message.as_deref().map(str::as_bytes);
            let mut all = ops::tag_create(git, path, name, commit, message, write).await?;
            if let Some(remote) = push {
                all.append(ops::tag_push(git, path, remote, name, write).await?);
            }
            Ok(all)
        }
        Plan::Delete { name, remote } => {
            let mut all = Output::default();
            if let Some(remote) = remote {
                all = ops::remote_tag_delete(git, path, remote, name, write).await?;
            }
            all.append(ops::tag_delete(git, path, name, write).await?);
            Ok(all)
        }
        Plan::Push { name, remote } => ops::tag_push(git, path, remote, name, write).await,
    }
}

/// Runs tag operation `request` on its repo of workspace `id` as a job
/// (section 6 "Tags"). Refused for an invalid name (SEC-4), a blocked repo
/// (GRP-8), an existing name to create or a missing tag; a delete on the
/// remote answers what it would do until `confirmed`.
pub async fn op_tag(
    workspaces: Arc<Workspaces>,
    summaries: Arc<Summaries>,
    jobs: Arc<Jobs>,
    git: GitRunner,
    id: WorkspaceId,
    request: TagRequest,
) -> Result<TagStart, TagError> {
    let TagRequest {
        repo_id,
        op,
        confirmed,
    } = request;
    let (path, repo) = open_unblocked(&workspaces, &summaries, &git, &id, &repo_id, false).await?;
    let plan = match tokio::task::spawn_blocking(move || plan(&repo, op)).await?? {
        Planned::Run(plan) => plan,
        Planned::ChooseRemote(remotes) => return Ok(TagStart::ChooseRemote { remotes }),
    };
    let destructive = plan.destructive();
    if !destructive.is_empty() && !confirmed {
        return Ok(TagStart::Confirm {
            repo: repo_name(&workspaces, &id, &repo_id)?,
            actions: destructive,
        });
    }
    let job = WriteJob {
        network: plan.network(),
        ..WriteJob::single(id, repo_id, plan.label())
    };
    let handle = jobs.start(git, job, move |job| async move {
        run_plan(&job, &path, &plan).await
    })?;
    Ok(TagStart::Started {
        job_id: handle.job_id,
    })
}
