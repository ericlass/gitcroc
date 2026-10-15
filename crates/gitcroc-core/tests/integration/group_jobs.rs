//! Group operations in the core (M3-08): preflight, the confirmation rule,
//! the run with one sub-job per repo, the report (GRP-6, GRP-7), GRP-8 at
//! preflight and at run, cancel (GRP-10) and the parallelism setting. A test
//! operation stands in for the real ones; what it runs in each repo is set
//! per repo, and its held runs block on channels the test releases, so
//! nothing here waits for a time.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use gitcroc_core::ops::{self, FastForward};
use gitcroc_core::{
    Class, ConfigDir, GitError, GitRepo, GitRunner, GroupError, GroupOp, GroupOperation, GroupOps,
    GroupSettings, JobEvents, JobId, JobState, JobView, Jobs, NothingToDo, OnCancel, Outcome,
    Output, PreflightId, PreflightReady, Prune, RepoId, RunFuture, SettingsStore, SkipReason,
    SubJobSpec, Summaries, SummaryCache, SummaryView, TargetSet, WorkspaceId, Workspaces, WriteJob,
    add_path,
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

/// Every ended job, by id, and the repos read again after a write.
#[derive(Default)]
struct Ended(
    Mutex<HashMap<JobId, JobView>>,
    tokio::sync::Notify,
    Mutex<Vec<RepoId>>,
);

impl JobEvents for Ended {
    fn progress(&self, _job: &JobView) {}

    fn ended(&self, job: &JobView) {
        self.0.lock().unwrap().insert(job.job_id, job.clone());
        self.1.notify_waiters();
    }

    fn summary(&self, view: SummaryView, read: bool) {
        if read {
            self.2.lock().unwrap().push(view.repo_id);
            self.1.notify_waiters();
        }
    }
}

/// What the test operation runs in a repo.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Act {
    /// Creates branch `made`: *succeeded*.
    Succeed,
    /// Runs nothing: *nothing to do*.
    Nothing,
    /// git fails.
    Fail,
    /// Merges branch `feature`, which conflicts: *conflict*.
    Conflict,
    /// Blocks until the test releases it, or until it is killed.
    Hold,
}

/// The test operation: classes and acts per repo name, held runs that the
/// test releases, and a count of the runs held at once.
struct TestOp {
    on_cancel: OnCancel,
    conflict_allowed: bool,
    destructive: bool,
    classes: HashMap<String, Class>,
    acts: HashMap<String, Act>,
    gates: Gates,
}

#[derive(Clone)]
struct Gates {
    started: mpsc::UnboundedSender<String>,
    releases: Arc<Mutex<HashMap<String, oneshot::Receiver<()>>>>,
    ran: Arc<Mutex<Vec<String>>>,
    held: Arc<AtomicUsize>,
    most: Arc<AtomicUsize>,
}

impl TestOp {
    fn new(acts: &[(&str, Act)]) -> (Self, Control) {
        let (started, started_rx) = mpsc::unbounded_channel();
        let gates = Gates {
            started,
            releases: Arc::default(),
            ran: Arc::default(),
            held: Arc::default(),
            most: Arc::default(),
        };
        let mut senders = HashMap::new();
        for (name, act) in acts {
            if *act == Act::Hold {
                let (release, released) = oneshot::channel();
                gates
                    .releases
                    .lock()
                    .unwrap()
                    .insert((*name).to_owned(), released);
                senders.insert((*name).to_owned(), release);
            }
        }
        let op = Self {
            on_cancel: OnCancel::Finish,
            conflict_allowed: false,
            destructive: false,
            classes: HashMap::new(),
            acts: acts
                .iter()
                .map(|(name, act)| ((*name).to_owned(), *act))
                .collect(),
            gates: gates.clone(),
        };
        let control = Control {
            started: started_rx,
            releases: senders,
            gates,
        };
        (op, control)
    }

    fn class(mut self, name: &str, class: Class) -> Self {
        self.classes.insert(name.to_owned(), class);
        self
    }
}

