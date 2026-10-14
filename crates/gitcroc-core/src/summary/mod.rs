//! Repo summaries (UI-3) and the start sequence (PERF-3): at start every
//! repo shows its cached summary, marked stale, at once; a background
//! refresh then reads each repo again, the selected repo first, and keeps
//! the fresh summary in the cache.
//!
//! The refresh reads [`REFRESH_READS`] repo at a time per workspace, each
//! read holding a worker of the pool it shares with jobs (PERF-4), which
//! serves jobs first. Each
//! read runs on a thread of its own at the lowest priority the OS gives, so
//! the webview and the reads the user waits for keep the CPU. A job's own
//! re-read of a repo (OPS-7) runs at once instead, and a background read
//! that began before it is not kept. All state is kept per workspace
//! (ARCH-9).

mod cache;

use std::collections::{HashMap, HashSet, VecDeque};
use std::path::Path;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use gitcroc_git::{GitRepo, GitRunner, RepoSummary, StatusRead, StatusReader};
use serde::{Deserialize, Serialize};

use crate::job::{POOL_SIZE, Pool, Priority};
use crate::{RepoId, WorkspaceError, WorkspaceId, Workspaces};

pub use cache::{CACHE_FILE, CACHE_SCHEMA, CacheDir, PlatformCacheDir, SummaryCache};

/// How many summaries the background refresh reads at once per workspace.
/// Reading up to 8 at once took a refresh of `target-project` from 0.9 s to
/// 0.33 s, but `memory-idle` from 370 to 460 MB and cold start from 596 to
/// 1112 ms, over their section 9 budgets (M2-01).
pub const REFRESH_READS: usize = 1;

/// A repo's summary as the UI sees it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
pub struct SummaryView {
    pub workspace_id: WorkspaceId,
    pub repo_id: RepoId,
    /// The last summary read, in this run or an earlier one; `None` when
    /// the repo has never been read.
    pub summary: Option<RepoSummary>,
    /// `summary` was not read in this run: it comes from the cache, or the
    /// repo could not be read again (PERF-3).
    pub stale: bool,
    /// The repo's folder does not exist (UI-3).
    pub missing: bool,
    /// Why the last read failed; git's own message when git refused the
    /// repo (SEC-6).
    pub error: Option<String>,
    /// A read of the repo is waiting or running (UI-3).
    pub refreshing: bool,
    /// `summary` says the repo is in conflict state (OPS-1, UI-3).
    pub conflict: bool,
    /// Goes up each time the repo's refs or `HEAD` are seen to have changed
    /// in this run; the history reads the repo again when it does (OPS-7).
    pub refs_version: u32,
    /// A write job waits or runs on the repo, so other writes are disabled
    /// (GRP-8) and the row shows the spinner (UI-3).
    pub busy: bool,
}

/// One repo's summary state.
#[derive(Debug, Clone)]
struct Entry {
    summary: Option<RepoSummary>,
    stale: bool,
    missing: bool,
    error: Option<String>,
    refreshing: bool,
    refs_version: u32,
    /// Write jobs waiting or running on the repo.
    busy: u32,
    /// Goes up when a job's re-read begins; a read that began earlier is
    /// not kept.
    epoch: u32,
}

impl Entry {
    fn cached(summary: Option<RepoSummary>) -> Self {
        Self {
            summary,
            stale: true,
            missing: false,
            error: None,
            refreshing: false,
            refs_version: 0,
            busy: 0,
            epoch: 0,
        }
    }

    fn view(&self, workspace: &WorkspaceId, repo: &RepoId) -> SummaryView {
        SummaryView {
            workspace_id: workspace.clone(),
            repo_id: repo.clone(),
            summary: self.summary.clone(),
            stale: self.stale,
            missing: self.missing,
            error: self.error.clone(),
            refreshing: self.refreshing,
            conflict: self
                .summary
                .as_ref()
                .is_some_and(|summary| summary.state.in_conflict()),
            refs_version: self.refs_version,
            busy: self.busy > 0,
        }
    }
}

