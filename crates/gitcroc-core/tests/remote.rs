//! Remotes: add, rename, remove and edit URLs (section 6 "Remotes", SEC-4,
//! GRP-8) with real git, each as a job. The remote is a bare repo on disk
//! reached over `file://`. Each test waits for the job's end event; nothing
//! waits for a time.

// Integration test crate: helpers outside #[test] fns are test code too (QA-4).
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::path::{Path, PathBuf};
use std::sync::Arc;

use gitcroc_core::{
    ConfigDir, GitRunner, JobEvents, JobId, JobView, Jobs, Outcome, RemoteError, RemoteOp,
    RemoteRequest, RemoteStart, RemoteUrlsView, RepoId, Summaries, SummaryCache, SummaryView,
    WorkspaceId, Workspaces, add_path, op_remote, remote_name_check, remote_urls,
};
use repo_gen::{Fixture, Repo};
use tokio::sync::mpsc;

struct Dirs {
    root: tempfile::TempDir,
}

/// git with no user or system config, so no hook or setting of the machine
/// takes part.
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

/// Sends each job's end to the test.
struct Events {
    ends: mpsc::UnboundedSender<JobView>,
}

impl JobEvents for Events {
    fn progress(&self, _job: &JobView) {}

    fn ended(&self, job: &JobView) {
        let _ = self.ends.send(job.clone());
    }

    fn summary(&self, _view: SummaryView, _read: bool) {}
}

/// A bare remote with `main` at two commits, and `app`, a clone of it over
/// `file://`, the repo the app works on.
struct World {
    _fixture: Fixture,
    bare: PathBuf,
    app: Repo,
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
        seed.commit_file("b.txt", "b\n", "Second").unwrap();
        let bare = fixture.path().join("origin.git");
        let mut args: Vec<std::ffi::OsString> = vec!["clone".into(), "--bare".into()];
        args.extend(["--quiet".into(), "--".into()]);
        args.extend([seed.path().into(), bare.clone().into()]);
        seed.git(args).unwrap();
        let app = Self::clone(&seed, &bare, fixture.path().join("app"));
        Self {
            _fixture: fixture,
            bare,
            app,
        }
    }

    fn clone(via: &Repo, bare: &Path, path: PathBuf) -> Repo {
        let mut args: Vec<std::ffi::OsString> = vec!["clone".into(), "--quiet".into(), "--".into()];
        args.extend([file_url(bare).into(), path.clone().into()]);
        via.git(args).unwrap();
        Repo::open(path)
    }
}

/// A workspace with one repo and the job engine.
struct Run {
    dirs: Dirs,
    workspaces: Arc<Workspaces>,
    summaries: Arc<Summaries>,
    jobs: Arc<Jobs>,
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
        let jobs = Jobs::new(
            workspaces.clone(),
            summaries.clone(),
            Arc::new(Events { ends: sender }),
        );
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
            ends,
            id,
            repo_id,
        }
    }

    async fn ask(&self, op: RemoteOp, confirmed: bool) -> Result<RemoteStart, RemoteError> {
        op_remote(
            self.workspaces.clone(),
            self.summaries.clone(),
            self.jobs.clone(),
            git(&self.dirs),
            self.id.clone(),
            RemoteRequest {
                repo_id: self.repo_id.clone(),
                op,
                confirmed,
            },
        )
        .await
    }

    async fn urls(&self, name: &str) -> Result<RemoteUrlsView, RemoteError> {
        remote_urls(
            self.workspaces.clone(),
            self.id.clone(),
            self.repo_id.clone(),
            name.to_owned(),
        )
        .await
    }

    /// Runs `op`, confirmed, to its job's end.
    async fn run(&mut self, op: RemoteOp) -> JobView {
        let start = self.ask(op, true).await.unwrap();
        let RemoteStart::Started { job_id } = start else {
            panic!("not started: {start:?}");
        };
        let ended = self.ends.recv().await.unwrap();
        assert_eq!(ended.job_id, job_id);
        ended
    }
}

fn refused(result: Result<RemoteStart, RemoteError>) -> String {
    match result {
        Err(error @ (RemoteError::Refused(_) | RemoteError::Name(_))) => error.to_string(),
        other => panic!("not refused: {other:?}"),
    }
}

fn succeeded(view: &JobView) {
    assert_eq!(view.outcome, Some(Outcome::Succeeded), "{view:#?}");
}

fn failed(view: &JobView) {
    assert_eq!(view.outcome, Some(Outcome::Failed), "{view:#?}");
}

