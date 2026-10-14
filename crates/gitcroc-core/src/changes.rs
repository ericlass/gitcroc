//! What the changes pane shows (UI-14, UI-15, UI-16): a commit's header and
//! changed files, or a repo's uncommitted lists, then one file's diff at a
//! time (ARCH-8). One view per workspace (ARCH-9): the core keeps its lists,
//! so the UI names a file by list and id, never by path (PLAT-2). A newer
//! view supersedes the older one, whose calls then return `None`.

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use gitcroc_git::{
    ChangedFile, CommitDetails, ContentKey, Context, DiffOptions, DiffSource, FileDiff, GitRepo,
    GitRunner, OpenError, StatusReader, UnmergedFile,
};
use serde::{Deserialize, Serialize};

use crate::{DiffReadError, RepoId, StatusReadError, WorkspaceError, WorkspaceId, Workspaces};

/// What to show.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ChangesTarget {
    /// A commit against one of its parents, 0 being the first (UI-16).
    Commit { hash: String, parent: u32 },
    /// The working copy (UI-15).
    Uncommitted,
}

/// What the changes pane lists.
#[derive(Debug, Clone, Serialize, specta::Type)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ChangesContent {
    Commit {
        details: CommitDetails,
        /// The parent the files are compared with.
        parent: u32,
        files: Vec<ChangedFile>,
    },
    Uncommitted {
        staged: Vec<ChangedFile>,
        /// Untracked files included.
        unstaged: Vec<ChangedFile>,
        /// In neither list above, as in `git status`.
        unmerged: Vec<UnmergedFile>,
        /// The last commit's message, which Amend starts from; `None` on a
        /// branch with no commit yet, where there is nothing to amend.
        last_message: Option<crate::CommitMessage>,
        /// In conflict state (OPS-1): no commit box, as Continue makes the
        /// commit (M2-08).
        in_conflict: bool,
        /// The config keys the offer to speed up status would turn on
        /// (PERF-7); `None` when there is no offer.
        offer_fsmonitor: Option<Vec<String>>,
    },
}

/// A view of the changes pane.
#[derive(Debug, Clone, Serialize, specta::Type)]
pub struct ChangesView {
    /// Names this view for `diff_get`, `diff_image` and the conflict
    /// editor's calls.
    pub handle: u32,
    pub content: ChangesContent,
}

/// The list a file of a view belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "snake_case")]
pub enum FileList {
    Commit,
    Staged,
    Unstaged,
}

/// A file of a view: its list and its id in that list.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, specta::Type)]
pub struct FileRef {
    pub list: FileList,
    pub id: u32,
}

/// The options of a file diff (UI-17, UI-18), as the UI sends them.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
pub struct DiffRequest {
    pub ignore_whitespace: bool,
    pub context: Context,
    /// Return the hunks of a diff over the line limit (UI-18).
    pub load_anyway: bool,
    /// Return both whole files with the hunks (side-by-side mode).
    pub full_text: bool,
}

impl From<DiffRequest> for DiffOptions {
    fn from(request: DiffRequest) -> Self {
        Self {
            ignore_whitespace: request.ignore_whitespace,
            context: request.context,
            load_anyway: request.load_anyway,
            full_text: request.full_text,
        }
    }
}

/// A side of an image diff.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "snake_case")]
pub enum ImageSide {
    Old,
    New,
}

