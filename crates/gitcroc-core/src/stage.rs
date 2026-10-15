//! Stage, unstage and discard in "Uncommitted changes" (UI-15): whole
//! files of a view's lists, or one hunk of the diff the view showed last.
//! The UI names files by list and id and hunks by index (PLAT-2); the core
//! finds the paths and builds the patch from its own diff. Each request is
//! one job (ARCH-7). Discard is destructive (GRP-9): without the user's
//! confirmation the core answers with what it would discard instead.

use std::path::PathBuf;
use std::sync::Arc;

use gitcroc_git::ops::{self, Apply};
use gitcroc_git::{ChangedFile, DiffError, DiffRead, FileStatus, GitRepo, HunkPatch};
use serde::{Deserialize, Serialize};

use crate::changes::{Lists, ViewState};
use crate::{
    Changes, ChangesError, DiffReadError, FileList, FileRef, GitRunner, JobId, Jobs, WorkspaceId,
    Workspaces, WriteJob,
};

/// What to do with the files or the hunk.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "snake_case")]
pub enum ChangeAction {
    /// Unstaged to staged.
    Stage,
    /// Staged to unstaged.
    Unstage,
    /// Unstaged changes undone in the working tree; untracked files and
    /// folders deleted. Destructive (GRP-9).
    Discard,
}

/// Which changes of a view.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ChangeTarget {
    /// Whole files, all of one list.
    Files { files: Vec<FileRef> },
    /// Hunk `hunk`, from 0, of the diff of `file` last read with
    /// `diff_get`.
    Hunk { file: FileRef, hunk: u32 },
}

/// What a request did.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, specta::Type)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ChangeStart {
    /// The job runs; its end re-reads the repo (OPS-7).
    Started { job_id: JobId },
    /// A discard needs the user's confirmation (GRP-9): ask again with
    /// `confirmed` once the user agreed. Names for display.
    Confirm { repo: String, files: Vec<String> },
    /// The hunk's file changed since its diff was read; nothing ran, and
    /// the view should be read again.
    FileChanged,
}

/// The list each action takes its files from.
fn list_of(action: ChangeAction) -> FileList {
    match action {
        ChangeAction::Stage | ChangeAction::Discard => FileList::Unstaged,
        ChangeAction::Unstage => FileList::Staged,
    }
}

/// What a job runs.
enum Plan {
    Files {
        files: Vec<ChangedFile>,
        unborn: bool,
    },
    Patch(Vec<u8>),
}

/// `git_path` and, for a rename, the path before it: what git is told.
fn paths_of(files: &[ChangedFile]) -> Vec<Vec<u8>> {
    files
        .iter()
        .flat_map(|file| {
            std::iter::once(file.git_path().to_vec()).chain(file.old_git_path().map(<[u8]>::to_vec))
        })
        .collect()
}

/// Runs `plan` in `path` as `action` says.
async fn run(
    git: &GitRunner,
    path: &std::path::Path,
    action: ChangeAction,
    plan: &Plan,
    write: gitcroc_git::Write<'_>,
) -> Result<gitcroc_git::Output, gitcroc_git::GitError> {
    match plan {
        Plan::Patch(patch) => {
            let apply = match action {
                ChangeAction::Stage => Apply::Stage,
                ChangeAction::Unstage => Apply::Unstage,
                ChangeAction::Discard => Apply::Discard,
            };
            ops::apply_patch(git, path, patch, apply, write).await
        }
        Plan::Files { files, unborn } => {
            let paths = paths_of(files);
            let paths: Vec<&[u8]> = paths.iter().map(Vec::as_slice).collect();
            match action {
                ChangeAction::Stage => ops::stage(git, path, &paths, write).await,
                ChangeAction::Unstage => ops::unstage(git, path, &paths, *unborn, write).await,
                ChangeAction::Discard => discard(git, path, files, write).await,
            }
        }
    }
}

/// Restores tracked files and deletes untracked ones, git's output of both
/// joined.
async fn discard(
    git: &GitRunner,
    path: &std::path::Path,
    files: &[ChangedFile],
    write: gitcroc_git::Write<'_>,
) -> Result<gitcroc_git::Output, gitcroc_git::GitError> {
    let (untracked, tracked): (Vec<&ChangedFile>, Vec<&ChangedFile>) = files
        .iter()
        .partition(|file| file.status == FileStatus::Untracked);
    let mut all = gitcroc_git::Output::default();
    if !tracked.is_empty() {
        let paths: Vec<&[u8]> = tracked.iter().map(|file| file.git_path()).collect();
        all.append(ops::discard_tracked(git, path, &paths, write).await?);
    }
    if !untracked.is_empty() {
        let paths: Vec<PathBuf> = untracked.iter().filter_map(|file| file.os_path()).collect();
        all.append(ops::discard_untracked(git, path, &paths, write).await?);
    }
    Ok(all)
}

/// The job's label: the action and what it touches.
fn label(action: ChangeAction, target: &ChangeTarget, files: &[ChangedFile]) -> String {
    let verb = match action {
        ChangeAction::Stage => "Stage",
        ChangeAction::Unstage => "Unstage",
        ChangeAction::Discard => "Discard",
    };
    match (target, files) {
        (ChangeTarget::Hunk { .. }, [file]) => format!("{verb} a hunk of {}", file.path),
        (_, [file]) => format!("{verb} {}", file.path),
        _ => format!("{verb} {} files", files.len()),
    }
}

