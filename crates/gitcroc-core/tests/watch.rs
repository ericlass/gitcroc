//! File watching and refresh (PERF-6, OPS-7).

// Integration test crate: helpers outside #[test] fns are test code too (QA-4).
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::collections::BTreeSet;
use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use gitcroc_core::{
    CacheDir, ConfigDir, GitRunner, Head, JobId, MAX_DELAY, RepoChange, RepoId, Summaries,
    SummaryCache, SummaryView, WatchedPath, Watchers, WorkspaceId, Workspaces, add_path,
    refresh_summaries, refresh_workspace,
};
use repo_gen::{Fixture, Repo};

/// Long enough for any event of the setup to have been reported.
const SETTLE: Duration = Duration::from_millis(600);

struct Dirs {
    root: tempfile::TempDir,
}

impl ConfigDir for Dirs {
    fn config_dir(&self) -> Option<PathBuf> {
        Some(self.root.path().join("config"))
    }
}

impl CacheDir for Dirs {
    fn cache_dir(&self) -> Option<PathBuf> {
        Some(self.root.path().join("cache"))
    }
}

/// A workspace with fixture repos, watched as the app watches it: each
/// change is recorded and refreshes the repo's summary.
struct Watched {
    _dirs: Dirs,
    workspaces: Arc<Workspaces>,
    summaries: Arc<Summaries>,
    watchers: Arc<Watchers>,
    id: WorkspaceId,
    changes: Arc<Mutex<Receiver<RepoChange>>>,
    views: Arc<Mutex<Receiver<SummaryView>>>,
}

impl Watched {
    /// Adds every repo below `path` and starts watching them.
    async fn start(path: &Path) -> Self {
        Self::start_with(path, None).await
    }

    async fn start_with(path: &Path, limit: Option<usize>) -> Self {
        let dirs = Dirs {
            root: tempfile::tempdir().unwrap(),
        };
        let workspaces = Arc::new(Workspaces::load(&dirs));
        let mut summaries = Summaries::load(SummaryCache::open(&dirs), &workspaces);
        if let Some(limit) = limit {
            summaries = summaries.with_limit(limit);
        }
        let summaries = Arc::new(summaries);
        let id = workspaces.views()[0].id.clone();
        add_path(
            workspaces.clone(),
            id.clone(),
            path.to_owned(),
            git(),
            JobId(1),
            |_| {},
        )
        .await
        .unwrap();

        let (change_sender, changes) = mpsc::channel();
        let (view_sender, views) = mpsc::channel::<SummaryView>();
        let runtime = tokio::runtime::Handle::current();
        let on_change = {
            let (workspaces, summaries) = (workspaces.clone(), summaries.clone());
            let change_sender = Mutex::new(change_sender);
            let view_sender = Mutex::new(view_sender);
            move |change: RepoChange| {
                if change.refs {
                    summaries.refs_changed(&change.workspace_id, &change.repo_id);
                }
                let views: Sender<SummaryView> = view_sender.lock().unwrap().clone();
                runtime.spawn(refresh_summaries(
                    workspaces.clone(),
                    summaries.clone(),
                    change.workspace_id.clone(),
                    vec![change.repo_id.clone()],
                    git(),
                    move |view| {
                        let _ = views.send(view);
                    },
                ));
                let _ = change_sender.lock().unwrap().send(change);
            }
        };
        let watchers = Arc::new(Watchers::new(on_change, |warning| {
            panic!("unexpected warning: {warning:?}")
        }));
        let watched = Self {
            _dirs: dirs,
            workspaces,
            summaries,
            watchers,
            id,
            changes: Arc::new(Mutex::new(changes)),
            views: Arc::new(Mutex::new(views)),
        };
        watched.sync().await;
        watched
    }

    async fn sync(&self) {
        let (watchers, workspaces, id) = (
            self.watchers.clone(),
            self.workspaces.clone(),
            self.id.clone(),
        );
        tokio::task::spawn_blocking(move || watchers.sync(&workspaces, &id))
            .await
            .unwrap();
    }

