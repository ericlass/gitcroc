//! File watching (PERF-6, OPS-7): a repo refreshes when it changes on disk.
//!
//! Each workspace has its own watcher (ARCH-9), so a background tab's can be
//! stopped (UI-30). It watches, for every repo, the git directory's own
//! files (`HEAD`, the index, the conflict files of OPS-1) and the refs
//! (`refs/` and `packed-refs`, in the common directory for a linked
//! worktree); and for the selected repo only, its working tree, without the
//! git directory. Events are debounced per repo: a repo's change is reported
//! [`DEBOUNCE`] after its last event, or [`MAX_DELAY`] after its first when
//! events keep coming. Lock files in the git directory are not changes.
//!
//! The OS watcher hands its events to a thread per workspace, which sorts
//! them by repo and debounces them; it never takes a lock the OS watcher
//! may wait for, so adding a watch cannot deadlock with an event.

use std::collections::{BTreeMap, HashMap};
use std::ffi::OsStr;
use std::path::{Component, Path, PathBuf};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError, Weak};
use std::time::{Duration, Instant};

use gitcroc_git::GitRepo;
use notify::{Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use serde::{Deserialize, Serialize};

use crate::{RepoId, WorkspaceId, Workspaces};

/// A repo's change is reported this long after its last event.
pub const DEBOUNCE: Duration = Duration::from_millis(200);

/// ...or this long after its first, when events keep coming, so a long
/// build in the working tree still refreshes the repo every second.
pub const MAX_DELAY: Duration = Duration::from_secs(1);

/// What changed in a repo, once its events settled.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RepoChange {
    pub workspace_id: WorkspaceId,
    pub repo_id: RepoId,
    /// `HEAD` or a ref changed, so the history may show other commits.
    pub refs: bool,
}

/// A path the watcher watches.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct WatchedPath {
    pub path: PathBuf,
    /// Its subfolders too.
    pub recursive: bool,
}

/// Watching a workspace failed, at least in part. Changes made outside the
/// app then show when the window regains focus (OPS-7).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
pub struct WatchWarning {
    pub workspace_id: WorkspaceId,
    pub message: String,
}

type ChangeFn = dyn Fn(RepoChange) + Send + Sync;
type WarningFn = dyn Fn(WatchWarning) + Send + Sync;

/// The watchers of every watched workspace.
pub struct Watchers {
    on_change: Arc<ChangeFn>,
    on_warning: Arc<WarningFn>,
    workspaces: Mutex<HashMap<WorkspaceId, WorkspaceWatch>>,
}

impl std::fmt::Debug for Watchers {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Watchers").finish_non_exhaustive()
    }
}

/// A repo's folders, canonical, as events name them.
#[derive(Debug, Clone)]
struct RepoDirs {
    id: RepoId,
    /// The path in the workspace, to notice a repo that moved.
    path: PathBuf,
    git_dir: PathBuf,
    common_dir: PathBuf,
    /// `None` for a bare repo.
    work_dir: Option<PathBuf>,
}

impl RepoDirs {
    fn open(id: RepoId, path: PathBuf) -> Option<Self> {
        let repo = GitRepo::open(&path).ok()?;
        let canonical = |dir: &Path| dunce::canonicalize(dir).unwrap_or_else(|_| dir.to_owned());
        Some(Self {
            id,
            git_dir: canonical(repo.git_dir()),
            common_dir: canonical(repo.common_dir()),
            work_dir: repo.work_dir().map(canonical),
            path,
        })
    }

    /// What the watcher watches for the repo's git files.
    fn git_paths(&self) -> Vec<WatchedPath> {
        let mut paths = vec![WatchedPath {
            path: self.git_dir.clone(),
            recursive: false,
        }];
        if self.common_dir != self.git_dir {
            paths.push(WatchedPath {
                path: self.common_dir.clone(),
                recursive: false,
            });
        }
        paths.push(WatchedPath {
            path: self.common_dir.join("refs"),
            recursive: true,
        });
        paths
    }
}

/// What events mean: the repos and which one is selected.
#[derive(Debug, Default)]
struct Routes {
    repos: Vec<RepoDirs>,
    selected: Option<RepoId>,
}

/// What one workspace's watching shares with its event thread.
struct Shared {
    id: WorkspaceId,
    routes: Mutex<Routes>,
    /// The warning given, at most one per workspace.
    warning: Mutex<Option<WatchWarning>>,
    on_change: Arc<ChangeFn>,
    on_warning: Arc<WarningFn>,
}

