//! The job engine (M2-01): one write per repo at a time (ARCH-4), the pool
//! shared with the refresh (PERF-4), outcomes (GRP-7), cancel (GRP-10) and
//! the re-read of the repos a job touched (OPS-7). Nothing here waits for a
//! time: jobs block on channels the test releases.

// Integration test crate: helpers outside #[test] fns are test code too (QA-4).
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::ffi::OsString;
use std::path::PathBuf;
use std::pin::pin;
use std::sync::{Arc, Mutex};
use std::task::{Context, Poll, Waker};
use std::time::Duration;

use gitcroc_core::{
    AddReport, CancelResult, ConfigDir, GitError, GitRunner, Head, JobEvents, JobHandle, JobRun,
    JobState, JobView, Jobs, NothingToDo, OnCancel, Outcome, Output, Pool, Priority, Refusal,
    RepoId, RepoView, SkipReason, Summaries, SummaryCache, SummaryView, WorkspaceId, Workspaces,
    WriteJob, add_path, refresh_summaries,
};
use repo_gen::{Fixture, Repo};
use tokio::sync::{mpsc, oneshot};

struct Dirs {
    root: tempfile::TempDir,
}

impl ConfigDir for Dirs {
    fn config_dir(&self) -> Option<PathBuf> {
        Some(self.root.path().join("config"))
    }
}

/// What the engine sent, in order.
#[derive(Debug, Clone)]
enum Sent {
    Progress(JobView),
    Ended(JobView),
    Summary(SummaryView, bool),
}

#[derive(Default)]
struct Recorder(Mutex<Vec<Sent>>);

impl JobEvents for Recorder {
    fn progress(&self, job: &JobView) {
        self.0.lock().unwrap().push(Sent::Progress(job.clone()));
    }

    fn ended(&self, job: &JobView) {
        self.0.lock().unwrap().push(Sent::Ended(job.clone()));
    }

    fn summary(&self, view: SummaryView, read: bool) {
        self.0.lock().unwrap().push(Sent::Summary(view, read));
    }
}

/// A workspace with repos `one` and `two`, and the engine.
struct Setup {
    _dirs: Dirs,
    fixture: Fixture,
    workspaces: Arc<Workspaces>,
    summaries: Arc<Summaries>,
    jobs: Arc<Jobs>,
    sent: Arc<Recorder>,
    id: WorkspaceId,
}

impl Setup {
    async fn new() -> Self {
        Self::with_pool(Pool::new(gitcroc_core::POOL_SIZE)).await
    }

    async fn with_pool(pool: Arc<Pool>) -> Self {
        let fixture = Fixture::new().unwrap();
        for name in ["one", "two"] {
            let repo = fixture.repo(name).unwrap();
            repo.commit_file("README.md", "hello\n", "First").unwrap();
        }
        let dirs = Dirs {
            root: tempfile::tempdir().unwrap(),
        };
        let workspaces = Arc::new(Workspaces::load(&dirs));
        let id = workspaces.views()[0].id.clone();
        add_path(
            workspaces.clone(),
            id.clone(),
            fixture.path().to_owned(),
            git(),
            gitcroc_core::JobId(0),
            |_| {},
        )
        .await
        .unwrap();
        let summaries =
            Arc::new(Summaries::load(SummaryCache::none(), &workspaces).with_pool(pool));
        let sent = Arc::new(Recorder::default());
        let jobs = Jobs::new(workspaces.clone(), summaries.clone(), sent.clone());
        Self {
            _dirs: dirs,
            fixture,
            workspaces,
            summaries,
            jobs,
            sent,
            id,
        }
    }

    fn repo_id(&self, name: &str) -> RepoId {
        let view = self.workspaces.view(&self.id).unwrap();
        view.repos
            .into_iter()
            .find(|repo| repo.name == name)
            .unwrap()
            .id
    }

    fn repo(&self, name: &str) -> Repo {
        Repo::open(self.fixture.path().join(name))
    }

    fn job(&self, repo: &str, label: &str) -> WriteJob {
        WriteJob {
            workspace_id: self.id.clone(),
            repos: vec![self.repo_id(repo)],
            label: label.to_owned(),
            on_cancel: OnCancel::Finish,
            may_conflict: false,
            nothing_to_do: NothingToDo::Never,
            network: false,
        }
    }

