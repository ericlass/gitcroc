//! What the changes pane shows (UI-14, UI-15, UI-16, UI-34): a commit's
//! header and changed files, a repo's uncommitted lists, or those of every
//! dirty repo of a group or multi-selection, then one file's diff at a time
//! (ARCH-8). One view per workspace (ARCH-9): the core keeps its lists, so
//! the UI names a file by handle, list and id, never by path (PLAT-2). A
//! merged view has one handle per repo, so each repo's files are staged and
//! diffed as one repo's. A newer view supersedes the older one, whose calls
//! then return `None`.

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use gitcroc_git::{
    ChangedFile, CommitDetails, ContentKey, Context, DiffOptions, DiffSource, FileDiff, GitRepo,
    GitRunner, OpenError, StatusReader, UnmergedFile,
};
use serde::{Deserialize, Serialize};

use crate::lock;
use crate::{
    DiffReadError, RepoId, StatusReadError, TargetSet, WorkspaceError, WorkspaceId, Workspaces,
};

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
    /// The working copies of a group or multi-selection (UI-34).
    Merged {
        /// The repos with uncommitted changes, in target-set order.
        repos: Vec<RepoChanges>,
        /// Every repo of the target set, dirty or not: a change in any of
        /// them may change the view.
        members: Vec<RepoId>,
    },
}

/// One repo's uncommitted changes in a merged view (UI-34).
#[derive(Debug, Clone, Serialize, specta::Type)]
pub struct RepoChanges {
    pub repo_id: RepoId,
    pub repo_name: String,
    /// Names the repo's lists for `diff_get`, `diff_image` and
    /// `changes_apply`, as a single repo's view handle does.
    pub handle: u32,
    pub staged: Vec<ChangedFile>,
    /// Untracked files included.
    pub unstaged: Vec<ChangedFile>,
    pub unmerged: Vec<UnmergedFile>,
    /// Why nothing can be changed here (GRP-8): the repo is in conflict
    /// state, or could not be read; `None` otherwise.
    pub blocked: Option<String>,
}

/// A view of the changes pane.
#[derive(Debug, Clone, Serialize, specta::Type)]
pub struct ChangesView {
    /// Names this view for `diff_get`, `diff_image` and the conflict
    /// editor's calls; 0 for a merged view, whose repos each have their
    /// own.
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