impl Shared {
    fn routes(&self) -> MutexGuard<'_, Routes> {
        // Each change is a single assignment; a panic leaves no half state.
        self.routes.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Gives the first warning; later ones are only the same problem again.
    fn warn(&self, message: String) {
        let warning = {
            let mut given = self.warning.lock().unwrap_or_else(PoisonError::into_inner);
            if given.is_some() {
                return;
            }
            let warning = WatchWarning {
                workspace_id: self.id.clone(),
                message,
            };
            *given = Some(warning.clone());
            warning
        };
        (self.on_warning)(warning);
    }
}

/// The working tree being watched.
#[derive(Debug)]
struct Tree {
    repo: RepoId,
    root: PathBuf,
    /// The folders watched for it: the root alone where the OS watches
    /// recursively in one go, every folder but the git directory elsewhere.
    dirs: Vec<PathBuf>,
}

/// What came of adding a watch.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Added {
    Yes,
    /// The path does not exist (any more).
    Missing,
    Failed,
}

/// The OS watcher and what it watches.
struct Watch {
    /// `None` when it could not be created; nothing is watched then.
    os: Option<RecommendedWatcher>,
    /// How many uses each watched path has: a repo and its linked worktree
    /// share their refs.
    uses: HashMap<PathBuf, usize>,
    git: BTreeMap<RepoId, Vec<WatchedPath>>,
    tree: Option<Tree>,
}

impl Watch {
    /// Watches `path` for one more use. A path that does not exist is
    /// skipped quietly: the repo is missing, which its summary shows.
    fn add(&mut self, shared: &Shared, path: &Path, recursive: bool) -> Added {
        if let Some(uses) = self.uses.get_mut(path) {
            *uses += 1;
            return Added::Yes;
        }
        let Some(os) = self.os.as_mut() else {
            return Added::Failed;
        };
        let mode = if recursive {
            RecursiveMode::Recursive
        } else {
            RecursiveMode::NonRecursive
        };
        match os.watch(path, mode) {
            Ok(()) => {
                self.uses.insert(path.to_owned(), 1);
                Added::Yes
            }
            Err(error) if not_found(&error) => Added::Missing,
            Err(error) => {
                shared.warn(format!("cannot watch {}: {error}", path.display()));
                Added::Failed
            }
        }
    }

    /// Drops one use of `path`, and the watch with the last.
    fn remove(&mut self, path: &Path) {
        let Some(uses) = self.uses.get_mut(path) else {
            return;
        };
        *uses -= 1;
        if *uses == 0 {
            self.uses.remove(path);
            if let Some(os) = self.os.as_mut() {
                // A folder deleted since is no longer watched anyway.
                let _ = os.unwatch(path);
            }
        }
    }

    /// Watches the repos' git files, dropping those of repos gone.
    fn set_repos(&mut self, shared: &Shared, repos: &[RepoDirs]) {
        let wanted: BTreeMap<RepoId, Vec<WatchedPath>> = repos
            .iter()
            .map(|repo| (repo.id.clone(), repo.git_paths()))
            .collect();
        let old = std::mem::take(&mut self.git);
        for (repo, paths) in &old {
            if wanted.get(repo) != Some(paths) {
                for path in paths {
                    self.remove(&path.path);
                }
            }
        }
        for (repo, paths) in wanted {
            if old.get(&repo) != Some(&paths) {
                for path in &paths {
                    self.add(shared, &path.path, path.recursive);
                }
            }
            self.git.insert(repo, paths);
        }
    }

    /// Moves the working tree watch to the selected repo's.
    fn follow_selection(&mut self, shared: &Shared) {
        let wanted = {
            let routes = shared.routes();
            routes
                .selected
                .as_ref()
                .and_then(|selected| routes.repos.iter().find(|repo| &repo.id == selected))
                .and_then(|repo| Some((repo.id.clone(), repo.work_dir.clone()?)))
        };
        let current = self.tree.as_ref().map(|tree| (&tree.repo, &tree.root));
        if current == wanted.as_ref().map(|(repo, root)| (repo, root)) {
            return;
        }
        if let Some(tree) = self.tree.take() {
            for dir in &tree.dirs {
                self.remove(dir);
            }
        }
        if let Some((repo, root)) = wanted {
            let mut tree = Tree {
                repo,
                root: root.clone(),
                dirs: Vec::new(),
            };
            self.add_tree(shared, &mut tree, &root);
            self.tree = Some(tree);
        }
    }