/// Why the changes could not be shown.
#[derive(Debug, thiserror::Error)]
pub enum ChangesError {
    #[error(transparent)]
    Workspace(#[from] WorkspaceError),
    #[error(transparent)]
    Open(#[from] OpenError),
    #[error(transparent)]
    Diff(#[from] DiffReadError),
    #[error(transparent)]
    Status(#[from] StatusReadError),
    #[error("the list has no file {0}")]
    UnknownFile(u32),
    #[error(transparent)]
    Job(#[from] crate::JobError),
    #[error("{0}")]
    Refused(&'static str),
    #[error(transparent)]
    Conflict(#[from] gitcroc_git::ConflictError),
    #[error("the read did not finish: {0}")]
    Stopped(#[from] tokio::task::JoinError),
}

/// The lists of a view, which file diffs are read from.
#[derive(Debug)]
pub(crate) enum Lists {
    Commit {
        hash: String,
        parent: usize,
        files: Vec<ChangedFile>,
    },
    Uncommitted {
        staged: Vec<ChangedFile>,
        unstaged: Vec<ChangedFile>,
        /// For the conflict editor (OPS-8), by id.
        unmerged: Vec<UnmergedFile>,
    },
}

impl Lists {
    /// The file `file` names and the source to diff it with.
    pub(crate) fn file(&self, file: FileRef) -> Option<(DiffSource, ChangedFile)> {
        let (source, list) = match (self, file.list) {
            (
                Self::Commit {
                    hash,
                    parent,
                    files,
                },
                FileList::Commit,
            ) => (
                DiffSource::Commit {
                    hash: hash.clone(),
                    parent: *parent,
                },
                files,
            ),
            (Self::Uncommitted { staged, .. }, FileList::Staged) => (DiffSource::Staged, staged),
            (Self::Uncommitted { unstaged, .. }, FileList::Unstaged) => {
                (DiffSource::Unstaged, unstaged)
            }
            _ => return None,
        };
        let found = list.get(usize::try_from(file.id).ok()?)?;
        Some((source, found.clone()))
    }

    /// Unmerged file `id`, if the lists are the working copy's and have it.
    pub(crate) fn unmerged(&self, id: u32) -> Option<&UnmergedFile> {
        match self {
            Self::Uncommitted { unmerged, .. } => unmerged.get(usize::try_from(id).ok()?),
            Self::Commit { .. } => None,
        }
    }
}

/// The bytes of the last image diff read, for `diff_image`.
#[derive(Debug)]
struct Images {
    file: FileRef,
    old: Option<Vec<u8>>,
    new: Option<Vec<u8>>,
}

/// The text diff last read: the file, how, and what from, so that one of
/// its hunks can be applied as the user saw it (UI-15).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ShownDiff {
    pub file: FileRef,
    pub options: DiffOptions,
    pub content: ContentKey,
}

/// A workspace's view. `lists` is `None` while it is read. It keeps the
/// repo's path, not the open repo: an open repo keeps the parts of its pack
/// files the reads used mapped into memory for as long as the view is shown
/// (M1-14). Each diff opens the repo again.
#[derive(Debug)]
struct Shown {
    handle: u32,
    repo: RepoId,
    path: PathBuf,
    lists: Option<Arc<Lists>>,
    images: Option<Images>,
    diff: Option<ShownDiff>,
}

/// What a write to a view's working copy needs from the view.
#[derive(Debug, Clone)]
pub(crate) struct ViewState {
    pub repo: RepoId,
    pub path: PathBuf,
    pub lists: Arc<Lists>,
    pub diff: Option<ShownDiff>,
}

/// The view of every open workspace, and the repos whose status settings
/// are being turned on (PERF-7).
#[derive(Debug)]
pub struct Changes {
    shown: Mutex<HashMap<WorkspaceId, Shown>>,
    pub(crate) enabling: Mutex<HashSet<(WorkspaceId, RepoId)>>,
    fsmonitor_threshold: usize,
}

impl Default for Changes {
    fn default() -> Self {
        Self::with_fsmonitor_threshold(crate::FSMONITOR_THRESHOLD)
    }
}

impl Changes {
    pub fn new() -> Self {
        Self::default()
    }

    /// `threshold`: see [`crate::FSMONITOR_THRESHOLD`]; tests make it
    /// small.
    pub fn with_fsmonitor_threshold(threshold: usize) -> Self {
        Self {
            shown: Mutex::new(HashMap::new()),
            enabling: Mutex::new(HashSet::new()),
            fsmonitor_threshold: threshold,
        }
    }

    /// Makes a new view the latest of workspace `id`.
    fn begin(&self, id: &WorkspaceId, repo: RepoId, path: PathBuf) -> u32 {
        let mut shown = lock(&self.shown);
        let handle = shown.get(id).map_or(1, |view| view.handle.wrapping_add(1));
        shown.insert(
            id.clone(),
            Shown {
                handle,
                repo,
                path,
                lists: None,
                images: None,
                diff: None,
            },
        );
        handle
    }

    /// The repo's path and the lists of view `handle`, if it is the latest
    /// and read.
    fn lists(&self, id: &WorkspaceId, handle: u32) -> Option<(PathBuf, Arc<Lists>)> {
        let shown = lock(&self.shown);
        let view = shown.get(id).filter(|view| view.handle == handle)?;
        Some((view.path.clone(), view.lists.clone()?))
    }

    /// What view `handle` shows, if it is the latest and read.
    pub(crate) fn state(&self, id: &WorkspaceId, handle: u32) -> Option<ViewState> {
        let shown = lock(&self.shown);
        let view = shown.get(id).filter(|view| view.handle == handle)?;
        Some(ViewState {
            repo: view.repo.clone(),
            path: view.path.clone(),
            lists: view.lists.clone()?,
            diff: view.diff,
        })
    }

    /// Runs `change` on view `handle` if it is still the latest.
    fn update(&self, id: &WorkspaceId, handle: u32, change: impl FnOnce(&mut Shown)) -> bool {
        let mut shown = lock(&self.shown);
        match shown.get_mut(id) {
            Some(view) if view.handle == handle => {
                change(view);
                true
            }
            _ => false,
        }
    }
}

pub(crate) fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    // Every change replaces whole fields, so a panic leaves no half-changed
    // state.
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

/// Reads what `target` shows in repo `repo_id` as a new view of workspace
/// `id`, which supersedes the previous one. `git` reads the working copy
/// where `gix` cannot (M1-04). `None` when a newer view superseded this one
/// before it was read.
pub async fn changes_get(
    changes: Arc<Changes>,
    workspaces: Arc<Workspaces>,
    git: GitRunner,
    id: WorkspaceId,
    repo_id: RepoId,
    target: ChangesTarget,
) -> Result<Option<ChangesView>, ChangesError> {
    let path = workspaces.repo_path(&id, &repo_id)?;
    let handle = changes.begin(&id, repo_id.clone(), path.clone());
    let repo = tokio::task::spawn_blocking(move || GitRepo::open(&path)).await??;
    let (content, lists) = match target {
        ChangesTarget::Commit { hash, parent } => {
            let index = parent as usize;
            let details = crate::commit_details(repo.clone(), hash.clone()).await?;
            let files = crate::changed_files(repo.clone(), hash.clone(), index).await?;
            let lists = Lists::Commit {
                hash,
                parent: index,
                files: files.clone(),
            };
            let content = ChangesContent::Commit {
                details,
                parent,
                files,
            };
            (content, lists)
        }
        ChangesTarget::Uncommitted => {
            let reader = StatusReader::new(repo.clone(), git);
            let working = crate::working_copy(reader.clone()).await?;
            let in_conflict = crate::repo_state(reader.clone()).await?.in_conflict();
            let offer_fsmonitor = if in_conflict {
                None
            } else {
                let threshold = changes.fsmonitor_threshold;
                crate::fsmonitor::offer(&changes, &workspaces, &id, &repo_id, reader, threshold)
                    .await?
            };
            let last_message = crate::commit::last_message(repo.clone()).await?;
            let lists = Lists::Uncommitted {
                staged: working.staged.clone(),
                unstaged: working.unstaged.clone(),
                unmerged: working.unmerged.clone(),
            };
            let content = ChangesContent::Uncommitted {
                staged: working.staged,
                unstaged: working.unstaged,
                unmerged: working.unmerged,
                last_message,
                in_conflict,
                offer_fsmonitor,
            };
            (content, lists)
        }
    };
    let kept = changes.update(&id, handle, |view| view.lists = Some(Arc::new(lists)));
    Ok(kept.then_some(ChangesView { handle, content }))
}

/// The diff of one file of view `handle` (ARCH-8). `None` when a newer view
/// of the workspace replaced it.
pub async fn diff_get(
    changes: Arc<Changes>,
    id: WorkspaceId,
    handle: u32,
    file: FileRef,
    request: DiffRequest,
) -> Result<Option<FileDiff>, ChangesError> {
    let Some((path, lists)) = changes.lists(&id, handle) else {
        return Ok(None);
    };
    let (source, changed) = lists.file(file).ok_or(ChangesError::UnknownFile(file.id))?;
    let repo = tokio::task::spawn_blocking(move || GitRepo::open(&path)).await??;
    let options = DiffOptions::from(request);
    let diff = crate::file_diff(repo, source, changed, options).await?;
    let shown = match &diff {
        FileDiff::Text(text) => Some(ShownDiff {
            file,
            options,
            content: text.content,
        }),
        _ => None,
    };
    let images = match &diff {
        FileDiff::Image { old, new } => Some(Images {
            file,
            old: old.as_ref().map(|image| image.bytes.clone()),
            new: new.as_ref().map(|image| image.bytes.clone()),
        }),
        _ => None,
    };
    let kept = changes.update(&id, handle, |view| {
        view.images = images;
        view.diff = shown;
    });
    Ok(kept.then_some(diff))
}

/// The bytes of one side of the image diff last read for `file` with
/// `diff_get` (UI-18). `None` when that side has no image, or the view or
/// the file is no longer the one read.
pub fn diff_image(
    changes: &Changes,
    id: &WorkspaceId,
    handle: u32,
    file: FileRef,
    side: ImageSide,
) -> Option<Vec<u8>> {
    let shown = lock(&changes.shown);
    let images = shown
        .get(id)
        .filter(|view| view.handle == handle)?
        .images
        .as_ref()
        .filter(|images| images.file == file)?;
    match side {
        ImageSide::Old => images.old.clone(),
        ImageSide::New => images.new.clone(),
    }
}
