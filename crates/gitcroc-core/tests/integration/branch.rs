//! Branch operations (section 6 "Branches", SEC-4, UI-7, GRP-8, GRP-9,
//! GRP-12) on fixture repos, with real git, each as a job. A clone's
//! `origin` is a fixture repo on disk, so remote branches need no network.
//! Nothing here waits for a time: each test waits for the job's end event.

use std::path::PathBuf;
use std::sync::Arc;

use gitcroc_core::{
    BranchChoices, BranchError, BranchOp, BranchRequest, BranchStart, ConfigDir, GitRunner,
    JobEvents, JobId, JobView, Jobs, NothingToDo, OnCancel, Outcome, Output, RemoteBranchRef,
    RepoId, StartPoint, Summaries, SummaryCache, SummaryView, WorkspaceId, Workspaces, WriteJob,
    add_path, branch_choices, branch_name_check, op_branch,
};
use repo_gen::{Fixture, Repo};
use tokio::sync::{mpsc, oneshot};

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
    ])
}

impl ConfigDir for Dirs {
    fn config_dir(&self) -> Option<PathBuf> {
        Some(self.root.path().join("config"))
    }
}

/// Sends each job's end to the test.
struct Ends(mpsc::UnboundedSender<JobView>);

impl JobEvents for Ends {
    fn progress(&self, _job: &JobView) {}

    fn ended(&self, job: &JobView) {
        let _ = self.0.send(job.clone());
    }

    fn summary(&self, _view: SummaryView, _read: bool) {}
}

/// A workspace with one repo and the job engine.
struct Run {
    dirs: Dirs,
    repo: Repo,
    workspaces: Arc<Workspaces>,
    summaries: Arc<Summaries>,
    jobs: Arc<Jobs>,
    ends: mpsc::UnboundedReceiver<JobView>,
    id: WorkspaceId,
    repo_id: RepoId,
}

impl Run {
    async fn new(repo: Repo) -> Self {
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
            Arc::new(Ends(sender)),
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
            repo,
            workspaces,
            summaries,
            jobs,
            ends,
            id,
            repo_id,
        }
    }

    /// Asks for `op`, confirmed or not.
    async fn ask(&self, op: BranchOp, confirmed: bool) -> Result<BranchStart, BranchError> {
        op_branch(
            self.workspaces.clone(),
            self.summaries.clone(),
            self.jobs.clone(),
            git(&self.dirs),
            self.id.clone(),
            BranchRequest {
                repo_id: self.repo_id.clone(),
                op,
                confirmed,
            },
        )
        .await
    }

    /// Runs `op`, confirmed, to its job's end.
    async fn run(&mut self, op: BranchOp) -> JobView {
        let BranchStart::Started { job_id } = self.ask(op, true).await.unwrap() else {
            panic!("not started");
        };
        let ended = self.ends.recv().await.unwrap();
        assert_eq!(ended.job_id, job_id);
        ended
    }

    async fn choices(&self) -> BranchChoices {
        branch_choices(
            self.workspaces.clone(),
            self.id.clone(),
            self.repo_id.clone(),
        )
        .await
        .unwrap()
    }

    /// The branch `HEAD` points to, or `HEAD` when detached.
    fn current(&self) -> String {
        self.repo
            .git(["rev-parse", "--abbrev-ref", "HEAD"])
            .unwrap()
            .trim()
            .to_owned()
    }

    fn branches(&self) -> String {
        self.repo
            .git(["branch", "--format=%(refname:short)"])
            .unwrap()
    }

    fn upstream(&self, branch: &str) -> Option<String> {
        let output = self
            .repo
            .git_output([
                "rev-parse",
                "--abbrev-ref",
                &format!("{branch}@{{upstream}}"),
            ])
            .unwrap();
        output
            .status
            .success()
            .then(|| String::from_utf8_lossy(&output.stdout).trim().to_owned())
    }
}

