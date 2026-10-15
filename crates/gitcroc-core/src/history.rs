//! History pages (PERF-2, ARCH-8), read on a blocking thread so the async
//! runtime's workers never wait on the disk (ARCH-7), and the history the
//! pane shows: one walk per workspace (ARCH-9), of one repo or merged from
//! a target set's repos (PERF-5), continued page by page, and the offer to
//! write a commit-graph file when a walk without one is slow.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::{Duration, Instant};

use gitcroc_git::{
    CommitRow, Cursor, Filter, GitRepo, GitRunner, HistoryError, HistoryStart, MergedCursor,
    MergedRow, OpenError, Page, RepoRead, merged_history, ops,
};
use serde::{Deserialize, Serialize};
use tokio::sync::oneshot;

use crate::{
    JobError, JobId, JobView, Jobs, NothingToDo, OnCancel, RepoId, TargetSet, WorkspaceError,
    WorkspaceId, Workspaces, WriteJob, target_repos,
};

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

/// What the history pane shows (section 8 "History pane"): for a
/// single-repo selection the repo or one of its refs, by short name as the
/// tree shows it; for a group or several repos their merged history.
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
    /// The commits of every repo of the target set from its `HEAD`, newest
    /// first by commit date (PERF-5). Its rows have no graph cell.
    Merged {
        target: TargetSet,
    },
}

impl HistoryTarget {
    /// Full ref names, so a tag and a branch of the same name stay apart.
    /// `None` for a merged history, which has a start per repo.
    fn start(&self) -> Option<HistoryStart> {
        Some(match self {
            Self::Repo => HistoryStart::All,
            Self::Branch { name } => HistoryStart::Ref(format!("refs/heads/{name}")),
            Self::RemoteBranch { name } => HistoryStart::Ref(format!("refs/remotes/{name}")),
            Self::Tag { name } => HistoryStart::Ref(format!("refs/tags/{name}")),
            Self::Stash { index } => HistoryStart::Stash(*index as usize),
            Self::Merged { .. } => return None,
        })
    }
}

/// A commit of the history pane and the repo it is in.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, specta::Type)]
pub struct HistoryRow {
    #[serde(flatten)]
    pub commit: CommitRow,
    pub repo_id: RepoId,
}

/// A page of the history the pane shows.
#[derive(Debug, Clone, Serialize, specta::Type)]
pub struct HistoryView {
    /// Names this walk for `history_more`.
    pub handle: u32,
    pub rows: Vec<HistoryRow>,
    /// The repos the history is of: the one repo, or the target set's in
    /// its order (GRP-5). A change to any of their refs makes it stale.
    pub repos: Vec<RepoId>,
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
    #[error("the history of a repo or one of its refs needs the repo")]
    NoRepo,
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

/// A workspace's walk between pages. It keeps where the walk continues,
/// not the open repos: an open repo keeps the parts of its pack files the
/// walk read mapped into memory for as long as the walk is shown (M1-14).
/// Each page opens the repos again.
#[derive(Debug)]
struct Walk {
    filter: Option<Filter>,
    kind: WalkKind,
}

#[derive(Debug)]
enum WalkKind {
    Single {
        repo: RepoId,
        path: PathBuf,
        start: HistoryStart,
        /// `None` at the end of history and while a page is being read.
        cursor: Option<Cursor>,
    },
    Merged {
        repos: Vec<RepoId>,
        paths: Vec<PathBuf>,
        /// `None` at the end of history and while a page is being read.
        cursor: Option<MergedCursor>,
    },
}

/// Where a walk continues, as a page returns it.
enum Next {
    Single(Option<Cursor>),
    Merged(Option<MergedCursor>),
}

impl Next {
    fn is_end(&self) -> bool {
        matches!(self, Self::Single(None) | Self::Merged(None))
    }
}

/// A workspace's latest walk handle, and its walk unless it was released
/// (UI-31). Handles keep counting up, so an old one never comes back.
#[derive(Debug)]
struct Slot {
    handle: u32,
    walk: Option<Walk>,
}

/// The walk of every open workspace, and the commit-graph writes running.
#[derive(Debug)]
pub struct Histories {
    walks: Mutex<HashMap<WorkspaceId, Slot>>,
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

    /// Drops the walk of workspace `id`, whose tab closed (ARCH-9). A
    /// commit-graph write it started runs on as a job.
    pub fn forget(&self, id: &WorkspaceId) {
        lock(&self.walks).remove(id);
    }

    /// Ends the walk of workspace `id`, whose tab went to the background
    /// (UI-31): its handle then gives `None`, as a superseded walk's does.
    pub fn release(&self, id: &WorkspaceId) {
        if let Some(slot) = lock(&self.walks).get_mut(id) {
            slot.handle = slot.handle.wrapping_add(1);
            slot.walk = None;
        }
    }

    /// Whether the core holds a walk of workspace `id`.
    pub fn holds_walk(&self, id: &WorkspaceId) -> bool {
        lock(&self.walks)
            .get(id)
            .is_some_and(|slot| slot.walk.is_some())
    }