    /// Starts a write whose "git process" tells `log` when it starts and
    /// ends, and blocks until the returned sender releases it.
    fn held(&self, repo: &str, label: &str, log: &Log) -> (JobHandle, oneshot::Sender<()>) {
        let (release, released) = oneshot::channel::<()>();
        let (log, label_owned) = (log.clone(), label.to_owned());
        let handle = self
            .jobs
            .start(git(), self.job(repo, label), move |_run| async move {
                log.push(format!("start {label_owned}"));
                let _ = released.await;
                log.push(format!("end {label_owned}"));
                Ok(empty())
            })
            .unwrap();
        (handle, release)
    }

    fn state(&self, job: &JobHandle) -> JobState {
        self.jobs
            .views(&self.id)
            .into_iter()
            .find(|view| view.job_id == job.job_id)
            .unwrap()
            .state
    }

    fn sent(&self) -> Vec<Sent> {
        self.sent.0.lock().unwrap().clone()
    }
}

/// What the held writes did, in order, and a channel that says each start.
#[derive(Clone)]
struct Log {
    lines: Arc<Mutex<Vec<String>>>,
    started: mpsc::UnboundedSender<String>,
}

impl Log {
    fn new() -> (Self, mpsc::UnboundedReceiver<String>) {
        let (started, receiver) = mpsc::unbounded_channel();
        let log = Self {
            lines: Arc::default(),
            started,
        };
        (log, receiver)
    }

    fn push(&self, line: String) {
        self.lines.lock().unwrap().push(line.clone());
        if let Some(label) = line.strip_prefix("start ") {
            let _ = self.started.send(label.to_owned());
        }
    }

    fn lines(&self) -> Vec<String> {
        self.lines.lock().unwrap().clone()
    }
}

fn git() -> GitRunner {
    GitRunner::new(PathBuf::from("git"))
}

fn empty() -> Output {
    Output {
        stdout: Vec::new(),
        stderr: Vec::new(),
    }
}

fn args(args: &[&str]) -> Vec<OsString> {
    args.iter().map(OsString::from).collect()
}

/// Waits for a job's end. The limit only keeps a broken engine from
/// hanging the run; it is never what a test asserts.
async fn ended(handle: JobHandle) -> JobView {
    tokio::time::timeout(Duration::from_secs(60), handle.ended)
        .await
        .expect("the job never ended")
        .unwrap()
}

fn head(view: &SummaryView) -> Option<String> {
    match &view.summary.as_ref()?.head {
        Head::Branch { hash, .. } => hash.clone(),
        Head::Detached { hash } => Some(hash.clone()),
    }
}

#[tokio::test]
async fn arch_4_one_write_per_repo_at_a_time() {
    let setup = Setup::new().await;
    let (log, mut started) = Log::new();
    let (first, release_first) = setup.held("one", "A", &log);
    let (second, release_second) = setup.held("one", "B", &log);
    let (other, release_other) = setup.held("two", "C", &log);

    // A and C run at the same time, on two repos; B waits for A.
    let mut running = vec![started.recv().await.unwrap(), started.recv().await.unwrap()];
    running.sort();
    assert_eq!(running, ["A", "C"]);
    assert_eq!(setup.state(&second), JobState::Queued);

    release_first.send(()).unwrap();
    assert_eq!(started.recv().await.unwrap(), "B");
    release_second.send(()).unwrap();
    release_other.send(()).unwrap();
    for handle in [first, second, other] {
        assert_eq!(ended(handle).await.outcome, Some(Outcome::Succeeded));
    }
    let lines = log.lines();
    let at = |line: &str| lines.iter().position(|seen| seen == line).unwrap();
    assert!(at("end A") < at("start B"), "{lines:?}");
    assert!(at("start C") < at("end A"), "{lines:?}");

    // `busy` while a write waited or ran, and not after.
    let busy: Vec<bool> = setup
        .sent()
        .iter()
        .filter_map(|sent| match sent {
            Sent::Summary(view, false) if view.repo_id == setup.repo_id("one") => Some(view.busy),
            _ => None,
        })
        .collect();
    assert_eq!(busy.first(), Some(&true));
    assert_eq!(busy.last(), Some(&false));
    let views = setup.summaries.views(&setup.workspaces, &setup.id).unwrap();
    assert!(views.iter().all(|view| !view.busy));
}

