//! Fetch, pull and push (section 6 "Sync", UI-20, GRP-8, GRP-9, GRP-10,
//! GRP-12, OPS-1, OPS-7) with real git, each as a job. The remote is a
//! bare repo on disk reached over `file://`; a second clone of it stands
//! in for someone else pushing. Nothing here waits for a time: each test
//! waits for the job's end event, and the cancel test for the fake ssh to
//! start.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use gitcroc_core::{
    CancelResult, ConfigDir, GitRunner, JobEvents, JobId, JobView, Jobs, Outcome, Prune,
    PullStrategy, PushChoices, RepoId, SkipReason, Summaries, SummaryCache, SummaryView, SyncError,
    SyncOp, SyncRequest, SyncStart, WorkspaceId, Workspaces, add_path, op_sync, push_choices,
};
use repo_gen::{Fixture, Repo};
use tokio::sync::mpsc;

struct Dirs {
    root: tempfile::TempDir,
}

/// git with no user or system config, so no hook or setting of the machine
/// takes part, and an identity for merge and rebase commits.
fn git(dirs: &Dirs) -> GitRunner {
    let home = dirs.root.path().join("home");
    GitRunner::new(PathBuf::from("git")).with_env([
        ("HOME", home.clone().into_os_string()),
        ("XDG_CONFIG_HOME", home.into_os_string()),
        ("GIT_CONFIG_NOSYSTEM", "1".into()),
        ("GIT_AUTHOR_NAME", "Test".into()),
        ("GIT_AUTHOR_EMAIL", "test@example.com".into()),
        ("GIT_COMMITTER_NAME", "Test".into()),
        ("GIT_COMMITTER_EMAIL", "test@example.com".into()),
    ])
}

impl ConfigDir for Dirs {
    fn config_dir(&self) -> Option<PathBuf> {
        Some(self.root.path().join("config"))
    }
}

/// Sends each job's end to the test and keeps its progress and the
/// summaries sent.
struct Events {
    ends: mpsc::UnboundedSender<JobView>,
    progress: Mutex<Vec<JobView>>,
    summaries: Mutex<Vec<SummaryView>>,
}

impl JobEvents for Events {
    fn progress(&self, job: &JobView) {
        self.progress.lock().unwrap().push(job.clone());
    }

    fn ended(&self, job: &JobView) {
        let _ = self.ends.send(job.clone());
    }

    fn summary(&self, view: SummaryView, read: bool) {
        if read {
            self.summaries.lock().unwrap().push(view);
        }
    }
}

/// A bare remote with `main` at one commit, and two clones of it over
/// `file://`: `app`, the repo the app works on, and `other`.
struct World {
    fixture: Fixture,
    bare: PathBuf,
    app: Repo,
    other: Repo,
}

fn file_url(path: &Path) -> String {
    let path = path.to_string_lossy().replace('\\', "/");
    if path.starts_with('/') {
        format!("file://{path}")
    } else {
        format!("file:///{path}")
    }
}

impl World {
    fn new() -> Self {
        let fixture = Fixture::new().unwrap();
        let seed = fixture.repo("seed").unwrap();
        seed.commit_file("a.txt", "a\n", "First").unwrap();
        let bare = fixture.path().join("origin.git");
        let mut args: Vec<std::ffi::OsString> = vec!["clone".into(), "--bare".into()];
        args.extend(["--quiet".into(), "--".into()]);
        args.extend([seed.path().into(), bare.clone().into()]);
        seed.git(args).unwrap();
        let app = Self::clone(&seed, &bare, fixture.path().join("app"));
        let other = Self::clone(&seed, &bare, fixture.path().join("other"));
        Self {
            fixture,
            bare,
            app,
            other,
        }
    }

    fn clone(via: &Repo, bare: &Path, path: PathBuf) -> Repo {
        let mut args: Vec<std::ffi::OsString> = vec!["clone".into(), "--quiet".into(), "--".into()];
        args.extend([file_url(bare).into(), path.clone().into()]);
        via.git(args).unwrap();
        Repo::open(path)
    }