    /// Watches the working tree from `dir` down, without git directories.
    fn add_tree(&mut self, shared: &Shared, tree: &mut Tree, dir: &Path) {
        if RECURSIVE_TREE {
            if self.add(shared, dir, true) == Added::Yes {
                tree.dirs.push(dir.to_owned());
            }
            return;
        }
        for folder in folders(dir) {
            match self.add(shared, &folder, false) {
                Added::Yes => tree.dirs.push(folder),
                Added::Missing => {}
                // Out of watches, most likely: the rest would fail too.
                Added::Failed => return,
            }
        }
    }

    /// Watches folders created in the working tree since it was watched;
    /// where the OS watches recursively it already does.
    fn add_new_folders(&mut self, shared: &Shared, created: &[PathBuf]) {
        let Some(mut tree) = self.tree.take() else {
            return;
        };
        for dir in created {
            if dir.starts_with(&tree.root) && !self.uses.contains_key(dir) {
                self.add_tree(shared, &mut tree, dir);
            }
        }
        self.tree = Some(tree);
    }
}

/// The OS watches a folder and everything below it as one watch (FSEvents,
/// `ReadDirectoryChangesW`). Elsewhere (inotify, kqueue) each folder is a
/// watch, and gitcroc adds them itself so the git directory takes none.
const RECURSIVE_TREE: bool = cfg!(any(target_os = "macos", target_os = "windows"));

/// `dir` and the folders below it, without git directories and without
/// following links.
fn folders(dir: &Path) -> Vec<PathBuf> {
    let mut found = Vec::new();
    let mut stack = vec![dir.to_owned()];
    while let Some(dir) = stack.pop() {
        if let Ok(entries) = std::fs::read_dir(&dir) {
            for entry in entries.flatten() {
                if entry.file_name() != ".git" && entry.file_type().is_ok_and(|kind| kind.is_dir())
                {
                    stack.push(entry.path());
                }
            }
        }
        found.push(dir);
    }
    found
}

fn not_found(error: &notify::Error) -> bool {
    match &error.kind {
        notify::ErrorKind::PathNotFound => true,
        notify::ErrorKind::Io(error) => error.kind() == std::io::ErrorKind::NotFound,
        _ => false,
    }
}

/// One watched workspace.
struct WorkspaceWatch {
    shared: Arc<Shared>,
    watch: Arc<Mutex<Watch>>,
}

impl WorkspaceWatch {
    fn start(id: WorkspaceId, on_change: Arc<ChangeFn>, on_warning: Arc<WarningFn>) -> Self {
        let shared = Arc::new(Shared {
            id,
            routes: Mutex::new(Routes::default()),
            warning: Mutex::new(None),
            on_change,
            on_warning,
        });
        let (sender, receiver) = mpsc::channel();
        let os = notify::recommended_watcher(move |event| {
            // Only fails once the event thread ended, when watching stops.
            let _ = sender.send(event);
        });
        let os = match os {
            Ok(os) => Some(os),
            Err(error) => {
                shared.warn(format!("cannot start watching files: {error}"));
                None
            }
        };
        let watch = Arc::new(Mutex::new(Watch {
            os,
            uses: HashMap::new(),
            git: BTreeMap::new(),
            tree: None,
        }));
        let (thread_shared, thread_watch) = (shared.clone(), Arc::downgrade(&watch));
        let spawned = std::thread::Builder::new()
            .name("gitcroc watch".to_owned())
            .spawn(move || debounce(&thread_shared, &thread_watch, &receiver));
        if let Err(error) = spawned {
            shared.warn(format!("cannot start watching files: {error}"));
        }
        Self { shared, watch }
    }

    fn lock(&self) -> MutexGuard<'_, Watch> {
        lock_watch(&self.watch)
    }
}

fn lock_watch(watch: &Mutex<Watch>) -> MutexGuard<'_, Watch> {
    // A panic while watches change leaves at worst a watch too many.
    watch.lock().unwrap_or_else(PoisonError::into_inner)
}

