//! The app's API (M6-05): `App` on the real core, in a config, cache and
//! log directory of its own, against `repo-gen` fixtures. Every test waits
//! for events, never for a time.

use std::path::{Path, PathBuf};

use gitcroc_app::app::{App, AppEvent, AppSetup, EVENT_CAPACITY};
use gitcroc_app::platform::DesktopFonts;
use gitcroc_core::{
    CacheDir, ConfigDir, GitStatus, HistoryTarget, LogDir, Outcome, RepoId, WorkspaceId,
};
use repo_gen::RepoSpec;
use tokio::sync::broadcast::Receiver;
use tokio::sync::broadcast::error::RecvError;

struct Dirs(PathBuf);

impl ConfigDir for Dirs {
    fn config_dir(&self) -> Option<PathBuf> {
        Some(self.0.join("config"))
    }
}

impl CacheDir for Dirs {
    fn cache_dir(&self) -> Option<PathBuf> {
        Some(self.0.join("cache"))
    }
}

impl LogDir for Dirs {
    fn log_dir(&self) -> Option<PathBuf> {
        Some(self.0.join("logs"))
    }
}

/// A started app, its first subscriber, and the directory it keeps its
/// files in.
struct Started {
    app: App,
    events: Receiver<AppEvent>,
    _dir: tempfile::TempDir,
}

/// Starts an app with `capacity` events per subscriber, and waits until git
/// has been detected.
async fn start(capacity: usize) -> Started {
    let dir = tempfile::tempdir().unwrap();
    let dirs = Dirs(dir.path().to_owned());
    let app = App::start(AppSetup {
        config: &dirs,
        cache: &dirs,
        logs: &dirs,
        runtime: tokio::runtime::Handle::current(),
        fonts: DesktopFonts::default(),
        event_capacity: capacity,
    });
    let mut events = app.subscribe();
    // The result is kept before it is sent, so while the state still says
    // `Checking`, its event is still to come to this subscriber.
    while matches!(app.git_status().await, GitStatus::Checking) {
        if let Err(RecvError::Closed) = events.recv().await {
            panic!("the app stopped");
        }
    }
    let status = app.git_status().await;
    assert!(matches!(status, GitStatus::Usable { .. }), "{status:?}");
    Started {
        app,
        events,
        _dir: dir,
    }
}

/// The next event `wanted` accepts.
async fn next(events: &mut Receiver<AppEvent>, wanted: impl Fn(&AppEvent) -> bool) -> AppEvent {
    loop {
        let event = events.recv().await.unwrap();
        if wanted(&event) {
            return event;
        }
    }
}

/// Adds the repos below `path` to `workspace`, and returns their ids once
/// the add job has reported.
async fn add(started: &mut Started, workspace: &WorkspaceId, path: &Path) -> Vec<RepoId> {
    let job = started
        .app
        .workspace_add_path(workspace.clone(), path.to_owned())
        .await
        .unwrap();
    let AppEvent::WorkspaceAddReport(report) = next(
        &mut started.events,
        |event| matches!(event, AppEvent::WorkspaceAddReport(report) if report.job_id == job),
    )
    .await
    else {
        unreachable!()
    };
    report.added.into_iter().map(|repo| repo.id).collect()
}

fn active(app: &App) -> WorkspaceId {
    app.workspace_get().active.unwrap()
}