/// One workspace's summaries and refresh queue.
#[derive(Debug, Default)]
struct WorkspaceState {
    entries: HashMap<RepoId, Entry>,
    /// Read before the rest of the queue when it is waiting.
    selected: Option<RepoId>,
    /// Repos waiting to be read, in workspace order.
    queue: VecDeque<RepoId>,
    /// Reads running now.
    workers: usize,
    /// Repos whose refs the watcher saw change since their last read began.
    refs_changed: HashSet<RepoId>,
}

impl WorkspaceState {
    /// The repo to read next, the selected one when it is waiting. `None`
    /// ends the calling worker.
    fn next(&mut self) -> Option<RepoId> {
        let selected = self
            .selected
            .as_ref()
            .and_then(|selected| self.queue.iter().position(|repo| repo == selected));
        let next = match selected {
            Some(index) => self.queue.remove(index),
            None => self.queue.pop_front(),
        };
        if next.is_none() {
            self.workers = self.workers.saturating_sub(1);
        }
        next
    }
}

/// The summaries of every open workspace, and the cache behind them.
#[derive(Debug)]
pub struct Summaries {
    cache: SummaryCache,
    /// The workers the refresh shares with jobs.
    pool: Arc<Pool>,
    /// Reads at once per workspace.
    limit: usize,
    state: Mutex<HashMap<WorkspaceId, WorkspaceState>>,
}

/// What reading a repo gave.
enum Outcome {
    Read(RepoSummary),
    Missing,
    Failed(String),
}

impl Summaries {
    /// Takes the cached summary of every repo of every open workspace, each
    /// marked stale (PERF-3). Reads only the cache, not the repos.
    pub fn load(cache: SummaryCache, workspaces: &Workspaces) -> Self {
        let mut state = HashMap::new();
        for view in workspaces.views() {
            let mut workspace = WorkspaceState::default();
            for (repo, path) in workspaces.repo_paths(&view.id).unwrap_or_default() {
                workspace
                    .entries
                    .insert(repo, Entry::cached(cache.get(&path)));
            }
            state.insert(view.id, workspace);
        }
        Self {
            cache,
            pool: Pool::new(POOL_SIZE),
            limit: REFRESH_READS,
            state: Mutex::new(state),
        }
    }

    /// Reads in workers of `pool` instead of a pool of its own.
    pub fn with_pool(mut self, pool: Arc<Pool>) -> Self {
        self.pool = pool;
        self
    }

    /// The workers the refresh shares with jobs (PERF-4).
    pub fn pool(&self) -> &Arc<Pool> {
        &self.pool
    }

    /// Runs at most `limit` reads at once per workspace instead of
    /// [`REFRESH_READS`].
    pub fn with_limit(mut self, limit: usize) -> Self {
        self.limit = limit.max(1);
        self
    }

    /// Reads at once per workspace.
    pub fn limit(&self) -> usize {
        self.limit
    }

    /// The current summary of each repo of workspace `id`, in workspace
    /// order. A repo with none yet is stale and has no summary.
    pub fn views(
        &self,
        workspaces: &Workspaces,
        id: &WorkspaceId,
    ) -> Result<Vec<SummaryView>, WorkspaceError> {
        let repos = workspaces.repo_paths(id)?;
        let state = self.lock();
        let entries = state.get(id).map(|workspace| &workspace.entries);
        Ok(repos
            .iter()
            .map(|(repo, _)| {
                entries.and_then(|entries| entries.get(repo)).map_or_else(
                    || Entry::cached(None).view(id, repo),
                    |entry| entry.view(id, repo),
                )
            })
            .collect())
    }

    /// Makes `repo` the selected repo of workspace `id`, read first when it
    /// waits for a refresh (PERF-4). `None` clears the selection.
    pub fn select(&self, id: &WorkspaceId, repo: Option<RepoId>) {
        self.lock().entry(id.clone()).or_default().selected = repo;
    }

    /// Notes that the refs or `HEAD` of `repo` changed on disk, so its next
    /// read raises its `refs_version` (OPS-7).
    pub fn refs_changed(&self, id: &WorkspaceId, repo: &RepoId) {
        self.lock()
            .entry(id.clone())
            .or_default()
            .refs_changed
            .insert(repo.clone());
    }