    /// `other` commits `file` and pushes it to `main`.
    fn other_pushes(&self, file: &str) -> String {
        self.other.git(["pull", "--quiet", "--ff-only"]).unwrap();
        self.other
            .commit_file(file, format!("{file} by other\n"), &format!("Other {file}"))
            .unwrap();
        self.other
            .git(["push", "--quiet", "origin", "main"])
            .unwrap();
        self.other.head().unwrap()
    }

    /// The commit branch `branch` of the remote points to.
    fn remote(&self, branch: &str) -> Option<String> {
        let mut args: Vec<std::ffi::OsString> = vec!["--git-dir".into(), self.bare.clone().into()];
        args.extend(["rev-parse".into(), "--verify".into(), "--quiet".into()]);
        args.push(format!("refs/heads/{branch}").into());
        let output = self.other.git_output(args).unwrap();
        output
            .status
            .success()
            .then(|| String::from_utf8_lossy(&output.stdout).trim().to_owned())
    }
}

/// A workspace with `app` and the job engine.
struct Run {
    dirs: Dirs,
    workspaces: Arc<Workspaces>,
    summaries: Arc<Summaries>,
    jobs: Arc<Jobs>,
    events: Arc<Events>,
    ends: mpsc::UnboundedReceiver<JobView>,
    id: WorkspaceId,
    repo_id: RepoId,
}

impl Run {
    async fn new(repo: &Repo) -> Self {
        let dirs = Dirs {
            root: tempfile::tempdir().unwrap(),
        };
        std::fs::create_dir_all(dirs.root.path().join("home")).unwrap();
        let workspaces = Arc::new(Workspaces::load(&dirs));
        let id = workspaces.views()[0].id.clone();
        let summaries = Arc::new(Summaries::load(SummaryCache::none(), &workspaces));
        let (sender, ends) = mpsc::unbounded_channel();
        let events = Arc::new(Events {
            ends: sender,
            progress: Mutex::default(),
            summaries: Mutex::default(),
        });
        let jobs = Jobs::new(workspaces.clone(), summaries.clone(), events.clone());
        add_path(
            workspaces.clone(),
            id.clone(),
            repo.path().to_owned(),
            git(&dirs),
            JobId(0),
            |_| {},
        )
        .await
        .unwrap();
        let repo_id = workspaces.view(&id).unwrap().repos[0].id.clone();
        Self {
            dirs,
            workspaces,
            summaries,
            jobs,
            events,
            ends,
            id,
            repo_id,
        }
    }

    async fn ask(&self, op: SyncOp, confirmed: bool) -> Result<SyncStart, SyncError> {
        op_sync(
            self.workspaces.clone(),
            self.summaries.clone(),
            self.jobs.clone(),
            git(&self.dirs),
            self.id.clone(),
            SyncRequest {
                repo_id: self.repo_id.clone(),
                op,
                confirmed,
            },
        )
        .await
    }

    /// Runs `op`, confirmed, to its job's end.
    async fn run(&mut self, op: SyncOp) -> JobView {
        let start = self.ask(op, true).await.unwrap();
        let SyncStart::Started { job_id } = start else {
            panic!("not started: {start:?}");
        };
        let ended = self.ends.recv().await.unwrap();
        assert_eq!(ended.job_id, job_id);
        ended
    }

    async fn choices(&self) -> PushChoices {
        push_choices(
            self.workspaces.clone(),
            self.id.clone(),
            self.repo_id.clone(),
        )
        .await
        .unwrap()
    }

    /// The last summary a job's re-read sent (OPS-7).
    fn last_summary(&self) -> SummaryView {
        self.events
            .summaries
            .lock()
            .unwrap()
            .last()
            .cloned()
            .unwrap()
    }
}

fn refused(result: Result<SyncStart, SyncError>) -> String {
    match result {
        Err(error @ SyncError::Refused(_)) => error.to_string(),
        other => panic!("not refused: {other:?}"),
    }
}