fn add(name: &str, url: &str, fetch: bool) -> RemoteOp {
    RemoteOp::Add {
        name: name.to_owned(),
        url: url.to_owned(),
        fetch,
    }
}

fn rename(name: &str, new_name: &str) -> RemoteOp {
    RemoteOp::Rename {
        name: name.to_owned(),
        new_name: new_name.to_owned(),
    }
}

fn remove(name: &str) -> RemoteOp {
    RemoteOp::Remove {
        name: name.to_owned(),
    }
}

fn set_url(name: &str, url: &str, push_url: Option<&str>) -> RemoteOp {
    RemoteOp::SetUrl {
        name: name.to_owned(),
        url: url.to_owned(),
        push_url: push_url.map(str::to_owned),
    }
}

/// The value of `key` in the repo's config; `None` when it is not set.
fn config(repo: &Repo, key: &str) -> Option<String> {
    let output = repo.git_output(["config", "--get", key]).unwrap();
    output
        .status
        .success()
        .then(|| String::from_utf8_lossy(&output.stdout).trim().to_owned())
}

fn has_ref(repo: &Repo, name: &str) -> bool {
    let output = repo
        .git_output(["rev-parse", "--verify", "--quiet", name])
        .unwrap();
    output.status.success()
}

#[tokio::test]
async fn add_a_remote_and_fetch_it_as_one_job_or_only_add_it() {
    let world = World::new();
    let app = &world.app;
    let mut run = Run::new(app).await;
    let url = file_url(&world.bare);

    succeeded(&run.run(add("mirror", &url, true)).await);
    assert_eq!(config(app, "remote.mirror.url"), Some(url.clone()));
    assert!(has_ref(app, "refs/remotes/mirror/main"));

    // Fetching is an option: without it, the remote has no branches yet.
    succeeded(&run.run(add("plain", &format!("  {url} "), false)).await);
    assert_eq!(config(app, "remote.plain.url"), Some(url));
    assert!(!has_ref(app, "refs/remotes/plain/main"));
}

#[tokio::test]
async fn add_with_a_fetch_that_fails_fails_and_keeps_the_remote() {
    let world = World::new();
    let app = &world.app;
    let mut run = Run::new(app).await;
    let gone = file_url(&world.bare.with_file_name("gone.git"));
    failed(&run.run(add("gone", &gone, true)).await);
    assert_eq!(config(app, "remote.gone.url"), Some(gone));
}

#[tokio::test]
async fn sec_4_add_refuses_a_taken_or_invalid_name_and_an_unusable_url_before_git_runs() {
    let world = World::new();
    let app = &world.app;
    let run = Run::new(app).await;
    let url = file_url(&world.bare);
    let cases = [
        (
            add("origin", &url, false),
            "a remote named origin exists already",
        ),
        (add("a/b", &url, false), "a remote name cannot contain '/'"),
        (add("a b", &url, false), "the name cannot contain spaces"),
        (add("-x", &url, false), "the name cannot start with '-'"),
        (
            add("x", "-oProxyCommand=evil", false),
            "the URL cannot start with '-'",
        ),
        (add("x", "  ", false), "the URL is empty"),
        (
            add("x", "a\nb", false),
            "the URL cannot contain control characters",
        ),
    ];
    for (op, why) in cases {
        assert_eq!(refused(run.ask(op, true).await), why);
    }
    assert_eq!(config(app, "remote.x.url"), None);
    assert_eq!(
        remote_name_check("a/b").as_deref(),
        Some("a remote name cannot contain '/'")
    );
    assert_eq!(remote_name_check("upstream"), None);
}

#[tokio::test]
async fn rename_keeps_upstreams_pointing_at_the_renamed_remote() {
    let world = World::new();
    let app = &world.app;
    let mut run = Run::new(app).await;
    assert_eq!(config(app, "branch.main.remote").as_deref(), Some("origin"));
    succeeded(&run.run(rename("origin", "upstream")).await);
    assert_eq!(
        config(app, "branch.main.remote").as_deref(),
        Some("upstream")
    );
    assert!(has_ref(app, "refs/remotes/upstream/main"));
    assert!(!has_ref(app, "refs/remotes/origin/main"));
    assert_eq!(config(app, "remote.origin.url"), None);
}

#[tokio::test]
async fn rename_refuses_a_missing_remote_a_taken_name_and_an_invalid_one() {
    let world = World::new();
    let app = &world.app;
    let mut run = Run::new(app).await;
    succeeded(&run.run(add("mirror", &file_url(&world.bare), false)).await);
    assert_eq!(
        refused(run.ask(rename("nope", "x"), true).await),
        "there is no remote nope"
    );
    assert_eq!(
        refused(run.ask(rename("origin", "mirror"), true).await),
        "a remote named mirror exists already"
    );
    assert_eq!(
        refused(run.ask(rename("origin", "origin"), true).await),
        "the remote is named origin already"
    );
    assert_eq!(
        refused(run.ask(rename("origin", "a..b"), true).await),
        "the name cannot contain '..'"
    );
    assert_eq!(config(app, "branch.main.remote").as_deref(), Some("origin"));
}