    /// Takes the note of [`Summaries::refs_changed`] as a read of `repo`
    /// begins, so a change during the read is kept for the next one, and
    /// returns it with the repo's epoch.
    fn begin_read(&self, id: &WorkspaceId, repo: &RepoId) -> (bool, u32) {
        let mut state = self.lock();
        let Some(workspace) = state.get_mut(id) else {
            return (false, 0);
        };
        let refs_changed = workspace.refs_changed.remove(repo);
        let epoch = workspace.entries.get(repo).map_or(0, |entry| entry.epoch);
        (refs_changed, epoch)
    }

    /// A write job waits or runs on `repo` of workspace `id` (GRP-8).
    pub fn busy(&self, id: &WorkspaceId, repo: &RepoId) -> bool {
        self.lock()
            .get(id)
            .and_then(|workspace| workspace.entries.get(repo))
            .is_some_and(|entry| entry.busy > 0)
    }

    /// Notes that a write job waits or runs on `repo` (`busy`), or that one
    /// ended, and returns the repo's new view.
    pub fn set_busy(&self, id: &WorkspaceId, repo: &RepoId, busy: bool) -> SummaryView {
        let mut state = self.lock();
        let entry = state
            .entry(id.clone())
            .or_default()
            .entries
            .entry(repo.clone())
            .or_insert_with(|| Entry::cached(None));
        entry.busy = if busy {
            entry.busy.saturating_add(1)
        } else {
            entry.busy.saturating_sub(1)
        };
        entry.view(id, repo)
    }

    /// Reads `repo` of workspace `id` again at once, at normal priority,
    /// after a job changed it (OPS-7); `refs_changed` says the job changed
    /// its refs, so the history reads it again. A background read that
    /// began before is not kept. Returns the new view, or `None` when the
    /// repo is no longer in the workspace. Reads the repo: call it from a
    /// thread that may block.
    pub fn reread(
        &self,
        workspaces: &Workspaces,
        id: &WorkspaceId,
        repo: &RepoId,
        git: &GitRunner,
        refs_changed: bool,
    ) -> Option<SummaryView> {
        let path = workspaces.repo_path(id, repo).ok()?;
        let (noted, epoch) = {
            let mut state = self.lock();
            let workspace = state.entry(id.clone()).or_default();
            let noted = workspace.refs_changed.remove(repo);
            let entry = workspace
                .entries
                .entry(repo.clone())
                .or_insert_with(|| Entry::cached(None));
            entry.epoch = entry.epoch.wrapping_add(1);
            (noted, entry.epoch)
        };
        let outcome = read(&path, |repo| StatusReader::new(repo, git.clone()));
        Some(self.store(id, repo, &path, outcome, refs_changed || noted, epoch))
    }

    fn lock(&self) -> MutexGuard<'_, HashMap<WorkspaceId, WorkspaceState>> {
        // Every change below is a single assignment, so a panic leaves no
        // half-changed state.
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Queues `repos` that are not waiting yet and takes the cached summary
    /// of those without one. Returns how many workers to start and the new
    /// views of the queued repos, now refreshing.
    fn enqueue(
        &self,
        workspaces: &Workspaces,
        id: &WorkspaceId,
        repos: &[RepoId],
    ) -> (usize, Vec<SummaryView>) {
        let paths = workspaces.repo_paths(id).unwrap_or_default();
        let mut state = self.lock();
        let workspace = state.entry(id.clone()).or_default();
        let mut views = Vec::new();
        for repo in repos {
            let entry = workspace.entries.entry(repo.clone()).or_insert_with(|| {
                let path = paths
                    .iter()
                    .find(|(known, _)| known == repo)
                    .map(|(_, path)| path);
                Entry::cached(path.and_then(|path| self.cache.get(path)))
            });
            entry.refreshing = true;
            views.push(entry.view(id, repo));
            if !workspace.queue.contains(repo) {
                workspace.queue.push_back(repo.clone());
            }
        }
        let start = self
            .limit
            .saturating_sub(workspace.workers)
            .min(workspace.queue.len());
        workspace.workers = workspace.workers.saturating_add(start);
        (start, views)
    }