fn fetch() -> SyncOp {
    SyncOp::Fetch {
        prune: Prune::Config,
    }
}

fn pull(strategy: PullStrategy) -> SyncOp {
    SyncOp::Pull { strategy }
}

fn push() -> SyncOp {
    SyncOp::Push {
        set_upstream: true,
        remote: None,
        force_with_lease: false,
    }
}

fn succeeded(view: &JobView) {
    assert_eq!(view.outcome, Some(Outcome::Succeeded), "{view:#?}");
}

fn failed(view: &JobView) {
    assert_eq!(view.outcome, Some(Outcome::Failed), "{view:#?}");
}

fn nothing_to_do(view: &JobView) {
    assert_eq!(view.outcome, Some(Outcome::NothingToDo), "{view:#?}");
}

fn rev(repo: &Repo, revision: &str) -> String {
    repo.rev_parse(revision).unwrap()
}

fn has_ref(repo: &Repo, name: &str) -> bool {
    repo.git_output(["rev-parse", "--verify", "--quiet", name])
        .unwrap()
        .status
        .success()
}

/// `app` and the remote both have a commit the other lacks; `app` has not
/// fetched the remote's.
fn diverged(world: &World) {
    world.other_pushes("theirs.txt");
    world.app.commit_file("ours.txt", "ours\n", "Ours").unwrap();
}

/// HEAD's parents.
fn parents(repo: &Repo) -> usize {
    repo.git(["rev-list", "--parents", "-n", "1", "HEAD"])
        .unwrap()
        .split_whitespace()
        .count()
        - 1
}

#[tokio::test]
async fn fetch_brings_new_commits_with_progress_then_nothing_to_do() {
    let world = World::new();
    let mut run = Run::new(&world.app).await;
    let theirs = world.other_pushes("b.txt");

    let ended = run.run(fetch()).await;
    succeeded(&ended);
    assert_eq!(ended.label, "Fetch all remotes");
    assert_eq!(rev(&world.app, "origin/main"), theirs);
    // The transfer's phases became the job's steps.
    let steps: Vec<String> = run
        .events
        .progress
        .lock()
        .unwrap()
        .iter()
        .filter(|view| view.job_id == ended.job_id)
        .filter_map(|view| view.step.clone())
        .collect();
    assert!(!steps.is_empty(), "no progress steps");
    // OPS-7: behind by one after the job's re-read.
    let upstream = run.last_summary().summary.unwrap().upstream.unwrap();
    assert_eq!((upstream.ahead, upstream.behind), (0, 1));

    nothing_to_do(&run.run(fetch()).await);
}

#[tokio::test]
async fn fetch_prune_follows_git_config_unless_picked() {
    let world = World::new();
    world
        .other
        .git(["push", "--quiet", "origin", "main:gone"])
        .unwrap();
    let mut run = Run::new(&world.app).await;
    succeeded(&run.run(fetch()).await);
    assert!(has_ref(&world.app, "refs/remotes/origin/gone"));
    world
        .other
        .git(["push", "--quiet", "origin", "--delete", "gone"])
        .unwrap();

    // No `fetch.prune`: kept.
    nothing_to_do(&run.run(fetch()).await);
    assert!(has_ref(&world.app, "refs/remotes/origin/gone"));
    // `fetch.prune` true, but the user picked no prune.
    world.app.git(["config", "fetch.prune", "true"]).unwrap();
    let keep = SyncOp::Fetch {
        prune: Prune::NoPrune,
    };
    nothing_to_do(&run.run(keep).await);
    assert!(has_ref(&world.app, "refs/remotes/origin/gone"));
    // As the config says.
    succeeded(&run.run(fetch()).await);
    assert!(!has_ref(&world.app, "refs/remotes/origin/gone"));

    // The user picked prune, against the config.
    world
        .other
        .git(["push", "--quiet", "origin", "main:gone"])
        .unwrap();
    world.app.git(["config", "fetch.prune", "false"]).unwrap();
    succeeded(&run.run(fetch()).await);
    world
        .other
        .git(["push", "--quiet", "origin", "--delete", "gone"])
        .unwrap();
    let prune = SyncOp::Fetch {
        prune: Prune::Prune,
    };
    succeeded(&run.run(prune).await);
    assert!(!has_ref(&world.app, "refs/remotes/origin/gone"));
}

