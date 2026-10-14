//! The conflict editor's calls (OPS-4 to OPS-8): one unmerged file of a
//! changes view read as a conflict, and resolved with the editor's text or
//! with one side. The UI names the file by the view's handle and its id in
//! the unmerged list (PLAT-2). Each resolution is a job (ARCH-7) whose end
//! re-reads the repo (OPS-7). A file resolved meanwhile, as in a terminal,
//! is refused as already resolved, so the UI reads the lists again.

use std::sync::Arc;

use gitcroc_git::ops::conflict::{self as ops, Choice, Side};
use gitcroc_git::{Conflict, ConflictContent, GitRepo, first_marker_line};
use serde::{Deserialize, Serialize};

use crate::changes::ViewState;
use crate::{
    Changes, ChangesError, GitRunner, JobId, Jobs, NothingToDo, OnCancel, WorkspaceId, WriteJob,
};

/// What `conflict_get` found.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, specta::Type)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ConflictRead {
    Conflict {
        // Boxed: a conflict is large, and the other variant empty.
        conflict: Box<Conflict>,
    },
    /// The file is no longer in conflict; read the lists again.
    AlreadyResolved,
}

/// What a resolution request did.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, specta::Type)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ConflictStart {
    /// The job runs; its end re-reads the repo (OPS-7).
    Started { job_id: JobId },
    /// The file is no longer in conflict; nothing ran. Read the lists
    /// again.
    AlreadyResolved,
    /// Mark resolved was refused: line `line`, from 1, of the text is a
    /// conflict marker line (OPS-5). Ask again with the override when the
    /// file legitimately has such lines.
    MarkersRemain { line: u32 },
}

/// Mark resolved with the editor's text (OPS-5).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
pub struct ResolveRequest {
    /// The changes view whose unmerged list `file` is from.
    pub handle: u32,
    pub file: u32,
    pub text: String,
    /// Mark resolved although the text has conflict marker lines.
    pub override_markers: bool,
}

/// A side of a conflict.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "snake_case")]
pub enum ConflictChoice {
    Ours,
    Theirs,
}

/// Resolve with one side (OPS-6).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
pub struct ChooseRequest {
    pub handle: u32,
    pub file: u32,
    pub side: ConflictChoice,
}

/// The view `handle` of workspace `id` and its unmerged file `file`, as git
/// stores the path. `None` when a newer view replaced it.
fn unmerged_of(
    changes: &Changes,
    id: &WorkspaceId,
    handle: u32,
    file: u32,
) -> Result<Option<(ViewState, Vec<u8>)>, ChangesError> {
    let Some(view) = changes.state(id, handle) else {
        return Ok(None);
    };
    let path = view
        .lists
        .unmerged(file)
        .ok_or(ChangesError::UnknownFile(file))?
        .git_path()
        .to_vec();
    Ok(Some((view, path)))
}

/// The conflict of `git_path` in the view's repo, read again now.
async fn read(view: &ViewState, git_path: Vec<u8>) -> Result<Option<Conflict>, ChangesError> {
    let path = view.path.clone();
    tokio::task::spawn_blocking(move || {
        let repo = GitRepo::open(&path)?;
        Ok::<_, ChangesError>(repo.conflict(&git_path)?)
    })
    .await?
}

/// Unmerged file `file` of view `handle` as the conflict editor shows it
/// (OPS-8): text hunks or a choice, both sides labelled. `None` when a
/// newer view of the workspace replaced the view.
pub async fn conflict_get(
    changes: Arc<Changes>,
    id: WorkspaceId,
    handle: u32,
    file: u32,
) -> Result<Option<ConflictRead>, ChangesError> {
    let Some((view, path)) = unmerged_of(&changes, &id, handle, file)? else {
        return Ok(None);
    };
    Ok(Some(match read(&view, path).await? {
        Some(conflict) => ConflictRead::Conflict {
            conflict: Box::new(conflict),
        },
        None => ConflictRead::AlreadyResolved,
    }))
}