fn name_of(path: &std::path::Path) -> String {
    path.file_name().unwrap().to_string_lossy().into_owned()
}

impl GroupOperation for TestOp {
    fn name(&self) -> String {
        "Test".to_owned()
    }

    fn destructive(&self) -> bool {
        self.destructive
    }

    fn spec(&self) -> SubJobSpec {
        SubJobSpec {
            on_cancel: self.on_cancel,
            may_conflict: true,
            nothing_to_do: NothingToDo::WhenRefsUnchanged,
            network: false,
            conflict_allowed: self.conflict_allowed,
        }
    }

    fn classify(&self, repo: &GitRepo, _git: &GitRunner) -> Class {
        let name = name_of(repo.work_dir().unwrap());
        self.classes.get(&name).cloned().unwrap_or(Class::WillRun)
    }

    fn run(&self, job: gitcroc_core::JobRun, path: PathBuf) -> RunFuture {
        let name = name_of(&path);
        let act = self.acts.get(&name).copied().unwrap_or(Act::Succeed);
        let gates = self.gates.clone();
        Box::pin(async move {
            gates.ran.lock().unwrap().push(name.clone());
            match act {
                Act::Succeed => {
                    ops::branch_create(&job.git, &path, "made", None, false, job.write()).await
                }
                Act::Nothing => Ok(Output {
                    stdout: b"nothing\n".to_vec(),
                    stderr: Vec::new(),
                }),
                Act::Fail => Err(GitError::Failed {
                    code: Some(1),
                    stdout: Vec::new(),
                    stderr: b"fatal: refused\n".to_vec(),
                }),
                Act::Conflict => {
                    ops::merge(&job.git, &path, "feature", FastForward::Config, job.write()).await
                }
                Act::Hold => {
                    let released = gates.releases.lock().unwrap().remove(&name).unwrap();
                    let held = gates.held.fetch_add(1, Ordering::SeqCst) + 1;
                    gates.most.fetch_max(held, Ordering::SeqCst);
                    gates.started.send(name).unwrap();
                    let write = job.write();
                    let killed = write.kill.unwrap();
                    let result = tokio::select! {
                        () = killed.requested() => Err(GitError::Killed),
                        _ = released => Ok(Output { stdout: Vec::new(), stderr: Vec::new() }),
                    };
                    gates.held.fetch_sub(1, Ordering::SeqCst);
                    result
                }
            }
        })
    }
}

/// The test's side of the held runs.
struct Control {
    started: mpsc::UnboundedReceiver<String>,
    releases: HashMap<String, oneshot::Sender<()>>,
    gates: Gates,
}

impl Control {
    async fn started(&mut self) -> String {
        tokio::time::timeout(Duration::from_secs(60), self.started.recv())
            .await
            .expect("no held run started")
            .unwrap()
    }

    fn release(&mut self, name: &str) {
        self.releases.remove(name).unwrap().send(()).unwrap();
    }

    fn held(&self) -> usize {
        self.gates.held.load(Ordering::SeqCst)
    }

    fn most(&self) -> usize {
        self.gates.most.load(Ordering::SeqCst)
    }

    fn ran(&self) -> Vec<String> {
        self.gates.ran.lock().unwrap().clone()
    }
}

/// A workspace whose repos are `names`, in a group "All" in that order,
/// and the engines.
struct Setup {
    _dirs: Dirs,
    fixture: Fixture,
    workspaces: Arc<Workspaces>,
    settings: Arc<SettingsStore>,
    jobs: Arc<Jobs>,
    groups: Arc<GroupOps>,
    ended: Arc<Ended>,
    ready: tokio::sync::Mutex<mpsc::UnboundedReceiver<PreflightReady>>,
    id: WorkspaceId,
    target: TargetSet,
}

fn git() -> GitRunner {
    GitRunner::new(PathBuf::from("git"))
}