    /// The unmerged files, in list order; none for a commit's lists.
    pub(crate) fn unmerged_files(&self) -> &[UnmergedFile] {
        match self {
            Self::Uncommitted { unmerged, .. } => unmerged,
            Self::Commit { .. } => &[],
        }
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

/// A repo's part of a workspace's view: all of a single repo's view, one
/// repo of a merged one. `lists` is `None` while it is read. It keeps the
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

/// A workspace's view: its parts, and the last handle given, so that a
/// handle names one part only while the app runs.
#[derive(Debug, Default)]
struct Views {
    last: u32,
    parts: Vec<Shown>,
}

impl Views {
    fn part(&self, handle: u32) -> Option<&Shown> {
        self.parts.iter().find(|part| part.handle == handle)
    }
}

/// The view of every open workspace, and the repos whose status settings
/// are being turned on (PERF-7).
#[derive(Debug)]
pub struct Changes {
    shown: Mutex<HashMap<WorkspaceId, Views>>,
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

    /// Drops the view of workspace `id`, whose tab closed (ARCH-9).
    pub fn forget(&self, id: &WorkspaceId) {
        lock(&self.shown).remove(id);
    }

    /// Ends the view of workspace `id`, whose tab went to the background
    /// (UI-31): its lists, diffs and images go, and its handles then give
    /// `None`, as a superseded view's do.
    pub fn release(&self, id: &WorkspaceId) {
        if let Some(views) = lock(&self.shown).get_mut(id) {
            views.parts.clear();
        }
    }

    /// Whether the core holds lists or images of a view of workspace `id`.
    pub fn holds_view(&self, id: &WorkspaceId) -> bool {
        lock(&self.shown).get(id).is_some_and(|views| {
            views
                .parts
                .iter()
                .any(|part| part.lists.is_some() || part.images.is_some())
        })
    }

    /// Makes a new view of `repos`, each with its repo's path, the latest
    /// of workspace `id`, and returns the handle of each.
    fn begin(&self, id: &WorkspaceId, repos: Vec<(RepoId, PathBuf)>) -> Vec<u32> {
        let mut shown = lock(&self.shown);
        let views = shown.entry(id.clone()).or_default();
        views.parts = repos
            .into_iter()
            .map(|(repo, path)| {
                views.last = views.last.wrapping_add(1).max(1);
                Shown {
                    handle: views.last,
                    repo,
                    path,
                    lists: None,
                    images: None,
                    diff: None,
                }
            })
            .collect();
        views.parts.iter().map(|part| part.handle).collect()
    }

    /// The repo's path and the lists of view `handle`, if it is the latest
    /// and read.
    fn lists(&self, id: &WorkspaceId, handle: u32) -> Option<(PathBuf, Arc<Lists>)> {
        let shown = lock(&self.shown);
        let view = shown.get(id)?.part(handle)?;
        Some((view.path.clone(), view.lists.clone()?))
    }

    /// What view `handle` shows, if it is the latest and read.
    pub(crate) fn state(&self, id: &WorkspaceId, handle: u32) -> Option<ViewState> {
        let shown = lock(&self.shown);
        let view = shown.get(id)?.part(handle)?;
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
        let part = shown
            .get_mut(id)
            .and_then(|views| views.parts.iter_mut().find(|part| part.handle == handle));
        match part {
            Some(view) => {
                change(view);
                true
            }
            None => false,
        }
    }
}

/// A working copy as the changes pane lists it.
struct Uncommitted {
    working: gitcroc_git::WorkingCopy,
    in_conflict: bool,
}

/// Reads the working copy and conflict state of `repo`.
async fn uncommitted(repo: GitRepo, git: GitRunner) -> Result<Uncommitted, ChangesError> {
    let reader = StatusReader::new(repo, git);
    let working = crate::working_copy(reader.clone()).await?;
    let in_conflict = crate::repo_state(reader).await?.in_conflict();
    Ok(Uncommitted {
        working,
        in_conflict,
    })
}

impl Uncommitted {
    fn lists(&self) -> Lists {
        Lists::Uncommitted {
            staged: self.working.staged.clone(),
            unstaged: self.working.unstaged.clone(),
            unmerged: self.working.unmerged.clone(),
        }
    }
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
    let begun = changes.begin(&id, vec![(repo_id.clone(), path.clone())]);
    let handle = begun.first().copied().unwrap_or_default();
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
            let read = uncommitted(repo.clone(), git.clone()).await?;
            let lists = read.lists();
            let Uncommitted {
                working,
                in_conflict,
            } = read;
            let offer_fsmonitor = if in_conflict {
                None
            } else {
                let threshold = changes.fsmonitor_threshold;
                let reader = StatusReader::new(repo.clone(), git);
                crate::fsmonitor::offer(&changes, &workspaces, &id, &repo_id, reader, threshold)
                    .await?
            };
            let last_message = crate::commit::last_message(repo.clone()).await?;
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

/// Reads the uncommitted changes of every repo of `target` in workspace
/// `id` as a new view, which supersedes the previous one (UI-34): one part
/// per dirty repo, in target-set order, each read as a single repo's. A
/// repo in conflict state is listed but blocked (GRP-8); a missing one is
/// left out. `None` when a newer view superseded this one before it was
/// read.
pub async fn changes_get_merged(
    changes: Arc<Changes>,
    workspaces: Arc<Workspaces>,
    git: GitRunner,
    id: WorkspaceId,
    target: TargetSet,
) -> Result<Option<ChangesView>, ChangesError> {
    let members = crate::target_repos(&workspaces, &id, &target)?;
    let known = workspaces.view(&id)?.repos;
    let mut repos = Vec::with_capacity(members.len());
    for repo in &members {
        let path = workspaces.repo_path(&id, repo)?;
        if path.exists() {
            repos.push((repo.clone(), path));
        }
    }
    let handles = changes.begin(&id, repos.clone());
    let reads: Vec<_> = repos
        .into_iter()
        .zip(handles)
        .map(|((repo, path), handle)| {
            let git = git.clone();
            let read = tokio::spawn(async move {
                let open = tokio::task::spawn_blocking(move || GitRepo::open(&path)).await??;
                uncommitted(open, git).await
            });
            (repo, handle, read)
        })
        .collect();
    let mut shown = Vec::new();
    for (repo, handle, read) in reads {
        let name = known
            .iter()
            .find(|entry| entry.id == repo)
            .map_or_else(|| repo.0.clone(), |entry| entry.name.clone());
        let read = match read.await {
            Ok(read) => read,
            Err(error) => Err(ChangesError::Stopped(error)),
        };
        let part = match read {
            Ok(read) => {
                let lists = read.lists();
                if !changes.update(&id, handle, |view| view.lists = Some(Arc::new(lists))) {
                    return Ok(None);
                }
                if read.working.is_clean() {
                    continue;
                }
                let Uncommitted {
                    working,
                    in_conflict,
                } = read;
                RepoChanges {
                    repo_id: repo,
                    repo_name: name,
                    handle,
                    staged: working.staged,
                    unstaged: working.unstaged,
                    unmerged: working.unmerged,
                    blocked: in_conflict.then(|| crate::unblocked::IN_CONFLICT.to_owned()),
                }
            }
            Err(error) => RepoChanges {
                repo_id: repo,
                repo_name: name,
                handle,
                staged: Vec::new(),
                unstaged: Vec::new(),
                unmerged: Vec::new(),
                blocked: Some(format!("cannot read the repo: {error}")),
            },
        };
        shown.push(part);
    }
    let content = ChangesContent::Merged {
        repos: shown,
        members,
    };
    Ok(Some(ChangesView { handle: 0, content }))
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
        .get(id)?
        .part(handle)?
        .images
        .as_ref()
        .filter(|images| images.file == file)?;
    match side {
        ImageSide::Old => images.old.clone(),
        ImageSide::New => images.new.clone(),
    }
}