#[tokio::test]
async fn ui_21_closing_a_workspace_lets_its_jobs_end_and_keeps_them_listed() {
    let setup = Setup::new().await;
    let name = setup.workspaces.view(&setup.id).unwrap().name;
    let (log, mut started) = Log::new();
    let (held, release) = setup.held("one", "A", &log);
    assert_eq!(started.recv().await.unwrap(), "A");

    // What the app does when the tab closes (M3-01).
    setup.workspaces.close(&setup.id).unwrap();
    setup.summaries.forget(&setup.id);
    release.send(()).unwrap();

    let view = ended(held).await;
    assert_eq!(view.outcome, Some(Outcome::Succeeded));
    let listed = setup.jobs.all_views();
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].job_id, view.job_id);
    assert_eq!(listed[0].workspace_name, name);
    assert_eq!(listed[0].state, JobState::Ended);
}

#[tokio::test]
async fn arch_4_reads_are_not_blocked_by_a_write() {
    let setup = Setup::new().await;
    let (log, mut started) = Log::new();
    let (write, release) = setup.held("one", "write", &log);
    started.recv().await.unwrap();

    let read = Arc::new(Mutex::new(Vec::new()));
    let sink = read.clone();
    refresh_summaries(
        setup.workspaces.clone(),
        setup.summaries.clone(),
        setup.id.clone(),
        vec![setup.repo_id("one")],
        git(),
        move |view| sink.lock().unwrap().push(view),
    )
    .await;
    // The read finished while the write is still held.
    assert_eq!(setup.state(&write), JobState::Running);
    let read = read.lock().unwrap().clone();
    let last = read.last().unwrap();
    assert!(!last.refreshing && last.summary.is_some(), "{last:?}");
    assert!(last.busy);

    release.send(()).unwrap();
    ended(write).await;
}

#[tokio::test]
async fn ops_7_a_job_rereads_the_repos_it_touched() {
    let setup = Setup::new().await;
    let repo = setup.repo("one");
    let before = repo.head().unwrap();
    let committing = setup.repo("one");
    let handle = setup
        .jobs
        .start(git(), setup.job("one", "commit"), move |_run| async move {
            // The test's stand-in for a write of `ops`.
            committing
                .git(["commit", "--quiet", "--allow-empty", "-m", "Second"])
                .unwrap();
            Ok(empty())
        })
        .unwrap();
    let job_id = handle.job_id;
    let view = ended(handle).await;
    assert_eq!(view.outcome, Some(Outcome::Succeeded));
    let after = repo.head().unwrap();
    assert_ne!(before, after);

    // No watcher runs here: the fresh summary comes from the job, before
    // its end.
    let sent = setup.sent();
    let read = sent
        .iter()
        .position(|sent| {
            matches!(sent, Sent::Summary(view, true)
                if view.repo_id == setup.repo_id("one") && head(view).as_deref() == Some(&after))
        })
        .expect("the new summary was sent");
    let began = sent
        .iter()
        .position(|sent| {
            matches!(sent, Sent::Progress(progress)
                if progress.job_id == job_id && progress.state == JobState::Running)
        })
        .unwrap();
    let end = sent
        .iter()
        .position(|sent| matches!(sent, Sent::Ended(view) if view.job_id == job_id))
        .unwrap();
    assert!(began < read && read < end);
    let Sent::Summary(fresh, _) = &sent[read] else {
        unreachable!()
    };
    // The refs changed, so the history reads the repo again.
    assert!(fresh.refs_version > 0);
}

#[test]
fn perf_4_user_jobs_run_before_the_background_refresh() {
    fn poll<F: Future>(future: std::pin::Pin<&mut F>) -> Poll<F::Output> {
        future.poll(&mut Context::from_waker(Waker::noop()))
    }
    let pool = Pool::new(2);
    // Every worker is busy with refresh reads, and another read waits.
    let Poll::Ready(first) = poll(pin!(pool.worker(Priority::Refresh))) else {
        panic!("a free worker was not given")
    };
    let Poll::Ready(_second) = poll(pin!(pool.worker(Priority::Refresh))) else {
        panic!("a free worker was not given")
    };
    let mut refresh = pin!(pool.worker(Priority::Refresh));
    assert!(poll(refresh.as_mut()).is_pending());
    let mut job = pin!(pool.worker(Priority::Job));
    assert!(poll(job.as_mut()).is_pending());
    assert_eq!(pool.waiting(), (1, 1));

    // The job, started later, takes the next free worker.
    drop(first);
    let Poll::Ready(_job_worker) = poll(job.as_mut()) else {
        panic!("the job did not get the free worker")
    };
    assert!(poll(refresh.as_mut()).is_pending());
    assert_eq!(pool.waiting(), (0, 1));
}

