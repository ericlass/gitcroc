//! The offer to speed up the status of a large repo (PERF-7): when the
//! index tracks more than [`FSMONITOR_THRESHOLD`] files and neither
//! `core.fsmonitor` nor `core.untrackedCache` is on, the changes pane offers
//! to turn them on, or not to ask again. On Linux, where git has no
//! built-in monitor, only the untracked cache is offered (M1-04). The
//! offer is read without writing anything; the config is written only by
//! [`fsmonitor_enable`], once the user accepted, as one job. "Don't ask
//! again" is kept in the workspace file.

use std::sync::Arc;

use gitcroc_git::{StatusReader, ops};

use crate::lock;
use crate::unblocked::{from_blocked, open_unblocked, repo_name};
use crate::{
    Changes, ChangesError, GitRunner, JobError, JobId, Jobs, OpenError, RepoId, StatusReadError,
    Summaries, WorkspaceError, WorkspaceId, Workspaces, WriteJob,
};

/// Over this many tracked files a repo gets the offer (PERF-7).
pub const FSMONITOR_THRESHOLD: usize = 50_000;

/// The keys the offer turns on, in the repo's local config.
fn keys() -> Vec<String> {
    ops::fsmonitor_keys()
        .into_iter()
        .map(str::to_owned)
        .collect()
}

/// The keys to offer for repo `repo_id` of workspace `id`, read by
/// `reader`; `None` when there is no offer: the repo tracks no more than
/// `threshold` files, has either setting on, the user declined, or the
/// settings are being turned on.
pub(crate) async fn offer(
    changes: &Changes,
    workspaces: &Workspaces,
    id: &WorkspaceId,
    repo_id: &RepoId,
    reader: StatusReader,
    threshold: usize,
) -> Result<Option<Vec<String>>, ChangesError> {
    if workspaces.fsmonitor_declined(id, repo_id)?
        || lock(&changes.enabling).contains(&(id.clone(), repo_id.clone()))
    {
        return Ok(None);
    }
    let state = tokio::task::spawn_blocking(move || reader.fsmonitor_state())
        .await
        .map_err(StatusReadError::from)?
        .map_err(StatusReadError::from)?;
    let offered = state.tracked > threshold && !state.fsmonitor && !state.untracked_cache;
    Ok(offered.then(keys))
}

/// Why the settings could not be turned on.
#[derive(Debug, thiserror::Error)]
pub enum FsmonitorError {
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

from_blocked!(FsmonitorError);

/// The user accepted the offer for repo `repo_id` of workspace `id`: turns
/// on the offered keys in its local config as a job (ARCH-1). Refused for a
/// blocked repo (GRP-8). No offer is made for the repo while it runs.
pub async fn fsmonitor_enable(
    workspaces: Arc<Workspaces>,
    summaries: Arc<Summaries>,
    changes: Arc<Changes>,
    jobs: Arc<Jobs>,
    git: GitRunner,
    id: WorkspaceId,
    repo_id: RepoId,
) -> Result<JobId, FsmonitorError> {
    let (path, _) = open_unblocked(&workspaces, &summaries, &git, &id, &repo_id, false).await?;
    let name = repo_name(&workspaces, &id, &repo_id)?;
    let label = if ops::BUILTIN_FSMONITOR {
        format!("Turn on fsmonitor and the untracked cache of {name}")
    } else {
        format!("Turn on the untracked cache of {name}")
    };
    let job = WriteJob::single(id.clone(), repo_id.clone(), label);
    let key = (id, repo_id);
    lock(&changes.enabling).insert(key.clone());
    let started = jobs.start(git, job, move |run| async move {
        ops::fsmonitor_enable(&run.git, &path, run.write()).await
    });
    let handle = match started {
        Ok(handle) => handle,
        Err(error) => {
            lock(&changes.enabling).remove(&key);
            return Err(error.into());
        }
    };
    let job_id = handle.job_id;
    tokio::spawn(async move {
        // Ended or dropped: either way the job no longer runs.
        let _ = handle.ended.await;
        lock(&changes.enabling).remove(&key);
    });
    Ok(job_id)
}