    async fn select(&self, repo: Option<&RepoId>) {
        self.summaries.select(&self.id, repo.cloned());
        self.watchers.select(&self.id, repo.cloned());
        let (watchers, id) = (self.watchers.clone(), self.id.clone());
        tokio::task::spawn_blocking(move || watchers.follow_selection(&id))
            .await
            .unwrap();
    }

    /// The repo whose folder is `path`.
    fn repo(&self, path: &Path) -> RepoId {
        let path = dunce::canonicalize(path).unwrap();
        self.workspaces.views()[0]
            .repos
            .iter()
            .find(|repo| dunce::canonicalize(&repo.path).unwrap() == path)
            .unwrap()
            .id
            .clone()
    }

    /// The changes reported within `time`.
    async fn changes_within(&self, time: Duration) -> Vec<RepoChange> {
        let changes = self.changes.clone();
        tokio::task::spawn_blocking(move || {
            let changes = changes.lock().unwrap();
            let end = Instant::now() + time;
            let mut found = Vec::new();
            while let Ok(change) =
                changes.recv_timeout(end.saturating_duration_since(Instant::now()))
            {
                found.push(change);
            }
            found
        })
        .await
        .unwrap()
    }

    /// Waits until the setup's own events have been reported and drops them
    /// and the views they gave.
    async fn settle(&self) {
        self.changes_within(SETTLE).await;
        while self.views.lock().unwrap().try_recv().is_ok() {}
    }

    /// The first read view that passes `test`, if one comes within `time`.
    async fn view_within(
        &self,
        time: Duration,
        test: impl Fn(&SummaryView) -> bool + Send + 'static,
    ) -> Option<SummaryView> {
        let views = self.views.clone();
        tokio::task::spawn_blocking(move || {
            let views = views.lock().unwrap();
            let end = Instant::now() + time;
            while let Ok(view) = views.recv_timeout(end.saturating_duration_since(Instant::now())) {
                if !view.refreshing && test(&view) {
                    return Some(view);
                }
            }
            None
        })
        .await
        .unwrap()
    }
}

fn git() -> GitRunner {
    GitRunner::new(PathBuf::from("git"))
}

fn repo_at(path: impl Into<PathBuf>) -> Repo {
    let repo = Repo::init(path).unwrap();
    repo.commit_file("README.md", "hello\n", "First").unwrap();
    repo
}

fn canonical(path: &Path) -> PathBuf {
    dunce::canonicalize(path).unwrap()
}

/// What the watcher watches for a plain repo's git files.
fn git_paths(repo: &Repo) -> Vec<WatchedPath> {
    let git_dir = canonical(&repo.path().join(".git"));
    vec![
        WatchedPath {
            path: git_dir.join("refs"),
            recursive: true,
        },
        WatchedPath {
            path: git_dir,
            recursive: false,
        },
    ]
}

fn tree(repo: &Repo) -> WatchedPath {
    WatchedPath {
        path: canonical(repo.path()),
        recursive: true,
    }
}

fn watched_set(paths: Vec<WatchedPath>) -> BTreeSet<WatchedPath> {
    paths.into_iter().collect()
}

fn head_hash(view: &SummaryView) -> Option<String> {
    match &view.summary.as_ref()?.head {
        Head::Branch { hash, .. } => hash.clone(),
        Head::Detached { hash } => Some(hash.clone()),
    }
}

#[tokio::test]
async fn perf_6_watches_git_files_of_every_repo_and_tree_of_selected_only() {
    let fixture = Fixture::new().unwrap();
    let repos = ["app", "lib", "web"].map(|name| repo_at(fixture.path().join(name)));
    let watched = Watched::start(fixture.path()).await;
    let git: BTreeSet<WatchedPath> = repos.iter().flat_map(git_paths).collect();

    // Nothing selected: no working tree.
    assert_eq!(watched_set(watched.watchers.watched(&watched.id)), git);

    let app = watched.repo(repos[0].path());
    watched.select(Some(&app)).await;
    let mut expected = git.clone();
    expected.insert(tree(&repos[0]));
    assert_eq!(watched_set(watched.watchers.watched(&watched.id)), expected);

    // The working tree watch moves with the selection.
    let web = watched.repo(repos[2].path());
    watched.select(Some(&web)).await;
    let mut expected = git.clone();
    expected.insert(tree(&repos[2]));
    assert_eq!(watched_set(watched.watchers.watched(&watched.id)), expected);

    watched.select(None).await;
    assert_eq!(watched_set(watched.watchers.watched(&watched.id)), git);

    // Stopped, as a background tab will be (UI-30).
    watched.watchers.stop(&watched.id);
    assert!(watched.watchers.watched(&watched.id).is_empty());
}