impl Setup {
    async fn new(names: &[&str]) -> Self {
        let fixture = Fixture::new().unwrap();
        for name in names {
            let repo = fixture.repo(name).unwrap();
            repo.commit_file("file.txt", "base\n", "Base").unwrap();
            repo.branch("feature").unwrap();
            repo.switch("feature").unwrap();
            repo.commit_file("file.txt", "feature\n", "Feature")
                .unwrap();
            repo.switch("main").unwrap();
            repo.commit_file("file.txt", "main\n", "Main").unwrap();
        }
        let dirs = Dirs {
            root: tempfile::tempdir().unwrap(),
        };
        let settings = Arc::new(SettingsStore::load(&dirs));
        let workspaces = Arc::new(Workspaces::load_with(&dirs, settings.clone()));
        let id = workspaces.views()[0].id.clone();
        add_path(
            workspaces.clone(),
            id.clone(),
            fixture.path().to_owned(),
            git(),
            JobId(0),
            |_| {},
        )
        .await
        .unwrap();
        let view = workspaces.view(&id).unwrap();
        let ordered: Vec<RepoId> = names
            .iter()
            .map(|name| {
                view.repos
                    .iter()
                    .find(|repo| repo.name == *name)
                    .unwrap()
                    .id
                    .clone()
            })
            .collect();
        let group = workspaces.create_group(&id, "All", &ordered).unwrap();
        let summaries = Arc::new(Summaries::load(SummaryCache::none(), &workspaces));
        let ended = Arc::new(Ended::default());
        let jobs = Jobs::new(workspaces.clone(), summaries.clone(), ended.clone());
        let (ready_tx, ready_rx) = mpsc::unbounded_channel();
        let groups = GroupOps::new(
            workspaces.clone(),
            summaries,
            settings.clone(),
            jobs.clone(),
            move |ready: &PreflightReady| {
                let _ = ready_tx.send(ready.clone());
            },
        );
        Self {
            _dirs: dirs,
            fixture,
            workspaces,
            settings,
            jobs,
            groups,
            ended,
            ready: tokio::sync::Mutex::new(ready_rx),
            id,
            target: TargetSet::Group { group_id: group },
        }
    }

    fn repo(&self, name: &str) -> Repo {
        Repo::open(self.fixture.path().join(name))
    }

    fn repo_id(&self, name: &str) -> RepoId {
        let view = self.workspaces.view(&self.id).unwrap();
        view.repos
            .into_iter()
            .find(|repo| repo.name == name)
            .unwrap()
            .id
    }

    /// Runs a preflight of `op` on the group and waits for its result.
    async fn preflight(&self, op: TestOp) -> PreflightReady {
        let id = self
            .groups
            .preflight(git(), &self.id, &self.target, Arc::new(op))
            .unwrap();
        // Each test runs one preflight at a time.
        let mut receiver = self.ready.lock().await;
        let ready = tokio::time::timeout(Duration::from_secs(60), receiver.recv())
            .await
            .expect("the preflight never finished")
            .unwrap();
        assert_eq!(ready.preflight_id, id);
        ready
    }

    /// Starts preflight `id`, confirmed.
    fn start(&self, id: PreflightId) -> JobId {
        self.groups.start(git(), id, true).unwrap()
    }

    /// Waits until repo `repo` was read again after a write.
    async fn reread(&self, repo: &RepoId) {
        tokio::time::timeout(Duration::from_secs(60), async {
            loop {
                let notified = self.ended.1.notified();
                if self.ended.2.lock().unwrap().contains(repo) {
                    return;
                }
                notified.await;
            }
        })
        .await
        .expect("the repo was never read again");
    }

    /// Waits for job `id`'s end.
    async fn ended(&self, id: JobId) -> JobView {
        tokio::time::timeout(Duration::from_secs(60), async {
            loop {
                let notified = self.ended.1.notified();
                if let Some(view) = self.ended.0.lock().unwrap().get(&id).cloned() {
                    return view;
                }
                notified.await;
            }
        })
        .await
        .expect("the job never ended")
    }