/// ARCH-7: adding repos is a job. The call returns its id, and its progress
/// and end arrive as events, then the workspace's new state and the report.
#[tokio::test(flavor = "multi_thread")]
async fn arch_7_slow_app_calls_return_jobs() {
    let fixture = repo_gen::Fixture::new().unwrap();
    fixture
        .repo("api")
        .unwrap()
        .commit_file("README.md", "hello\n", "First")
        .unwrap();
    let mut started = start(EVENT_CAPACITY).await;
    let workspace = active(&started.app);

    let job = started
        .app
        .workspace_add_path(workspace.clone(), fixture.path().to_owned())
        .await
        .unwrap();

    let mut seen = Vec::new();
    loop {
        match started.events.recv().await.unwrap() {
            AppEvent::JobProgress(view) if view.job_id == job => seen.push("progress"),
            AppEvent::JobEnded(view) if view.job_id == job => {
                assert_eq!(view.outcome, Some(Outcome::Succeeded), "{view:?}");
                seen.push("ended");
            }
            AppEvent::WorkspaceChanged(view) if view.id == workspace => seen.push("workspace"),
            AppEvent::WorkspaceAddReport(report) if report.job_id == job => {
                assert_eq!(report.added.len(), 1, "{report:?}");
                assert!(report.refused.is_empty(), "{report:?}");
                seen.push("report");
                break;
            }
            _ => {}
        }
    }
    assert_eq!(seen.first(), Some(&"progress"), "{seen:?}");
    assert_eq!(&seen[seen.len() - 3..], ["workspace", "ended", "report"]);
    assert_eq!(
        started.app.workspace_get().open[0].repos.len(),
        1,
        "the state shows the repo too"
    );
}

/// ARCH-6: events arrive in the order they were sent. A subscriber that
/// reads nothing falls behind, is told so, and reads the state again.
#[tokio::test(flavor = "multi_thread")]
async fn arch_6_events_keep_their_order_and_report_lag() {
    const CAPACITY: usize = 8;
    let mut started = start(CAPACITY).await;
    let mut idle = started.app.subscribe();

    for n in 0..20 {
        let name = format!("tab {n}");
        let tabs = started.app.workspace_create(name.clone()).await.unwrap();
        assert_eq!(
            tabs.open.last().map(|view| view.name.as_str()),
            Some(name.as_str())
        );
        // This subscriber keeps up, and sees the tabs in the order they were
        // made.
        let AppEvent::WorkspacesChanged(sent) = next(&mut started.events, |event| {
            matches!(event, AppEvent::WorkspacesChanged(_))
        })
        .await
        else {
            unreachable!()
        };
        assert_eq!(
            sent.open.last().map(|view| view.name.as_str()),
            Some(name.as_str())
        );
    }

    let Err(RecvError::Lagged(missed)) = idle.recv().await else {
        panic!("a subscriber 20 tabs behind is not told it lagged");
    };
    assert!(missed > 0);
    // It reads the state again, and has every tab.
    let tabs = started.app.workspace_get();
    assert_eq!(tabs.open.len(), 21);
    assert_eq!(tabs.active, Some(tabs.open[20].id.clone()));
}

/// ARCH-9: the same calls on two workspaces keep their own state: a history
/// walk in one goes on while the other starts its own.
#[tokio::test(flavor = "multi_thread")]
async fn arch_9_app_calls_name_their_workspace() {
    let dir = tempfile::tempdir().unwrap();
    let spec = RepoSpec {
        name: "long".into(),
        files: 4,
        lines: 10,
        commits: 600,
        branches: 1,
    };
    repo_gen::generate_repo(&spec, 7, &dir.path().join("long")).unwrap();
    let mut started = start(EVENT_CAPACITY).await;
    let first = active(&started.app);
    let tabs = started
        .app
        .workspace_create("second".to_owned())
        .await
        .unwrap();
    let second = tabs.active.unwrap();
    assert_ne!(first, second);
    let in_first = add(&mut started, &first, dir.path()).await;
    let in_second = add(&mut started, &second, dir.path()).await;

    let app = &started.app;
    let walk = app
        .history_get(
            first.clone(),
            Some(in_first[0].clone()),
            HistoryTarget::Repo,
            String::new(),
        )
        .await
        .unwrap()
        .unwrap();
    assert!(!walk.done);
    let other = app
        .history_get(
            second.clone(),
            Some(in_second[0].clone()),
            HistoryTarget::Repo,
            String::new(),
        )
        .await
        .unwrap()
        .unwrap();
    // The second workspace's walk did not end the first's.
    let more = app
        .history_more(first.clone(), walk.handle)
        .await
        .unwrap()
        .expect("the first workspace's walk goes on");
    assert_eq!(walk.rows.len() + more.rows.len(), 600);
    // Nor did reading on in the first end the second's.
    let more = app
        .history_more(second, other.handle)
        .await
        .unwrap()
        .expect("the second workspace's walk goes on");
    assert_eq!(other.rows.len() + more.rows.len(), 600);
}