fn refused(result: Result<BranchStart, BranchError>) -> String {
    match result {
        Err(error @ (BranchError::Refused(_) | BranchError::Name(_))) => error.to_string(),
        other => panic!("not refused: {other:?}"),
    }
}

fn name(name: &str) -> String {
    name.to_owned()
}

fn remote(remote: &str, branch: &str) -> RemoteBranchRef {
    RemoteBranchRef {
        remote: remote.to_owned(),
        branch: branch.to_owned(),
    }
}

/// `main` with two commits, `merged` at the first and `side` with a commit
/// of its own; `main` checked out.
fn local_repo(fixture: &Fixture) -> Repo {
    let repo = fixture.repo("app").unwrap();
    repo.commit_file("a.txt", "a\n", "First").unwrap();
    repo.branch("merged").unwrap();
    repo.branch("side").unwrap();
    repo.switch("side").unwrap();
    repo.commit_file("side.txt", "side\n", "On side").unwrap();
    repo.switch("main").unwrap();
    repo.commit_file("a.txt", "a2\n", "Second").unwrap();
    repo
}

/// A clone of `origin`, which has `main` and `feature/x`; the clone has
/// local `main` only, tracking `origin/main`.
fn cloned_repo(fixture: &Fixture) -> Repo {
    let origin = fixture.repo("origin").unwrap();
    origin.commit_file("a.txt", "a\n", "First").unwrap();
    origin.branch("feature/x").unwrap();
    origin.switch("feature/x").unwrap();
    origin.commit_file("x.txt", "x\n", "On feature/x").unwrap();
    origin.switch("main").unwrap();
    origin.commit_file("a.txt", "a2\n", "Second").unwrap();
    origin.clone_to(fixture.path().join("app")).unwrap()
}

#[tokio::test]
async fn sec_4_ref_names_follow_git_rules() {
    // `@` and `-` are left out: git reads them as `HEAD` and the previous
    // branch, and the app refuses both (see the unit tests).
    let names = [
        "main",
        "feature/x",
        "a.b",
        "x-1",
        "UPPER_case",
        "ünïcödé",
        "a@b",
        "v1.0",
        "a/b/c",
        "x.lock2",
        "x/HEAD",
        "",
        "-x",
        "a b",
        "a\tb",
        "a\u{7f}b",
        "a..b",
        "a~1",
        "a^",
        "a:b",
        "a?",
        "a*",
        "a[b",
        "a\\b",
        ".hidden",
        "a/.b",
        "a.lock",
        "a/b.lock",
        "a/",
        "/a",
        "a//b",
        "a.",
        "a@{b",
        "a@{1}",
    ];
    let fixture = Fixture::new().unwrap();
    let repo = fixture.repo("app").unwrap();
    for name in names {
        let git = repo
            .git_output(["check-ref-format", "--branch", name])
            .unwrap()
            .status
            .success();
        let ours = branch_name_check(name);
        assert_eq!(ours.is_none(), git, "{name:?}: {ours:?}");
    }
    assert_eq!(
        branch_name_check("a..b").as_deref(),
        Some("the name cannot contain '..'")
    );
}

#[tokio::test]
async fn sec_4_invalid_names_are_refused_before_git_runs() {
    let fixture = Fixture::new().unwrap();
    let repo = local_repo(&fixture);
    let run = Run::new(repo).await;
    let create = |name: &str| BranchOp::Create {
        name: name.to_owned(),
        start: StartPoint::Head,
        switch: false,
    };
    assert_eq!(
        refused(run.ask(create("a b"), false).await),
        "the name cannot contain spaces"
    );
    assert_eq!(
        refused(run.ask(create("--force"), false).await),
        "the name cannot start with '-'"
    );
    assert_eq!(
        refused(
            run.ask(
                BranchOp::Rename {
                    name: name("side"),
                    new_name: name("x.lock"),
                },
                false
            )
            .await
        ),
        "no part of the name between slashes can end with '.lock'"
    );
    assert_eq!(run.branches(), "main\nmerged\nside\n");
    assert!(run.jobs.all_views().is_empty());
}

