//! Background tabs (UI-30, UI-31): a workspace whose tab is in the
//! background is not watched or refreshed, a job in it re-reads only the
//! repos it touched, and it lets go of its history walk and changes view.
//! Nothing here waits for a time: what must not happen is checked by
//! asking for it directly, as a stray watcher event or a focus refresh
//! would, and seeing that nothing is read.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use gitcroc_core::{
    CacheDir, ChangesContent, ChangesTarget, ConfigDir, DiffRequest, FileList, FileRef, GitRunner,
    Head, Histories, HistoryTarget, JobEvents, JobId, JobView, Jobs, NothingToDo, OnCancel,
    Outcome, Output, RepoId, Summaries, SummaryCache, SummaryView, TabSwitch, WatchedPath,
    Watchers, WorkspaceId, Workspaces, WriteJob, add_path, changes_get, diff_get, history_get,
    history_more, refresh_summaries, refresh_workspace,
};
use repo_gen::{Fixture, Repo};

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

/// The summaries a sink was sent, in order.
#[derive(Clone, Default)]
struct Sink(Arc<Mutex<Vec<SummaryView>>>);

impl Sink {
    fn emit(&self) -> impl Fn(SummaryView) + Send + Sync + 'static {
        let sent = self.0.clone();
        move |view| sent.lock().unwrap().push(view)
    }

    /// The repos read, in order, leaving out the views sent as a read was
    /// queued.
    fn read(&self) -> Vec<RepoId> {
        self.0
            .lock()
            .unwrap()
            .iter()
            .filter(|view| !view.refreshing)
            .map(|view| view.repo_id.clone())
            .collect()
    }

    fn sent(&self) -> Vec<SummaryView> {
        self.0.lock().unwrap().clone()
    }
}

impl JobEvents for Sink {
    fn progress(&self, _job: &JobView) {}

    fn ended(&self, _job: &JobView) {}

    /// Only reads: the views a job sends as it waits and ends only turn
    /// the row's spinner on and off (UI-3).
    fn summary(&self, view: SummaryView, read: bool) {
        if read {
            self.0.lock().unwrap().push(view);
        }
    }
}

/// Two open workspaces: `A` with repo `a1`, and `B` with repos `b1` and
/// `b2`, each in a folder of its own. `A` is active and watched.
struct Tabs {
    _dirs: Dirs,
    fixture: Fixture,
    switch: TabSwitch,
    a: WorkspaceId,
    b: WorkspaceId,
}

impl Tabs {
    async fn new() -> Self {
        let fixture = Fixture::new().unwrap();
        for name in ["a/a1", "b/b1", "b/b2"] {
            let repo = Repo::init(fixture.path().join(name)).unwrap();
            repo.commit_file("README.md", "hello\n", "First").unwrap();
        }
        let dirs = Dirs {
            root: tempfile::tempdir().unwrap(),
        };
        let workspaces = Arc::new(Workspaces::load(&dirs));
        let a = workspaces.views()[0].id.clone();
        let b = workspaces.create("B").unwrap();
        for (id, folder) in [(&a, "a"), (&b, "b")] {
            add_path(
                workspaces.clone(),
                id.clone(),
                fixture.path().join(folder),
                git(),
                JobId(1),
                |_| {},
            )
            .await
            .unwrap();
        }
        workspaces.activate(&a).unwrap();
        let summaries = Arc::new(Summaries::load(SummaryCache::open(&dirs), &workspaces));
        // Each change is read again, as the app does (OPS-7).
        let on_change = {
            let (workspaces, summaries) = (workspaces.clone(), summaries.clone());
            let runtime = tokio::runtime::Handle::current();
            move |change: gitcroc_core::RepoChange| {
                runtime.spawn(refresh_summaries(
                    workspaces.clone(),
                    summaries.clone(),
                    change.workspace_id,
                    vec![change.repo_id],
                    git(),
                    |_| {},
                ));
            }
        };
        let watchers = Arc::new(Watchers::new(on_change, |warning| {
            panic!("unexpected warning: {warning:?}")
        }));
        let switch = TabSwitch {
            workspaces,
            summaries,
            watchers,
            histories: Arc::new(Histories::new(Duration::from_secs(60))),
            changes: Arc::new(gitcroc_core::Changes::new()),
        };
        switch
            .foreground(a.clone(), Some(git()), |_| {})
            .await
            .unwrap();
        Self {
            _dirs: dirs,
            fixture,
            switch,
            a,
            b,
        }
    }

    /// Makes `to` the active tab as the app does: the tab left goes to the
    /// background, then `to` is watched and read, each view sent to `sink`.
    async fn switch_to(&self, to: &WorkspaceId, sink: &Sink) {
        let from = self.switch.workspaces.active().unwrap();
        self.switch.workspaces.activate(to).unwrap();
        for view in self.switch.background(&from) {
            sink.emit()(view);
        }
        self.switch
            .foreground(to.clone(), Some(git()), sink.emit())
            .await
            .unwrap();
    }