    fn view(&self, id: JobId) -> JobView {
        self.jobs
            .views(&self.id)
            .into_iter()
            .find(|view| view.job_id == id)
            .unwrap()
    }

    /// Runs `op` on the group, confirmed, and returns its report as
    /// (repo name, outcome).
    async fn run(&self, op: TestOp) -> (JobView, Vec<(String, Outcome)>) {
        let ready = self.preflight(op).await;
        let view = self.ended(self.start(ready.preflight_id)).await;
        let report = outcomes(&view);
        (view, report)
    }

    fn set_parallelism(&self, parallelism: u32) {
        self.settings
            .set_groups(GroupSettings {
                group_parallelism: parallelism,
                always_show_preflight: false,
            })
            .unwrap();
    }
}

fn outcomes(view: &JobView) -> Vec<(String, Outcome)> {
    view.report
        .as_ref()
        .expect("the job has no report")
        .iter()
        .map(|line| (line.repo_name.clone(), line.outcome.clone()))
        .collect()
}

fn classes(ready: &PreflightReady) -> Vec<(String, Class)> {
    ready
        .repos
        .iter()
        .map(|repo| (repo.repo_name.clone(), repo.class.clone()))
        .collect()
}

fn blocked(reason: &str) -> Outcome {
    Outcome::Skipped {
        reason: SkipReason::Blocked {
            reason: reason.to_owned(),
        },
    }
}

fn cancelled() -> Outcome {
    Outcome::Skipped {
        reason: SkipReason::Cancelled,
    }
}

const BUSY: &str = "another operation is running on the repo";
const MISSING: &str = "the repo's folder is missing";
const IN_CONFLICT: &str = "the repo is in conflict state; continue or abort first";

#[tokio::test]
async fn grp_6_one_failure_does_not_stop_others() {
    let setup = Setup::new(&["a", "b", "c"]).await;
    let (op, control) = TestOp::new(&[("a", Act::Fail), ("b", Act::Succeed), ("c", Act::Succeed)]);
    let (view, report) = setup.run(op).await;
    assert_eq!(
        report,
        [
            ("a".to_owned(), Outcome::Failed),
            ("b".to_owned(), Outcome::Succeeded),
            ("c".to_owned(), Outcome::Succeeded),
        ]
    );
    let mut ran = control.ran();
    ran.sort();
    assert_eq!(ran, ["a", "b", "c"]);
    // Nothing is reverted: b and c keep the branch they made.
    for name in ["b", "c"] {
        assert!(setup.repo(name).rev_parse("made").is_ok());
    }
    // The job failed, which the status bar shows (UI-21), with git's output
    // in the failed repo's line.
    assert_eq!(view.outcome, Some(Outcome::Failed));
    assert_eq!(view.message.as_deref(), Some("1 of 3 repos failed"));
    let line = &view.report.as_ref().unwrap()[0];
    assert_eq!(line.output, "fatal: refused\n");
    assert!(line.message.is_some());
}

#[tokio::test]
async fn grp_7_every_repo_gets_exactly_one_outcome() {
    let names = ["ok", "same", "skip", "bad", "clash", "gone"];
    let setup = Setup::new(&names).await;
    let (op, control) = TestOp::new(&[
        ("ok", Act::Succeed),
        ("same", Act::Nothing),
        ("bad", Act::Fail),
        ("clash", Act::Conflict),
    ]);
    let op = op.class(
        "skip",
        Class::WillSkip {
            reason: "no remote".to_owned(),
        },
    );
    std::fs::remove_dir_all(setup.fixture.path().join("gone")).unwrap();
    let (view, report) = setup.run(op).await;
    assert_eq!(
        report,
        [
            ("ok".to_owned(), Outcome::Succeeded),
            ("same".to_owned(), Outcome::NothingToDo),
            (
                "skip".to_owned(),
                Outcome::Skipped {
                    reason: SkipReason::Preflight {
                        reason: "no remote".to_owned()
                    }
                }
            ),
            ("bad".to_owned(), Outcome::Failed),
            ("clash".to_owned(), Outcome::Conflict),
            ("gone".to_owned(), blocked(MISSING)),
        ]
    );
    // Skipped repos run nothing and have no sub-job.
    let mut ran = control.ran();
    ran.sort();
    assert_eq!(ran, ["bad", "clash", "ok", "same"]);
    assert_eq!(
        view.sub_jobs
            .iter()
            .map(|sub| sub.repo_name.as_str())
            .collect::<Vec<_>>(),
        ["ok", "same", "bad", "clash"]
    );
    assert!(
        view.sub_jobs
            .iter()
            .all(|sub| sub.state == JobState::Ended && sub.outcome.is_some())
    );
    assert_eq!((view.done, view.total), (4, 4));
    assert_eq!(view.repos.len(), names.len());
    assert_eq!(view.label, "Test · All (6 repos)");
    // Conflict is not a failure, but a failure wins the job's outcome.
    assert_eq!(view.outcome, Some(Outcome::Failed));
}

