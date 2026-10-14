//! Fetch, pull and push of one repo (section 6 "Sync", section 5 rows
//! "Fetch", "Pull" and "Push"), each one network job (ARCH-7, D10) whose
//! transfer reports git's `--progress` phases. A repo that is busy, missing
//! or in conflict state is refused, except that fetch may run in conflict
//! state (GRP-8). Cancel kills a fetch, and a pull while it fetches; a push
//! runs to its end (GRP-10). Force-with-lease is destructive (GRP-9):
//! without the user's confirmation the core answers with what it would do.
//! Nothing is stashed to make a pull succeed (GRP-12).

use std::path::Path;
use std::sync::Arc;

use gitcroc_git::ops::{PullMode, PushTo};
use gitcroc_git::{GitError, GitRepo, Output, Write, ops, parse_progress};
use serde::{Deserialize, Serialize};

use crate::unblocked::{BlockedError, open_unblocked, repo_name};
use crate::{
    GitRunner, JobError, JobId, JobRun, Jobs, NothingToDo, OnCancel, OpenError, RepoId,
    StatusReadError, Summaries, WorkspaceError, WorkspaceId, Workspaces, WriteJob,
};

/// Whether a fetch removes remote-tracking branches whose branch is gone
/// from the remote.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "snake_case")]
pub enum Prune {
    /// As `fetch.prune` and `remote.<name>.prune` say.
    Config,
    Prune,
    NoPrune,
}

/// How a pull brings in its upstream (section 5 "Pull").
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "snake_case")]
pub enum PullStrategy {
    /// As `branch.<name>.rebase`, `pull.rebase` and `pull.ff` say; with
    /// `pull.rebase` and `pull.ff` unset, fast-forward only.
    Config,
    FfOnly,
    /// Fast-forward when possible, else a merge commit.
    Merge,
    Rebase,
}

/// A sync operation (section 6 "Sync").
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum SyncOp {
    /// Fetches every remote.
    Fetch { prune: Prune },
    /// Updates the current branch from its upstream.
    Pull { strategy: PullStrategy },
    /// Pushes the current branch to its upstream. A branch without one goes
    /// to `remote`, else to `origin`, else to the only remote, and
    /// `set_upstream` makes that its upstream. `force_with_lease` replaces
    /// the remote branch when the push is not a fast-forward; destructive
    /// (GRP-9).
    Push {
        set_upstream: bool,
        remote: Option<String>,
        force_with_lease: bool,
    },
}

/// A sync operation on one repo.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
pub struct SyncRequest {
    pub repo_id: RepoId,
    pub op: SyncOp,
    /// The user confirmed a destructive operation (GRP-9).
    pub confirmed: bool,
}

/// What a request did.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, specta::Type)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum SyncStart {
    /// The job runs; its end re-reads the repo (OPS-7).
    Started { job_id: JobId },
    /// The operation is destructive (GRP-9): ask again with `confirmed`
    /// once the user agreed. `actions` says what cannot be undone.
    Confirm { repo: String, actions: Vec<String> },
    /// A push of a branch without an upstream, in a repo with several
    /// remotes and no `origin`: ask again with one of `remotes`.
    ChooseRemote { remotes: Vec<String> },
}

/// What the push options offer for one repo.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, specta::Type)]
pub struct PushChoices {
    /// The current branch; `None` when `HEAD` is detached.
    pub branch: Option<String>,
    /// Where the branch pushes to, such as `origin/main`; `None` without an
    /// upstream.
    pub upstream: Option<String>,
    /// Every remote, sorted.
    pub remotes: Vec<String>,
    /// Where a push without an upstream goes when no remote is picked;
    /// `None` when the user must pick one.
    pub remote: Option<String>,
}

/// Why a sync operation did not start.
#[derive(Debug, thiserror::Error)]
pub enum SyncError {
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

impl From<BlockedError> for SyncError {
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

fn refused(why: impl Into<String>) -> SyncError {
    SyncError::Refused(why.into())
}

/// What a job runs.
#[derive(Debug, Clone)]
enum Plan {
    Fetch {
        prune: Option<bool>,
    },
    /// Fetches `remote`, then brings `tracking` into `branch`.
    Pull {
        branch: String,
        remote: String,
        /// The remote-tracking branch, such as `refs/remotes/origin/main`.
        tracking: String,
        /// As the label shows it, such as `origin/main`.
        upstream: String,
        mode: PullMode,
    },
    Push {
        remote: String,
        branch: String,
        remote_branch: String,
        set_upstream: bool,
        force_with_lease: bool,
    },
}

impl Plan {
    /// The job's label.
    fn label(&self) -> String {
        match self {
            Self::Fetch { .. } => "Fetch all remotes".to_owned(),
            Self::Pull {
                branch, upstream, ..
            } => format!("Pull {upstream} into {branch}"),
            Self::Push {
                remote,
                branch,
                remote_branch,
                force_with_lease,
                ..
            } => {
                let verb = if *force_with_lease {
                    "Force push"
                } else {
                    "Push"
                };
                format!("{verb} {branch} to {remote}/{remote_branch}")
            }
        }
    }