    fn repo(&self, name: &str) -> Repo {
        Repo::open(self.fixture.path().join(name))
    }

    fn repo_id(&self, workspace: &WorkspaceId, name: &str) -> RepoId {
        let view = self.switch.workspaces.view(workspace).unwrap();
        view.repos
            .into_iter()
            .find(|repo| repo.name == name)
            .unwrap()
            .id
    }

    fn summary(&self, workspace: &WorkspaceId, repo: &RepoId) -> SummaryView {
        self.switch
            .summaries
            .views(&self.switch.workspaces, workspace)
            .unwrap()
            .into_iter()
            .find(|view| view.repo_id == *repo)
            .unwrap()
    }

    fn watched(&self, workspace: &WorkspaceId) -> Vec<WatchedPath> {
        self.switch.watchers.watched(workspace)
    }
}

fn git() -> GitRunner {
    GitRunner::new(PathBuf::from("git"))
}

fn head(view: &SummaryView) -> Option<String> {
    match &view.summary.as_ref()?.head {
        Head::Branch { hash, .. } => hash.clone(),
        Head::Detached { hash } => Some(hash.clone()),
    }
}

fn canonical(path: &Path) -> PathBuf {
    dunce::canonicalize(path).unwrap()
}

#[tokio::test]
async fn ui_30_a_background_workspace_is_not_watched_or_refreshed() {
    let tabs = Tabs::new().await;
    let b1 = tabs.repo_id(&tabs.b, "b1");
    let sink = Sink::default();
    tabs.switch_to(&tabs.b, &sink).await;
    assert!(!tabs.watched(&tabs.b).is_empty());
    assert!(tabs.watched(&tabs.a).is_empty());
    let before = head(&tabs.summary(&tabs.b, &b1));

    tabs.switch_to(&tabs.a, &Sink::default()).await;
    assert!(tabs.watched(&tabs.b).is_empty());
    assert!(!tabs.watched(&tabs.a).is_empty());

    // A commit in a background repo: nothing watches it, and a read asked
    // for anyway, as by a late watcher event or a focus refresh, reads
    // nothing.
    let committed = tabs
        .repo("b/b1")
        .commit_file("two.txt", "two\n", "Second")
        .unwrap();
    let stray = Sink::default();
    let switch = &tabs.switch;
    refresh_summaries(
        switch.workspaces.clone(),
        switch.summaries.clone(),
        tabs.b.clone(),
        vec![b1.clone()],
        git(),
        stray.emit(),
    )
    .await;
    refresh_workspace(
        switch.workspaces.clone(),
        switch.summaries.clone(),
        tabs.b.clone(),
        git(),
        stray.emit(),
    )
    .await
    .unwrap();
    assert!(stray.sent().is_empty());
    assert_eq!(head(&tabs.summary(&tabs.b, &b1)), before);

    // Activated, the workspace shows the commit.
    let sink = Sink::default();
    tabs.switch_to(&tabs.b, &sink).await;
    let read = sink
        .sent()
        .into_iter()
        .rfind(|view| view.repo_id == b1 && !view.refreshing)
        .unwrap();
    assert_eq!(head(&read), Some(committed));
}

#[tokio::test]
async fn ui_30_a_job_in_a_background_workspace_rereads_only_its_repos() {
    let tabs = Tabs::new().await;
    let (b1, b2) = (tabs.repo_id(&tabs.b, "b1"), tabs.repo_id(&tabs.b, "b2"));
    let sent = Arc::new(Sink::default());
    let jobs = Jobs::new(
        tabs.switch.workspaces.clone(),
        tabs.switch.summaries.clone(),
        sent.clone(),
    );
    let job = WriteJob {
        workspace_id: tabs.b.clone(),
        repos: vec![b1.clone()],
        label: "commit".to_owned(),
        on_cancel: OnCancel::Finish,
        may_conflict: false,
        nothing_to_do: NothingToDo::Never,
        network: false,
    };
    let committing = tabs.repo("b/b1");
    let other = tabs.repo("b/b2");
    let handle = jobs
        .start(git(), job, move |_run| async move {
            committing
                .git(["commit", "--quiet", "--allow-empty", "-m", "Second"])
                .unwrap();
            // A change the job did not make, in a repo it did not touch.
            other
                .git(["commit", "--quiet", "--allow-empty", "-m", "Elsewhere"])
                .unwrap();
            Ok(Output {
                stdout: Vec::new(),
                stderr: Vec::new(),
            })
        })
        .unwrap();
    let view = tokio::time::timeout(Duration::from_secs(60), handle.ended)
        .await
        .expect("the job never ended")
        .unwrap();
    assert_eq!(view.outcome, Some(Outcome::Succeeded));

    assert_eq!(sent.read(), vec![b1.clone()]);
    let fresh = tabs.summary(&tabs.b, &b1);
    assert!(!fresh.stale);
    assert_eq!(head(&fresh), Some(tabs.repo("b/b1").head().unwrap()));
    // `b2` keeps the summary it had: it is read when `B` is active again.
    assert!(tabs.summary(&tabs.b, &b2).stale);
    assert!(tabs.watched(&tabs.b).is_empty());
}

