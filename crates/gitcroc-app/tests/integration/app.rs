//! The app's API (M6-05): `App` on the real core, in a config, cache and
//! log directory of its own, against `repo-gen` fixtures. Every test waits
//! for events, never for a time.

use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use gitcroc_ai::{ApiKey, Provider, ProviderSpec};
use gitcroc_app::ai::{AiBackend, BoxFuture, DetectedCli, ModelsResult, SelfTest};
use gitcroc_app::app::{App, AppErrorCode, AppEvent, AppSetup, EVENT_CAPACITY};
use gitcroc_app::askpass::PromptKind;
use gitcroc_app::platform::{DesktopFonts, DialogFuture, Dialogs, FileFilter, MemoryKeychain};
use gitcroc_app::update::NoUpdates;
use gitcroc_core::{
    CacheDir, ChangeAction, ChangeRequest, ChangeStart, ChangeTarget, ChangesContent,
    ChangesTarget, CommitMessage, CommitRequest, ConfigDir, FileList, FileRef, GitStatus, GroupOp,
    HistoryTarget, LogDir, Outcome, ProviderEntry, ProviderKind, Prune, RepoId, StartPoint, SyncOp,
    SyncRequest, SyncStart, TargetSet, WorkspaceId,
};
use repo_gen::RepoSpec;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt};
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

/// The process's one command log, which the askpass tests read: git's
/// command log is set once per process, by whichever test starts first.
impl LogDir for Dirs {
    fn log_dir(&self) -> Option<PathBuf> {
        Some(crate::askpass::log_dir().to_owned())
    }
}

/// A dialog the app opened: which one, its title, and the file name and
/// filter it suggested.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Opened {
    kind: &'static str,
    title: String,
    file_name: Option<String>,
    filter: Option<FileFilter>,
}

/// Dialogs that answer from a list, in order, and note what was opened.
/// With no answer left, the user cancels.
#[derive(Default)]
struct FakeDialogs {
    answers: Mutex<VecDeque<Option<PathBuf>>>,
    opened: Mutex<Vec<Opened>>,
}

impl FakeDialogs {
    fn answer(&self, path: Option<PathBuf>) {
        self.answers.lock().unwrap().push_back(path);
    }

    fn opened(&self) -> Vec<Opened> {
        self.opened.lock().unwrap().clone()
    }

    fn open(&self, opened: Opened) -> DialogFuture {
        self.opened.lock().unwrap().push(opened);
        let answer = self.answers.lock().unwrap().pop_front().flatten();
        Box::pin(async move { answer })
    }
}

impl Dialogs for FakeDialogs {
    fn pick_folder(&self, title: &str) -> DialogFuture {
        self.open(Opened {
            kind: "pick_folder",
            title: title.to_owned(),
            file_name: None,
            filter: None,
        })
    }

    fn pick_file(&self, title: &str, filter: Option<&FileFilter>) -> DialogFuture {
        self.open(Opened {
            kind: "pick_file",
            title: title.to_owned(),
            file_name: None,
            filter: filter.cloned(),
        })
    }

    fn save_file(&self, title: &str, file_name: &str, filter: Option<&FileFilter>) -> DialogFuture {
        self.open(Opened {
            kind: "save_file",
            title: title.to_owned(),
            file_name: Some(file_name.to_owned()),
            filter: filter.cloned(),
        })
    }
}

/// A started app, its first subscriber, its dialogs, and the directory it
/// keeps its files in.
/// Reaches no AI provider: detects no CLI and runs none, so no test calls
/// a real one.
struct NoProviders;

impl AiBackend for NoProviders {
    fn detect(&self) -> Vec<DetectedCli> {
        Vec::new()
    }

    fn self_test<'a>(&'a self, _kind: ProviderKind, _path: &'a Path) -> BoxFuture<'a, SelfTest> {
        Box::pin(async {
            SelfTest::Failed {
                reason: "no providers in tests".to_owned(),
            }
        })
    }

    fn list_models<'a>(
        &'a self,
        _entry: &'a ProviderEntry,
        _key: Option<ApiKey>,
    ) -> BoxFuture<'a, ModelsResult> {
        Box::pin(async { ModelsResult::NotOffered })
    }

    fn provider(&self, _spec: ProviderSpec) -> Result<Box<dyn Provider>, String> {
        Err("no providers in tests".to_owned())
    }
}