#[tokio::test]
async fn fetch_fails_on_an_unreachable_remote() {
    let world = World::new();
    let missing = file_url(&world.fixture.path().join("missing.git"));
    world
        .app
        .git(["remote", "set-url", "origin", &missing])
        .unwrap();
    let mut run = Run::new(&world.app).await;
    let ended = run.run(fetch()).await;
    failed(&ended);
    assert!(
        ended.output.contains("missing.git"),
        "git's message: {}",
        ended.output
    );
}

#[tokio::test]
async fn grp_8_fetch_is_allowed_in_conflict_state() {
    let world = World::new();
    let app = &world.app;
    app.git(["switch", "--quiet", "-c", "side"]).unwrap();
    app.commit_file("a.txt", "side\n", "Side").unwrap();
    app.switch("main").unwrap();
    app.commit_file("a.txt", "main\n", "Main").unwrap();
    app.merge_with_conflicts("side").unwrap();
    let theirs = world.other_pushes("b.txt");
    let mut run = Run::new(app).await;

    succeeded(&run.run(fetch()).await);
    assert_eq!(rev(app, "origin/main"), theirs);
    let blocked = "the repo is in conflict state; continue or abort first";
    assert_eq!(
        refused(run.ask(pull(PullStrategy::Config), false).await),
        blocked
    );
    assert_eq!(refused(run.ask(push(), false).await), blocked);
}

#[tokio::test]
async fn sync_is_refused_with_the_reason() {
    let world = World::new();
    let app = &world.app;
    let run = Run::new(app).await;

    // Detached HEAD.
    app.git(["switch", "--quiet", "--detach"]).unwrap();
    assert_eq!(
        refused(run.ask(pull(PullStrategy::Config), false).await),
        "HEAD is detached; check out a branch to pull"
    );
    assert_eq!(
        refused(run.ask(push(), false).await),
        "HEAD is detached; check out a branch to push"
    );

    // No upstream.
    app.git(["switch", "--quiet", "-c", "topic"]).unwrap();
    assert_eq!(
        refused(run.ask(pull(PullStrategy::Config), false).await),
        "topic has no upstream; set one to pull"
    );

    // No remote.
    app.git(["remote", "remove", "origin"]).unwrap();
    assert_eq!(
        refused(run.ask(fetch(), false).await),
        "the repo has no remote"
    );
    assert_eq!(
        refused(run.ask(push(), false).await),
        "the repo has no remote"
    );
    assert!(run.jobs.all_views().is_empty());
}

#[tokio::test]
async fn pull_fast_forwards_then_nothing_to_do() {
    let world = World::new();
    let mut run = Run::new(&world.app).await;
    let theirs = world.other_pushes("b.txt");

    let ended = run.run(pull(PullStrategy::Config)).await;
    succeeded(&ended);
    assert_eq!(ended.label, "Pull origin/main into main");
    assert_eq!(world.app.head().unwrap(), theirs);
    let upstream = run.last_summary().summary.unwrap().upstream.unwrap();
    assert_eq!((upstream.ahead, upstream.behind), (0, 0));

    nothing_to_do(&run.run(pull(PullStrategy::Config)).await);
}

#[tokio::test]
async fn pull_ff_only_is_failed_on_diverged_branches() {
    let world = World::new();
    diverged(&world);
    let ours = world.app.head().unwrap();
    let mut run = Run::new(&world.app).await;

    failed(&run.run(pull(PullStrategy::FfOnly)).await);
    assert_eq!(world.app.head().unwrap(), ours);
}