#[tokio::test]
async fn ui_30_activating_refreshes_the_workspace() {
    let tabs = Tabs::new().await;
    let (b1, b2) = (tabs.repo_id(&tabs.b, "b1"), tabs.repo_id(&tabs.b, "b2"));
    // `b2` was selected in `B` before; the core keeps it in the background.
    tabs.switch.summaries.select(&tabs.b, Some(b2.clone()));

    let sink = Sink::default();
    tabs.switch_to(&tabs.b, &sink).await;
    // Every repo, the selected one first (PERF-4).
    assert_eq!(sink.read(), vec![b2.clone(), b1.clone()]);
    assert!(!tabs.summary(&tabs.b, &b1).refreshing);
    assert!(!tabs.summary(&tabs.b, &b2).stale);

    // Watched again: the git files of each repo and the selected working
    // tree.
    let b2_tree = WatchedPath {
        path: canonical(&tabs.fixture.path().join("b/b2")),
        recursive: true,
    };
    let watched = tabs.watched(&tabs.b);
    assert!(watched.contains(&b2_tree), "{watched:?}");
    for name in ["b/b1", "b/b2"] {
        let git_dir = WatchedPath {
            path: canonical(&tabs.fixture.path().join(name).join(".git")),
            recursive: false,
        };
        assert!(watched.contains(&git_dir), "{watched:?}");
    }
}

#[tokio::test]
async fn ui_31_a_background_workspace_releases_history_and_diffs() {
    let tabs = Tabs::new().await;
    let b1 = tabs.repo_id(&tabs.b, "b1");
    tabs.switch_to(&tabs.b, &Sink::default()).await;
    let switch = &tabs.switch;
    for round in 0..3 {
        tabs.repo("b/b1")
            .commit_file("round.txt", format!("{round}\n"), "Round")
            .unwrap();
    }
    tabs.repo("b/b1").write("edited.txt", "edited\n").unwrap();

    let history = history_get(
        switch.histories.clone(),
        switch.workspaces.clone(),
        tabs.b.clone(),
        Some(b1.clone()),
        HistoryTarget::Repo,
        String::new(),
    )
    .await
    .unwrap()
    .unwrap();
    let changes = changes_get(
        switch.changes.clone(),
        switch.workspaces.clone(),
        git(),
        tabs.b.clone(),
        b1.clone(),
        ChangesTarget::Uncommitted,
    )
    .await
    .unwrap()
    .unwrap();
    let ChangesContent::Uncommitted { unstaged, .. } = &changes.content else {
        panic!("not the working copy: {changes:?}");
    };
    assert_eq!(unstaged.len(), 1);
    let file = FileRef {
        list: FileList::Unstaged,
        id: 0,
    };
    let diff = |handle| {
        diff_get(
            switch.changes.clone(),
            tabs.b.clone(),
            handle,
            file,
            DiffRequest::default(),
        )
    };
    assert!(diff(changes.handle).await.unwrap().is_some());
    assert!(switch.histories.holds_walk(&tabs.b));
    assert!(switch.changes.holds_view(&tabs.b));
    assert!(tabs.summary(&tabs.b, &b1).summary.is_some());

    tabs.switch_to(&tabs.a, &Sink::default()).await;
    assert!(!switch.histories.holds_walk(&tabs.b));
    assert!(!switch.changes.holds_view(&tabs.b));
    // The handles give `None`, as superseded ones do.
    let more = history_more(switch.histories.clone(), tabs.b.clone(), history.handle);
    assert!(more.await.unwrap().is_none());
    assert!(diff(changes.handle).await.unwrap().is_none());
    // The summaries stay. A read in flight at the switch may still land,
    // so the summary is kept, not frozen.
    assert!(tabs.summary(&tabs.b, &b1).summary.is_some());

    // Back, the history and the changes are read again.
    tabs.switch_to(&tabs.b, &Sink::default()).await;
    let again = history_get(
        switch.histories.clone(),
        switch.workspaces.clone(),
        tabs.b.clone(),
        Some(b1),
        HistoryTarget::Repo,
        String::new(),
    )
    .await
    .unwrap()
    .unwrap();
    assert_ne!(again.handle, history.handle);
    assert_eq!(again.rows, history.rows);
}