    fn next(&self, id: &WorkspaceId) -> Option<RepoId> {
        self.lock().get_mut(id).and_then(WorkspaceState::next)
    }

    /// Keeps what reading `repo` gave and returns its new view.
    /// `refs_changed` says the watcher saw its refs change before the read.
    fn store(
        &self,
        id: &WorkspaceId,
        repo: &RepoId,
        path: &Path,
        outcome: Outcome,
        refs_changed: bool,
        epoch: u32,
    ) -> SummaryView {
        let mut state = self.lock();
        let workspace = state.entry(id.clone()).or_default();
        let current = workspace
            .entries
            .get(repo)
            .is_none_or(|entry| entry.epoch == epoch);
        if refs_changed && !(current && matches!(outcome, Outcome::Read(_))) {
            // Counted at the next read that succeeds and is kept.
            workspace.refs_changed.insert(repo.clone());
        }
        let entry = workspace
            .entries
            .entry(repo.clone())
            .or_insert_with(|| Entry::cached(None));
        if !current {
            // A job's re-read began since: what this read saw is older.
            entry.refreshing = false;
            return entry.view(id, repo);
        }
        if let Outcome::Read(summary) = &outcome {
            self.cache.put(path, summary);
        }
        match outcome {
            Outcome::Read(summary) => {
                // A summary read in this run shows `HEAD` moving even when
                // no watcher saw it, as after a focus refresh.
                let head_moved = !entry.stale
                    && entry
                        .summary
                        .as_ref()
                        .is_some_and(|known| known.head != summary.head);
                let refs_version = if refs_changed || head_moved {
                    entry.refs_version.wrapping_add(1)
                } else {
                    entry.refs_version
                };
                *entry = Entry {
                    summary: Some(summary),
                    stale: false,
                    missing: false,
                    error: None,
                    refreshing: false,
                    refs_version,
                    busy: entry.busy,
                    epoch: entry.epoch,
                };
            }
            Outcome::Missing => {
                entry.stale = true;
                entry.missing = true;
                entry.error = None;
            }
            Outcome::Failed(error) => {
                entry.stale = true;
                entry.missing = false;
                entry.error = Some(error);
            }
        }
        entry.refreshing = false;
        entry.view(id, repo)
    }
}

/// Reads the summaries of `repos` of workspace `workspace` again, in their
/// order but the selected repo first. Sends each repo's view to `emit`
/// when it is queued, marked refreshing, and again when it has been read.
/// Repos already waiting keep their place; when reads of this workspace
/// are already running, they take the new repos too and this returns
/// sooner. Fresh summaries go to the cache.
pub async fn refresh_summaries(
    workspaces: Arc<Workspaces>,
    summaries: Arc<Summaries>,
    workspace: WorkspaceId,
    repos: Vec<RepoId>,
    git: GitRunner,
    emit: impl Fn(SummaryView) + Send + Sync + 'static,
) {
    let (start, queued) = {
        let (workspaces, summaries, workspace) =
            (workspaces.clone(), summaries.clone(), workspace.clone());
        // The cache lookups of new repos read the disk (ARCH-7).
        tokio::task::spawn_blocking(move || summaries.enqueue(&workspaces, &workspace, &repos))
            .await
            .unwrap_or_default()
    };
    queued.into_iter().for_each(&emit);
    let emit = Arc::new(emit);
    let workers: Vec<_> = (0..start)
        .map(|_| {
            let (workspaces, summaries, workspace, git, emit) = (
                workspaces.clone(),
                summaries.clone(),
                workspace.clone(),
                git.clone(),
                emit.clone(),
            );
            tokio::spawn(async move {
                let pool = summaries.pool.clone();
                let mut reader = Reader {
                    summaries,
                    workspace,
                    done: false,
                };
                loop {
                    let _worker = pool.worker(Priority::Refresh).await;
                    let Some(repo) = reader.next() else {
                        break;
                    };
                    // A repo removed while it waited is skipped.
                    let Ok(path) = workspaces.repo_path(&reader.workspace, &repo) else {
                        continue;
                    };
                    let (refs_changed, epoch) =
                        reader.summaries.begin_read(&reader.workspace, &repo);
                    let (read_path, git) = (path.clone(), git.clone());
                    let outcome = run_in_background(move || {
                        read(&read_path, |repo| background_reader(repo, &git))
                    })
                    .await
                    .unwrap_or_else(|| Outcome::Failed("the read did not finish".to_owned()));
                    emit(reader.summaries.store(
                        &reader.workspace,
                        &repo,
                        &path,
                        outcome,
                        refs_changed,
                        epoch,
                    ));
                }
            })
        })
        .collect();
    for worker in workers {
        // A reader that panicked gave back its place (see `Reader`); the
        // others go on.
        let _ = worker.await;
    }
}