#[tokio::test]
async fn pull_merges_or_rebases_diverged_branches() {
    let world = World::new();
    diverged(&world);
    let mut run = Run::new(&world.app).await;
    succeeded(&run.run(pull(PullStrategy::Merge)).await);
    assert_eq!(parents(&world.app), 2);
    assert_eq!(rev(&world.app, "HEAD^2"), rev(&world.app, "origin/main"));
    world
        .app
        .git(["push", "--quiet", "origin", "main"])
        .unwrap();

    world.other_pushes("c.txt");
    world.app.commit_file("d.txt", "d\n", "Ours again").unwrap();
    succeeded(&run.run(pull(PullStrategy::Rebase)).await);
    assert_eq!(parents(&world.app), 1);
    assert_eq!(rev(&world.app, "HEAD~1"), rev(&world.app, "origin/main"));
    assert!(world.app.path().join("d.txt").exists());
}

#[tokio::test]
async fn pull_follows_the_git_config_default() {
    let world = World::new();
    diverged(&world);
    world.app.git(["config", "pull.rebase", "true"]).unwrap();
    let mut run = Run::new(&world.app).await;
    succeeded(&run.run(pull(PullStrategy::Config)).await);
    assert_eq!(parents(&world.app), 1);
    assert_eq!(rev(&world.app, "HEAD~1"), rev(&world.app, "origin/main"));

    world
        .app
        .git(["push", "--quiet", "origin", "main"])
        .unwrap();
    world.other_pushes("c.txt");
    world.app.commit_file("d.txt", "d\n", "Ours again").unwrap();
    world.app.git(["config", "pull.rebase", "false"]).unwrap();
    succeeded(&run.run(pull(PullStrategy::Config)).await);
    assert_eq!(parents(&world.app), 2);
}

#[tokio::test]
async fn pull_is_ff_only_when_git_config_is_unset() {
    let world = World::new();
    diverged(&world);
    let ours = world.app.head().unwrap();
    let mut run = Run::new(&world.app).await;
    failed(&run.run(pull(PullStrategy::Config)).await);
    assert_eq!(world.app.head().unwrap(), ours);
    assert_eq!(parents(&world.app), 1);
}

/// The remote has a change of `a.txt` that `app` has not fetched.
fn they_change_a(world: &World) {
    world
        .other
        .commit_file("a.txt", "theirs\n", "Theirs")
        .unwrap();
    world
        .other
        .git(["push", "--quiet", "origin", "main"])
        .unwrap();
}

/// `app` and the remote both changed `a.txt`.
fn conflicting(world: &World) {
    they_change_a(world);
    world.app.commit_file("a.txt", "ours\n", "Ours").unwrap();
}

#[tokio::test]
async fn ops_1_a_conflicting_merge_pull_ends_conflict() {
    let world = World::new();
    conflicting(&world);
    let mut run = Run::new(&world.app).await;
    let ended = run.run(pull(PullStrategy::Merge)).await;
    assert_eq!(ended.outcome, Some(Outcome::Conflict), "{ended:#?}");
    assert!(has_ref(&world.app, "MERGE_HEAD"));
}

#[tokio::test]
async fn ops_1_a_conflicting_rebase_pull_ends_conflict() {
    let world = World::new();
    conflicting(&world);
    let mut run = Run::new(&world.app).await;
    let ended = run.run(pull(PullStrategy::Rebase)).await;
    assert_eq!(ended.outcome, Some(Outcome::Conflict), "{ended:#?}");
    let git_dir = world.app.path().join(".git");
    assert!(git_dir.join("rebase-merge").is_dir() || git_dir.join("rebase-apply").is_dir());
}

#[tokio::test]
async fn grp_12_pull_never_stashes() {
    let world = World::new();
    they_change_a(&world);
    world.app.write("a.txt", "local edit\n").unwrap();
    let ours = world.app.head().unwrap();
    // Even when git config asks for it.
    world
        .app
        .git(["config", "rebase.autoStash", "true"])
        .unwrap();
    world
        .app
        .git(["config", "merge.autoStash", "true"])
        .unwrap();
    let mut run = Run::new(&world.app).await;

    for strategy in [
        PullStrategy::Config,
        PullStrategy::Merge,
        PullStrategy::Rebase,
    ] {
        let ended = run.run(pull(strategy)).await;
        failed(&ended);
        assert!(ended.message.is_some(), "git's message: {ended:#?}");
        assert_eq!(world.app.head().unwrap(), ours);
        assert_eq!(
            std::fs::read_to_string(world.app.path().join("a.txt")).unwrap(),
            "local edit\n"
        );
        assert_eq!(world.app.git(["stash", "list"]).unwrap(), "");
    }
}