    /// What cannot be undone, for the confirmation (GRP-9); empty when
    /// nothing is destructive.
    fn destructive(&self) -> Vec<String> {
        match self {
            Self::Push {
                remote,
                branch,
                remote_branch,
                force_with_lease: true,
                ..
            } => vec![format!(
                "Replace {remote_branch} on the remote {remote} with {branch}, even if it is \
                 not a fast-forward; commits only on the remote branch may be lost"
            )],
            _ => Vec::new(),
        }
    }

    /// When it did nothing: no ref changed. A push that sets the upstream
    /// changes the branch's config, so it always did something.
    fn nothing_to_do(&self) -> NothingToDo {
        match self {
            Self::Push {
                set_upstream: true, ..
            } => NothingToDo::Never,
            _ => NothingToDo::WhenRefsUnchanged,
        }
    }

    /// A fetch is killed on cancel, and a pull while it fetches; a push
    /// runs to its end (GRP-10).
    fn on_cancel(&self) -> OnCancel {
        match self {
            Self::Fetch { .. } | Self::Pull { .. } => OnCancel::Kill,
            Self::Push { .. } => OnCancel::Finish,
        }
    }
}

/// What a request turns into.
enum Planned {
    Run(Plan),
    /// The push needs a remote picked from these.
    ChooseRemote(Vec<String>),
}

/// The current branch, or why `what` needs one.
fn current_branch(repo: &GitRepo, what: &str) -> Result<String, SyncError> {
    repo.current_branch()
        .ok_or_else(|| refused(format!("HEAD is detached; check out a branch to {what}")))
}

/// What `op` runs in `repo`, or why it is refused. Reads the repo: call it
/// from a thread that may block.
fn plan(repo: &GitRepo, op: SyncOp) -> Result<Planned, SyncError> {
    match op {
        SyncOp::Fetch { prune } => {
            if repo.remote_names().is_empty() {
                return Err(refused("the repo has no remote"));
            }
            let prune = match prune {
                Prune::Config => None,
                Prune::Prune => Some(true),
                Prune::NoPrune => Some(false),
            };
            Ok(Planned::Run(Plan::Fetch { prune }))
        }
        SyncOp::Pull { strategy } => {
            let branch = current_branch(repo, "pull")?;
            let no_upstream = || refused(format!("{branch} has no upstream; set one to pull"));
            let tracked = repo.tracked_branch(&branch).ok_or_else(no_upstream)?;
            let tracking = repo.tracking_ref(&branch).ok_or_else(no_upstream)?;
            let mode = match strategy {
                PullStrategy::Config => PullMode::Config,
                PullStrategy::FfOnly => PullMode::FfOnly,
                PullStrategy::Merge => PullMode::Merge,
                PullStrategy::Rebase => PullMode::Rebase,
            };
            Ok(Planned::Run(Plan::Pull {
                upstream: format!("{}/{}", tracked.remote, tracked.branch),
                branch,
                remote: tracked.remote,
                tracking,
                mode,
            }))
        }
        SyncOp::Push {
            set_upstream,
            remote,
            force_with_lease,
        } => {
            let branch = current_branch(repo, "push")?;
            if let Some(tracked) = repo.tracked_branch(&branch) {
                return Ok(Planned::Run(Plan::Push {
                    remote: tracked.remote,
                    branch,
                    remote_branch: tracked.branch,
                    set_upstream: false,
                    force_with_lease,
                }));
            }
            let remotes = repo.remote_names();
            let remote = match remote {
                Some(remote) if remotes.contains(&remote) => remote,
                Some(remote) => return Err(refused(format!("there is no remote {remote}"))),
                None => match default_remote(&remotes) {
                    Some(remote) => remote,
                    None if remotes.is_empty() => return Err(refused("the repo has no remote")),
                    None => return Ok(Planned::ChooseRemote(remotes)),
                },
            };
            Ok(Planned::Run(Plan::Push {
                remote,
                remote_branch: branch.clone(),
                branch,
                set_upstream,
                force_with_lease,
            }))
        }
    }
}

/// Where a branch without an upstream pushes to: `origin`, else the only
/// remote; `None` when the user must pick.
pub(crate) fn default_remote(remotes: &[String]) -> Option<String> {
    if remotes.iter().any(|remote| remote == "origin") {
        return Some("origin".to_owned());
    }
    match remotes {
        [only] => Some(only.clone()),
        _ => None,
    }
}

/// `first`'s output, then `then`'s, also when `then` failed.
fn joined(first: Output, then: Result<Output, GitError>) -> Result<Output, GitError> {
    let join = |mut head: Vec<u8>, tail: Vec<u8>| {
        head.extend(tail);
        head
    };
    match then {
        Ok(then) => Ok(Output {
            stdout: join(first.stdout, then.stdout),
            stderr: join(first.stderr, then.stderr),
        }),
        Err(GitError::Failed {
            code,
            stdout,
            stderr,
        }) => Err(GitError::Failed {
            code,
            stdout: join(first.stdout, stdout),
            stderr: join(first.stderr, stderr),
        }),
        Err(error) => Err(error),
    }
}

/// Runs `plan` in `path`, git's progress lines reported as the job's
/// steps.
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
    match plan {
        Plan::Fetch { prune } => ops::fetch_all(&job.git, path, *prune, write).await,
        Plan::Pull {
            remote,
            tracking,
            mode,
            ..
        } => {
            let fetched = ops::fetch_remote(&job.git, path, remote, write).await?;
            // Fetched, the pull runs to its end even when cancelled (GRP-10).
            let write = Write {
                kill: None,
                ..write
            };
            let pulled = ops::pull_fetched(&job.git, path, tracking, *mode, write).await;
            joined(fetched, pulled)
        }
        Plan::Push {
            remote,
            branch,
            remote_branch,
            set_upstream,
            force_with_lease,
        } => {
            let to = PushTo {
                remote,
                branch,
                remote_branch,
                set_upstream: *set_upstream,
                force_with_lease: *force_with_lease,
            };
            ops::push(&job.git, path, to, write).await
        }
    }
}

/// Runs sync operation `request` on its repo of workspace `id` as a job
/// (section 6 "Sync"). Refused for a blocked repo (GRP-8) or what the
/// operation cannot apply to; a force push answers what it would do until
/// `confirmed` (GRP-9).
pub async fn op_sync(
    workspaces: Arc<Workspaces>,
    summaries: Arc<Summaries>,
    jobs: Arc<Jobs>,
    git: GitRunner,
    id: WorkspaceId,
    request: SyncRequest,
) -> Result<SyncStart, SyncError> {
    let SyncRequest {
        repo_id,
        op,
        confirmed,
    } = request;
    let fetch = matches!(op, SyncOp::Fetch { .. });
    let (path, repo) = open_unblocked(&workspaces, &summaries, &git, &id, &repo_id, fetch).await?;
    let plan = match tokio::task::spawn_blocking(move || plan(&repo, op)).await?? {
        Planned::Run(plan) => plan,
        Planned::ChooseRemote(remotes) => return Ok(SyncStart::ChooseRemote { remotes }),
    };
    let destructive = plan.destructive();
    if !destructive.is_empty() && !confirmed {
        return Ok(SyncStart::Confirm {
            repo: repo_name(&workspaces, &id, &repo_id)?,
            actions: destructive,
        });
    }
    let job = WriteJob {
        workspace_id: id,
        repos: vec![repo_id],
        label: plan.label(),
        on_cancel: plan.on_cancel(),
        may_conflict: matches!(plan, Plan::Pull { .. }),
        nothing_to_do: plan.nothing_to_do(),
        network: true,
    };
    let handle = jobs.start(git, job, move |job| async move {
        let path = job.paths.first().cloned().unwrap_or(path);
        run(&job, &path, &plan).await
    })?;
    Ok(SyncStart::Started {
        job_id: handle.job_id,
    })
}

/// What the push options of repo `repo` offer: the branch, its upstream and
/// the remotes to push to without one.
pub async fn push_choices(
    workspaces: Arc<Workspaces>,
    id: WorkspaceId,
    repo: RepoId,
) -> Result<PushChoices, SyncError> {
    let path = workspaces.repo_path(&id, &repo)?;
    tokio::task::spawn_blocking(move || {
        let repo = GitRepo::open(&path)?;
        let branch = repo.current_branch();
        let upstream = branch
            .as_deref()
            .and_then(|branch| repo.tracked_branch(branch))
            .map(|tracked| format!("{}/{}", tracked.remote, tracked.branch));
        let remotes = repo.remote_names();
        Ok(PushChoices {
            branch,
            upstream,
            remote: default_remote(&remotes),
            remotes,
        })
    })
    .await?
}