#[tokio::test]
async fn grp_8_missing_conflict_and_busy_repos_are_blocked_and_fetch_is_allowed_in_conflict() {
    let setup = Setup::new(&["ok", "gone", "clash", "busy"]).await;
    std::fs::remove_dir_all(setup.fixture.path().join("gone")).unwrap();
    setup.repo("clash").merge_with_conflicts("feature").unwrap();
    let (release, released) = oneshot::channel::<()>();
    let held = setup
        .jobs
        .start(
            git(),
            WriteJob::single(setup.id.clone(), setup.repo_id("busy"), "Held".to_owned()),
            move |_run| async move {
                let _ = released.await;
                Ok(Output {
                    stdout: Vec::new(),
                    stderr: Vec::new(),
                })
            },
        )
        .unwrap();

    let (op, _control) = TestOp::new(&[]);
    let ready = setup.preflight(op).await;
    let blocked = |reason: &str| Class::Blocked {
        reason: reason.to_owned(),
    };
    assert_eq!(
        classes(&ready),
        [
            ("ok".to_owned(), Class::WillRun),
            ("gone".to_owned(), blocked(MISSING)),
            ("clash".to_owned(), blocked(IN_CONFLICT)),
            ("busy".to_owned(), blocked(BUSY)),
        ]
    );
    assert!(ready.needs_confirmation);

    // Fetch may run in conflict state.
    let (mut op, _control) = TestOp::new(&[]);
    op.conflict_allowed = true;
    let ready = setup.preflight(op).await;
    assert_eq!(ready.repos[2].class, Class::WillRun);
    assert_eq!(ready.repos[3].class, blocked(BUSY));

    release.send(()).unwrap();
    held.ended.await.unwrap();
}

#[tokio::test]
async fn grp_8_a_repo_blocked_after_preflight_is_skipped_at_run() {
    let setup = Setup::new(&["ok", "busy", "clash", "gone"]).await;
    let (op, control) = TestOp::new(&[]);
    let ready = setup.preflight(op).await;
    assert!(ready.repos.iter().all(|repo| repo.class == Class::WillRun));

    // Meanwhile: a write starts on busy, clash stops in conflict, and gone's
    // folder goes.
    let (release, released) = oneshot::channel::<()>();
    let held = setup
        .jobs
        .start(
            git(),
            WriteJob::single(setup.id.clone(), setup.repo_id("busy"), "Held".to_owned()),
            move |_run| async move {
                let _ = released.await;
                Ok(Output {
                    stdout: Vec::new(),
                    stderr: Vec::new(),
                })
            },
        )
        .unwrap();
    setup.repo("clash").merge_with_conflicts("feature").unwrap();
    std::fs::remove_dir_all(setup.fixture.path().join("gone")).unwrap();

    let view = setup.ended(setup.start(ready.preflight_id)).await;
    assert_eq!(
        outcomes(&view),
        [
            ("ok".to_owned(), Outcome::Succeeded),
            ("busy".to_owned(), blocked(BUSY)),
            ("clash".to_owned(), blocked(IN_CONFLICT)),
            ("gone".to_owned(), blocked(MISSING)),
        ]
    );
    // git did not run where the repo was blocked.
    assert_eq!(control.ran(), ["ok"]);

    // The held write still runs, and its repo is free again after it.
    release.send(()).unwrap();
    assert_eq!(held.ended.await.unwrap().outcome, Some(Outcome::Succeeded));
}