#[tokio::test]
async fn push_to_the_upstream_then_nothing_to_do() {
    let world = World::new();
    world.app.commit_file("b.txt", "b\n", "Ours").unwrap();
    let mut run = Run::new(&world.app).await;
    assert_eq!(
        run.choices().await,
        PushChoices {
            branch: Some("main".to_owned()),
            upstream: Some("origin/main".to_owned()),
            remotes: vec!["origin".to_owned()],
            remote: Some("origin".to_owned()),
        }
    );

    let ended = run.run(push()).await;
    succeeded(&ended);
    assert_eq!(ended.label, "Push main to origin/main");
    assert_eq!(world.remote("main"), Some(world.app.head().unwrap()));
    let upstream = run.last_summary().summary.unwrap().upstream.unwrap();
    assert_eq!((upstream.ahead, upstream.behind), (0, 0));

    nothing_to_do(&run.run(push()).await);
}

#[tokio::test]
async fn push_sets_the_upstream_of_a_new_branch() {
    let world = World::new();
    let app = &world.app;
    app.git(["switch", "--quiet", "-c", "topic"]).unwrap();
    app.commit_file("t.txt", "t\n", "Topic").unwrap();
    let mut run = Run::new(app).await;
    assert_eq!(run.choices().await.upstream, None);

    let ended = run.run(push()).await;
    succeeded(&ended);
    assert_eq!(ended.label, "Push topic to origin/topic");
    assert_eq!(world.remote("topic"), Some(app.head().unwrap()));
    assert_eq!(
        app.git(["rev-parse", "--abbrev-ref", "topic@{upstream}"])
            .unwrap()
            .trim(),
        "origin/topic"
    );

    // Without set upstream: pushed, and still no upstream.
    app.git(["switch", "--quiet", "-c", "loose"]).unwrap();
    let no_upstream = SyncOp::Push {
        set_upstream: false,
        remote: None,
        force_with_lease: false,
    };
    succeeded(&run.run(no_upstream).await);
    assert_eq!(world.remote("loose"), Some(app.head().unwrap()));
    assert!(!has_ref(app, "loose@{upstream}"));
}

#[tokio::test]
async fn push_asks_which_remote_without_origin() {
    let world = World::new();
    let app = &world.app;
    let url = file_url(&world.bare);
    app.git(["remote", "rename", "origin", "b"]).unwrap();
    app.git(["remote", "add", "a", &url]).unwrap();
    app.git(["switch", "--quiet", "-c", "topic"]).unwrap();
    let mut run = Run::new(app).await;
    let choices = run.choices().await;
    assert_eq!(choices.remotes, ["a", "b"]);
    assert_eq!(choices.remote, None);

    assert_eq!(
        run.ask(push(), false).await.unwrap(),
        SyncStart::ChooseRemote {
            remotes: vec!["a".to_owned(), "b".to_owned()]
        }
    );
    let to_b = SyncOp::Push {
        set_upstream: true,
        remote: Some("b".to_owned()),
        force_with_lease: false,
    };
    let ended = run.run(to_b).await;
    succeeded(&ended);
    assert_eq!(ended.label, "Push topic to b/topic");
    let to_c = SyncOp::Push {
        set_upstream: true,
        remote: Some("c".to_owned()),
        force_with_lease: false,
    };
    app.git(["switch", "--quiet", "-c", "next"]).unwrap();
    assert_eq!(refused(run.ask(to_c, false).await), "there is no remote c");
}