#[tokio::test]
async fn remove_asks_first_naming_the_upstreams_it_unsets() {
    let world = World::new();
    let app = &world.app;
    app.git(["branch", "--quiet", "--track", "feature", "origin/main"])
        .unwrap();
    let mut run = Run::new(app).await;

    let start = run.ask(remove("origin"), false).await.unwrap();
    assert_eq!(
        start,
        RemoteStart::Confirm {
            repo: "app".to_owned(),
            actions: vec![
                "Remove remote origin and its remote-tracking branches".to_owned(),
                "Unset the upstream of feature (origin/main)".to_owned(),
                "Unset the upstream of main (origin/main)".to_owned(),
            ],
        }
    );
    // Not confirmed: nothing ran.
    assert!(config(app, "remote.origin.url").is_some());

    succeeded(&run.run(remove("origin")).await);
    assert_eq!(config(app, "remote.origin.url"), None);
    assert_eq!(config(app, "branch.main.remote"), None);
    assert_eq!(config(app, "branch.feature.remote"), None);
    assert!(!has_ref(app, "refs/remotes/origin/main"));
}

#[tokio::test]
async fn remove_without_upstreams_still_asks_and_a_missing_remote_is_refused() {
    let world = World::new();
    let app = &world.app;
    let mut run = Run::new(app).await;
    succeeded(&run.run(add("mirror", &file_url(&world.bare), false)).await);
    let start = run.ask(remove("mirror"), false).await.unwrap();
    let RemoteStart::Confirm { actions, .. } = start else {
        panic!("not asked: {start:?}");
    };
    assert_eq!(actions[1], "No branch has its upstream on mirror");
    assert_eq!(
        refused(run.ask(remove("nope"), true).await),
        "there is no remote nope"
    );
}

#[tokio::test]
async fn set_url_changes_the_fetch_url_and_a_separate_push_url() {
    let world = World::new();
    let app = &world.app;
    let mut run = Run::new(app).await;
    let url = file_url(&world.bare);
    assert_eq!(
        run.urls("origin").await.unwrap(),
        RemoteUrlsView {
            url: Some(url.clone()),
            push_url: None,
        }
    );

    let moved = file_url(&world.bare.with_file_name("moved.git"));
    succeeded(&run.run(set_url("origin", &moved, None)).await);
    assert_eq!(config(app, "remote.origin.url"), Some(moved.clone()));
    assert_eq!(config(app, "remote.origin.pushurl"), None);

    succeeded(&run.run(set_url("origin", &url, Some(&moved))).await);
    assert_eq!(
        run.urls("origin").await.unwrap(),
        RemoteUrlsView {
            url: Some(url),
            push_url: Some(moved),
        }
    );
}

#[tokio::test]
async fn set_url_refuses_a_missing_remote_and_an_unusable_url() {
    let world = World::new();
    let app = &world.app;
    let run = Run::new(app).await;
    let url = file_url(&world.bare);
    assert_eq!(
        refused(run.ask(set_url("nope", &url, None), true).await),
        "there is no remote nope"
    );
    assert_eq!(
        refused(run.ask(set_url("origin", "-x", None), true).await),
        "the URL cannot start with '-'"
    );
    assert_eq!(
        refused(run.ask(set_url("origin", &url, Some("")), true).await),
        "the URL is empty"
    );
    let missing = run.urls("nope").await;
    assert!(
        matches!(missing, Err(RemoteError::Refused(_))),
        "{missing:?}"
    );
    assert_eq!(config(app, "remote.origin.url"), Some(url));
}

#[tokio::test]
async fn grp_8_remotes_are_refused_in_conflict_state() {
    let world = World::new();
    let app = &world.app;
    app.git(["switch", "--quiet", "-c", "side"]).unwrap();
    app.commit_file("a.txt", "side\n", "Side").unwrap();
    app.switch("main").unwrap();
    app.commit_file("a.txt", "main\n", "Main").unwrap();
    app.merge_with_conflicts("side").unwrap();
    let run = Run::new(app).await;
    assert_eq!(
        refused(run.ask(rename("origin", "upstream"), true).await),
        "the repo is in conflict state; continue or abort first"
    );
}