/// Starts the job that resolves `git_path` of the view's repo with `run`.
fn start<F, Fut>(
    jobs: &Arc<Jobs>,
    git: GitRunner,
    id: WorkspaceId,
    view: ViewState,
    label: String,
    run: F,
) -> Result<ConflictStart, ChangesError>
where
    F: FnOnce(GitRunner, std::path::PathBuf, crate::JobRun) -> Fut + Send + 'static,
    Fut: std::future::Future<Output = Result<gitcroc_git::Output, gitcroc_git::GitError>>
        + Send
        + 'static,
{
    let job = WriteJob {
        workspace_id: id,
        repos: vec![view.repo],
        label,
        on_cancel: OnCancel::Finish,
        may_conflict: false,
        nothing_to_do: NothingToDo::Never,
        network: false,
    };
    let fallback = view.path;
    let handle = jobs.start(git, job, move |job| async move {
        let path = job.paths.first().cloned().unwrap_or(fallback);
        run(job.git.clone(), path, job).await
    })?;
    Ok(ConflictStart::Started {
        job_id: handle.job_id,
    })
}

/// Mark resolved (OPS-5): writes the editor's text to the file and stages
/// it, as a job. Refused while the text has a conflict marker line, unless
/// overridden, and for a file resolved meanwhile. `None` when a newer view
/// of the workspace replaced the request's.
pub async fn conflict_resolve(
    changes: Arc<Changes>,
    jobs: Arc<Jobs>,
    git: GitRunner,
    id: WorkspaceId,
    request: ResolveRequest,
) -> Result<Option<ConflictStart>, ChangesError> {
    let Some((view, path)) = unmerged_of(&changes, &id, request.handle, request.file)? else {
        return Ok(None);
    };
    if !request.override_markers
        && let Some(line) = first_marker_line(&request.text)
    {
        return Ok(Some(ConflictStart::MarkersRemain { line }));
    }
    let Some(conflict) = read(&view, path.clone()).await? else {
        return Ok(Some(ConflictStart::AlreadyResolved));
    };
    if !matches!(conflict.content, ConflictContent::Text { .. }) {
        return Err(ChangesError::Refused(
            "the file cannot be shown as text; choose a side",
        ));
    }
    let label = format!("Resolve {}", conflict.path);
    let text = request.text.into_bytes();
    let started = start(
        &jobs,
        git,
        id,
        view,
        label,
        move |git, repo, job| async move {
            ops::mark_resolved(&git, &repo, &path, &text, job.write()).await
        },
    )?;
    Ok(Some(started))
}

/// How choosing `side` of `conflict` is done in the repo.
fn choice(conflict: &Conflict, side: ConflictChoice) -> Choice {
    let (chosen, other) = match side {
        ConflictChoice::Ours => (Side::Ours, Side::Theirs),
        ConflictChoice::Theirs => (Side::Theirs, Side::Ours),
    };
    match (conflict.stage(chosen), conflict.stage(other)) {
        (Some(entry), _) => Choice::Stage(entry),
        (None, Some(other)) if other.is_submodule() => Choice::RemoveFromIndex,
        (None, _) => Choice::Remove,
    }
}

/// Resolves a file with one side (OPS-6), as a job: that side's file, link,
/// mode or submodule commit, or the file deleted when that side deleted
/// it. Refused for a file resolved meanwhile. `None` when a newer view of
/// the workspace replaced the request's.
pub async fn conflict_choose(
    changes: Arc<Changes>,
    jobs: Arc<Jobs>,
    git: GitRunner,
    id: WorkspaceId,
    request: ChooseRequest,
) -> Result<Option<ConflictStart>, ChangesError> {
    let Some((view, path)) = unmerged_of(&changes, &id, request.handle, request.file)? else {
        return Ok(None);
    };
    let Some(conflict) = read(&view, path.clone()).await? else {
        return Ok(Some(ConflictStart::AlreadyResolved));
    };
    let name = match request.side {
        ConflictChoice::Ours => &conflict.labels.ours,
        ConflictChoice::Theirs => &conflict.labels.theirs,
    };
    let label = format!("Resolve {} with {name}", conflict.path);
    let choice = choice(&conflict, request.side);
    let started = start(
        &jobs,
        git,
        id,
        view,
        label,
        move |git, repo, job| async move { ops::choose(&git, &repo, &path, &choice, job.write()).await },
    )?;
    Ok(Some(started))
}
