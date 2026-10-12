//! History pages (PERF-2, ARCH-8), read on a blocking thread so the async
//! runtime's workers never wait on the disk (ARCH-7), and the history the
//! pane shows: one walk per workspace (ARCH-9), continued page by page, and
//! the offer to write a commit-graph file when a walk without one is slow.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::{Duration, Instant};

use gitcroc_git::{
    CommitRow, Cursor, Filter, GitError, GitRepo, GitRunner, HistoryError, HistoryStart, OpenError,
    Page, RepoRead,
};
use serde::{Deserialize, Serialize};

use crate::{JobId, JobProgress, RepoId, WorkspaceError, WorkspaceId, Workspaces};

/// A first page slower than this, read without a commit-graph file, earns
/// the offer to write one: the section 9 budget for the first page.
pub const COMMIT_GRAPH_THRESHOLD: Duration = Duration::from_millis(200);

/// Why a history page could not be produced.
#[derive(Debug, thiserror::Error)]
pub enum HistoryPageError {
    #[error(transparent)]
    Read(#[from] HistoryError),
    #[error("the history read did not finish: {0}")]
    Stopped(#[from] tokio::task::JoinError),
}

/// A page of `repo`'s history; see [`RepoRead::history`].
pub async fn history_page<R>(
    repo: R,
    start: HistoryStart,
    cursor: Option<Cursor>,
    filter: Option<Filter>,
) -> Result<Page, HistoryPageError>
where
    R: RepoRead + Send + 'static,
{
    let page = tokio::task::spawn_blocking(move || repo.history(start, cursor, filter)).await??;
    Ok(page)
}

/// What the history pane shows for a single-repo selection (section 8
/// "History pane"). Names are short, as the tree shows them.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum HistoryTarget {
    /// Every local and remote branch.
    Repo,
    /// A local branch, such as `main`.
    Branch {
        name: String,
    },
    /// A remote branch with its remote, such as `origin/main`.
    RemoteBranch {
        name: String,
    },
    Tag {
        name: String,
    },
    /// `stash@{index}`, shown as one row.
    Stash {
        index: u32,
    },
}

impl HistoryTarget {
    /// Full ref names, so a tag and a branch of the same name stay apart.
    fn start(&self) -> HistoryStart {
        match self {
            Self::Repo => HistoryStart::All,
            Self::Branch { name } => HistoryStart::Ref(format!("refs/heads/{name}")),
            Self::RemoteBranch { name } => HistoryStart::Ref(format!("refs/remotes/{name}")),
            Self::Tag { name } => HistoryStart::Ref(format!("refs/tags/{name}")),
            Self::Stash { index } => HistoryStart::Stash(*index as usize),
        }
    }
}

/// A page of the history the pane shows.
#[derive(Debug, Clone, Serialize, specta::Type)]
pub struct HistoryView {
    /// Names this walk for `history_more`.
    pub handle: u32,
    pub rows: Vec<CommitRow>,
    /// No page follows.
    pub done: bool,
    /// Offer to write a commit-graph file for the repo (PERF-2). Only a
    /// first page sets it.
    pub offer_commit_graph: bool,
}

/// Why the history could not be shown.
#[derive(Debug, thiserror::Error)]
pub enum HistoryViewError {
    #[error(transparent)]
    Workspace(#[from] WorkspaceError),
    #[error(transparent)]
    Open(#[from] OpenError),
    #[error(transparent)]
    Page(#[from] HistoryPageError),
    #[error("no further page is waiting: the history ended or a page is still being read")]
    NoPage,
}

impl From<HistoryError> for HistoryViewError {
    fn from(error: HistoryError) -> Self {
        Self::Page(error.into())
    }
}

impl From<tokio::task::JoinError> for HistoryViewError {
    fn from(error: tokio::task::JoinError) -> Self {
        Self::Page(error.into())
    }
}

/// A workspace's walk between pages.
#[derive(Debug)]
struct Walk {
    handle: u32,
    repo: Option<GitRepo>,
    start: HistoryStart,
    filter: Option<Filter>,
    /// `None` at the end of history and while a page is being read.
    cursor: Option<Cursor>,
}

/// The walk of every open workspace, and the commit-graph writes running.
#[derive(Debug)]
pub struct Histories {
    walks: Mutex<HashMap<WorkspaceId, Walk>>,
    writes: Mutex<HashMap<(WorkspaceId, RepoId), JobId>>,
    threshold: Duration,
}

impl Default for Histories {
    fn default() -> Self {
        Self::new(COMMIT_GRAPH_THRESHOLD)
    }
}

impl Histories {
    /// `threshold`: see [`COMMIT_GRAPH_THRESHOLD`]; tests make it small.
    pub fn new(threshold: Duration) -> Self {
        Self {
            walks: Mutex::new(HashMap::new()),
            writes: Mutex::new(HashMap::new()),
            threshold,
        }
    }

    /// Makes a new walk the latest of workspace `id`; any older one is
    /// dropped.
    fn begin(&self, id: &WorkspaceId, start: HistoryStart, filter: Option<Filter>) -> u32 {
        let mut walks = lock(&self.walks);
        let handle = walks.get(id).map_or(1, |walk| walk.handle.wrapping_add(1));
        walks.insert(
            id.clone(),
            Walk {
                handle,
                repo: None,
                start,
                filter,
                cursor: None,
            },
        );
        handle
    }

    /// Keeps where walk `handle` continues, unless a newer walk replaced
    /// it; returns whether it is still the latest.
    fn keep(&self, id: &WorkspaceId, handle: u32, repo: GitRepo, cursor: Option<Cursor>) -> bool {
        let mut walks = lock(&self.walks);
        match walks.get_mut(id) {
            Some(walk) if walk.handle == handle => {
                walk.repo = Some(repo);
                walk.cursor = cursor;
                true
            }
            _ => false,
        }
    }

    fn writing(&self, id: &WorkspaceId, repo: &RepoId) -> bool {
        lock(&self.writes).contains_key(&(id.clone(), repo.clone()))
    }
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    // Every change is a single insert, take or removal, so a panic leaves
    // no half-changed state.
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

fn filter_of(text: &str) -> Option<Filter> {
    (!text.is_empty()).then(|| Filter::new(text))
}

/// The first page of `target` in `repo`, filtered by `text` when it is not
/// empty (UI-12), as a new walk that supersedes the workspace's previous
/// one. `None` when a newer walk superseded this one before it was read.
pub async fn history_get(
    histories: Arc<Histories>,
    workspaces: Arc<Workspaces>,
    id: WorkspaceId,
    repo_id: RepoId,
    target: HistoryTarget,
    text: String,
) -> Result<Option<HistoryView>, HistoryViewError> {
    let start = target.start();
    let filter = filter_of(&text);
    let handle = histories.begin(&id, start.clone(), filter.clone());
    let path = workspaces.repo_path(&id, &repo_id)?;
    let declined = workspaces.commit_graph_declined(&id, &repo_id)?;
    let (repo, page, elapsed) = tokio::task::spawn_blocking(move || {
        let began = Instant::now();
        let repo = GitRepo::open(&path)?;
        let page = repo.history(start, None, filter)?;
        Ok::<_, HistoryViewError>((repo, page, began.elapsed()))
    })
    .await??;
    let offer = !declined
        && !page.commit_graph
        && elapsed > histories.threshold
        && !histories.writing(&id, &repo_id);
    let done = page.next.is_none();
    if !histories.keep(&id, handle, repo, page.next) {
        return Ok(None);
    }
    Ok(Some(HistoryView {
        handle,
        rows: page.rows,
        done,
        offer_commit_graph: offer,
    }))
}

/// The next page of walk `handle`. `None` when a newer walk of the
/// workspace replaced it.
pub async fn history_more(
    histories: Arc<Histories>,
    id: WorkspaceId,
    handle: u32,
) -> Result<Option<HistoryView>, HistoryViewError> {
    let (repo, start, filter, cursor) = {
        let mut walks = lock(&histories.walks);
        let Some(walk) = walks.get_mut(&id).filter(|walk| walk.handle == handle) else {
            return Ok(None);
        };
        let (Some(repo), Some(cursor)) = (walk.repo.clone(), walk.cursor.take()) else {
            return Err(HistoryViewError::NoPage);
        };
        (repo, walk.start.clone(), walk.filter.clone(), cursor)
    };
    let reader = repo.clone();
    let page =
        tokio::task::spawn_blocking(move || reader.history(start, Some(cursor), filter)).await??;
    let done = page.next.is_none();
    if !histories.keep(&id, handle, repo, page.next) {
        return Ok(None);
    }
    Ok(Some(HistoryView {
        handle,
        rows: page.rows,
        done,
        offer_commit_graph: false,
    }))
}

/// Why writing a commit-graph file failed.
#[derive(Debug, thiserror::Error)]
pub enum CommitGraphError {
    #[error(transparent)]
    Workspace(#[from] WorkspaceError),
    #[error("git could not write the commit-graph file: {0}")]
    Git(#[from] GitError),
}

/// Starts writing `repo`'s commit-graph file as job `job_id`, unless one
/// is being written already: returns the job that writes it and whether
/// it is new. A new job must then be run with [`commit_graph_write`].
pub fn commit_graph_begin(
    histories: &Histories,
    id: &WorkspaceId,
    repo: &RepoId,
    job_id: JobId,
) -> (JobId, bool) {
    let mut writes = lock(&histories.writes);
    match writes.get(&(id.clone(), repo.clone())) {
        Some(running) => (*running, false),
        None => {
            writes.insert((id.clone(), repo.clone()), job_id);
            (job_id, true)
        }
    }
}

/// Writes `repo`'s commit-graph file with `git commit-graph write
/// --reachable` through the runner (ARCH-1), reporting progress as job
/// `job_id`: unknown at start, done at the end. The job must have been
/// started with [`commit_graph_begin`]. Nothing else changes: no git
/// config, and the file is not kept current afterwards.
pub async fn commit_graph_write(
    histories: Arc<Histories>,
    workspaces: Arc<Workspaces>,
    id: WorkspaceId,
    repo: RepoId,
    git: GitRunner,
    job_id: JobId,
    progress: impl Fn(JobProgress),
) -> Result<(), CommitGraphError> {
    let result = write(&workspaces, &id, &repo, &git, job_id, &progress).await;
    lock(&histories.writes).remove(&(id, repo));
    result
}

async fn write(
    workspaces: &Workspaces,
    id: &WorkspaceId,
    repo: &RepoId,
    git: &GitRunner,
    job_id: JobId,
    progress: &impl Fn(JobProgress),
) -> Result<(), CommitGraphError> {
    let path: PathBuf = workspaces.repo_path(id, repo)?;
    let name = workspaces
        .view(id)?
        .repos
        .into_iter()
        .find(|view| view.id == *repo)
        .map_or_else(|| repo.to_string(), |view| view.name);
    let report = |done, total| {
        progress(JobProgress {
            job_id,
            workspace_id: id.clone(),
            label: format!("Writing the commit-graph file of {name}"),
            done,
            total,
        });
    };
    report(0, 0);
    let args = ["commit-graph", "write", "--reachable"].map(Into::into);
    git.run(&args, Some(&path)).await?;
    report(1, 1);
    Ok(())
}

#[cfg(test)]
mod tests {
    use gitcroc_git::GitRepo;
    use repo_gen::Fixture;

    use super::*;

    /// A stand-in for a git CLI fallback behind the same trait.
    struct Fallback;

    impl RepoRead for Fallback {
        fn history(
            &self,
            start: HistoryStart,
            _cursor: Option<Cursor>,
            _filter: Option<Filter>,
        ) -> Result<Page, HistoryError> {
            match start {
                HistoryStart::Stash(index) => Err(HistoryError::NoStash { index }),
                _ => Ok(Page {
                    rows: Vec::new(),
                    commit_graph: false,
                    next: None,
                }),
            }
        }
    }

    #[tokio::test]
    async fn arch_2_history_reads_go_through_the_read_trait() {
        let page = history_page(Fallback, HistoryStart::All, None, None)
            .await
            .unwrap();
        assert!(page.rows.is_empty());
        let error = history_page(Fallback, HistoryStart::Stash(3), None, None).await;
        assert!(matches!(
            error,
            Err(HistoryPageError::Read(HistoryError::NoStash { index: 3 }))
        ));
    }

    #[tokio::test]
    async fn perf_2_core_forwards_history_pages() {
        let fixture = Fixture::new().unwrap();
        let git = fixture.repo("app").unwrap();
        git.commit_file("a.txt", "a\n", "First").unwrap();
        git.commit_file("b.txt", "b\n", "Second").unwrap();
        let repo = GitRepo::open(git.path()).unwrap();

        let page = history_page(repo.clone(), HistoryStart::All, None, None)
            .await
            .unwrap();
        let subjects: Vec<&str> = page.rows.iter().map(|row| row.subject.as_str()).collect();
        assert_eq!(subjects, ["Second", "First"]);
        assert!(page.next.is_none());

        let missing = history_page(repo, HistoryStart::Ref("nope".into()), None, None).await;
        assert!(matches!(
            missing,
            Err(HistoryPageError::Read(HistoryError::UnknownRef { .. }))
        ));
    }
}