#[tokio::test]
async fn ui_7_checkout_a_local_branch() {
    let fixture = Fixture::new().unwrap();
    let repo = local_repo(&fixture);
    let mut run = Run::new(repo).await;

    let ended = run.run(BranchOp::Checkout { name: name("side") }).await;
    assert_eq!(ended.outcome, Some(Outcome::Succeeded), "{ended:?}");
    assert_eq!(ended.label, "Check out side");
    assert_eq!(run.current(), "side");

    // Already checked out.
    let ended = run.run(BranchOp::Checkout { name: name("side") }).await;
    assert_eq!(ended.outcome, Some(Outcome::NothingToDo), "{ended:?}");

    assert_eq!(
        refused(
            run.ask(BranchOp::Checkout { name: name("nope") }, false)
                .await
        ),
        "there is no local branch nope"
    );
}

#[tokio::test]
async fn grp_12_checkout_never_stashes() {
    let fixture = Fixture::new().unwrap();
    let repo = local_repo(&fixture);
    // `side` has the first a.txt; a local edit cannot be carried there.
    repo.write("a.txt", "edited\n").unwrap();
    let mut run = Run::new(repo).await;

    let ended = run.run(BranchOp::Checkout { name: name("side") }).await;
    assert_eq!(ended.outcome, Some(Outcome::Failed), "{ended:?}");
    assert!(
        ended.output.contains("would be overwritten by checkout"),
        "{}",
        ended.output
    );
    assert_eq!(run.current(), "main");
    assert_eq!(
        std::fs::read_to_string(run.repo.path().join("a.txt")).unwrap(),
        "edited\n"
    );
    assert_eq!(run.repo.git(["stash", "list"]).unwrap(), "");
}

#[tokio::test]
async fn ui_7_checkout_a_remote_branch() {
    let fixture = Fixture::new().unwrap();
    let repo = cloned_repo(&fixture);
    let mut run = Run::new(repo).await;

    // No local feature/x: a new one tracks the remote branch.
    let ended = run
        .run(BranchOp::CheckoutRemote {
            branch: remote("origin", "feature/x"),
        })
        .await;
    assert_eq!(ended.outcome, Some(Outcome::Succeeded), "{ended:?}");
    assert_eq!(ended.label, "Check out origin/feature/x");
    assert_eq!(run.current(), "feature/x");
    assert_eq!(
        run.upstream("feature/x").as_deref(),
        Some("origin/feature/x")
    );

    // A local main exists: it is checked out as it is.
    run.repo.switch("main").unwrap();
    run.repo.commit_file("b.txt", "b\n", "Local only").unwrap();
    let local_main = run.repo.head().unwrap();
    run.repo.switch("feature/x").unwrap();
    let ended = run
        .run(BranchOp::CheckoutRemote {
            branch: remote("origin", "main"),
        })
        .await;
    assert_eq!(ended.outcome, Some(Outcome::Succeeded), "{ended:?}");
    assert_eq!(run.current(), "main");
    assert_eq!(run.repo.head().unwrap(), local_main);

    assert_eq!(
        refused(
            run.ask(
                BranchOp::CheckoutRemote {
                    branch: remote("upstream", "main"),
                },
                false
            )
            .await
        ),
        "there is no remote upstream"
    );
}

#[tokio::test]
async fn ui_7_checkout_a_commit_detached() {
    let fixture = Fixture::new().unwrap();
    let repo = local_repo(&fixture);
    let first = repo.rev_parse("main^").unwrap();
    let mut run = Run::new(repo).await;

    let ended = run
        .run(BranchOp::CheckoutDetached {
            revision: first[..10].to_owned(),
        })
        .await;
    assert_eq!(ended.outcome, Some(Outcome::Succeeded), "{ended:?}");
    assert_eq!(run.current(), "HEAD");
    assert_eq!(run.repo.head().unwrap(), first);

    let ended = run
        .run(BranchOp::CheckoutDetached {
            revision: first.clone(),
        })
        .await;
    assert_eq!(ended.outcome, Some(Outcome::NothingToDo), "{ended:?}");

    assert_eq!(
        refused(
            run.ask(
                BranchOp::CheckoutDetached {
                    revision: name("no-such-thing"),
                },
                false
            )
            .await
        ),
        "'no-such-thing' names no commit"
    );
}