impl Watchers {
    /// Watchers that report each repo change to `on_change` and the first
    /// failure of each workspace to `on_warning`. Both are called on the
    /// watchers' own threads.
    pub fn new(
        on_change: impl Fn(RepoChange) + Send + Sync + 'static,
        on_warning: impl Fn(WatchWarning) + Send + Sync + 'static,
    ) -> Self {
        Self {
            on_change: Arc::new(on_change),
            on_warning: Arc::new(on_warning),
            workspaces: Mutex::new(HashMap::new()),
        }
    }

    fn lock(&self) -> MutexGuard<'_, HashMap<WorkspaceId, WorkspaceWatch>> {
        // Entries are only inserted and removed whole.
        self.workspaces
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
    }

    /// The watch of workspace `id`, started if it was not.
    fn workspace(&self, id: &WorkspaceId) -> (Arc<Shared>, Arc<Mutex<Watch>>) {
        let mut workspaces = self.lock();
        let watch = workspaces.entry(id.clone()).or_insert_with(|| {
            WorkspaceWatch::start(id.clone(), self.on_change.clone(), self.on_warning.clone())
        });
        (watch.shared.clone(), watch.watch.clone())
    }

    /// Watches the repos of workspace `id` as they are now, starting to
    /// watch the workspace if it was not: new repos are watched, removed
    /// ones no longer. A repo that cannot be opened is not watched until a
    /// later call finds it. Opens repos and adds watches: call it off the
    /// async workers (ARCH-7).
    pub fn sync(&self, workspaces: &Workspaces, id: &WorkspaceId) {
        let Ok(paths) = workspaces.repo_paths(id) else {
            return;
        };
        let (shared, watch) = self.workspace(id);
        let mut watch = lock_watch(&watch);
        let known = std::mem::take(&mut shared.routes().repos);
        let repos: Vec<RepoDirs> = paths
            .into_iter()
            .filter_map(|(repo, path)| {
                match known
                    .iter()
                    .find(|dirs| dirs.id == repo && dirs.path == path)
                {
                    Some(dirs) => Some(dirs.clone()),
                    None => RepoDirs::open(repo, path),
                }
            })
            .collect();
        watch.set_repos(&shared, &repos);
        shared.routes().repos = repos;
        watch.follow_selection(&shared);
    }

    /// Makes `repo` the selected repo of workspace `id`, whose working tree
    /// alone is watched; `None` watches no working tree. Only records it, at
    /// once: [`Watchers::follow_selection`] moves the watch. Selections are
    /// kept in the order of these calls, however the moves run.
    pub fn select(&self, id: &WorkspaceId, repo: Option<RepoId>) {
        let (shared, _) = self.workspace(id);
        shared.routes().selected = repo;
    }

    /// Moves the working tree watch of workspace `id` to its selected repo.
    /// Adds a watch per folder of a large tree: call it off the async
    /// workers (ARCH-7).
    pub fn follow_selection(&self, id: &WorkspaceId) {
        let (shared, watch) = self.workspace(id);
        lock_watch(&watch).follow_selection(&shared);
    }

    /// Stops watching workspace `id`, as for a background tab (UI-30).
    pub fn stop(&self, id: &WorkspaceId) {
        let stopped = self.lock().remove(id);
        drop(stopped);
    }

    /// What is watched for workspace `id`: the git paths of each repo and
    /// the selected repo's working tree, which is watched recursively
    /// without its git directory. Empty when it is not watched.
    pub fn watched(&self, id: &WorkspaceId) -> Vec<WatchedPath> {
        let Some(watch) = self.lock().get(id).map(|watch| watch.watch.clone()) else {
            return Vec::new();
        };
        let watch = lock_watch(&watch);
        let mut paths: Vec<WatchedPath> = watch.git.values().flatten().cloned().collect();
        if let Some(tree) = &watch.tree {
            paths.push(WatchedPath {
                path: tree.root.clone(),
                recursive: true,
            });
        }
        paths.sort();
        paths.dedup();
        paths
    }

    /// The warning workspace `id` gave, if any.
    pub fn warning(&self, id: &WorkspaceId) -> Option<WatchWarning> {
        let shared = self.lock().get(id)?.shared.clone();
        shared
            .warning
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }
}

impl Drop for WorkspaceWatch {
    fn drop(&mut self) {
        // The OS watcher goes first; its handler then drops the sender,
        // which ends the event thread.
        self.lock().os = None;
    }
}