/// The files `refs` name in `list` of `lists`, each at most once.
fn files_of(
    lists: &Lists,
    refs: &[FileRef],
    list: FileList,
) -> Result<Vec<ChangedFile>, ChangesError> {
    let mut files: Vec<ChangedFile> = Vec::with_capacity(refs.len());
    for file in refs {
        if file.list != list {
            return Err(ChangesError::Refused(
                "the files are not in the list this acts on",
            ));
        }
        let (_, found) = lists
            .file(*file)
            .ok_or(ChangesError::UnknownFile(file.id))?;
        if !files.iter().any(|known| known.id == found.id) {
            files.push(found);
        }
    }
    if files.is_empty() {
        return Err(ChangesError::Refused("no file was named"));
    }
    Ok(files)
}

/// A stage, unstage or discard request of the changes pane.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
pub struct ChangeRequest {
    /// The view whose files or diff the request names.
    pub handle: u32,
    pub action: ChangeAction,
    pub target: ChangeTarget,
    /// The user confirmed the discard (GRP-9).
    pub confirmed: bool,
}

/// Whether the repo at `path` is on a branch with no commit yet.
async fn unborn(path: PathBuf) -> Result<bool, ChangesError> {
    Ok(
        tokio::task::spawn_blocking(move || GitRepo::open(&path).map(|repo| repo.unborn()))
            .await??,
    )
}

/// What applying `hunk` of the shown diff of `file` takes, or `None` when
/// the file changed since.
async fn hunk_plan(
    view: &ViewState,
    changed: ChangedFile,
    file: FileRef,
    hunk: u32,
) -> Result<Option<HunkPatch>, ChangesError> {
    let shown = view
        .diff
        .filter(|shown| shown.file == file)
        .ok_or(ChangesError::Refused(
            "the hunk's diff is not the one shown",
        ))?;
    if shown.options.ignore_whitespace {
        return Err(ChangesError::Refused(
            "hunks cannot be applied while whitespace is ignored",
        ));
    }
    let path = view.path.clone();
    let patch = tokio::task::spawn_blocking(move || {
        let repo = GitRepo::open(&path)?;
        Ok::<_, ChangesError>(repo.hunk_patch(
            &changed,
            shown.options,
            hunk as usize,
            &shown.content,
        ))
    })
    .await??;
    match patch {
        Ok(patch) => Ok(Some(patch)),
        Err(DiffError::FileChanged) => Ok(None),
        Err(error) => Err(ChangesError::Diff(DiffReadError::from(error))),
    }
}

/// Stages, unstages or discards what `request` names in its view of
/// workspace `id` (UI-15), as a job. A discard runs only when confirmed
/// (GRP-9); without it the answer is what it would discard. A hunk is
/// applied only while its file is as the diff showed it. `None` when a
/// newer view of the workspace replaced the request's.
pub async fn changes_apply(
    changes: Arc<Changes>,
    jobs: Arc<Jobs>,
    workspaces: Arc<Workspaces>,
    git: GitRunner,
    id: WorkspaceId,
    request: ChangeRequest,
) -> Result<Option<ChangeStart>, ChangesError> {
    let ChangeRequest {
        handle,
        action,
        target,
        confirmed,
    } = request;
    let Some(view) = changes.state(&id, handle) else {
        return Ok(None);
    };
    if matches!(*view.lists, Lists::Commit { .. }) {
        return Err(ChangesError::Refused("a commit's files cannot be changed"));
    }
    let list = list_of(action);
    let files = match &target {
        ChangeTarget::Files { files } => files_of(&view.lists, files, list)?,
        ChangeTarget::Hunk { file, .. } => files_of(&view.lists, &[*file], list)?,
    };
    let patch = match &target {
        ChangeTarget::Files { .. } => None,
        ChangeTarget::Hunk { file, hunk } => {
            let changed = files
                .first()
                .cloned()
                .ok_or(ChangesError::UnknownFile(file.id))?;
            match hunk_plan(&view, changed, *file, *hunk).await? {
                Some(HunkPatch::Patch(patch)) => Some(patch),
                Some(HunkPatch::WholeFile) => None,
                None => return Ok(Some(ChangeStart::FileChanged)),
            }
        }
    };
    let plan = match patch {
        Some(patch) => Plan::Patch(patch),
        None => Plan::Files {
            unborn: action == ChangeAction::Unstage && unborn(view.path.clone()).await?,
            files: files.clone(),
        },
    };
    if action == ChangeAction::Discard && !confirmed {
        let repo = workspaces
            .view(&id)?
            .repos
            .into_iter()
            .find(|known| known.id == view.repo)
            .map_or_else(|| view.repo.to_string(), |known| known.name);
        return Ok(Some(ChangeStart::Confirm {
            repo,
            files: files.iter().map(|file| file.path.clone()).collect(),
        }));
    }
    let job = WriteJob::single(id, view.repo, label(action, &target, &files));
    let handle = jobs.start(git, job, move |job| async move {
        let path = view.path;
        run(&job.git, &path, action, &plan, job.write()).await
    })?;
    Ok(Some(ChangeStart::Started {
        job_id: handle.job_id,
    }))
}