#[tokio::test]
async fn push_rejected_as_not_a_fast_forward_is_failed() {
    let world = World::new();
    diverged(&world);
    let theirs = world.remote("main");
    let mut run = Run::new(&world.app).await;

    let ended = run.run(push()).await;
    failed(&ended);
    assert!(ended.output.contains("rejected"), "{}", ended.output);
    assert_eq!(world.remote("main"), theirs);
}

#[tokio::test]
async fn grp_9_force_with_lease_needs_confirmation() {
    let world = World::new();
    diverged(&world);
    world.app.git(["fetch", "--quiet"]).unwrap();
    let theirs = world.remote("main");
    let mut run = Run::new(&world.app).await;
    let force = SyncOp::Push {
        set_upstream: true,
        remote: None,
        force_with_lease: true,
    };

    let start = run.ask(force.clone(), false).await.unwrap();
    let SyncStart::Confirm { repo, actions } = start else {
        panic!("no confirmation: {start:?}");
    };
    assert_eq!(repo, "app");
    assert_eq!(actions.len(), 1);
    assert!(actions[0].starts_with("Replace main on the remote origin with main"));
    assert!(run.jobs.all_views().is_empty());
    assert_eq!(world.remote("main"), theirs);

    let ended = run.run(force).await;
    succeeded(&ended);
    assert_eq!(ended.label, "Force push main to origin/main");
    assert_eq!(world.remote("main"), Some(world.app.head().unwrap()));
}

/// A shell script git runs as ssh: it notes that it started, then reads
/// its input until git, which writes it, is gone.
fn fake_ssh(dir: &Path) -> (PathBuf, PathBuf) {
    let slash = |path: &Path| path.to_string_lossy().replace('\\', "/");
    let started = dir.join("ssh-started");
    let script = dir.join("fake-ssh.sh");
    std::fs::write(
        &script,
        format!("echo started > '{}'\ncat > /dev/null\n", slash(&started)),
    )
    .unwrap();
    (script, started)
}

#[tokio::test]
async fn grp_10_cancel_kills_a_running_fetch() {
    let world = World::new();
    let (script, started) = fake_ssh(world.fixture.path());
    let app = &world.app;
    app.git([
        "remote",
        "set-url",
        "origin",
        "ssh://gitcroc.invalid/repo.git",
    ])
    .unwrap();
    let command = format!("sh '{}'", script.to_string_lossy().replace('\\', "/"));
    app.git(["config", "core.sshCommand", &command]).unwrap();
    app.git(["config", "ssh.variant", "simple"]).unwrap();
    let mut run = Run::new(app).await;

    let SyncStart::Started { job_id } = run.ask(fetch(), false).await.unwrap() else {
        panic!("not started");
    };
    // Waits for the fake ssh, or fails when the job ends first.
    let ssh_runs = async {
        // Polls a file: the wait ends when it appears, however long that takes.
        while !started.exists() {
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
    };
    tokio::select! {
        ended = run.ends.recv() => panic!("ended before ssh ran: {ended:#?}"),
        () = ssh_runs => {}
    }
    assert_eq!(run.jobs.cancel(job_id), CancelResult::Killing);
    let ended = run.ends.recv().await.unwrap();
    assert_eq!(ended.job_id, job_id);
    assert_eq!(
        ended.outcome,
        Some(Outcome::Skipped {
            reason: SkipReason::Cancelled
        })
    );

    // A push runs to its end.
    app.git(["remote", "set-url", "origin", &file_url(&world.bare)])
        .unwrap();
    app.commit_file("b.txt", "b\n", "Ours").unwrap();
    let SyncStart::Started { job_id } = run.ask(push(), false).await.unwrap() else {
        panic!("not started");
    };
    let cancel = run.jobs.cancel(job_id);
    let ended = run.ends.recv().await.unwrap();
    match cancel {
        CancelResult::Finishing => succeeded(&ended),
        // Cancelled before it started.
        CancelResult::Cancelled => assert_eq!(world.remote("main"), Some(rev(app, "HEAD~1"))),
        other => panic!("{other:?}"),
    }
}