/// A repo's events not reported yet.
#[derive(Debug, Clone, Copy)]
struct Pending {
    first: Instant,
    last: Instant,
    refs: bool,
}

impl Pending {
    fn due(&self) -> Instant {
        (self.last + DEBOUNCE).min(self.first + MAX_DELAY)
    }
}

/// The per-repo debounce, on a clock its caller passes in, so tests drive
/// it without waiting.
#[derive(Debug, Default)]
struct Debouncer {
    pending: HashMap<RepoId, Pending>,
}

impl Debouncer {
    /// Notes an event of `repo` at `now`.
    fn add(&mut self, repo: RepoId, refs: bool, now: Instant) {
        let entry = self.pending.entry(repo).or_insert(Pending {
            first: now,
            last: now,
            refs: false,
        });
        entry.last = now;
        entry.refs |= refs;
    }

    /// When the next repo is due, if any is pending.
    fn next_due(&self) -> Option<Instant> {
        self.pending.values().map(Pending::due).min()
    }

    /// The repos due at `now`, each with whether its refs changed, in repo
    /// order; they are no longer pending.
    fn take_due(&mut self, now: Instant) -> Vec<(RepoId, bool)> {
        let mut due: Vec<(RepoId, bool)> = self
            .pending
            .iter()
            .filter(|(_, pending)| pending.due() <= now)
            .map(|(repo, pending)| (repo.clone(), pending.refs))
            .collect();
        due.sort();
        for (repo, _) in &due {
            self.pending.remove(repo);
        }
        due
    }
}

/// The event thread: sorts events by repo and reports each repo's change
/// once its events settled. Ends when the OS watcher is dropped.
fn debounce(shared: &Shared, watch: &Weak<Mutex<Watch>>, events: &Receiver<notify::Result<Event>>) {
    let mut debouncer = Debouncer::default();
    loop {
        let received = match debouncer.next_due() {
            Some(due) => events.recv_timeout(due.saturating_duration_since(Instant::now())),
            None => events.recv().map_err(|_| RecvTimeoutError::Disconnected),
        };
        match received {
            Ok(Ok(event)) => {
                let now = Instant::now();
                let (changed, created) = route(&shared.routes(), &event);
                for (repo, refs) in changed {
                    debouncer.add(repo, refs, now);
                }
                if !created.is_empty()
                    && let Some(watch) = watch.upgrade()
                {
                    lock_watch(&watch).add_new_folders(shared, &created);
                }
            }
            Ok(Err(error)) => shared.warn(format!("file watching failed: {error}")),
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => return,
        }
        for (repo, refs) in debouncer.take_due(Instant::now()) {
            (shared.on_change)(RepoChange {
                workspace_id: shared.id.clone(),
                repo_id: repo,
                refs,
            });
        }
    }
}

/// The repos `event` changes, each with whether its refs changed, and the
/// folders it created in the watched working tree.
fn route(routes: &Routes, event: &Event) -> (Vec<(RepoId, bool)>, Vec<PathBuf>) {
    let mut changed: Vec<(RepoId, bool)> = Vec::new();
    let mut created = Vec::new();
    if matches!(event.kind, EventKind::Access(_)) {
        // Reading, the app's own reads included, changes nothing.
        return (changed, created);
    }
    let mut mark = |repo: &RepoId, refs: bool| match changed.iter_mut().find(|(id, _)| id == repo) {
        Some((_, known)) => *known |= refs,
        None => changed.push((repo.clone(), refs)),
    };
    if event.need_rescan() {
        // Events were lost: anything may have changed.
        for repo in &routes.repos {
            mark(&repo.id, true);
        }
        return (changed, created);
    }
    let creates = matches!(
        event.kind,
        EventKind::Create(_) | EventKind::Modify(notify::event::ModifyKind::Name(_))
    );
    for path in &event.paths {
        for repo in &routes.repos {
            if let Ok(inside) = path.strip_prefix(&repo.git_dir) {
                if let Some(refs) = git_file(inside) {
                    mark(&repo.id, refs);
                }
            } else if let Ok(inside) = path.strip_prefix(&repo.common_dir) {
                if shared_ref(inside) {
                    mark(&repo.id, true);
                }
            } else if routes.selected.as_ref() == Some(&repo.id)
                && let Some(work_dir) = &repo.work_dir
                && let Ok(inside) = path.strip_prefix(work_dir)
                && !inside
                    .components()
                    .any(|part| part == Component::Normal(OsStr::new(".git")))
            {
                mark(&repo.id, false);
                if creates && !RECURSIVE_TREE && path.is_dir() {
                    created.push(path.clone());
                }
            }
        }
    }
    (changed, created)
}

