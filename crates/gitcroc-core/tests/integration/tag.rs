//! Tag create, delete and push (section 6 "Tags", section 5 "Create tag
//! X", SEC-4, GRP-8) with real git, each as a job. The remote is a bare
//! repo on disk reached over `file://`; a second clone of it stands in for
//! someone else pushing. Each test waits for the job's end event; nothing
//! waits for a time.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use gitcroc_core::{
    ConfigDir, GitRunner, JobEvents, JobId, JobView, Jobs, Outcome, RepoId, Summaries,
    SummaryCache, SummaryView, TagError, TagOp, TagRequest, TagStart, WorkspaceId, Workspaces,
    add_path, op_tag, tag_name_check,
};
use repo_gen::{Fixture, Repo};
use tokio::sync::mpsc;

struct Dirs {
    root: tempfile::TempDir,
}

/// git with no user or system config, so no hook or setting of the machine
/// takes part, and an identity for annotated tags.
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

/// A bare remote with `main` at two commits, and two clones of it over
/// `file://`: `app`, the repo the app works on, and `other`.
struct World {
    _fixture: Fixture,
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
        seed.commit_file("b.txt", "b\n", "Second").unwrap();
        let bare = fixture.path().join("origin.git");
        let mut args: Vec<std::ffi::OsString> = vec!["clone".into(), "--bare".into()];
        args.extend(["--quiet".into(), "--".into()]);
        args.extend([seed.path().into(), bare.clone().into()]);
        seed.git(args).unwrap();
        let app = Self::clone(&seed, &bare, fixture.path().join("app"));
        let other = Self::clone(&seed, &bare, fixture.path().join("other"));
        Self {
            _fixture: fixture,
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

    /// The commit tag `name` of the remote points to.
    fn remote_tag(&self, name: &str) -> Option<String> {
        let mut args: Vec<std::ffi::OsString> = vec!["--git-dir".into(), self.bare.clone().into()];
        args.extend(["rev-parse".into(), "--verify".into(), "--quiet".into()]);
        args.push(format!("refs/tags/{name}^{{commit}}").into());
        let output = self.other.git_output(args).unwrap();
        output
            .status
            .success()
            .then(|| String::from_utf8_lossy(&output.stdout).trim().to_owned())
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

    async fn ask(&self, op: TagOp, confirmed: bool) -> Result<TagStart, TagError> {
        op_tag(
            self.workspaces.clone(),
            self.summaries.clone(),
            self.jobs.clone(),
            git(&self.dirs),
            self.id.clone(),
            TagRequest {
                repo_id: self.repo_id.clone(),
                op,
                confirmed,
            },
        )
        .await
    }

    /// Runs `op`, confirmed, to its job's end.
    async fn run(&mut self, op: TagOp) -> JobView {
        let start = self.ask(op, true).await.unwrap();
        let TagStart::Started { job_id } = start else {
            panic!("not started: {start:?}");
        };
        let ended = self.ends.recv().await.unwrap();
        assert_eq!(ended.job_id, job_id);
        ended
    }
}

fn refused(result: Result<TagStart, TagError>) -> String {
    match result {
        Err(error @ (TagError::Refused(_) | TagError::Name(_))) => error.to_string(),
        other => panic!("not refused: {other:?}"),
    }
}

fn succeeded(view: &JobView) {
    assert_eq!(view.outcome, Some(Outcome::Succeeded), "{view:#?}");
}

fn failed(view: &JobView) {
    assert_eq!(view.outcome, Some(Outcome::Failed), "{view:#?}");
}

fn name(name: &str) -> String {
    name.to_owned()
}

fn create(tag: &str, commit: Option<String>, message: Option<&str>, push: bool) -> TagOp {
    TagOp::Create {
        name: name(tag),
        commit,
        annotated: message.is_some(),
        message: message.unwrap_or_default().to_owned(),
        push,
        remote: None,
    }
}

fn delete(tag: &str, on_remote: bool) -> TagOp {
    TagOp::Delete {
        name: name(tag),
        on_remote,
        remote: None,
    }
}

fn push(tag: &str) -> TagOp {
    TagOp::Push {
        name: name(tag),
        remote: None,
    }
}

/// The object type tag `name` points to: `tag` when annotated, `commit`
/// when lightweight; `None` when there is no such tag.
fn tag_type(repo: &Repo, name: &str) -> Option<String> {
    let full = format!("refs/tags/{name}");
    let output = repo.git_output(["cat-file", "-t", &full]).unwrap();
    output
        .status
        .success()
        .then(|| String::from_utf8_lossy(&output.stdout).trim().to_owned())
}

#[tokio::test]
async fn create_annotated_and_lightweight_tags_at_head_or_a_commit() {
    let world = World::new();
    let app = &world.app;
    let first = app.rev_parse("HEAD~1").unwrap();
    let mut run = Run::new(app).await;

    // Annotated at HEAD, the message as typed: a line starting with `#`
    // is kept.
    let ended = run
        .run(create("v1", None, Some("Release 1\n\n#1 fixed"), false))
        .await;
    succeeded(&ended);
    assert_eq!(ended.label, "Create tag v1");
    assert_eq!(tag_type(app, "v1").as_deref(), Some("tag"));
    assert_eq!(app.rev_parse("v1^{commit}").unwrap(), app.head().unwrap());
    assert_eq!(
        app.git(["tag", "-l", "--format=%(contents)", "v1"])
            .unwrap(),
        "Release 1\n\n#1 fixed\n\n"
    );

    // Lightweight at an earlier commit.
    succeeded(
        &run.run(create("v0", Some(first.clone()), None, false))
            .await,
    );
    assert_eq!(tag_type(app, "v0").as_deref(), Some("commit"));
    assert_eq!(app.rev_parse("v0").unwrap(), first);
    // Not pushed: the push option is off by default.
    assert_eq!(world.remote_tag("v0"), None);
}

#[tokio::test]
async fn create_tag_refuses_an_existing_name_a_missing_commit_and_a_blank_message() {
    let world = World::new();
    world.app.tag("v1").unwrap();
    let run = Run::new(&world.app).await;
    assert_eq!(
        refused(run.ask(create("v1", None, None, false), false).await),
        "a tag named v1 exists already"
    );
    assert_eq!(
        refused(
            run.ask(create("v2", Some(name("0123abc")), None, false), false)
                .await
        ),
        "'0123abc' names no commit"
    );
    assert_eq!(
        refused(run.ask(create("v2", None, Some(" \n"), false), false).await),
        "an annotated tag needs a message"
    );
    assert!(run.jobs.all_views().is_empty());
}

#[tokio::test]
async fn sec_4_invalid_tag_names_are_refused_before_git_runs() {
    let world = World::new();
    let run = Run::new(&world.app).await;
    for (tag, why) in [
        ("a b", "the name cannot contain spaces"),
        ("--force", "the name cannot start with '-'"),
        (
            "v1.lock",
            "no part of the name between slashes can end with '.lock'",
        ),
        ("HEAD", "'HEAD' is not a valid name"),
    ] {
        assert_eq!(
            refused(run.ask(create(tag, None, None, false), false).await),
            why
        );
        assert_eq!(refused(run.ask(push(tag), false).await), why);
        assert_eq!(refused(run.ask(delete(tag, false), false).await), why);
    }
    assert_eq!(
        tag_name_check("v1^").as_deref(),
        Some("the name cannot contain '^'")
    );
    assert_eq!(tag_name_check("release/1.0"), None);
    assert_eq!(world.app.git(["tag", "-l"]).unwrap(), "");
    assert!(run.jobs.all_views().is_empty());
}

#[tokio::test]
async fn create_with_push_creates_and_pushes_the_tag_as_one_job() {
    let world = World::new();
    let mut run = Run::new(&world.app).await;
    let ended = run.run(create("v1", None, Some("Release"), true)).await;
    succeeded(&ended);
    assert_eq!(ended.label, "Create tag v1 and push it to origin");
    assert_eq!(world.remote_tag("v1"), Some(world.app.head().unwrap()));

    // The push fails when the remote has another v2: the tag was created
    // all the same, and the job failed with git's message.
    world.other.git(["tag", "v2", "HEAD~1"]).unwrap();
    world
        .other
        .git(["push", "--quiet", "origin", "refs/tags/v2"])
        .unwrap();
    let ended = run.run(create("v2", None, None, true)).await;
    failed(&ended);
    assert!(ended.output.contains("already exists"), "{}", ended.output);
    assert_eq!(
        world.app.rev_parse("v2").unwrap(),
        world.app.head().unwrap()
    );
    assert_eq!(
        world.remote_tag("v2"),
        Some(world.other.rev_parse("HEAD~1").unwrap())
    );
}

#[tokio::test]
async fn push_tag_to_a_file_remote_and_fail_on_a_different_tag_of_that_name() {
    let world = World::new();
    world.app.annotated_tag("v1", "Release").unwrap();
    let mut run = Run::new(&world.app).await;
    let ended = run.run(push("v1")).await;
    succeeded(&ended);
    assert_eq!(ended.label, "Push tag v1 to origin");
    assert_eq!(world.remote_tag("v1"), Some(world.app.head().unwrap()));

    // Someone else pushed a v2 at another commit: git rejects ours and the
    // remote keeps theirs.
    world.other.git(["tag", "v2", "HEAD~1"]).unwrap();
    world
        .other
        .git(["push", "--quiet", "origin", "refs/tags/v2"])
        .unwrap();
    world.app.tag("v2").unwrap();
    let ended = run.run(push("v2")).await;
    failed(&ended);
    assert!(ended.message.is_some(), "{ended:#?}");
    assert!(ended.output.contains("already exists"), "{}", ended.output);
    assert_eq!(
        world.remote_tag("v2"),
        Some(world.other.rev_parse("HEAD~1").unwrap())
    );

    // A tag that is not there is refused.
    assert_eq!(
        refused(run.ask(push("v9"), false).await),
        "there is no tag v9"
    );
}

#[tokio::test]
async fn push_tag_asks_for_a_remote_when_there_is_no_origin_and_several() {
    let world = World::new();
    let app = &world.app;
    app.tag("v1").unwrap();
    app.git(["remote", "rename", "origin", "upstream"]).unwrap();
    let url = file_url(&world.bare);
    app.git(["remote", "add", "mirror", &url]).unwrap();
    let mut run = Run::new(app).await;

    assert_eq!(
        run.ask(push("v1"), false).await.unwrap(),
        TagStart::ChooseRemote {
            remotes: vec![name("mirror"), name("upstream")]
        }
    );
    let picked = TagOp::Push {
        name: name("v1"),
        remote: Some(name("mirror")),
    };
    let ended = run.run(picked).await;
    succeeded(&ended);
    assert_eq!(ended.label, "Push tag v1 to mirror");
    assert_eq!(world.remote_tag("v1"), Some(app.head().unwrap()));
    let unknown = TagOp::Push {
        name: name("v1"),
        remote: Some(name("nowhere")),
    };
    assert_eq!(
        refused(run.ask(unknown, false).await),
        "there is no remote nowhere"
    );
}

#[tokio::test]
async fn delete_tag_locally() {
    let world = World::new();
    world.app.tag("v1").unwrap();
    let mut run = Run::new(&world.app).await;
    let ended = run.run(delete("v1", false)).await;
    succeeded(&ended);
    assert_eq!(ended.label, "Delete tag v1");
    assert_eq!(tag_type(&world.app, "v1"), None);
    assert_eq!(
        refused(run.ask(delete("v1", false), false).await),
        "there is no tag v1"
    );
}

#[tokio::test]
async fn delete_tag_also_on_the_remote_after_a_confirmation() {
    let world = World::new();
    let app = &world.app;
    app.tag("v1").unwrap();
    app.git(["push", "--quiet", "origin", "refs/tags/v1"])
        .unwrap();
    let mut run = Run::new(app).await;

    // Not confirmed: what it would do, and nothing runs.
    assert_eq!(
        run.ask(delete("v1", true), false).await.unwrap(),
        TagStart::Confirm {
            repo: name("app"),
            actions: vec![name(
                "Delete tag v1 on the remote origin, for everyone who fetches from it"
            )],
        }
    );
    assert!(run.jobs.all_views().is_empty());
    assert!(world.remote_tag("v1").is_some());

    let ended = run.run(delete("v1", true)).await;
    succeeded(&ended);
    assert_eq!(ended.label, "Delete tag v1 here and on origin");
    assert_eq!(tag_type(app, "v1"), None);
    assert_eq!(world.remote_tag("v1"), None);
}

#[tokio::test]
async fn delete_on_an_unreachable_remote_fails_and_keeps_the_local_tag() {
    let world = World::new();
    let app = &world.app;
    app.tag("v1").unwrap();
    let missing = file_url(&world.bare.with_file_name("missing.git"));
    app.git(["remote", "set-url", "origin", &missing]).unwrap();
    let mut run = Run::new(app).await;
    let ended = run.run(delete("v1", true)).await;
    failed(&ended);
    assert!(ended.output.contains("missing.git"), "{}", ended.output);
    // The remote's delete runs first: the local tag is still there.
    assert_eq!(tag_type(app, "v1").as_deref(), Some("commit"));
}

#[tokio::test]
async fn grp_8_tags_are_refused_in_conflict_state() {
    let world = World::new();
    let app = &world.app;
    app.git(["switch", "--quiet", "-c", "side"]).unwrap();
    app.commit_file("a.txt", "side\n", "Side").unwrap();
    app.switch("main").unwrap();
    app.commit_file("a.txt", "main\n", "Main").unwrap();
    app.merge_with_conflicts("side").unwrap();
    let run = Run::new(app).await;
    assert_eq!(
        refused(run.ask(create("v1", None, None, false), false).await),
        "the repo is in conflict state; continue or abort first"
    );
}