#[tokio::test]
async fn ui_7_create_a_branch() {
    let fixture = Fixture::new().unwrap();
    let repo = cloned_repo(&fixture);
    let mut run = Run::new(repo).await;
    run.repo.commit_file("b.txt", "b\n", "Local").unwrap();
    let head = run.repo.head().unwrap();
    let first = run.repo.rev_parse("HEAD~2").unwrap();

    // From HEAD, switched to.
    let ended = run
        .run(BranchOp::Create {
            name: name("from-head"),
            start: StartPoint::Head,
            switch: true,
        })
        .await;
    assert_eq!(ended.outcome, Some(Outcome::Succeeded), "{ended:?}");
    assert_eq!(ended.label, "Create branch from-head");
    assert_eq!(run.current(), "from-head");
    assert_eq!(run.repo.head().unwrap(), head);

    // From the default branch, origin/main, without switching.
    let ended = run
        .run(BranchOp::Create {
            name: name("from-default"),
            start: StartPoint::DefaultBranch,
            switch: false,
        })
        .await;
    assert_eq!(ended.outcome, Some(Outcome::Succeeded), "{ended:?}");
    assert_eq!(run.current(), "from-head");
    assert_eq!(
        run.repo.rev_parse("from-default").unwrap(),
        run.repo.rev_parse("origin/main").unwrap()
    );

    // From a commit, switched to.
    let ended = run
        .run(BranchOp::Create {
            name: name("from-commit"),
            start: StartPoint::Revision {
                revision: first.clone(),
            },
            switch: true,
        })
        .await;
    assert_eq!(ended.outcome, Some(Outcome::Succeeded), "{ended:?}");
    assert_eq!(run.current(), "from-commit");
    assert_eq!(run.repo.head().unwrap(), first);

    // An existing name is refused, never reset.
    assert_eq!(
        refused(
            run.ask(
                BranchOp::Create {
                    name: name("from-head"),
                    start: StartPoint::Revision { revision: first },
                    switch: false,
                },
                false
            )
            .await
        ),
        "a branch named from-head exists already"
    );
    assert_eq!(run.repo.rev_parse("from-head").unwrap(), head);

    assert_eq!(
        run.choices().await,
        BranchChoices {
            start_points: [
                "from-commit",
                "from-default",
                "from-head",
                "main",
                "origin/feature/x",
                "origin/main"
            ]
            .map(str::to_owned)
            .to_vec(),
            default_branch: Some(name("origin/main")),
            remote_branches: vec![remote("origin", "feature/x"), remote("origin", "main")],
        }
    );
}

#[tokio::test]
async fn ui_7_rename_a_branch() {
    let fixture = Fixture::new().unwrap();
    let repo = local_repo(&fixture);
    let mut run = Run::new(repo).await;

    let ended = run
        .run(BranchOp::Rename {
            name: name("side"),
            new_name: name("feature/side"),
        })
        .await;
    assert_eq!(ended.outcome, Some(Outcome::Succeeded), "{ended:?}");
    assert_eq!(ended.label, "Rename side to feature/side");
    assert_eq!(run.branches(), "feature/side\nmain\nmerged\n");

    // To an existing name: git refuses.
    let ended = run
        .run(BranchOp::Rename {
            name: name("merged"),
            new_name: name("main"),
        })
        .await;
    assert_eq!(ended.outcome, Some(Outcome::Failed), "{ended:?}");
    assert!(ended.output.contains("already exists"), "{}", ended.output);
    assert_eq!(run.branches(), "feature/side\nmain\nmerged\n");
}