/// Whether a change to `inside`, a path in a git directory, is one, and
/// whether it is one of the refs or `HEAD`. Lock files are not changes:
/// git renames them over the file they lock when it is done.
fn git_file(inside: &Path) -> Option<bool> {
    if inside.extension() == Some(OsStr::new("lock")) {
        return None;
    }
    let mut parts = inside.components();
    let Some(Component::Normal(first)) = parts.next() else {
        return None;
    };
    let alone = parts.next().is_none();
    match first.to_str()? {
        "HEAD" | "packed-refs" if alone => Some(true),
        "refs" => Some(true),
        "index" | "MERGE_HEAD" | "CHERRY_PICK_HEAD" | "REVERT_HEAD" if alone => Some(false),
        "rebase-merge" | "rebase-apply" => Some(false),
        _ => None,
    }
}

/// Whether `inside`, a path in a linked worktree's common directory, is
/// one of the refs it shares.
fn shared_ref(inside: &Path) -> bool {
    git_file(inside).is_some_and(|refs| refs) && !inside.starts_with("HEAD")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn perf_6_git_files_that_count() {
        assert_eq!(git_file(Path::new("HEAD")), Some(true));
        assert_eq!(git_file(Path::new("packed-refs")), Some(true));
        assert_eq!(git_file(Path::new("refs/heads/topic/one")), Some(true));
        assert_eq!(git_file(Path::new("index")), Some(false));
        assert_eq!(git_file(Path::new("MERGE_HEAD")), Some(false));
        assert_eq!(git_file(Path::new("rebase-merge/done")), Some(false));
        assert_eq!(git_file(Path::new("index.lock")), None);
        assert_eq!(git_file(Path::new("refs/heads/main.lock")), None);
        assert_eq!(git_file(Path::new("HEAD.lock")), None);
        assert_eq!(git_file(Path::new("ORIG_HEAD")), None);
        assert_eq!(git_file(Path::new("FETCH_HEAD")), None);
        assert_eq!(git_file(Path::new("objects/ab")), None);
        assert_eq!(git_file(Path::new("logs/HEAD")), None);
        assert!(shared_ref(Path::new("refs/tags/v1")));
        assert!(shared_ref(Path::new("packed-refs")));
        assert!(!shared_ref(Path::new("HEAD")));
        assert!(!shared_ref(Path::new("index")));
    }

    fn repo(id: &str) -> RepoId {
        RepoId(id.to_owned())
    }

    /// Two plain repos, `app` selected, and a linked worktree of `app`.
    fn routes() -> Routes {
        let plain = |id: &str, dir: &str| RepoDirs {
            id: repo(id),
            path: PathBuf::from(dir),
            git_dir: Path::new(dir).join(".git"),
            common_dir: Path::new(dir).join(".git"),
            work_dir: Some(PathBuf::from(dir)),
        };
        Routes {
            repos: vec![
                plain("app", "/src/app"),
                plain("lib", "/src/lib"),
                RepoDirs {
                    id: repo("wt"),
                    path: PathBuf::from("/src/wt"),
                    git_dir: PathBuf::from("/src/app/.git/worktrees/wt"),
                    common_dir: PathBuf::from("/src/app/.git"),
                    work_dir: Some(PathBuf::from("/src/wt")),
                },
            ],
            selected: Some(repo("app")),
        }
    }

    fn changed(kind: EventKind, path: &str) -> Vec<(RepoId, bool)> {
        route(&routes(), &Event::new(kind).add_path(PathBuf::from(path))).0
    }

    fn modified(path: &str) -> Vec<(RepoId, bool)> {
        changed(EventKind::Modify(notify::event::ModifyKind::Any), path)
    }

    #[test]
    fn perf_6_lock_files_alone_trigger_nothing() {
        for path in [
            "/src/app/.git/index.lock",
            "/src/app/.git/HEAD.lock",
            "/src/app/.git/packed-refs.lock",
            "/src/app/.git/refs/heads/main.lock",
            "/src/lib/.git/index.lock",
        ] {
            assert_eq!(modified(path), Vec::new(), "{path}");
        }
        // git renames the lock over the file when it is done.
        assert_eq!(modified("/src/app/.git/index"), vec![(repo("app"), false)]);
        // In the working tree a lock file is an ordinary file.
        assert_eq!(modified("/src/app/Cargo.lock"), vec![(repo("app"), false)]);
    }

    #[test]
    fn perf_6_only_the_selected_working_tree_counts() {
        assert_eq!(modified("/src/app/src/main.rs"), vec![(repo("app"), false)]);
        assert_eq!(modified("/src/lib/src/lib.rs"), Vec::new());
        // Its git directory counts as git files only.
        assert_eq!(modified("/src/app/.git/objects/ab/cdef"), Vec::new());
        assert_eq!(modified("/src/app/vendor/dep/.git/HEAD"), Vec::new());
        // Unselected repos still report their git files.
        assert_eq!(modified("/src/lib/.git/HEAD"), vec![(repo("lib"), true)]);
        assert_eq!(modified("/src/lib/.git/index"), vec![(repo("lib"), false)]);
    }

    #[test]
    fn perf_6_a_linked_worktree_reports_its_own_and_its_shared_refs() {
        assert_eq!(
            modified("/src/app/.git/worktrees/wt/HEAD"),
            vec![(repo("wt"), true)]
        );
        assert_eq!(
            modified("/src/app/.git/refs/heads/topic"),
            vec![(repo("app"), true), (repo("wt"), true)]
        );
        // The main repo's HEAD is not the worktree's.
        assert_eq!(modified("/src/app/.git/HEAD"), vec![(repo("app"), true)]);
    }

    #[test]
    fn perf_6_reading_is_not_a_change_and_lost_events_refresh_all() {
        let read = EventKind::Access(notify::event::AccessKind::Open(
            notify::event::AccessMode::Any,
        ));
        assert_eq!(changed(read, "/src/app/.git/index"), Vec::new());
        let lost = Event::new(EventKind::Other).set_flag(notify::event::Flag::Rescan);
        assert_eq!(
            route(&routes(), &lost).0,
            vec![(repo("app"), true), (repo("lib"), true), (repo("wt"), true)]
        );
    }

    #[test]
    fn perf_6_a_burst_of_100_changes_in_100_ms_gives_one_refresh() {
        let start = Instant::now();
        let mut debouncer = Debouncer::default();
        for index in 0..100u32 {
            let now = start + Duration::from_millis(u64::from(index));
            debouncer.add(repo("r1"), index == 50, now);
            assert_eq!(debouncer.take_due(now), Vec::new(), "event {index}");
        }
        // Quiet for 200 ms after the last event.
        let last = start + Duration::from_millis(99);
        assert_eq!(debouncer.next_due(), Some(last + DEBOUNCE));
        assert_eq!(
            debouncer.take_due(last + DEBOUNCE - Duration::from_millis(1)),
            Vec::new()
        );
        assert_eq!(
            debouncer.take_due(last + DEBOUNCE),
            vec![(repo("r1"), true)]
        );
        assert_eq!(debouncer.next_due(), None);
    }

    #[test]
    fn perf_6_changes_that_keep_coming_refresh_every_second() {
        let start = Instant::now();
        let mut debouncer = Debouncer::default();
        let mut fired = Vec::new();
        // An event every 100 ms for 2.5 s, as a build writing its output.
        for tick in 0..=25u32 {
            let now = start + Duration::from_millis(u64::from(tick) * 100);
            debouncer.add(repo("r1"), false, now);
            if !debouncer.take_due(now).is_empty() {
                fired.push(now - start);
            }
        }
        assert_eq!(
            fired,
            vec![MAX_DELAY, MAX_DELAY * 2 + Duration::from_millis(100)]
        );
    }

    #[test]
    fn perf_6_each_repo_is_debounced_on_its_own() {
        let start = Instant::now();
        let mut debouncer = Debouncer::default();
        debouncer.add(repo("r1"), false, start);
        debouncer.add(repo("r2"), true, start + Duration::from_millis(150));
        assert_eq!(
            debouncer.take_due(start + DEBOUNCE),
            vec![(repo("r1"), false)]
        );
        assert_eq!(
            debouncer.take_due(start + Duration::from_millis(350)),
            vec![(repo("r2"), true)]
        );
    }
}