/// Runs `work` on a thread of its own at the lowest priority the OS gives
/// (PERF-4) and resolves to what it returned, or `None` when it panicked.
/// On Linux the threads `gix` starts for a status inherit the priority. Not
/// tokio's blocking pool: its threads also run the reads the user waits
/// for, and a thread's priority cannot be raised again without privileges.
/// When no thread can be started, `work` runs there at normal priority.
async fn run_in_background<T: Send + 'static>(
    work: impl FnOnce() -> T + Send + 'static,
) -> Option<T> {
    let work = Arc::new(Mutex::new(Some(work)));
    let take = |work: &Mutex<Option<_>>| work.lock().unwrap_or_else(PoisonError::into_inner).take();
    let (done, ended) = tokio::sync::oneshot::channel::<T>();
    let spawned = std::thread::Builder::new()
        .name("gitcroc-refresh".to_owned())
        .spawn({
            let work = work.clone();
            move || {
                // A thread left at normal priority still reads.
                let _ = thread_priority::set_current_thread_priority(
                    thread_priority::ThreadPriority::Min,
                );
                if let Some(work) = take(&work) {
                    let _ = done.send(work());
                }
            }
        });
    if spawned.is_ok() {
        ended.await.ok()
    } else if let Some(work) = take(&work) {
        tokio::task::spawn_blocking(work).await.ok()
    } else {
        None
    }
}

/// Reads the summary of every repo of workspace `workspace` again, the
/// selected repo first, as when the window regains focus (OPS-7). Sends
/// each view to `emit` as [`refresh_summaries`] does.
pub async fn refresh_workspace(
    workspaces: Arc<Workspaces>,
    summaries: Arc<Summaries>,
    workspace: WorkspaceId,
    git: GitRunner,
    emit: impl Fn(SummaryView) + Send + Sync + 'static,
) -> Result<(), WorkspaceError> {
    let repos = workspaces
        .repo_paths(&workspace)?
        .into_iter()
        .map(|(repo, _)| repo)
        .collect();
    refresh_summaries(workspaces, summaries, workspace, repos, git, emit).await;
    Ok(())
}

/// A running read loop's place in its workspace's reader count.
struct Reader {
    summaries: Arc<Summaries>,
    workspace: WorkspaceId,
    /// The place was given back when the queue ran empty.
    done: bool,
}

impl Reader {
    fn next(&mut self) -> Option<RepoId> {
        let next = self.summaries.next(&self.workspace);
        self.done = next.is_none();
        next
    }
}

impl Drop for Reader {
    /// Gives the place back when the loop ended by a panic, so later
    /// refreshes still start their workers.
    fn drop(&mut self) {
        if !self.done
            && let Some(workspace) = self.summaries.lock().get_mut(&self.workspace)
        {
            workspace.workers = workspace.workers.saturating_sub(1);
        }
    }
}

/// How many threads one background status read uses at most. Unlimited,
/// each read starts one per core, and glibc's malloc keeps memory for each
/// of them after the refresh. Four keep a
/// refresh of `target-project` within 1 s, measured pinned to 8 cores; one
/// took 1.9 s (M1-14).
const STATUS_THREADS: usize = 4;

