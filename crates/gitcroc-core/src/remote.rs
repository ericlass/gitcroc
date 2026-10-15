//! Remotes of one repo (section 6 "Remotes", UI-7): add, rename, remove and
//! edit URLs, each one job (ARCH-7). A new name is checked against git's
//! ref format rules before anything runs (SEC-4); a URL that would read as
//! an option is refused. A repo that is busy, missing or in conflict state
//! is refused (GRP-8). Removing a remote unsets the upstreams on it: without
//! the user's confirmation the core answers with what it would do instead.

use std::path::Path;
use std::sync::Arc;

use gitcroc_git::{GitError, GitRepo, Output, Write, ops, parse_progress};
use serde::{Deserialize, Serialize};

use crate::ref_name::{RefNameError, check_remote_name};
use crate::unblocked::{from_blocked, open_unblocked, repo_name};
use crate::{
    GitRunner, JobError, JobId, JobRun, Jobs, OpenError, RepoId, StatusReadError, Summaries,
    WorkspaceError, WorkspaceId, Workspaces, WriteJob,
};

/// A remote operation (section 6 "Remotes"). URLs are passed to git as
/// typed, without spaces around them.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum RemoteOp {
    /// Adds remote `name` at `url`; with `fetch`, the same job then
    /// fetches it.
    Add {
        name: String,
        url: String,
        fetch: bool,
    },
    /// Renames remote `name` to `new_name`; git moves its remote-tracking
    /// branches and the upstreams on it.
    Rename { name: String, new_name: String },
    /// Removes remote `name` with its remote-tracking branches; the
    /// upstreams on it are unset.
    Remove { name: String },
    /// Sets remote `name`'s URL, and its push URL when `push_url` is given.
    SetUrl {
        name: String,
        url: String,
        push_url: Option<String>,
    },
}

/// A remote operation on one repo.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
pub struct RemoteRequest {
    pub repo_id: RepoId,
    pub op: RemoteOp,
    /// The user confirmed what a remove unsets.
    pub confirmed: bool,
}

/// What a request did.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, specta::Type)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum RemoteStart {
    /// The job runs; its end re-reads the repo (OPS-7).
    Started { job_id: JobId },
    /// A remove: ask again with `confirmed` once the user agreed. `actions`
    /// says what it removes and which upstreams it unsets.
    Confirm { repo: String, actions: Vec<String> },
}

/// A remote's URLs, for the edit dialog.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, specta::Type)]
pub struct RemoteUrlsView {
    /// The fetch URL, and the push URL unless `push_url` is set.
    pub url: Option<String>,
    /// A separate push URL, when one is set.
    pub push_url: Option<String>,
}

/// Why a remote operation did not start.
#[derive(Debug, thiserror::Error)]
pub enum RemoteError {
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

from_blocked!(RemoteError);

fn refused(why: impl Into<String>) -> RemoteError {
    RemoteError::Refused(why.into())
}

/// `name` checked as the name of a new remote (SEC-4): why it is not one,
/// or `None`.
pub fn remote_name_check(name: &str) -> Option<String> {
    check_remote_name(name).err().map(|error| error.to_string())
}

/// `url` without spaces around it, or why git should not get it. Clone
/// checks its URL the same way.
pub(crate) fn check_url(url: &str) -> Result<String, &'static str> {
    let url = url.trim();
    if url.is_empty() {
        return Err("the URL is empty");
    }
    if url.starts_with('-') {
        return Err("the URL cannot start with '-'");
    }
    if url.chars().any(char::is_control) {
        return Err("the URL cannot contain control characters");
    }
    Ok(url.to_owned())
}

fn checked_url(url: &str) -> Result<String, RemoteError> {
    check_url(url).map_err(refused)
}

/// What a job runs.
#[derive(Debug, Clone)]
enum Plan {
    Add {
        name: String,
        url: String,
        fetch: bool,
    },
    Rename {
        name: String,
        new_name: String,
    },
    Remove {
        name: String,
        /// The local branches whose upstream is on it, as
        /// `main (origin/main)`.
        upstreams: Vec<String>,
    },
    SetUrl {
        name: String,
        url: String,
        push_url: Option<String>,
    },
}

impl Plan {
    /// The job's label.
    fn label(&self) -> String {
        match self {
            Self::Add {
                name, fetch: true, ..
            } => format!("Add remote {name} and fetch it"),
            Self::Add { name, .. } => format!("Add remote {name}"),
            Self::Rename { name, new_name } => format!("Rename remote {name} to {new_name}"),
            Self::Remove { name, .. } => format!("Remove remote {name}"),
            Self::SetUrl { name, .. } => format!("Set the URL of remote {name}"),
        }
    }

    /// What the user confirms first; empty when nothing.
    fn confirm(&self) -> Vec<String> {
        let Self::Remove { name, upstreams } = self else {
            return Vec::new();
        };
        let mut actions = vec![format!(
            "Remove remote {name} and its remote-tracking branches"
        )];
        if upstreams.is_empty() {
            actions.push(format!("No branch has its upstream on {name}"));
        }
        actions.extend(
            upstreams
                .iter()
                .map(|upstream| format!("Unset the upstream of {upstream}")),
        );
        actions
    }