#[tokio::test]
async fn perf_4_a_job_waits_for_a_worker_behind_no_refresh() {
    // With the only worker held by a refresh read, a job waits for it and
    // then runs before the refresh read queued before it.
    let pool = Pool::new(1);
    let setup = Setup::with_pool(pool.clone()).await;
    let held = pool.worker(Priority::Refresh).await;
    let refresh = tokio::spawn({
        let pool = pool.clone();
        async move {
            let _worker = pool.worker(Priority::Refresh).await;
        }
    });
    while pool.waiting() != (0, 1) {
        tokio::task::yield_now().await;
    }
    let (log, mut started) = Log::new();
    let (job, release) = setup.held("one", "job", &log);
    while pool.waiting() != (1, 1) {
        tokio::task::yield_now().await;
    }
    drop(held);
    assert_eq!(started.recv().await.unwrap(), "job");
    assert!(!refresh.is_finished());
    release.send(()).unwrap();
    ended(job).await;
    refresh.await.unwrap();
}

#[tokio::test]
async fn grp_10_cancel_kills_a_running_process() {
    let setup = Setup::new().await;
    // A remote that accepts the connection and never answers: git waits
    // for it until killed.
    let silent = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("git://{}/never.git", silent.local_addr().unwrap());
    let (spawned, is_running) = oneshot::channel();
    let mut job = setup.job("one", "Fetch");
    job.on_cancel = OnCancel::Kill;
    let handle = setup
        .jobs
        .start(git(), job, move |run: JobRun| async move {
            let _ = spawned.send(());
            // The test's stand-in for the fetch of `ops` (M2-07).
            let path = run.paths[0].clone();
            run.git
                .run_write(&args(&["fetch", &url]), &path, run.write())
                .await
        })
        .unwrap();
    is_running.await.unwrap();
    assert_eq!(setup.jobs.cancel(handle.job_id), CancelResult::Killing);
    let view = ended(handle).await;
    assert_eq!(
        view.outcome,
        Some(Outcome::Skipped {
            reason: SkipReason::Cancelled
        })
    );
    drop(silent);

    // Any other running job runs to its end.
    let (log, mut started) = Log::new();
    let (other, release) = setup.held("one", "commit", &log);
    started.recv().await.unwrap();
    assert_eq!(setup.jobs.cancel(other.job_id), CancelResult::Finishing);
    release.send(()).unwrap();
    assert_eq!(ended(other).await.outcome, Some(Outcome::Succeeded));
    assert_eq!(setup.jobs.cancel(other_id()), CancelResult::Ended);
}

/// An id no job has.
fn other_id() -> gitcroc_core::JobId {
    gitcroc_core::JobId(u32::MAX)
}

#[tokio::test]
async fn grp_10_a_queued_job_is_cancelled_before_it_starts() {
    let setup = Setup::new().await;
    let (log, mut started) = Log::new();
    let (first, release) = setup.held("one", "A", &log);
    let (queued, _never) = setup.held("one", "B", &log);
    let (after, release_after) = setup.held("one", "C", &log);
    assert_eq!(started.recv().await.unwrap(), "A");

    assert_eq!(setup.jobs.cancel(queued.job_id), CancelResult::Cancelled);
    let view = ended(queued).await;
    assert_eq!(
        view.outcome,
        Some(Outcome::Skipped {
            reason: SkipReason::Cancelled
        })
    );
    // The queue goes on without it.
    release.send(()).unwrap();
    assert_eq!(started.recv().await.unwrap(), "C");
    release_after.send(()).unwrap();
    ended(first).await;
    ended(after).await;
    assert!(!log.lines().iter().any(|line| line.ends_with(" B")));

    // Newest first, each with its outcome.
    let views = setup.jobs.views(&setup.id);
    let labels: Vec<&str> = views.iter().map(|view| view.label.as_str()).collect();
    assert_eq!(labels, ["C", "B", "A"]);
    assert!(views.iter().all(|view| view.state == JobState::Ended));
}