#[tokio::test]
async fn perf_6_a_linked_worktree_is_watched_in_its_real_git_directory() {
    let fixture = Fixture::new().unwrap();
    let main = repo_at(fixture.path().join("main"));
    let linked_path = fixture.path().join("linked");
    main.git([
        OsStr::new("worktree"),
        OsStr::new("add"),
        OsStr::new("--quiet"),
        OsStr::new("-b"),
        OsStr::new("topic"),
        linked_path.as_os_str(),
    ])
    .unwrap();
    let linked = Repo::open(&linked_path);
    let watched = Watched::start(&linked_path).await;
    let common = canonical(&main.path().join(".git"));
    assert_eq!(
        watched_set(watched.watchers.watched(&watched.id)),
        watched_set(vec![
            WatchedPath {
                path: common.join("worktrees").join("linked"),
                recursive: false,
            },
            WatchedPath {
                path: common.clone(),
                recursive: false,
            },
            WatchedPath {
                path: common.join("refs"),
                recursive: true,
            },
        ])
    );

    // A commit in the worktree moves its branch, kept in the main repo.
    let id = watched.repo(&linked_path);
    watched.settle().await;
    linked.commit_file("topic.txt", "topic\n", "Topic").unwrap();
    let changes = watched.changes_within(SETTLE).await;
    assert!(
        changes
            .iter()
            .any(|change| change.repo_id == id && change.refs),
        "{changes:?}"
    );
}

#[tokio::test]
async fn ops_7_external_commit_refreshes_the_summary() {
    let fixture = Fixture::new().unwrap();
    let app = repo_at(fixture.path().join("app"));
    repo_at(fixture.path().join("lib"));
    let watched = Watched::start(fixture.path()).await;
    let id = watched.repo(app.path());
    watched.settle().await;

    // Not selected: its git files are watched all the same.
    let hash = app.commit_file("two.txt", "two\n", "Second").unwrap();
    let committed = Instant::now();
    let expected = hash.clone();
    let view = watched
        .view_within(Duration::from_secs(1), move |view| {
            head_hash(view).as_deref() == Some(expected.as_str())
        })
        .await
        .expect("a new summary within one second");
    assert!(committed.elapsed() < Duration::from_secs(1));
    assert_eq!(view.repo_id, id);
    assert!(!view.stale);
    // HEAD moved: the history reads the repo again.
    assert_eq!(view.refs_version, 1);

    // A new tag is a ref change too, though HEAD stays.
    watched.settle().await;
    app.tag("v1").unwrap();
    let view = watched
        .view_within(Duration::from_secs(1), |view| view.refs_version == 2)
        .await
        .expect("the tag raises the refs version");
    assert_eq!(head_hash(&view), Some(hash));
}

