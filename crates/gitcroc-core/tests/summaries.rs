//! The summary cache and the start sequence (PERF-3, PERF-4, DATA-3).

// Integration test crate: helpers outside #[test] fns are test code too (QA-4).
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use gitcroc_core::{
    CACHE_FILE, CACHE_SCHEMA, CacheDir, ChangedFile, ConfigDir, DiffOptions, DiffSource, GitRepo,
    GitRunner, Head, HistoryStart, JobId, MAX_REFRESHES, RepoId, StatusReader, Summaries,
    SummaryCache, SummaryView, WorkspaceId, Workspaces, add_path, changed_files, file_diff,
    history_page, refresh_summaries, repo_summary, working_copy,
};
use repo_gen::{Fixture, Repo};

/// A config dir and a cache dir inside one temporary dir, kept across
/// "runs" of the app.
struct Dirs {
    root: tempfile::TempDir,
}

impl Dirs {
    fn new() -> Self {
        Self {
            root: tempfile::tempdir().unwrap(),
        }
    }

    fn cache_file(&self) -> PathBuf {
        self.cache_dir().unwrap().join(CACHE_FILE)
    }
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

/// One start of the app: the workspace and the cached summaries loaded.
struct Run {
    workspaces: Arc<Workspaces>,
    summaries: Arc<Summaries>,
    id: WorkspaceId,
}

impl Run {
    fn start(dirs: &Dirs) -> Self {
        Self::start_with(dirs, None)
    }

    fn start_with(dirs: &Dirs, limit: Option<usize>) -> Self {
        let workspaces = Arc::new(Workspaces::load(dirs));
        let mut summaries = Summaries::load(SummaryCache::open(dirs), &workspaces);
        if let Some(limit) = limit {
            summaries = summaries.with_limit(limit);
        }
        let id = workspaces.views()[0].id.clone();
        Self {
            workspaces,
            summaries: Arc::new(summaries),
            id,
        }
    }

    async fn add(&self, path: &Path) -> Vec<RepoId> {
        let report = add_path(
            self.workspaces.clone(),
            self.id.clone(),
            path.to_owned(),
            git(),
            JobId(1),
            |_| {},
        )
        .await
        .unwrap();
        report.added.into_iter().map(|repo| repo.id).collect()
    }

    fn views(&self) -> Vec<SummaryView> {
        self.summaries.views(&self.workspaces, &self.id).unwrap()
    }

    fn repos(&self) -> Vec<RepoId> {
        self.workspaces.views()[0]
            .repos
            .iter()
            .map(|repo| repo.id.clone())
            .collect()
    }