/// A repo with a branch whose change to `file.txt` conflicts with `main`'s.
fn conflicting(repo: &Repo) {
    // The job's merge runs through the runner, without the fixture's own
    // identity: git needs one to start a merge, as on a CI runner without
    // a global config.
    repo.git(["config", "user.name", "Test"]).unwrap();
    repo.git(["config", "user.email", "test@example.com"])
        .unwrap();
    repo.commit_file("file.txt", "base\n", "Base").unwrap();
    repo.branch("topic").unwrap();
    repo.switch("topic").unwrap();
    repo.commit_file("file.txt", "theirs\n", "Theirs").unwrap();
    repo.switch("main").unwrap();
    repo.commit_file("file.txt", "ours\n", "Ours").unwrap();
}

#[tokio::test]
async fn grp_7_conflict_is_read_from_the_repo() {
    let setup = Setup::new().await;
    let repo = setup.repo("one");
    conflicting(&repo);
    let mut job = setup.job("one", "Merge topic");
    job.may_conflict = true;
    let handle = setup
        .jobs
        .start(git(), job, move |run: JobRun| async move {
            // The test's stand-in for the merge of `ops` (M2-08); git exits
            // with 1 and leaves `MERGE_HEAD`.
            let path = run.paths[0].clone();
            run.git
                .run_write(
                    &args(&["merge", "--no-ff", "--no-edit", "topic"]),
                    &path,
                    run.write(),
                )
                .await
        })
        .unwrap();
    let view = ended(handle).await;
    assert!(setup.fixture.path().join("one/.git/MERGE_HEAD").is_file());
    assert_eq!(view.outcome, Some(Outcome::Conflict));
    assert_eq!(view.message, None);
    assert!(view.output.contains("CONFLICT"), "{}", view.output);
    let summary = setup.summaries.views(&setup.workspaces, &setup.id).unwrap();
    assert!(
        summary
            .iter()
            .any(|view| view.repo_id == setup.repo_id("one") && view.conflict)
    );
}

#[tokio::test]
async fn grp_7_a_failed_write_is_failed_and_nothing_to_do_is_read_from_the_refs() {
    let setup = Setup::new().await;
    let failing = setup
        .jobs
        .start(git(), setup.job("one", "Merge"), |run: JobRun| async move {
            let path = run.paths[0].clone();
            run.git
                .run_write(&args(&["merge", "no-such-branch"]), &path, run.write())
                .await
        })
        .unwrap();
    let view = ended(failing).await;
    assert_eq!(view.outcome, Some(Outcome::Failed));
    assert!(
        view.message.as_deref().unwrap().contains("no-such-branch"),
        "{view:?}"
    );

    let mut job = setup.job("one", "Nothing");
    job.nothing_to_do = NothingToDo::WhenRefsUnchanged;
    let unchanged = setup
        .jobs
        .start(git(), job.clone(), |_run| async { Ok(empty()) })
        .unwrap();
    assert_eq!(ended(unchanged).await.outcome, Some(Outcome::NothingToDo));
    let repo = setup.repo("one");
    let changed = setup
        .jobs
        .start(git(), job, move |_run| async move {
            repo.tag("v1").unwrap();
            Ok(empty())
        })
        .unwrap();
    assert_eq!(ended(changed).await.outcome, Some(Outcome::Succeeded));
}

#[tokio::test]
async fn a_job_on_a_repo_not_in_the_workspace_does_not_start() {
    let setup = Setup::new().await;
    let mut job = setup.job("one", "x");
    job.repos = vec![RepoId("no-such-repo".to_owned())];
    let started = setup
        .jobs
        .start(git(), job, |_run| async { Ok::<_, GitError>(empty()) });
    assert!(started.is_err());
    assert!(setup.jobs.views(&setup.id).is_empty());
}

/// Each view the engine sent for job `job_id`, in order.
fn progress_of(sent: &[Sent], job_id: gitcroc_core::JobId) -> Vec<JobView> {
    sent.iter()
        .filter_map(|sent| match sent {
            Sent::Progress(view) if view.job_id == job_id => Some(view.clone()),
            _ => None,
        })
        .collect()
}

