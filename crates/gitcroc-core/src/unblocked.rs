//! The GRP-8 check every single-repo write makes before it starts: a repo
//! that is busy with another write, missing or in conflict state is
//! refused. Fetch alone may run in conflict state.

use std::path::PathBuf;

use gitcroc_git::{GitRepo, StatusReader};

use crate::{GitRunner, OpenError, RepoId, StatusReadError, Summaries, WorkspaceError};
use crate::{WorkspaceId, Workspaces};

/// Why a repo cannot take a write now.
#[derive(Debug, thiserror::Error)]
pub(crate) enum BlockedError {
    #[error(transparent)]
    Workspace(#[from] WorkspaceError),
    #[error(transparent)]
    Open(#[from] OpenError),
    #[error(transparent)]
    Status(#[from] StatusReadError),
    #[error("the read did not finish: {0}")]
    Stopped(#[from] tokio::task::JoinError),
    /// Blocked (GRP-8), with the reason.
    #[error("{0}")]
    Blocked(String),
}

/// `From<BlockedError>` for an operation's error, which has the variants
/// `Workspace`, `Open`, `Status`, `Stopped` and `Refused`: a blocked repo is
/// a refusal.
macro_rules! from_blocked {
    ($error:ty) => {
        impl From<$crate::unblocked::BlockedError> for $error {
            fn from(error: $crate::unblocked::BlockedError) -> Self {
                use $crate::unblocked::BlockedError;
                match error {
                    BlockedError::Workspace(error) => Self::Workspace(error),
                    BlockedError::Open(error) => Self::Open(error),
                    BlockedError::Status(error) => Self::Status(error),
                    BlockedError::Stopped(error) => Self::Stopped(error),
                    BlockedError::Blocked(why) => Self::Refused(why),
                }
            }
        }
    };
}
pub(crate) use from_blocked;

/// Opens repo `repo` of workspace `id`, refusing it while blocked (GRP-8):
/// busy with another write, missing, or in conflict state unless
/// `conflict_allowed`.
pub(crate) async fn open_unblocked(
    workspaces: &Workspaces,
    summaries: &Summaries,
    git: &GitRunner,
    id: &WorkspaceId,
    repo: &RepoId,
    conflict_allowed: bool,
) -> Result<(PathBuf, GitRepo), BlockedError> {
    let blocked = |why: &str| BlockedError::Blocked(why.to_owned());
    if summaries.busy(id, repo) {
        return Err(blocked("another operation is running on the repo"));
    }
    let path = workspaces.repo_path(id, repo)?;
    if !path.exists() {
        return Err(blocked("the repo's folder is missing"));
    }
    let opened = {
        let path = path.clone();
        tokio::task::spawn_blocking(move || GitRepo::open(&path)).await??
    };
    if !conflict_allowed {
        let state = crate::repo_state(StatusReader::new(opened.clone(), git.clone())).await?;
        if state.in_conflict() {
            return Err(blocked(
                "the repo is in conflict state; continue or abort first",
            ));
        }
    }
    Ok((path, opened))
}

/// Repo `repo`'s name in workspace `id`, for a confirmation (GRP-9); its id
/// when it is not listed.
pub(crate) fn repo_name(
    workspaces: &Workspaces,
    id: &WorkspaceId,
    repo: &RepoId,
) -> Result<String, WorkspaceError> {
    Ok(workspaces
        .view(id)?
        .repos
        .into_iter()
        .find(|known| known.id == *repo)
        .map_or_else(|| repo.to_string(), |known| known.name))
}