#[tokio::test]
async fn ops_7_external_edit_refreshes_the_selected_repo() {
    let fixture = Fixture::new().unwrap();
    let app = repo_at(fixture.path().join("app"));
    let lib = repo_at(fixture.path().join("lib"));
    let watched = Watched::start(fixture.path()).await;
    let (app_id, lib_id) = (watched.repo(app.path()), watched.repo(lib.path()));
    watched.select(Some(&app_id)).await;
    watched.settle().await;

    app.write("notes.txt", "edited\n").unwrap();
    let view = watched
        .view_within(Duration::from_secs(1), |view| {
            view.summary.as_ref().is_some_and(|summary| summary.dirty)
        })
        .await
        .expect("a new working copy state within one second");
    assert_eq!(view.repo_id, app_id);
    // Only the working tree changed: the history stays.
    assert_eq!(view.refs_version, 0);
    watched.settle().await;

    // The unselected repo's working tree is not watched.
    lib.write("notes.txt", "edited\n").unwrap();
    let changes = watched.changes_within(SETTLE).await;
    assert!(
        changes.iter().all(|change| change.repo_id != lib_id),
        "{changes:?}"
    );

    // A folder made after the watch started is watched too.
    std::fs::create_dir_all(app.path().join("new").join("deeper")).unwrap();
    watched.settle().await;
    app.write("new/deeper/file.txt", "x\n").unwrap();
    // Windows may report the write's last steps after the debounce, as a
    // second change; what matters is that the new folder is watched.
    let changes = watched.changes_within(SETTLE).await;
    assert!(!changes.is_empty(), "the new folder is not watched");
    let expected = RepoChange {
        workspace_id: watched.id.clone(),
        repo_id: app_id,
        refs: false,
    };
    assert!(
        changes.iter().all(|change| *change == expected),
        "{changes:?}"
    );
}

#[tokio::test]
async fn ops_7_focus_refreshes_all_repos() {
    let fixture = Fixture::new().unwrap();
    let repos = ["app", "lib", "web"].map(|name| repo_at(fixture.path().join(name)));
    let watched = Watched::start_with(fixture.path(), Some(1)).await;
    let ids: Vec<RepoId> = repos.iter().map(|repo| watched.repo(repo.path())).collect();
    watched.summaries.select(&watched.id, Some(ids[2].clone()));

    let sent = Arc::new(Mutex::new(Vec::new()));
    let sink = sent.clone();
    refresh_workspace(
        watched.workspaces.clone(),
        watched.summaries.clone(),
        watched.id.clone(),
        git(),
        move |view| sink.lock().unwrap().push(view),
    )
    .await
    .unwrap();
    let read: Vec<RepoId> = sent
        .lock()
        .unwrap()
        .iter()
        .filter(|view| !view.refreshing)
        .map(|view| view.repo_id.clone())
        .collect();
    assert_eq!(read, vec![ids[2].clone(), ids[0].clone(), ids[1].clone()]);
}

#[tokio::test]
async fn perf_6_lock_files_alone_trigger_nothing() {
    let fixture = Fixture::new().unwrap();
    let app = repo_at(fixture.path().join("app"));
    let watched = Watched::start(fixture.path()).await;
    let id = watched.repo(app.path());
    watched.select(Some(&id)).await;
    watched.settle().await;

    let git_dir = app.path().join(".git");
    for lock in [
        "index.lock",
        "HEAD.lock",
        "packed-refs.lock",
        "refs/heads/main.lock",
    ] {
        std::fs::write(git_dir.join(lock), b"").unwrap();
        std::fs::remove_file(git_dir.join(lock)).unwrap();
    }
    let changes = watched.changes_within(SETTLE).await;
    assert_eq!(changes, Vec::new());

    // In the working tree a lock file is an ordinary file.
    app.write("Cargo.lock", "# locked\n").unwrap();
    let changes = watched.changes_within(SETTLE).await;
    assert_eq!(changes.len(), 1, "{changes:?}");
}

#[tokio::test]
async fn perf_6_a_burst_of_changes_gives_one_refresh() {
    let fixture = Fixture::new().unwrap();
    let app = repo_at(fixture.path().join("app"));
    let watched = Watched::start(fixture.path()).await;
    let id = watched.repo(app.path());
    watched.select(Some(&id)).await;
    watched.settle().await;

    // 100 writes back to back: sleeping between them overshoots on CI
    // runners (a 1 ms sleep took 9 ms on macOS).
    let start = Instant::now();
    for index in 0..100 {
        app.write(&format!("burst/{}.txt", index % 10), format!("{index}\n"))
            .unwrap();
    }
    // A burst longer than MAX_DELAY is meant to give a second refresh.
    assert!(
        start.elapsed() < MAX_DELAY,
        "the burst took {:?}",
        start.elapsed()
    );
    let changes = watched.changes_within(Duration::from_millis(1500)).await;
    assert_eq!(changes.len(), 1, "{changes:?}");
}
