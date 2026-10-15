//! Reset of the current branch to a commit (section 6 "Reset", UI-13), one
//! job (ARCH-7): soft, mixed or hard. A repo that is busy, missing or in
//! conflict state is refused (GRP-8). A detached `HEAD` is reset itself,
//! and the label and the confirmation say so. A hard reset is destructive
//! (GRP-9): without the user's confirmation the core answers with the
//! branch, the commit and how many files' uncommitted changes it would
//! lose.

use std::collections::HashSet;
use std::sync::Arc;

use gitcroc_git::{FileStatus, GitRepo, StatusReader, ops};
use serde::{Deserialize, Serialize};

use crate::unblocked::{from_blocked, open_unblocked, repo_name};
use crate::{
    GitRunner, JobError, JobId, Jobs, OpenError, RepoId, StatusReadError, Summaries,
    WorkspaceError, WorkspaceId, Workspaces, WriteJob,
};

/// What a reset puts at the commit besides the branch.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "snake_case")]
pub enum ResetMode {
    /// The branch only; the index and the working tree stay.
    Soft,
    /// The branch and the index; the working tree stays.
    Mixed,
    /// The branch, the index and the working tree. Destructive (GRP-9).
    Hard,
}

/// A reset of the current branch of one repo to `commit`, a hash.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
pub struct ResetRequest {
    pub repo_id: RepoId,
    pub commit: String,
    pub mode: ResetMode,
    /// The user confirmed a hard reset (GRP-9).
    pub confirmed: bool,
}

/// What a request did.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, specta::Type)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ResetStart {
    /// The job runs; its end re-reads the repo (OPS-7).
    Started { job_id: JobId },
    /// A hard reset (GRP-9): ask again with `confirmed` once the user
    /// agreed. `actions` says what cannot be undone.
    Confirm { repo: String, actions: Vec<String> },
}

/// Why a reset did not start.
#[derive(Debug, thiserror::Error)]
pub enum ResetError {
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

from_blocked!(ResetError);

/// What a job runs.
#[derive(Debug, Clone)]
struct Plan {
    /// The full hash.
    commit: String,
    /// As `1a2b3c4 Subject`.
    commit_text: String,
    /// `None` when `HEAD` is detached.
    branch: Option<String>,
    mode: ResetMode,
}

impl Plan {
    /// What is reset: the branch, or a detached `HEAD`.
    fn target(&self) -> String {
        match &self.branch {
            Some(branch) => branch.clone(),
            None => "HEAD (detached)".to_owned(),
        }
    }

    /// The job's label.
    fn label(&self) -> String {
        let short = self
            .commit_text
            .split_once(' ')
            .map_or(self.commit_text.as_str(), |(short, _)| short);
        let mode = match self.mode {
            ResetMode::Soft => "soft",
            ResetMode::Mixed => "mixed",
            ResetMode::Hard => "hard",
        };
        format!("Reset {} to {short} ({mode})", self.target())
    }

    /// What a hard reset loses, for the confirmation (GRP-9), with `lost`
    /// files' uncommitted changes.
    fn destructive(&self, lost: usize) -> Vec<String> {
        let moved = match &self.branch {
            Some(branch) => format!("Reset branch {branch} to {}", self.commit_text),
            None => format!(
                "HEAD is detached: reset HEAD, not a branch, to {}",
                self.commit_text
            ),
        };
        let files = match lost {
            0 => "No uncommitted changes are lost; untracked files stay".to_owned(),
            1 => "Discard the uncommitted changes to 1 file; untracked files stay".to_owned(),
            n => format!("Discard the uncommitted changes to {n} files; untracked files stay"),
        };
        vec![moved, files]
    }
}

/// What `request` runs in `repo`, or why it is refused. Reads the repo:
/// call it from a thread that may block.
fn plan(repo: &GitRepo, commit: &str, mode: ResetMode) -> Result<Plan, ResetError> {
    let names_none = || ResetError::Refused(format!("'{commit}' names no commit"));
    if commit.is_empty() || commit.starts_with('-') {
        return Err(names_none());
    }
    let commit = repo.resolve_commit(commit).ok_or_else(names_none)?;
    let commit_text = repo.commit_text(&commit).unwrap_or_else(|| commit.clone());
    Ok(Plan {
        commit,
        commit_text,
        branch: repo.current_branch(),
        mode,
    })
}

/// The tracked paths with uncommitted changes, staged or not: what a hard
/// reset puts back. Untracked files are not among them.
async fn tracked_changes(repo: GitRepo, git: GitRunner) -> Result<usize, StatusReadError> {
    let working = crate::working_copy(StatusReader::new(repo, git)).await?;
    let paths: HashSet<&[u8]> = working
        .staged
        .iter()
        .chain(
            working
                .unstaged
                .iter()
                .filter(|file| file.status != FileStatus::Untracked),
        )
        .map(|file| file.git_path())
        .collect();
    Ok(paths.len())
}

/// Resets the current branch of `request`'s repo of workspace `id` as a
/// job (section 6 "Reset"). Refused for a blocked repo (GRP-8) or what
/// names no commit; a hard reset answers what it would lose until
/// `confirmed` (GRP-9).
pub async fn op_reset(
    workspaces: Arc<Workspaces>,
    summaries: Arc<Summaries>,
    jobs: Arc<Jobs>,
    git: GitRunner,
    id: WorkspaceId,
    request: ResetRequest,
) -> Result<ResetStart, ResetError> {
    let ResetRequest {
        repo_id,
        commit,
        mode,
        confirmed,
    } = request;
    let (path, repo) = open_unblocked(&workspaces, &summaries, &git, &id, &repo_id, false).await?;
    let plan = {
        let repo = repo.clone();
        tokio::task::spawn_blocking(move || plan(&repo, &commit, mode)).await??
    };
    if mode == ResetMode::Hard && !confirmed {
        let lost = tracked_changes(repo, git.clone()).await?;
        return Ok(ResetStart::Confirm {
            repo: repo_name(&workspaces, &id, &repo_id)?,
            actions: plan.destructive(lost),
        });
    }
    let job = WriteJob::single(id, repo_id, plan.label());
    let handle = jobs.start(git, job, move |job| async move {
        let mode = match plan.mode {
            ResetMode::Soft => ops::ResetMode::Soft,
            ResetMode::Mixed => ops::ResetMode::Mixed,
            ResetMode::Hard => ops::ResetMode::Hard,
        };
        ops::reset(&job.git, &path, &plan.commit, mode, job.write()).await
    })?;
    Ok(ResetStart::Started {
        job_id: handle.job_id,
    })
}