    /// Makes a new walk the latest of workspace `id`; any older one is
    /// dropped.
    fn begin(&self, id: &WorkspaceId, walk: Walk) -> u32 {
        let mut walks = lock(&self.walks);
        let handle = walks.get(id).map_or(1, |slot| slot.handle.wrapping_add(1));
        walks.insert(
            id.clone(),
            Slot {
                handle,
                walk: Some(walk),
            },
        );
        handle
    }

    /// Keeps where walk `handle` continues, unless a newer walk replaced
    /// it; returns whether it is still the latest.
    fn keep(&self, id: &WorkspaceId, handle: u32, next: Next) -> bool {
        let mut walks = lock(&self.walks);
        let Some(Slot {
            handle: latest,
            walk: Some(walk),
        }) = walks.get_mut(id)
        else {
            return false;
        };
        if *latest != handle {
            return false;
        }
        match (&mut walk.kind, next) {
            (WalkKind::Single { cursor, .. }, Next::Single(next)) => *cursor = next,
            (WalkKind::Merged { cursor, .. }, Next::Merged(next)) => *cursor = next,
            _ => return false,
        }
        true
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

/// The first page of `target`, of repo `repo_id` unless it is merged,
/// filtered by `text` when it is not empty (UI-12), as a new walk that
/// supersedes the workspace's previous one. `None` when a newer walk
/// superseded this one before it was read.
pub async fn history_get(
    histories: Arc<Histories>,
    workspaces: Arc<Workspaces>,
    id: WorkspaceId,
    repo_id: Option<RepoId>,
    target: HistoryTarget,
    text: String,
) -> Result<Option<HistoryView>, HistoryViewError> {
    let filter = filter_of(&text);
    let (Some(start), Some(repo_id)) = (target.start(), repo_id) else {
        return match target {
            HistoryTarget::Merged { target } => {
                merged_get(histories, workspaces, id, &target, filter).await
            }
            _ => Err(HistoryViewError::NoRepo),
        };
    };
    let path = workspaces.repo_path(&id, &repo_id)?;
    let handle = histories.begin(
        &id,
        Walk {
            filter: filter.clone(),
            kind: WalkKind::Single {
                repo: repo_id.clone(),
                path: path.clone(),
                start: start.clone(),
                cursor: None,
            },
        },
    );
    let declined = workspaces.commit_graph_declined(&id, &repo_id)?;
    let (page, elapsed) = tokio::task::spawn_blocking(move || {
        let began = Instant::now();
        let page = GitRepo::open(&path)?.history(start, None, filter)?;
        Ok::<_, HistoryViewError>((page, began.elapsed()))
    })
    .await??;
    let offer = !declined
        && !page.commit_graph
        && elapsed > histories.threshold
        && !histories.writing(&id, &repo_id);
    let done = page.next.is_none();
    if !histories.keep(&id, handle, Next::Single(page.next)) {
        return Ok(None);
    }
    Ok(Some(HistoryView {
        handle,
        rows: single_rows(&repo_id, page.rows),
        repos: vec![repo_id],
        done,
        offer_commit_graph: offer,
    }))
}

/// The first page of the merged history of `target` (PERF-5).
async fn merged_get(
    histories: Arc<Histories>,
    workspaces: Arc<Workspaces>,
    id: WorkspaceId,
    target: &TargetSet,
    filter: Option<Filter>,
) -> Result<Option<HistoryView>, HistoryViewError> {
    let repos = target_repos(&workspaces, &id, target)?;
    let paths = repos
        .iter()
        .map(|repo| workspaces.repo_path(&id, repo))
        .collect::<Result<Vec<_>, _>>()?;
    let handle = histories.begin(
        &id,
        Walk {
            filter: filter.clone(),
            kind: WalkKind::Merged {
                repos: repos.clone(),
                paths: paths.clone(),
                cursor: None,
            },
        },
    );
    let page = tokio::task::spawn_blocking(move || merged_history(&paths, None, filter)).await??;
    let rows = merged_rows(&repos, page.rows);
    let done = page.next.is_none();
    if !histories.keep(&id, handle, Next::Merged(page.next)) {
        return Ok(None);
    }
    Ok(Some(HistoryView {
        handle,
        rows,
        repos,
        done,
        offer_commit_graph: false,
    }))
}

fn single_rows(repo: &RepoId, rows: Vec<CommitRow>) -> Vec<HistoryRow> {
    rows.into_iter()
        .map(|commit| HistoryRow {
            commit,
            repo_id: repo.clone(),
        })
        .collect()
}

/// The rows of a merged page with the repo of each; a row names its repo
/// by its index among `repos`.
fn merged_rows(repos: &[RepoId], rows: Vec<MergedRow>) -> Vec<HistoryRow> {
    rows.into_iter()
        .filter_map(|row| {
            repos.get(row.repo).map(|repo| HistoryRow {
                commit: row.row,
                repo_id: repo.clone(),
            })
        })
        .collect()
}

/// What `history_more` reads: a walk taken out of its slot.
enum Taken {
    Single {
        repo: RepoId,
        path: PathBuf,
        start: HistoryStart,
        cursor: Cursor,
    },
    Merged {
        repos: Vec<RepoId>,
        paths: Vec<PathBuf>,
        cursor: MergedCursor,
    },
}

/// The next page of walk `handle`. `None` when a newer walk of the
/// workspace replaced it.
pub async fn history_more(
    histories: Arc<Histories>,
    id: WorkspaceId,
    handle: u32,
) -> Result<Option<HistoryView>, HistoryViewError> {
    let (filter, taken) = {
        let mut walks = lock(&histories.walks);
        let Some(walk) = walks
            .get_mut(&id)
            .filter(|slot| slot.handle == handle)
            .and_then(|slot| slot.walk.as_mut())
        else {
            return Ok(None);
        };
        let taken = match &mut walk.kind {
            WalkKind::Single {
                repo,
                path,
                start,
                cursor,
            } => cursor.take().map(|cursor| Taken::Single {
                repo: repo.clone(),
                path: path.clone(),
                start: start.clone(),
                cursor,
            }),
            WalkKind::Merged {
                repos,
                paths,
                cursor,
            } => cursor.take().map(|cursor| Taken::Merged {
                repos: repos.clone(),
                paths: paths.clone(),
                cursor,
            }),
        };
        let Some(taken) = taken else {
            return Err(HistoryViewError::NoPage);
        };
        (walk.filter.clone(), taken)
    };
    let (rows, repos, next) = tokio::task::spawn_blocking(move || match taken {
        Taken::Single {
            repo,
            path,
            start,
            cursor,
        } => {
            let page = GitRepo::open(&path)?.history(start, Some(cursor), filter)?;
            let rows = single_rows(&repo, page.rows);
            Ok::<_, HistoryViewError>((rows, vec![repo], Next::Single(page.next)))
        }
        Taken::Merged {
            repos,
            paths,
            cursor,
        } => {
            let page = merged_history(&paths, Some(cursor), filter)?;
            let rows = merged_rows(&repos, page.rows);
            Ok((rows, repos, Next::Merged(page.next)))
        }
    })
    .await??;
    let done = next.is_end();
    if !histories.keep(&id, handle, next) {
        return Ok(None);
    }
    Ok(Some(HistoryView {
        handle,
        rows,
        repos,
        done,
        offer_commit_graph: false,
    }))
}

/// Why a commit-graph write could not start.
#[derive(Debug, thiserror::Error)]
pub enum CommitGraphError {
    #[error(transparent)]
    Workspace(#[from] WorkspaceError),
    #[error(transparent)]
    Job(#[from] JobError),
}

/// A commit-graph write: the job that writes the file, and its view when it
/// ends; `None` when the request joined a write already running.
#[derive(Debug)]
pub struct CommitGraphWrite {
    pub job_id: JobId,
    pub ended: Option<oneshot::Receiver<JobView>>,
}

/// Starts a job of `jobs` that writes `repo`'s commit-graph file with `git
/// commit-graph write --reachable` (ARCH-1), unless one is running for the
/// repo already: then that job is returned. Nothing else changes: no git
/// config, and the file is not kept current afterwards. Needs a tokio
/// runtime.
pub fn commit_graph_write(
    histories: &Arc<Histories>,
    jobs: &Arc<Jobs>,
    workspaces: &Workspaces,
    id: &WorkspaceId,
    repo: &RepoId,
    git: GitRunner,
) -> Result<CommitGraphWrite, CommitGraphError> {
    let path = workspaces.repo_path(id, repo)?;
    let name = workspaces
        .view(id)?
        .repos
        .into_iter()
        .find(|view| view.id == *repo)
        .map_or_else(|| repo.to_string(), |view| view.name);
    let key = (id.clone(), repo.clone());
    let mut writes = lock(&histories.writes);
    if let Some(running) = writes.get(&key) {
        return Ok(CommitGraphWrite {
            job_id: *running,
            ended: None,
        });
    }
    let job = WriteJob {
        workspace_id: id.clone(),
        repos: vec![repo.clone()],
        label: format!("Writing the commit-graph file of {name}"),
        on_cancel: OnCancel::Finish,
        may_conflict: false,
        nothing_to_do: NothingToDo::Never,
        network: false,
    };
    let handle = jobs.start(git, job, move |run| async move {
        ops::commit_graph_write(&run.git, &path, run.write()).await
    })?;
    writes.insert(key.clone(), handle.job_id);
    drop(writes);
    let (forward, ended) = oneshot::channel();
    let histories = histories.clone();
    tokio::spawn(async move {
        let view = handle.ended.await;
        lock(&histories.writes).remove(&key);
        if let Ok(view) = view {
            // The caller may not wait for the end.
            let _ = forward.send(view);
        }
    });
    Ok(CommitGraphWrite {
        job_id: handle.job_id,
        ended: Some(ended),
    })
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
