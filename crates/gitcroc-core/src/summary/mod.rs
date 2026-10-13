//! Repo summaries (UI-3) and the start sequence (PERF-3): at start every
//! repo shows its cached summary, marked stale, at once; a background
//! refresh then reads each repo again, the selected repo first, and keeps
//! the fresh summary in the cache.
//!
//! The refresh is the minimal scheduler M1 needs: at most as many reads at
//! once as the machine has cores, capped at [`MAX_REFRESHES`]. M2's job
//! engine replaces it with the shared job pool and the full PERF-4 order.
//! All state is kept per workspace (ARCH-9).

mod cache;

use std::collections::{HashMap, HashSet, VecDeque};
use std::num::NonZero;
use std::path::Path;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use gitcroc_git::{GitRepo, GitRunner, RepoSummary, StatusRead, StatusReader};
use serde::{Deserialize, Serialize};

use crate::{RepoId, WorkspaceError, WorkspaceId, Workspaces};

pub use cache::{CACHE_FILE, CACHE_SCHEMA, CacheDir, PlatformCacheDir, SummaryCache};

/// At most this many summaries are read at once, fewer on a machine with
/// fewer cores.
pub const MAX_REFRESHES: usize = 8;

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
        let limit = std::thread::available_parallelism()
            .map(NonZero::get)
            .unwrap_or(1)
            .min(MAX_REFRESHES);
        Self {
            cache,
            limit,
            state: Mutex::new(state),
        }
    }

    /// Runs at most `limit` reads at once per workspace instead of one per
    /// core.
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
    /// begins, so a change during the read is kept for the next one.
    fn take_refs_changed(&self, id: &WorkspaceId, repo: &RepoId) -> bool {
        self.lock()
            .get_mut(id)
            .is_some_and(|workspace| workspace.refs_changed.remove(repo))
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
    ) -> SummaryView {
        if let Outcome::Read(summary) = &outcome {
            self.cache.put(path, summary);
        }
        let mut state = self.lock();
        let workspace = state.entry(id.clone()).or_default();
        if refs_changed && !matches!(outcome, Outcome::Read(_)) {
            // Counted at the next read that succeeds.
            workspace.refs_changed.insert(repo.clone());
        }
        let entry = workspace
            .entries
            .entry(repo.clone())
            .or_insert_with(|| Entry::cached(None));
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
            tokio::task::spawn_blocking(move || {
                let mut worker = Worker {
                    summaries,
                    workspace,
                    done: false,
                };
                while let Some(repo) = worker.next() {
                    // A repo removed while it waited is skipped.
                    let Ok(path) = workspaces.repo_path(&worker.workspace, &repo) else {
                        continue;
                    };
                    let refs_changed = worker.summaries.take_refs_changed(&worker.workspace, &repo);
                    let outcome = read(&path, &git);
                    emit(worker.summaries.store(
                        &worker.workspace,
                        &repo,
                        &path,
                        outcome,
                        refs_changed,
                    ));
                }
            })
        })
        .collect();
    for worker in workers {
        // A worker that panicked gave back its place (see `Worker`); the
        // others go on.
        let _ = worker.await;
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

/// A running read loop's place in its workspace's worker count.
struct Worker {
    summaries: Arc<Summaries>,
    workspace: WorkspaceId,
    /// The place was given back when the queue ran empty.
    done: bool,
}

impl Worker {
    fn next(&mut self) -> Option<RepoId> {
        let next = self.summaries.next(&self.workspace);
        self.done = next.is_none();
        next
    }
}

impl Drop for Worker {
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

fn read(path: &Path, git: &GitRunner) -> Outcome {
    match path.try_exists() {
        Ok(true) => {}
        Ok(false) => return Outcome::Missing,
        Err(error) => return Outcome::Failed(format!("cannot open {}: {error}", path.display())),
    }
    let repo = match GitRepo::open(path) {
        Ok(repo) => repo,
        Err(error) => return Outcome::Failed(error.to_string()),
    };
    match StatusReader::new(repo, git.clone()).summary() {
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
}
