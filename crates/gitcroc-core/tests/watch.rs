//! File watching and refresh (PERF-6, OPS-7).
//!
//! These run on hosted CI runners, whose speed varies, so nothing here
//! depends on timing (see "Timing in CI" in docs/tasks/README.md). Each
//! test waits for a specific outcome; the deadline only ends a test that
//! would otherwise hang. What must not happen (lock files, unselected
//! working trees) and the debounce times are tested without the OS and
//! without waiting in `src/watch.rs`.

// Integration test crate: helpers outside #[test] fns are test code too (QA-4).
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::collections::BTreeSet;
use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use gitcroc_core::{
    CacheDir, ConfigDir, GitRunner, Head, JobId, RepoChange, RepoId, Summaries, SummaryCache,
    SummaryView, WatchedPath, Watchers, WorkspaceId, Workspaces, add_path, refresh_summaries,
    refresh_workspace,
};
use repo_gen::{Fixture, Repo};

/// Ends a test that would otherwise hang when an outcome never comes; not
/// a budget.
const EVENTUALLY: Duration = Duration::from_secs(60);

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

    /// The first change that passes `test`, if one comes within `time`.
    async fn change_within(
        &self,
        time: Duration,
        test: impl Fn(&RepoChange) -> bool + Send + 'static,
    ) -> Option<RepoChange> {
        let changes = self.changes.clone();
        tokio::task::spawn_blocking(move || {
            let changes = changes.lock().unwrap();
            let end = Instant::now() + time;
            while let Ok(change) =
                changes.recv_timeout(end.saturating_duration_since(Instant::now()))
            {
                if test(&change) {
                    return Some(change);
                }
            }
            None
        })
        .await
        .unwrap()
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
    linked.commit_file("topic.txt", "topic\n", "Topic").unwrap();
    watched
        .change_within(EVENTUALLY, move |change| {
            change.repo_id == id && change.refs
        })
        .await
        .expect("the worktree's commit is seen");
}

#[tokio::test]
async fn ops_7_external_commit_refreshes_the_summary() {
    let fixture = Fixture::new().unwrap();
    let app = repo_at(fixture.path().join("app"));
    repo_at(fixture.path().join("lib"));
    let watched = Watched::start(fixture.path()).await;
    let id = watched.repo(app.path());

    // Not selected: its git files are watched all the same.
    let hash = app.commit_file("two.txt", "two\n", "Second").unwrap();
    let expected = hash.clone();
    let view = watched
        .view_within(EVENTUALLY, move |view| {
            head_hash(view).as_deref() == Some(expected.as_str())
        })
        .await
        .expect("a new summary");
    assert_eq!(view.repo_id, id);
    assert!(!view.stale);
    // HEAD moved: the history reads the repo again.
    assert_eq!(view.refs_version, 1);

    // A new tag is a ref change too, though HEAD stays.
    app.tag("v1").unwrap();
    let view = watched
        .view_within(EVENTUALLY, |view| view.refs_version == 2)
        .await
        .expect("the tag raises the refs version");
    assert_eq!(head_hash(&view), Some(hash));
}

#[tokio::test]
async fn ops_7_external_edit_refreshes_the_selected_repo() {
    let fixture = Fixture::new().unwrap();
    let app = repo_at(fixture.path().join("app"));
    repo_at(fixture.path().join("lib"));
    let watched = Watched::start(fixture.path()).await;
    let app_id = watched.repo(app.path());
    watched.select(Some(&app_id)).await;

    // An unselected repo's working tree counts for nothing: see the
    // routing tests in src/watch.rs, which need no waiting.
    app.write("notes.txt", "edited\n").unwrap();
    let view = watched
        .view_within(EVENTUALLY, |view| {
            view.summary.as_ref().is_some_and(|summary| summary.dirty)
        })
        .await
        .expect("a new working copy state");
    assert_eq!(view.repo_id, app_id);
    // Only the working tree changed: the history stays.
    assert_eq!(view.refs_version, 0);
}

#[tokio::test]
async fn perf_6_a_folder_made_in_the_selected_tree_is_watched() {
    let fixture = Fixture::new().unwrap();
    let app = repo_at(fixture.path().join("app"));
    let watched = Watched::start(fixture.path()).await;
    let app_id = watched.repo(app.path());
    watched.select(Some(&app_id)).await;

    // Making the folders is a change, reported once the new folders are
    // watched; empty folders leave the working copy clean.
    std::fs::create_dir_all(app.path().join("new").join("deeper")).unwrap();
    let id = app_id.clone();
    watched
        .view_within(EVENTUALLY, move |view| {
            view.repo_id == id && view.summary.as_ref().is_some_and(|summary| !summary.dirty)
        })
        .await
        .expect("the new folders are a change");

    // Only a watched folder reports this file.
    app.write("new/deeper/file.txt", "x\n").unwrap();
    let view = watched
        .view_within(EVENTUALLY, |view| {
            view.summary.as_ref().is_some_and(|summary| summary.dirty)
        })
        .await
        .expect("the new folder is watched");
    assert_eq!(view.repo_id, app_id);
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

/// The acceptance of M1-11: a commit made in a terminal shows within one
/// second. A time, so it never runs on hosted CI runners; `just timing`
/// runs it on the reference machine.
#[tokio::test]
#[ignore = "timing: run on the reference machine with `just timing`"]
async fn ops_7_external_commit_shows_within_one_second() {
    let fixture = Fixture::new().unwrap();
    let app = repo_at(fixture.path().join("app"));
    let watched = Watched::start(fixture.path()).await;
    let mut slowest = Duration::ZERO;
    for round in 0..5 {
        let hash = app
            .commit_file("round.txt", format!("{round}\n"), "Round")
            .unwrap();
        let committed = Instant::now();
        watched
            .view_within(EVENTUALLY, move |view| {
                head_hash(view).as_deref() == Some(hash.as_str())
            })
            .await
            .expect("a new summary");
        slowest = slowest.max(committed.elapsed());
    }
    eprintln!("slowest of 5 commits: {slowest:?}");
    assert!(slowest < Duration::from_secs(1), "{slowest:?}");
}

#[tokio::test]
async fn ops_7_a_write_jobs_own_changes_are_not_refreshed_again() {
    let fixture = Fixture::new().unwrap();
    let app = repo_at(fixture.path().join("app"));
    let lib = repo_at(fixture.path().join("lib"));
    let watched = Watched::start(fixture.path()).await;
    let (app_id, lib_id) = (watched.repo(app.path()), watched.repo(lib.path()));

    // A write job holds `app`: its changes are dropped, since the job reads
    // the repo again itself. `lib`'s change, made after, marks that the
    // watcher has seen them.
    watched.watchers.hold(&watched.id, &app_id, true);
    app.commit_file("two.txt", "two\n", "Second").unwrap();
    lib.commit_file("two.txt", "two\n", "Second").unwrap();
    let change = watched.change_within(EVENTUALLY, |_| true).await.unwrap();
    assert_eq!(change.repo_id, lib_id);

    // Released, `app` is watched again.
    watched.watchers.hold(&watched.id, &app_id, false);
    app.commit_file("three.txt", "three\n", "Third").unwrap();
    let change = watched.change_within(EVENTUALLY, |_| true).await.unwrap();
    assert_eq!(change.repo_id, app_id);
}