#[tokio::test]
async fn ui_7_delete_a_branch() {
    let fixture = Fixture::new().unwrap();
    let repo = local_repo(&fixture);
    let mut run = Run::new(repo).await;
    let delete = |branch: &str| BranchOp::Delete {
        name: branch.to_owned(),
        force: false,
        on_remote: false,
    };

    let ended = run.run(delete("merged")).await;
    assert_eq!(ended.outcome, Some(Outcome::Succeeded), "{ended:?}");
    assert_eq!(ended.label, "Delete branch merged");
    assert_eq!(run.branches(), "main\nside\n");

    // Not merged: git refuses with its message.
    let ended = run.run(delete("side")).await;
    assert_eq!(ended.outcome, Some(Outcome::Failed), "{ended:?}");
    assert!(
        ended.output.contains("not fully merged"),
        "{}",
        ended.output
    );
    assert_eq!(run.branches(), "main\nside\n");

    assert_eq!(
        refused(run.ask(delete("main"), false).await),
        "the current branch cannot be deleted"
    );
}

#[tokio::test]
async fn grp_9_force_delete_needs_confirmation() {
    let fixture = Fixture::new().unwrap();
    let repo = local_repo(&fixture);
    let mut run = Run::new(repo).await;
    let force = BranchOp::Delete {
        name: name("side"),
        force: true,
        on_remote: false,
    };

    let answer = run.ask(force.clone(), false).await.unwrap();
    assert_eq!(
        answer,
        BranchStart::Confirm {
            repo: name("app"),
            actions: vec![name(
                "Delete branch side even if it is not merged; its commits may be lost"
            )],
        }
    );
    assert_eq!(run.branches(), "main\nmerged\nside\n");
    assert!(run.jobs.all_views().is_empty());

    let ended = run.run(force).await;
    assert_eq!(ended.outcome, Some(Outcome::Succeeded), "{ended:?}");
    assert_eq!(ended.label, "Force delete branch side");
    assert_eq!(run.branches(), "main\nmerged\n");
}

#[tokio::test]
async fn grp_9_remote_delete_needs_confirmation() {
    let fixture = Fixture::new().unwrap();
    let repo = cloned_repo(&fixture);
    let origin = Repo::open(fixture.path().join("origin"));
    let mut run = Run::new(repo).await;
    run.repo
        .git(["branch", "--track", "feature/x", "origin/feature/x"])
        .unwrap();
    origin.branch("gone-soon").unwrap();
    run.repo.git(["fetch", "--quiet", "origin"]).unwrap();
    let remote_branches = || origin.git(["branch", "--format=%(refname:short)"]).unwrap();

    // From a remote branch row.
    let delete_remote = BranchOp::DeleteRemote {
        branch: remote("origin", "gone-soon"),
    };
    assert_eq!(
        run.ask(delete_remote.clone(), false).await.unwrap(),
        BranchStart::Confirm {
            repo: name("app"),
            actions: vec![name("Delete gone-soon on the remote origin")],
        }
    );
    assert_eq!(remote_branches(), "feature/x\ngone-soon\nmain\n");
    let ended = run.run(delete_remote).await;
    assert_eq!(ended.outcome, Some(Outcome::Succeeded), "{ended:?}");
    assert_eq!(ended.label, "Delete origin/gone-soon on the remote");
    assert_eq!(remote_branches(), "feature/x\nmain\n");

    // A local branch and the branch it tracks.
    let both = BranchOp::Delete {
        name: name("feature/x"),
        force: false,
        on_remote: true,
    };
    assert_eq!(
        run.ask(both.clone(), false).await.unwrap(),
        BranchStart::Confirm {
            repo: name("app"),
            actions: vec![name("Delete feature/x on the remote origin")],
        }
    );
    let ended = run.run(both).await;
    assert_eq!(ended.outcome, Some(Outcome::Succeeded), "{ended:?}");
    assert_eq!(remote_branches(), "main\n");
    assert_eq!(run.branches(), "main\n");

    assert_eq!(
        refused(
            run.ask(
                BranchOp::Delete {
                    name: name("main"),
                    force: false,
                    on_remote: true,
                },
                true
            )
            .await
        ),
        "the current branch cannot be deleted"
    );
}