struct Started {
    app: App,
    events: Receiver<AppEvent>,
    dialogs: Arc<FakeDialogs>,
    _dir: tempfile::TempDir,
}

/// Starts an app with `capacity` events per subscriber, and waits until git
/// has been detected.
async fn start(capacity: usize) -> Started {
    start_with(
        capacity,
        PathBuf::from(env!("CARGO_BIN_EXE_askpass-helper")),
    )
    .await
}

/// As [`start`], with `askpass_helper` as git's helper.
async fn start_with(capacity: usize, askpass_helper: PathBuf) -> Started {
    let dir = tempfile::tempdir().unwrap();
    let dirs = Dirs(dir.path().to_owned());
    let dialogs = Arc::new(FakeDialogs::default());
    let app = App::start(AppSetup {
        config: &dirs,
        cache: &dirs,
        logs: &dirs,
        runtime: tokio::runtime::Handle::current(),
        fonts: DesktopFonts::default(),
        event_capacity: capacity,
        dialogs: dialogs.clone(),
        keychain: Arc::new(MemoryKeychain::default()),
        ai: Arc::new(NoProviders),
        updates: Arc::new(NoUpdates),
        askpass_helper,
        update_test: None,
        commit_graph_threshold: gitcroc_core::COMMIT_GRAPH_THRESHOLD,
        fsmonitor_threshold: gitcroc_core::FSMONITOR_THRESHOLD,
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
        dialogs,
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

/// Waits for job `job` to end, and returns how.
async fn ended(events: &mut Receiver<AppEvent>, job: gitcroc_core::JobId) -> Option<Outcome> {
    let AppEvent::JobEnded(view) = next(
        events,
        |event| matches!(event, AppEvent::JobEnded(view) if view.job_id == job),
    )
    .await
    else {
        unreachable!()
    };
    view.outcome
}

/// The uncommitted changes of `repo`, as a new view.
async fn uncommitted(app: &App, workspace: &WorkspaceId, repo: &RepoId) -> (u32, ChangesContent) {
    let view = app
        .changes_get(workspace.clone(), repo.clone(), ChangesTarget::Uncommitted)
        .await
        .unwrap()
        .expect("no newer view replaced it");
    (view.handle, view.content)
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

/// ARCH-6: after a commit job, the repo's fresh summary arrives as an event
/// before the job's end, and the history shows the commit.
#[tokio::test(flavor = "multi_thread")]
async fn arch_6_app_sends_fresh_state_after_a_job() {
    let fixture = repo_gen::Fixture::new().unwrap();
    let repo = fixture.repo("api").unwrap();
    repo.commit_file("README.md", "hello\n", "First").unwrap();
    repo.write("README.md", "hello again\n").unwrap();
    // The app's git reads the runner's config, which may have no identity;
    // the repo's own config gives it one.
    repo.git(["config", "user.name", "Test"]).unwrap();
    repo.git(["config", "user.email", "test@example.com"])
        .unwrap();
    let mut started = start(EVENT_CAPACITY).await;
    let workspace = active(&started.app);
    let repo_id = add(&mut started, &workspace, fixture.path())
        .await
        .remove(0);
    let app = started.app.clone();

    // Stage the change.
    let (handle, content) = uncommitted(&app, &workspace, &repo_id).await;
    let ChangesContent::Uncommitted { unstaged, .. } = content else {
        panic!("{content:?}");
    };
    assert_eq!(unstaged.len(), 1, "{unstaged:?}");
    let start = app
        .changes_apply(
            workspace.clone(),
            ChangeRequest {
                handle,
                action: ChangeAction::Stage,
                target: ChangeTarget::Files {
                    files: vec![FileRef {
                        list: FileList::Unstaged,
                        id: u32::try_from(unstaged[0].id).unwrap(),
                    }],
                },
                confirmed: false,
            },
        )
        .await
        .unwrap()
        .expect("no newer view replaced it");
    let ChangeStart::Started { job_id } = start else {
        panic!("{start:?}");
    };
    assert_eq!(
        ended(&mut started.events, job_id).await,
        Some(Outcome::Succeeded)
    );

    // Commit it.
    let (handle, _) = uncommitted(&app, &workspace, &repo_id).await;
    let job = app
        .changes_commit(
            workspace.clone(),
            CommitRequest {
                handle,
                message: CommitMessage {
                    subject: "Second".to_owned(),
                    body: String::new(),
                },
                amend: false,
            },
        )
        .await
        .unwrap()
        .expect("no newer view replaced it");
    let mut clean = false;
    loop {
        match started.events.recv().await.unwrap() {
            AppEvent::RepoSummary(view) if view.repo_id == repo_id && !view.refreshing => {
                clean = view.summary.as_ref().is_some_and(|summary| !summary.dirty);
            }
            AppEvent::JobEnded(view) if view.job_id == job => {
                assert_eq!(view.outcome, Some(Outcome::Succeeded), "{view:?}");
                break;
            }
            _ => {}
        }
    }
    assert!(clean, "the last summary before the end shows the commit");

    let history = app
        .history_get(workspace, Some(repo_id), HistoryTarget::Repo, String::new())
        .await
        .unwrap()
        .unwrap();
    let subjects: Vec<&str> = history
        .rows
        .iter()
        .map(|row| row.commit.subject.as_str())
        .collect();
    assert_eq!(subjects, ["Second", "First"]);
}

/// PLAT-1: the calls that ask for a path open the app's dialogs, and act on
/// the path picked. A cancelled dialog does nothing.
#[tokio::test(flavor = "multi_thread")]
async fn plat_1_calls_that_ask_for_a_path_open_dialogs() {
    let fixture = repo_gen::Fixture::new().unwrap();
    fixture
        .repo("api")
        .unwrap()
        .commit_file("README.md", "hello\n", "First")
        .unwrap();
    let mut started = start(EVENT_CAPACITY).await;
    let workspace = active(&started.app);
    let app = started.app.clone();

    // The user cancels, then picks the fixture.
    started.dialogs.answer(None);
    app.workspace_pick_and_add(workspace.clone()).await.unwrap();
    started.dialogs.answer(Some(fixture.path().to_owned()));
    app.workspace_pick_and_add(workspace.clone()).await.unwrap();
    let AppEvent::WorkspaceAddReport(report) = next(&mut started.events, |event| {
        matches!(event, AppEvent::WorkspaceAddReport(_))
    })
    .await
    else {
        unreachable!()
    };
    assert_eq!(report.added.len(), 1, "{report:?}");

    // The parent folder of a clone comes back with its key.
    let parent = fixture.path().join("clones");
    started.dialogs.answer(Some(parent.clone()));
    app.clone_pick_parent(workspace.clone(), Some("api".to_owned()))
        .unwrap();
    let event = next(&mut started.events, |event| {
        matches!(
            event,
            AppEvent::CloneParentPicked { .. } | AppEvent::WorkspaceAddReport(_)
        )
    })
    .await;
    let AppEvent::CloneParentPicked {
        workspace_id,
        path,
        key,
    } = event
    else {
        panic!("the cancelled dialog added repos: {event:?}");
    };
    assert_eq!(
        (workspace_id, path, key),
        (workspace, parent, Some("api".to_owned()))
    );

    let opened: Vec<(&str, String)> = started
        .dialogs
        .opened()
        .into_iter()
        .map(|opened| (opened.kind, opened.title))
        .collect();
    assert_eq!(
        opened,
        [
            ("pick_folder", "Add repositories".to_owned()),
            ("pick_folder", "Add repositories".to_owned()),
            ("pick_folder", "Clone into folder".to_owned()),
        ]
    );
}

/// PLAT-3: git picked in a dialog is chosen, saved, and detected again.
#[tokio::test(flavor = "multi_thread")]
async fn plat_3_git_picked_in_a_dialog_is_chosen() {
    let mut started = start(EVENT_CAPACITY).await;
    let GitStatus::Usable { path, .. } = started.app.git_status().await else {
        unreachable!("start waits for a usable git")
    };
    started.dialogs.answer(Some(PathBuf::from(&path)));
    started.app.git_pick_path();
    let status = loop {
        if let AppEvent::GitStatus(status) = started.events.recv().await.unwrap()
            && !matches!(status, GitStatus::Checking)
        {
            break status;
        }
    };
    let GitStatus::Usable { chosen, .. } = status else {
        panic!("{status:?}");
    };
    assert_eq!(chosen, Some(path));
    assert_eq!(started.dialogs.opened()[0].title, "Choose git");
}

/// DATA-2: a workspace exported to the file picked in the save dialog
/// imports from the open dialog into a new workspace, opened as the active
/// tab.
#[tokio::test(flavor = "multi_thread")]
async fn data_2_workspaces_export_and_import_through_dialogs() {
    let fixture = repo_gen::Fixture::new().unwrap();
    fixture
        .repo("api")
        .unwrap()
        .commit_file("README.md", "hello\n", "First")
        .unwrap();
    let mut started = start(EVENT_CAPACITY).await;
    let app = started.app.clone();
    let tabs = app.workspace_create("Mine: all".to_owned()).await.unwrap();
    let workspace = tabs.active.unwrap();
    add(&mut started, &workspace, fixture.path()).await;

    let file = fixture.path().join("shared.toml");
    started.dialogs.answer(Some(file.clone()));
    app.workspace_export(workspace.clone()).unwrap();
    let AppEvent::WorkspaceExported {
        workspace_id,
        report,
        error,
    } = next(&mut started.events, |event| {
        matches!(event, AppEvent::WorkspaceExported { .. })
    })
    .await
    else {
        unreachable!()
    };
    assert_eq!(workspace_id, workspace);
    assert_eq!(error, None);
    assert!(report.unwrap().absolute.is_empty());
    assert!(file.is_file());

    started.dialogs.answer(Some(file));
    app.workspace_import().await.unwrap();
    let AppEvent::WorkspaceImported { report, error } = next(&mut started.events, |event| {
        matches!(event, AppEvent::WorkspaceImported { .. })
    })
    .await
    else {
        unreachable!()
    };
    assert_eq!(error, None);
    let report = report.unwrap();
    assert!(report.missing.is_empty(), "{report:?}");
    let tabs = app.workspace_get();
    assert_eq!(tabs.active, Some(report.workspace_id.clone()));
    let imported = tabs
        .open
        .iter()
        .find(|view| view.id == report.workspace_id)
        .unwrap();
    assert_eq!(imported.name, "Mine: all (2)");
    assert_eq!(imported.repos.len(), 1);

    let workspace_files = Some(FileFilter {
        name: "Workspace".to_owned(),
        extensions: vec!["toml".to_owned()],
    });
    assert_eq!(
        started.dialogs.opened(),
        [
            Opened {
                kind: "save_file",
                title: "Export workspace".to_owned(),
                file_name: Some("Mine- all.toml".to_owned()),
                filter: workspace_files.clone(),
            },
            Opened {
                kind: "pick_file",
                title: "Import workspace".to_owned(),
                file_name: None,
                filter: workspace_files,
            },
        ]
    );
}

const PASSWORD: &str = "s3cret-for-askpass";

/// A host that wants a password: it answers every request with 401 and
/// keeps each request's `Authorization` header. Returns a repo URL on it.
async fn refusing_host() -> (String, Arc<Mutex<Vec<Option<String>>>>) {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}/api.git", listener.local_addr().unwrap());
    let seen = Arc::new(Mutex::new(Vec::new()));
    let kept = seen.clone();
    tokio::spawn(async move {
        loop {
            let (stream, _) = listener.accept().await.unwrap();
            let kept = kept.clone();
            tokio::spawn(async move {
                let mut stream = tokio::io::BufReader::new(stream);
                let mut authorization = None;
                loop {
                    let mut line = String::new();
                    if stream.read_line(&mut line).await.unwrap() == 0 || line == "\r\n" {
                        break;
                    }
                    if let Some((name, value)) = line.split_once(':')
                        && name.eq_ignore_ascii_case("authorization")
                    {
                        authorization = Some(value.trim().to_owned());
                    }
                }
                kept.lock().unwrap().push(authorization);
                stream
                    .get_mut()
                    .write_all(
                        b"HTTP/1.1 401 Unauthorized\r\n\
                          WWW-Authenticate: Basic realm=\"gitcroc\"\r\n\
                          Content-Length: 0\r\nConnection: close\r\n\r\n",
                    )
                    .await
                    .unwrap();
            });
        }
    });
    (url, seen)
}

/// D10: git's prompts during a fetch arrive as `AskpassPrompt` events,
/// naming the job and the repo, and `askpass_answer` sends each answer to
/// git, which sends it to the host.
#[tokio::test(flavor = "multi_thread")]
async fn d10_askpass_prompt_round_trips_through_app_events() {
    let (url, seen) = refusing_host().await;
    let fixture = repo_gen::Fixture::new().unwrap();
    let repo = fixture.repo("api").unwrap();
    repo.commit_file("README.md", "hello\n", "First").unwrap();
    repo.git(["remote", "add", "origin", &url]).unwrap();
    // No credential helper or proxy of the runner's config answers or
    // gets in the way: git asks its askpass helper.
    repo.git(["config", "credential.helper", ""]).unwrap();
    repo.git(["config", "http.proxy", ""]).unwrap();
    let mut started = start(EVENT_CAPACITY).await;
    let workspace = active(&started.app);
    let repo_id = add(&mut started, &workspace, fixture.path())
        .await
        .remove(0);

    let start = started
        .app
        .op_sync(
            workspace,
            SyncRequest {
                repo_id,
                op: SyncOp::Fetch {
                    prune: Prune::NoPrune,
                },
                confirmed: false,
            },
        )
        .await
        .unwrap();
    let SyncStart::Started { job_id } = start else {
        panic!("{start:?}");
    };

    let mut asked = Vec::new();
    for answer in ["alice", PASSWORD] {
        let AppEvent::AskpassPrompt(prompt) = next(&mut started.events, |event| {
            matches!(event, AppEvent::AskpassPrompt(_))
        })
        .await
        else {
            unreachable!()
        };
        started
            .app
            .askpass_answer(prompt.prompt_id, Some(answer.to_owned()));
        asked.push(prompt);
    }
    // The host refuses the password too, so the fetch fails.
    assert_eq!(
        ended(&mut started.events, job_id).await,
        Some(Outcome::Failed)
    );

    let kinds: Vec<PromptKind> = asked.iter().map(|prompt| prompt.kind).collect();
    assert_eq!(kinds, [PromptKind::Text, PromptKind::Secret]);
    assert!(asked[0].text.starts_with("Username for "), "{asked:?}");
    assert_eq!(asked[1].repo_names, ["api"]);
    assert!(asked[1].job_label.starts_with("Fetch"), "{asked:?}");
    // base64 of `alice:s3cret-for-askpass`.
    let sent = Some("Basic YWxpY2U6czNjcmV0LWZvci1hc2twYXNz".to_owned());
    assert!(seen.lock().unwrap().contains(&sent), "{seen:?}");
}

/// A host that wants a password and keeps each connection open, as git's
/// remote helper does while it asks. Returns a repo URL on it, and how many
/// connections are open.
async fn keeping_host() -> (String, Arc<AtomicUsize>) {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}/api.git", listener.local_addr().unwrap());
    let open = Arc::new(AtomicUsize::new(0));
    let counted = open.clone();
    tokio::spawn(async move {
        while let Ok((stream, _)) = listener.accept().await {
            counted.fetch_add(1, Ordering::SeqCst);
            let counted = counted.clone();
            tokio::spawn(async move {
                let mut stream = tokio::io::BufReader::new(stream);
                let mut line = String::new();
                // Each request ends with an empty line; the connection ends
                // when git's remote helper closes it or exits.
                while matches!(stream.read_line(&mut line).await, Ok(1..)) {
                    if line == "\r\n"
                        && stream
                            .get_mut()
                            .write_all(
                                b"HTTP/1.1 401 Unauthorized\r\n\
                                  WWW-Authenticate: Basic realm=\"gitcroc\"\r\n\
                                  Content-Length: 0\r\n\r\n",
                            )
                            .await
                            .is_err()
                    {
                        break;
                    }
                    line.clear();
                }
                counted.fetch_sub(1, Ordering::SeqCst);
            });
        }
    });
    (url, open)
}

/// Waits until `done` holds. The deadline only ends a hung test.
#[track_caller]
fn wait_until(what: &str, mut done: impl FnMut() -> bool) {
    for _ in 0..3000 {
        if done() {
            return;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    panic!("{what}");
}

/// D10: the app quits, as `gitcroc-next`'s `main` does, while a fetch
/// waits on git's prompt. The shutdown gives the helper no answer and it
/// exits with status 1; then git's remote helper, the process that talks
/// to the host and that git waits on, ends: nothing is left waiting on the
/// app.
#[test]
fn d10_quit_ends_an_open_askpass_prompt() {
    // The host outlives the app, on a runtime of its own.
    let host = tokio::runtime::Runtime::new().unwrap();
    let (url, open) = host.block_on(keeping_host());
    let fixture = repo_gen::Fixture::new().unwrap();
    let repo = fixture.repo("api").unwrap();
    repo.commit_file("README.md", "hello\n", "First").unwrap();
    repo.git(["remote", "add", "origin", &url]).unwrap();
    repo.git(["config", "credential.helper", ""]).unwrap();
    repo.git(["config", "http.proxy", ""]).unwrap();
    // A copy of the helper that writes down how it exited.
    let programs = tempfile::tempdir().unwrap();
    let program = programs.path().join(
        Path::new(env!("CARGO_BIN_EXE_askpass-helper"))
            .file_name()
            .unwrap(),
    );
    std::fs::copy(env!("CARGO_BIN_EXE_askpass-helper"), &program).unwrap();
    let suffixed = |suffix: &str| {
        let mut name = program.clone().into_os_string();
        name.push(suffix);
        PathBuf::from(name)
    };
    std::fs::write(suffixed(".record"), "").unwrap();

    let runtime = tokio::runtime::Runtime::new().unwrap();
    let started = runtime.block_on(async {
        let mut started = start_with(EVENT_CAPACITY, program.clone()).await;
        let workspace = active(&started.app);
        let repo_id = add(&mut started, &workspace, fixture.path())
            .await
            .remove(0);
        let start = started
            .app
            .op_sync(
                workspace,
                SyncRequest {
                    repo_id,
                    op: SyncOp::Fetch {
                        prune: Prune::NoPrune,
                    },
                    confirmed: false,
                },
            )
            .await
            .unwrap();
        assert!(matches!(start, SyncStart::Started { .. }), "{start:?}");
        next(&mut started.events, |event| {
            matches!(event, AppEvent::AskpassPrompt(_))
        })
        .await;
        started
    });
    assert!(open.load(Ordering::SeqCst) > 0, "git holds no connection");

    // The helper has its reply before the runtime goes, so it does not
    // lean on the socket closing with the app.
    runtime.block_on(started.app.shutdown());
    let exit = suffixed(".exit");
    wait_until("the helper is still waiting", || {
        std::fs::read_to_string(&exit).is_ok_and(|code| !code.is_empty())
    });
    assert_eq!(std::fs::read_to_string(&exit).unwrap(), "1");
    runtime.shutdown_background();
    wait_until("git's remote helper is still running", || {
        open.load(Ordering::SeqCst) == 0
    });
}

/// GRP-5: a group operation on a multi-selection runs through its
/// preflight, whose result arrives as `PreflightReady`, then its job; a
/// dropped preflight cannot start.
#[tokio::test(flavor = "multi_thread")]
async fn grp_5_group_operation_runs_on_the_target_set_through_app_events() {
    let fixture = repo_gen::Fixture::new().unwrap();
    let repos: Vec<_> = ["api", "web"]
        .into_iter()
        .map(|name| {
            let repo = fixture.repo(name).unwrap();
            repo.commit_file("README.md", "hello\n", "First").unwrap();
            repo
        })
        .collect();
    let mut started = start(EVENT_CAPACITY).await;
    let workspace = active(&started.app);
    let repo_ids = add(&mut started, &workspace, fixture.path()).await;
    assert_eq!(repo_ids.len(), 2);
    let target = TargetSet::Repos { repo_ids };
    let op = GroupOp::CreateBranch {
        name: "feature".to_owned(),
        start: StartPoint::Head,
        switch: false,
    };

    let preflight = started
        .app
        .group_preflight(workspace.clone(), target.clone(), op.clone())
        .await
        .unwrap();
    let AppEvent::PreflightReady(ready) = next(
        &mut started.events,
        |event| matches!(event, AppEvent::PreflightReady(ready) if ready.preflight_id == preflight),
    )
    .await
    else {
        unreachable!()
    };
    assert_eq!(ready.repos.len(), 2, "{ready:?}");
    let job = started.app.group_start(preflight, true).await.unwrap();
    assert_eq!(
        ended(&mut started.events, job).await,
        Some(Outcome::Succeeded)
    );
    for repo in &repos {
        let branches = repo.git(["branch", "--list", "feature"]).unwrap();
        assert!(branches.contains("feature"), "{branches:?}");
    }

    let dropped = started
        .app
        .group_preflight(workspace, target, op)
        .await
        .unwrap();
    next(
        &mut started.events,
        |event| matches!(event, AppEvent::PreflightReady(ready) if ready.preflight_id == dropped),
    )
    .await;
    started.app.group_drop(dropped);
    let error = started.app.group_start(dropped, true).await.unwrap_err();
    assert_eq!(error.code, AppErrorCode::Refused, "{error:?}");
}