/// The status reader of the background refresh (PERF-4).
fn background_reader(repo: GitRepo, git: &GitRunner) -> StatusReader {
    StatusReader::new(repo, git.clone()).with_thread_limit(STATUS_THREADS)
}

/// Reads the summary of the repo at `path` with the reader `reader` makes.
fn read(path: &Path, reader: impl FnOnce(GitRepo) -> StatusReader) -> Outcome {
    match path.try_exists() {
        Ok(true) => {}
        Ok(false) => return Outcome::Missing,
        Err(error) => return Outcome::Failed(format!("cannot open {}: {error}", path.display())),
    }
    let repo = match GitRepo::open(path) {
        Ok(repo) => repo,
        Err(error) => return Outcome::Failed(error.to_string()),
    };
    match reader(repo).summary() {
        Ok(summary) => Outcome::Read(summary),
        Err(error) => Outcome::Failed(error.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn perf_4_the_selected_repo_jumps_the_queue() {
        let repo = |id: &str| RepoId(id.to_owned());
        let mut state = WorkspaceState {
            queue: [repo("r1"), repo("r2"), repo("r3")].into(),
            selected: Some(repo("r3")),
            workers: 1,
            ..WorkspaceState::default()
        };
        assert_eq!(state.next(), Some(repo("r3")));
        assert_eq!(state.next(), Some(repo("r1")));
        state.selected = Some(repo("r1"));
        assert_eq!(state.next(), Some(repo("r2")));
        assert_eq!(state.workers, 1);
        assert_eq!(state.next(), None);
        assert_eq!(state.workers, 0);
    }

    #[test]
    fn perf_4_background_status_reads_use_few_threads() {
        let fixture = repo_gen::Fixture::new().unwrap();
        let git = fixture.repo("app").unwrap();
        let repo = GitRepo::open(git.path()).unwrap();
        let runner = GitRunner::new(std::path::PathBuf::from("git"));
        assert_eq!(
            background_reader(repo, &runner).thread_limit(),
            Some(STATUS_THREADS)
        );
    }

    /// The niceness of the calling thread, from `/proc`.
    #[cfg(target_os = "linux")]
    fn niceness() -> i32 {
        let stat = std::fs::read_to_string("/proc/thread-self/stat").unwrap();
        // The command name, in parentheses, may hold spaces; niceness is
        // the 19th field, the 17th after it.
        let (_, rest) = stat.rsplit_once(')').unwrap();
        rest.split_whitespace().nth(16).unwrap().parse().unwrap()
    }

    #[cfg(target_os = "linux")]
    #[tokio::test]
    async fn perf_4_background_reads_run_at_the_lowest_priority() {
        let (sender, received) = std::sync::mpsc::channel();
        run_in_background(move || {
            // `gix` starts threads of its own for a status.
            let started = std::thread::spawn(niceness).join().unwrap();
            sender.send((niceness(), started)).unwrap();
        })
        .await;
        assert_eq!(received.recv().unwrap(), (19, 19));
        // The threads that run the reads the user waits for stay at normal
        // priority.
        let normal = std::thread::spawn(niceness).join().unwrap();
        let waited_for = tokio::task::spawn_blocking(niceness).await.unwrap();
        assert_eq!(waited_for, normal);
        assert!(normal < 19);
    }

    #[cfg(not(target_os = "linux"))]
    #[tokio::test]
    async fn perf_4_background_reads_run_at_the_lowest_priority() {
        use thread_priority::get_current_thread_priority;
        let (sender, received) = std::sync::mpsc::channel();
        run_in_background(move || {
            sender.send(get_current_thread_priority().unwrap()).unwrap();
        })
        .await;
        let background = received.recv().unwrap();
        let normal = std::thread::spawn(|| get_current_thread_priority().unwrap())
            .join()
            .unwrap();
        let waited_for = tokio::task::spawn_blocking(|| get_current_thread_priority().unwrap())
            .await
            .unwrap();
        assert_ne!(background, normal);
        assert_eq!(waited_for, normal);
    }
}