#[tokio::test]
async fn grp_10_cancel_skips_queued_kills_fetch_and_lets_others_finish() {
    let names = ["a", "b", "c", "d"];
    let setup = Setup::new(&names).await;
    setup.set_parallelism(2);
    let held: Vec<(&str, Act)> = names.iter().map(|name| (*name, Act::Hold)).collect();

    // A fetch-like operation: running sub-jobs are killed.
    let (mut op, mut control) = TestOp::new(&held);
    op.on_cancel = OnCancel::Kill;
    let ready = setup.preflight(op).await;
    let job = setup.start(ready.preflight_id);
    let mut running = vec![control.started().await, control.started().await];
    running.sort();
    assert_eq!(running, ["a", "b"]);
    setup.jobs.cancel(job);
    let view = setup.ended(job).await;
    assert_eq!(
        outcomes(&view),
        names.map(|name| (name.to_owned(), cancelled()))
    );
    let mut ran = control.ran();
    ran.sort();
    assert_eq!(ran, ["a", "b"], "queued sub-jobs never start");

    // Any other operation: running sub-jobs finish.
    let (op, mut control) = TestOp::new(&held);
    let ready = setup.preflight(op).await;
    let job = setup.start(ready.preflight_id);
    let mut running = vec![control.started().await, control.started().await];
    running.sort();
    assert_eq!(running, ["a", "b"]);
    setup.jobs.cancel(job);
    assert_eq!(setup.view(job).state, JobState::Running);
    control.release("a");
    control.release("b");
    let view = setup.ended(job).await;
    assert_eq!(
        outcomes(&view),
        [
            ("a".to_owned(), Outcome::NothingToDo),
            ("b".to_owned(), Outcome::NothingToDo),
            ("c".to_owned(), cancelled()),
            ("d".to_owned(), cancelled()),
        ]
    );
    let mut ran = control.ran();
    ran.sort();
    assert_eq!(ran, ["a", "b"]);
}

#[tokio::test]
async fn group_parallelism_bounds_running_sub_jobs() {
    let names = ["a", "b", "c", "d", "e"];
    let setup = Setup::new(&names).await;
    let held: Vec<(&str, Act)> = names.iter().map(|name| (*name, Act::Hold)).collect();
    for parallelism in [1, 3] {
        setup.set_parallelism(parallelism);
        let (op, mut control) = TestOp::new(&held);
        let ready = setup.preflight(op).await;
        let job = setup.start(ready.preflight_id);
        let at_once = parallelism as usize;
        let mut started: Vec<String> = Vec::new();
        for _ in 0..at_once {
            started.push(control.started().await);
        }
        // Full: the next sub-job waits for a running one to end.
        assert_eq!(control.held(), at_once);
        let queued = setup
            .view(job)
            .sub_jobs
            .iter()
            .filter(|sub| sub.state == JobState::Queued)
            .count();
        assert_eq!(queued, names.len() - at_once);
        while started.len() < names.len() {
            let first = started
                .iter()
                .find(|name| control.releases.contains_key(*name))
                .unwrap()
                .clone();
            control.release(&first);
            started.push(control.started().await);
        }
        for name in names {
            if control.releases.contains_key(name) {
                control.release(name);
            }
        }
        setup.ended(job).await;
        assert_eq!(control.most(), at_once, "parallelism {parallelism}");
        // Sub-jobs start in target-set order: the first ones together, then
        // one as each running one ends.
        let mut first = started[..at_once].to_vec();
        first.sort();
        assert_eq!(first, names[..at_once]);
        assert_eq!(started[at_once..], names[at_once..]);
    }
}