#[tokio::test]
async fn ui_7_set_and_unset_the_upstream() {
    let fixture = Fixture::new().unwrap();
    let repo = cloned_repo(&fixture);
    let mut run = Run::new(repo).await;
    run.repo.branch("topic").unwrap();
    assert_eq!(run.upstream("topic"), None);

    let ended = run
        .run(BranchOp::SetUpstream {
            name: name("topic"),
            upstream: Some(remote("origin", "feature/x")),
        })
        .await;
    assert_eq!(ended.outcome, Some(Outcome::Succeeded), "{ended:?}");
    assert_eq!(ended.label, "Set the upstream of topic to origin/feature/x");
    assert_eq!(run.upstream("topic").as_deref(), Some("origin/feature/x"));

    let ended = run
        .run(BranchOp::SetUpstream {
            name: name("topic"),
            upstream: None,
        })
        .await;
    assert_eq!(ended.outcome, Some(Outcome::Succeeded), "{ended:?}");
    assert_eq!(run.upstream("topic"), None);

    // Unset again: git refuses, as the branch tracks nothing.
    let ended = run
        .run(BranchOp::SetUpstream {
            name: name("topic"),
            upstream: None,
        })
        .await;
    assert_eq!(ended.outcome, Some(Outcome::Failed), "{ended:?}");

    assert_eq!(
        refused(
            run.ask(
                BranchOp::SetUpstream {
                    name: name("topic"),
                    upstream: Some(remote("origin", "nope")),
                },
                false
            )
            .await
        ),
        "there is no remote branch origin/nope"
    );
}

#[tokio::test]
async fn grp_8_blocked_repos_are_refused() {
    let fixture = Fixture::new().unwrap();
    let repo = local_repo(&fixture);
    let mut run = Run::new(repo).await;
    let checkout = || BranchOp::Checkout { name: name("side") };

    // Busy: another write waits or runs on the repo.
    let (release, held) = oneshot::channel::<()>();
    let job = WriteJob {
        workspace_id: run.id.clone(),
        repos: vec![run.repo_id.clone()],
        label: name("Held"),
        on_cancel: OnCancel::Finish,
        may_conflict: false,
        nothing_to_do: NothingToDo::Never,
        network: false,
    };
    run.jobs
        .start(git(&run.dirs), job, move |_| async move {
            let _ = held.await;
            Ok(Output {
                stdout: Vec::new(),
                stderr: Vec::new(),
            })
        })
        .unwrap();
    assert_eq!(
        refused(run.ask(checkout(), false).await),
        "another operation is running on the repo"
    );
    release.send(()).unwrap();
    run.ends.recv().await.unwrap();

    // In conflict state.
    run.repo.switch("side").unwrap();
    run.repo
        .commit_file("a.txt", "side's a\n", "Change a on side")
        .unwrap();
    run.repo.switch("main").unwrap();
    run.repo.merge_with_conflicts("side").unwrap();
    assert_eq!(
        refused(run.ask(checkout(), false).await),
        "the repo is in conflict state; continue or abort first"
    );
    run.repo.git(["merge", "--abort"]).unwrap();
    let ended = run.run(checkout()).await;
    assert_eq!(ended.outcome, Some(Outcome::Succeeded), "{ended:?}");

    // Missing.
    std::fs::rename(run.repo.path(), fixture.path().join("moved")).unwrap();
    assert_eq!(
        refused(run.ask(checkout(), false).await),
        "the repo's folder is missing"
    );
}