    /// Refreshes every repo and returns the views in the order they were
    /// sent.
    async fn refresh(&self) -> Vec<SummaryView> {
        let sent = Arc::new(Mutex::new(Vec::new()));
        let sink = sent.clone();
        refresh_summaries(
            self.workspaces.clone(),
            self.summaries.clone(),
            self.id.clone(),
            self.repos(),
            git(),
            move |view| sink.lock().unwrap().push(view),
        )
        .await;
        sent.lock().unwrap().clone()
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

fn head_hash(view: &SummaryView) -> Option<String> {
    match &view.summary.as_ref()?.head {
        Head::Branch { hash, .. } => hash.clone(),
        Head::Detached { hash } => Some(hash.clone()),
    }
}

#[tokio::test]
async fn perf_3_cached_summaries_are_shown_stale_then_refreshed() {
    let fixture = Fixture::new().unwrap();
    let app = repo_at(fixture.path().join("app"));
    let lib = repo_at(fixture.path().join("lib"));
    let dirs = Dirs::new();

    // The previous run: add the repos and read them once.
    let first = Run::start(&dirs);
    first.add(fixture.path()).await;
    let old_app = app.head().unwrap();
    let old_lib = lib.head().unwrap();
    let sent = first.refresh().await;
    assert_eq!(sent.len(), 2);
    assert!(
        sent.iter()
            .all(|view| !view.stale && view.summary.is_some())
    );
    drop(first);

    // Between runs, both repos change.
    let new_app = app.commit_file("a.txt", "a\n", "Second").unwrap();
    lib.write("dirty.txt", "x\n").unwrap();

    let second = Run::start(&dirs);
    let cached = second.views();
    assert_eq!(cached.len(), 2);
    assert!(
        cached
            .iter()
            .all(|view| view.stale && view.workspace_id == second.id)
    );
    assert_eq!(head_hash(&cached[0]), Some(old_app));
    assert_eq!(head_hash(&cached[1]), Some(old_lib.clone()));
    assert!(!cached[1].summary.as_ref().unwrap().dirty);

    let sent = second.refresh().await;
    assert_eq!(sent.len(), 2);
    let fresh = second.views();
    assert!(fresh.iter().all(|view| !view.stale && view.error.is_none()));
    assert_eq!(head_hash(&fresh[0]), Some(new_app.clone()));
    assert_eq!(head_hash(&fresh[1]), Some(old_lib));
    assert!(fresh[1].summary.as_ref().unwrap().dirty);
    // What was sent is what the store now holds.
    let mut sent = sent;
    sent.sort_by(|a, b| a.repo_id.cmp(&b.repo_id));
    assert_eq!(sent, fresh);

    // The fresh values were cached for the next run.
    let third = Run::start(&dirs);
    assert_eq!(head_hash(&third.views()[0]), Some(new_app));
    assert!(third.views()[1].summary.as_ref().unwrap().dirty);
}

#[tokio::test]
async fn perf_3_cache_is_disposable() {
    let fixture = Fixture::new().unwrap();
    let app = repo_at(fixture.path().join("app"));
    let dirs = Dirs::new();
    let first = Run::start(&dirs);
    first.add(app.path()).await;
    first.refresh().await;
    drop(first);
    assert!(dirs.cache_file().is_file());

    type Breakage = (&'static str, fn(&Path));
    let breakages: [Breakage; 3] = [
        ("deleted", |path| fs::remove_file(path).unwrap()),
        ("corrupt", |path| {
            fs::write(path, b"this is not a SQLite database at all").unwrap();
        }),
        ("wrong version", |path| {
            let connection = rusqlite::Connection::open(path).unwrap();
            connection
                .pragma_update(None, "user_version", CACHE_SCHEMA + 1)
                .unwrap();
        }),
    ];
    for (name, break_it) in breakages {
        break_it(&dirs.cache_file());

        let run = Run::start(&dirs);
        let views = run.views();
        assert_eq!(views.len(), 1, "{name}");
        assert!(views[0].stale && views[0].summary.is_none(), "{name}");
        run.refresh().await;
        assert!(!run.views()[0].stale, "{name}");
        drop(run);

        // The recreated cache works for the next run.
        let next = Run::start(&dirs);
        assert_eq!(
            head_hash(&next.views()[0]),
            Some(app.head().unwrap()),
            "{name}"
        );
    }
}

#[tokio::test]
async fn perf_3_without_a_cache_dir_the_app_still_starts() {
    struct NoCache(Dirs);
    impl CacheDir for NoCache {
        fn cache_dir(&self) -> Option<PathBuf> {
            None
        }
    }
    let fixture = Fixture::new().unwrap();
    let app = repo_at(fixture.path().join("app"));
    let dirs = NoCache(Dirs::new());
    let workspaces = Arc::new(Workspaces::load(&dirs.0));
    let summaries = Arc::new(Summaries::load(SummaryCache::open(&dirs), &workspaces));
    let run = Run {
        id: workspaces.views()[0].id.clone(),
        workspaces,
        summaries,
    };
    run.add(app.path()).await;
    run.refresh().await;
    assert_eq!(head_hash(&run.views()[0]), Some(app.head().unwrap()));
}

#[tokio::test]
async fn perf_3_missing_repo_keeps_its_cached_summary() {
    let fixture = Fixture::new().unwrap();
    let app = repo_at(fixture.path().join("app"));
    let hash = app.head().unwrap();
    let dirs = Dirs::new();
    let first = Run::start(&dirs);
    first.add(app.path()).await;
    first.refresh().await;
    drop(first);

    fs::rename(app.path(), fixture.path().join("moved")).unwrap();
    let run = Run::start(&dirs);
    let sent = run.refresh().await;
    assert_eq!(sent.len(), 1);
    let view = &run.views()[0];
    assert!(view.missing && view.stale);
    assert_eq!(view.error, None);
    assert_eq!(head_hash(view), Some(hash));
}

#[tokio::test]
async fn perf_3_unreadable_repo_keeps_its_cached_summary_and_says_why() {
    let fixture = Fixture::new().unwrap();
    let app = repo_at(fixture.path().join("app"));
    let hash = app.head().unwrap();
    let dirs = Dirs::new();
    let first = Run::start(&dirs);
    first.add(app.path()).await;
    first.refresh().await;
    drop(first);

    // The folder is still there, but it is no longer a repo.
    fs::remove_dir_all(app.path().join(".git")).unwrap();
    let run = Run::start(&dirs);
    run.refresh().await;
    let view = &run.views()[0];
    assert!(view.stale && !view.missing);
    assert!(view.error.is_some());
    assert_eq!(head_hash(view), Some(hash));
}

#[tokio::test]
async fn perf_4_selected_repo_is_refreshed_first() {
    let fixture = Fixture::new().unwrap();
    for name in ["a", "b", "c", "d"] {
        repo_at(fixture.path().join(name));
    }
    let dirs = Dirs::new();
    // One read at a time, so the order sent is the order read.
    let run = Run::start_with(&dirs, Some(1));
    run.add(fixture.path()).await;
    let repos = run.repos();
    assert_eq!(repos.len(), 4);

    run.summaries.select(&run.id, Some(repos[2].clone()));
    let order: Vec<RepoId> = run.refresh().await.into_iter().map(|v| v.repo_id).collect();
    assert_eq!(
        order,
        [&repos[2], &repos[0], &repos[1], &repos[3]].map(Clone::clone)
    );

    // Without a selection, workspace order.
    run.summaries.select(&run.id, None);
    let order: Vec<RepoId> = run.refresh().await.into_iter().map(|v| v.repo_id).collect();
    assert_eq!(order, repos);
}

#[test]
fn perf_4_reads_at_once_follow_the_cores_up_to_8() {
    let dirs = Dirs::new();
    let workspaces = Workspaces::load(&dirs);
    let summaries = Summaries::load(SummaryCache::none(), &workspaces);
    let cores = std::thread::available_parallelism().unwrap().get();
    assert_eq!(summaries.limit(), cores.min(MAX_REFRESHES));
    assert_eq!(MAX_REFRESHES, 8);
}

#[tokio::test]
async fn arch_9_summaries_name_their_workspace_and_repo() {
    let fixture = Fixture::new().unwrap();
    let app = repo_at(fixture.path().join("app"));
    let dirs = Dirs::new();
    let run = Run::start(&dirs);
    let added = run.add(app.path()).await;
    let sent = run.refresh().await;
    assert_eq!(sent[0].workspace_id, run.id);
    assert_eq!(sent[0].repo_id, added[0]);
    assert!(
        run.summaries
            .views(&run.workspaces, &WorkspaceId("other".to_owned()))
            .is_err()
    );
}

/// Every folder and file under `path`, with the files' contents.
fn snapshot(path: &Path) -> BTreeMap<PathBuf, Option<Vec<u8>>> {
    let mut entries = BTreeMap::new();
    let mut pending = vec![path.to_owned()];
    while let Some(folder) = pending.pop() {
        for entry in fs::read_dir(&folder).unwrap() {
            let entry = entry.unwrap();
            if entry.file_type().unwrap().is_dir() {
                entries.insert(entry.path(), None);
                pending.push(entry.path());
            } else {
                entries.insert(entry.path(), Some(fs::read(entry.path()).unwrap()));
            }
        }
    }
    entries
}

#[tokio::test]
async fn data_3_reads_create_no_files_in_the_repo() {
    let fixture = Fixture::new().unwrap();
    let app = repo_at(fixture.path().join("app"));
    app.commit_file("src/lib.rs", "fn a() {}\n", "Second")
        .unwrap();
    app.write("src/lib.rs", "fn b() {}\n").unwrap();
    app.write("new.txt", "new\n").unwrap();
    let before = snapshot(app.path());

    let dirs = Dirs::new();
    let run = Run::start(&dirs);
    run.add(app.path()).await;
    run.refresh().await;
    let repo = GitRepo::open(app.path()).unwrap();
    let reader = StatusReader::new(repo.clone(), git());
    repo_summary(reader.clone()).await.unwrap();
    let files = working_copy(reader).await.unwrap();
    assert_eq!(files.unstaged.len(), 2);
    let page = history_page(repo.clone(), HistoryStart::All, None, None)
        .await
        .unwrap();
    let hash = page.rows[0].hash.clone();
    let changed: Vec<ChangedFile> = changed_files(repo.clone(), hash.clone(), 0).await.unwrap();
    file_diff(
        repo.clone(),
        DiffSource::Commit { hash, parent: 0 },
        changed[0].clone(),
        DiffOptions::default(),
    )
    .await
    .unwrap();
    file_diff(
        repo,
        DiffSource::Unstaged,
        files.unstaged[0].clone(),
        DiffOptions::default(),
    )
    .await
    .unwrap();

    // No CLI fallback applies to this repo, so git wrote nothing either.
    assert_eq!(snapshot(app.path()), before);
}