#[tokio::test]
async fn group_confirmation_rule() {
    let setup = Setup::new(&["a", "b"]).await;

    // Every repo will run: no confirmation.
    let (op, _control) = TestOp::new(&[]);
    let ready = setup.preflight(op).await;
    assert!(!ready.needs_confirmation);
    let job = setup
        .groups
        .start(git(), ready.preflight_id, false)
        .unwrap();
    setup.ended(job).await;

    // One will skip: confirmation, and the preflight stays until it comes.
    let (op, _control) = TestOp::new(&[]);
    let op = op.class(
        "b",
        Class::WillSkip {
            reason: "no upstream".to_owned(),
        },
    );
    let ready = setup.preflight(op).await;
    assert!(ready.needs_confirmation);
    assert!(matches!(
        setup.groups.start(git(), ready.preflight_id, false),
        Err(GroupError::NeedsConfirmation)
    ));
    setup.ended(setup.start(ready.preflight_id)).await;

    // Needs attention counts as not *will run* too.
    let (op, _control) = TestOp::new(&[]);
    let op = op.class(
        "a",
        Class::NeedsAttention {
            reason: "no upstream; will set origin/main".to_owned(),
        },
    );
    assert!(setup.preflight(op).await.needs_confirmation);

    // A destructive operation always asks (GRP-9).
    let (mut op, _control) = TestOp::new(&[]);
    op.destructive = true;
    let ready = setup.preflight(op).await;
    assert!(ready.needs_confirmation);
    assert!(ready.destructive);

    // So does every operation with the setting on.
    setup
        .settings
        .set_groups(GroupSettings {
            group_parallelism: 8,
            always_show_preflight: true,
        })
        .unwrap();
    let (op, _control) = TestOp::new(&[]);
    assert!(setup.preflight(op).await.needs_confirmation);
}

#[tokio::test]
async fn group_preflight_is_used_once_and_not_after_the_workspace_changed() {
    let setup = Setup::new(&["a", "b"]).await;
    let (op, _control) = TestOp::new(&[]);
    let ready = setup.preflight(op).await;
    setup.ended(setup.start(ready.preflight_id)).await;
    assert!(matches!(
        setup.groups.start(git(), ready.preflight_id, true),
        Err(GroupError::UnknownPreflight)
    ));

    let (op, control) = TestOp::new(&[]);
    let ready = setup.preflight(op).await;
    setup
        .workspaces
        .remove_from_group(
            &setup.id,
            &setup.workspaces.groups(&setup.id).unwrap()[0].id,
            &[setup.repo_id("b")],
        )
        .unwrap();
    assert!(matches!(
        setup.groups.start(git(), ready.preflight_id, true),
        Err(GroupError::WorkspaceChanged)
    ));
    assert!(control.ran().is_empty());
}

#[tokio::test]
async fn group_ops_are_not_available_until_their_task() {
    let op = GroupOp::Fetch {
        prune: Prune::Config,
    };
    assert!(matches!(op.operation(), Err(GroupError::NotAvailable(name)) if name == "Fetch"));
}

#[tokio::test]
async fn ops_7_a_repo_is_read_again_when_its_sub_job_ends() {
    let setup = Setup::new(&["a", "b"]).await;
    let (op, mut control) = TestOp::new(&[("a", Act::Hold), ("b", Act::Hold)]);
    let ready = setup.preflight(op).await;
    let job = setup.start(ready.preflight_id);
    control.started().await;
    control.started().await;
    control.release("a");
    setup.reread(&setup.repo_id("a")).await;
    // b still runs: a was not left for the job's end.
    assert_eq!(setup.view(job).state, JobState::Running);
    assert!(!setup.ended.2.lock().unwrap().contains(&setup.repo_id("b")));
    control.release("b");
    setup.ended(job).await;
    assert!(setup.ended.2.lock().unwrap().contains(&setup.repo_id("b")));
}