    /// It contacts a remote, so git gets the askpass helper (D10).
    fn network(&self) -> bool {
        matches!(self, Self::Add { fetch: true, .. })
    }
}

/// `name` is a configured remote.
fn check_existing(repo: &GitRepo, name: &str) -> Result<(), RemoteError> {
    if repo.remote_names().iter().any(|known| known == name) {
        Ok(())
    } else {
        Err(refused(format!("there is no remote {name}")))
    }
}

/// `name` is valid for a new remote and not taken.
fn check_new(repo: &GitRepo, name: &str) -> Result<(), RemoteError> {
    check_remote_name(name)?;
    if repo.remote_names().iter().any(|known| known == name) {
        return Err(refused(format!("a remote named {name} exists already")));
    }
    Ok(())
}

/// What `op` runs in `repo`, or why it is refused. Reads the repo: call it
/// from a thread that may block.
fn plan(repo: &GitRepo, op: RemoteOp) -> Result<Plan, RemoteError> {
    match op {
        RemoteOp::Add { name, url, fetch } => {
            check_new(repo, &name)?;
            let url = checked_url(&url)?;
            Ok(Plan::Add { name, url, fetch })
        }
        RemoteOp::Rename { name, new_name } => {
            check_existing(repo, &name)?;
            if name == new_name {
                return Err(refused(format!("the remote is named {name} already")));
            }
            check_new(repo, &new_name)?;
            Ok(Plan::Rename { name, new_name })
        }
        RemoteOp::Remove { name } => {
            check_existing(repo, &name)?;
            let upstreams = repo
                .branches_tracking(&name)
                .into_iter()
                .map(|branch| match repo.tracked_branch(&branch) {
                    Some(tracked) => format!("{branch} ({}/{})", tracked.remote, tracked.branch),
                    None => branch,
                })
                .collect();
            Ok(Plan::Remove { name, upstreams })
        }
        RemoteOp::SetUrl {
            name,
            url,
            push_url,
        } => {
            check_existing(repo, &name)?;
            let url = checked_url(&url)?;
            let push_url = push_url.as_deref().map(checked_url).transpose()?;
            Ok(Plan::SetUrl {
                name,
                url,
                push_url,
            })
        }
    }
}

/// Runs `plan` in `path`, a fetch's progress lines reported as the job's
/// steps. A fetch after an add that fails leaves the remote added.
async fn run(job: &JobRun, path: &Path, plan: &Plan) -> Result<Output, GitError> {
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
        Plan::Add { name, url, fetch } => {
            let mut all = ops::remote_add(git, path, name, url, write).await?;
            if *fetch {
                all.append(ops::fetch_remote(git, path, name, write).await?);
            }
            Ok(all)
        }
        Plan::Rename { name, new_name } => {
            ops::remote_rename(git, path, name, new_name, write).await
        }
        Plan::Remove { name, .. } => ops::remote_remove(git, path, name, write).await,
        Plan::SetUrl {
            name,
            url,
            push_url,
        } => {
            let mut all = ops::remote_set_url(git, path, name, url, false, write).await?;
            if let Some(push_url) = push_url {
                all.append(ops::remote_set_url(git, path, name, push_url, true, write).await?);
            }
            Ok(all)
        }
    }
}

/// Remote `name`'s URLs in repo `repo_id` of workspace `id`.
pub async fn remote_urls(
    workspaces: Arc<Workspaces>,
    id: WorkspaceId,
    repo_id: RepoId,
    name: String,
) -> Result<RemoteUrlsView, RemoteError> {
    let path = workspaces.repo_path(&id, &repo_id)?;
    tokio::task::spawn_blocking(move || {
        let repo = GitRepo::open(&path)?;
        let urls = repo
            .remote_urls(&name)
            .ok_or_else(|| refused(format!("there is no remote {name}")))?;
        Ok(RemoteUrlsView {
            url: urls.url,
            push_url: urls.push_url,
        })
    })
    .await?
}

/// Runs remote operation `request` on its repo of workspace `id` as a job
/// (section 6 "Remotes"). Refused for an invalid or taken name (SEC-4), an
/// unusable URL, a missing remote or a blocked repo (GRP-8); a remove
/// answers what it unsets until `confirmed`.
pub async fn op_remote(
    workspaces: Arc<Workspaces>,
    summaries: Arc<Summaries>,
    jobs: Arc<Jobs>,
    git: GitRunner,
    id: WorkspaceId,
    request: RemoteRequest,
) -> Result<RemoteStart, RemoteError> {
    let RemoteRequest {
        repo_id,
        op,
        confirmed,
    } = request;
    let (path, repo) = open_unblocked(&workspaces, &summaries, &git, &id, &repo_id, false).await?;
    let plan = tokio::task::spawn_blocking(move || plan(&repo, op)).await??;
    let confirm = plan.confirm();
    if !confirm.is_empty() && !confirmed {
        return Ok(RemoteStart::Confirm {
            repo: repo_name(&workspaces, &id, &repo_id)?,
            actions: confirm,
        });
    }
    let job = WriteJob {
        network: plan.network(),
        ..WriteJob::single(id, repo_id, plan.label())
    };
    let handle = jobs.start(
        git,
        job,
        move |job| async move { run(&job, &path, &plan).await },
    )?;
    Ok(RemoteStart::Started {
        job_id: handle.job_id,
    })
}