#[tokio::test]
async fn ui_21_a_job_is_sent_queued_then_running_with_progress_then_ended() {
    let setup = Setup::new().await;
    let (log, mut started) = Log::new();
    let (first, release_first) = setup.held("one", "A", &log);
    started.recv().await.unwrap();
    let second = setup
        .jobs
        .start(git(), setup.job("one", "B"), |run| async move {
            run.progress(1, 2);
            run.progress(2, 2);
            Ok(empty())
        })
        .unwrap();
    let id = second.job_id;

    // Queued behind A, with the names the job list shows.
    let queued = progress_of(&setup.sent(), id);
    assert_eq!(queued.len(), 1, "{queued:?}");
    let workspace_name = setup.workspaces.view(&setup.id).unwrap().name;
    assert_eq!(queued[0].state, JobState::Queued);
    assert_eq!(queued[0].workspace_name, workspace_name);
    assert_eq!(queued[0].repo_names, ["one"]);
    assert_eq!(queued[0].started_at, None);

    release_first.send(()).unwrap();
    ended(first).await;
    let view = ended(second).await;
    let steps: Vec<(JobState, u32, u32)> = progress_of(&setup.sent(), id)
        .iter()
        .map(|view| (view.state, view.done, view.total))
        .collect();
    assert_eq!(
        steps,
        [
            (JobState::Queued, 0, 0),
            (JobState::Running, 0, 0),
            (JobState::Running, 1, 2),
            (JobState::Running, 2, 2),
        ]
    );
    assert_eq!(view.state, JobState::Ended);
    assert_eq!(view.outcome, Some(Outcome::Succeeded));
    assert!(
        view.started_at.is_some() && view.duration_ms.is_some(),
        "{view:?}"
    );
    assert_eq!(setup.jobs.views(&setup.id)[0], view);
    assert_eq!(setup.jobs.all_views()[0], view);
}

fn repo_view(name: &str) -> RepoView {
    RepoView {
        id: RepoId(name.to_owned()),
        name: name.to_owned(),
        path: format!("/work/{name}"),
        remote: None,
    }
}

#[tokio::test]
async fn ui_21_an_add_job_is_listed_with_its_outcome() {
    let setup = Setup::new().await;
    let tracked = setup.jobs.track(&setup.id, "Add repos from /work").unwrap();
    let id = tracked.job_id();
    tracked.progress(1, 3);
    let report = AddReport {
        workspace_id: setup.id.clone(),
        job_id: id,
        added: vec![repo_view("new")],
        already_present: vec![repo_view("old")],
        refused: vec![Refusal {
            path: "/work/broken".to_owned(),
            reason: "not a repo".to_owned(),
        }],
    };
    let (outcome, output, message) = report.ending();
    tracked.end(outcome, output, message);

    let steps: Vec<(JobState, u32, u32)> = progress_of(&setup.sent(), id)
        .iter()
        .map(|view| (view.state, view.done, view.total))
        .collect();
    assert_eq!(
        steps,
        [(JobState::Running, 0, 0), (JobState::Running, 1, 3)]
    );
    let view = setup.jobs.views(&setup.id)[0].clone();
    assert_eq!(view.job_id, id);
    assert_eq!(view.state, JobState::Ended);
    assert_eq!(view.outcome, Some(Outcome::Succeeded));
    assert_eq!(
        view.output,
        "added /work/new\nalready present /work/old\nrefused /work/broken: not a repo\n"
    );
    assert!(
        setup
            .sent()
            .iter()
            .any(|sent| matches!(sent, Sent::Ended(ended) if *ended == view))
    );

    // Dropped before its end, as when its task panics, it failed.
    let dropped = setup.jobs.track(&setup.id, "Add repos from /gone").unwrap();
    let dropped_id = dropped.job_id();
    drop(dropped);
    let view = setup.jobs.views(&setup.id)[0].clone();
    assert_eq!(
        (view.job_id, view.outcome),
        (dropped_id, Some(Outcome::Failed))
    );
}

#[test]
fn grp_7_an_add_job_succeeds_when_it_added_a_repo() {
    let report = |added: Vec<RepoView>, present: Vec<RepoView>, refused: Vec<Refusal>| AddReport {
        workspace_id: WorkspaceId("ws".to_owned()),
        job_id: gitcroc_core::JobId(1),
        added,
        already_present: present,
        refused,
    };
    let refusal = || Refusal {
        path: "/work/broken".to_owned(),
        reason: "not a repo".to_owned(),
    };
    let outcome = |report: AddReport| {
        let (outcome, _, message) = report.ending();
        (outcome, message)
    };
    assert_eq!(
        outcome(report(vec![repo_view("new")], vec![], vec![refusal()])),
        (Outcome::Succeeded, None)
    );
    assert_eq!(
        outcome(report(vec![], vec![repo_view("old")], vec![])),
        (Outcome::NothingToDo, None)
    );
    assert_eq!(
        outcome(report(vec![], vec![repo_view("old")], vec![refusal()])),
        (Outcome::Failed, Some("no repo was added".to_owned()))
    );
}
