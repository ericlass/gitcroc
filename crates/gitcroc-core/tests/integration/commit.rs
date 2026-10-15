//! Commit and amend in "Uncommitted changes" (UI-15, SEC-6) on fixture
//! repos, with real git, each as a job. git runs with a `HOME` and
//! `XDG_CONFIG_HOME` that hold no config and without the system config, so
//! only the repo's config counts: the identity, sign-off and hooks each test
//! sets. Nothing here waits for a time: each test waits for the job's end
//! event.

use std::path::PathBuf;
use std::sync::Arc;

use gitcroc_core::{
    Changes, ChangesContent, ChangesError, ChangesTarget, ChangesView, CommitMessage,
    CommitRequest, ConfigDir, GitRunner, JobEvents, JobId, JobView, Jobs, Outcome, Summaries,
    SummaryCache, SummaryView, WorkspaceId, Workspaces, add_path, changes_commit, changes_get,
};
use repo_gen::{Fixture, Repo};
use tokio::sync::mpsc;

struct Dirs {
    root: tempfile::TempDir,
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

/// A workspace with one repo, its changes view and the job engine.
struct Run {
    dirs: Dirs,
    repo: Repo,
    workspaces: Arc<Workspaces>,
    changes: Arc<Changes>,
    jobs: Arc<Jobs>,
    ends: mpsc::UnboundedReceiver<JobView>,
    id: WorkspaceId,
}

/// The repo's identity; the fixture's own commits use another.
const NAME: &str = "Test User";
const EMAIL: &str = "test@example.com";

impl Run {
    /// A run whose git reads the repo's config only; `identity` sets
    /// `user.name` and `user.email` in it.
    async fn new(repo: Repo, identity: bool) -> Self {
        repo.git(["config", "core.autocrlf", "false"]).unwrap();
        if identity {
            repo.git(["config", "user.name", NAME]).unwrap();
            repo.git(["config", "user.email", EMAIL]).unwrap();
        }
        let dirs = Dirs {
            root: tempfile::tempdir().unwrap(),
        };
        std::fs::create_dir_all(dirs.root.path().join("home")).unwrap();
        let workspaces = Arc::new(Workspaces::load(&dirs));
        let id = workspaces.views()[0].id.clone();
        let summaries = Arc::new(Summaries::load(SummaryCache::none(), &workspaces));
        let (sender, ends) = mpsc::unbounded_channel();
        let jobs = Jobs::new(workspaces.clone(), summaries, Arc::new(Ends(sender)));
        let run = Self {
            dirs,
            repo,
            workspaces,
            changes: Arc::new(Changes::new()),
            jobs,
            ends,
            id,
        };
        add_path(
            run.workspaces.clone(),
            run.id.clone(),
            run.repo.path().to_owned(),
            run.git(),
            JobId(0),
            |_| {},
        )
        .await
        .unwrap();
        run
    }

    /// git with no user or system config.
    fn git(&self) -> GitRunner {
        let home = self.dirs.root.path().join("home");
        GitRunner::new(PathBuf::from("git")).with_env([
            ("HOME", home.clone().into_os_string()),
            ("XDG_CONFIG_HOME", home.into_os_string()),
            ("GIT_CONFIG_NOSYSTEM", "1".into()),
            ("EMAIL", "".into()),
        ])
    }

    /// Reads "Uncommitted changes" as a new view.
    async fn view(&self) -> ChangesView {
        let repo = self.workspaces.view(&self.id).unwrap().repos[0].id.clone();
        changes_get(
            self.changes.clone(),
            self.workspaces.clone(),
            self.git(),
            self.id.clone(),
            repo,
            ChangesTarget::Uncommitted,
        )
        .await
        .unwrap()
        .unwrap()
    }

    /// Asks for a commit of a fresh view.
    async fn ask(&self, subject: &str, body: &str, amend: bool) -> Result<JobId, ChangesError> {
        let view = self.view().await;
        let request = CommitRequest {
            handle: view.handle,
            message: CommitMessage {
                subject: subject.to_owned(),
                body: body.to_owned(),
            },
            amend,
        };
        changes_commit(
            self.changes.clone(),
            self.jobs.clone(),
            self.git(),
            self.id.clone(),
            request,
        )
        .await
        .map(Option::unwrap)
    }

    /// Commits, or amends, to the job's end.
    async fn commit(&mut self, subject: &str, body: &str, amend: bool) -> JobView {
        let job_id = self.ask(subject, body, amend).await.unwrap();
        let ended = self.ends.recv().await.unwrap();
        assert_eq!(ended.job_id, job_id);
        ended
    }

    /// The message of `HEAD`, exactly as stored.
    fn message(&self) -> String {
        let raw = self.repo.git(["cat-file", "commit", "HEAD"]).unwrap();
        let (_, message) = raw.split_once("\n\n").unwrap();
        message.to_owned()
    }

    /// `git status --porcelain`, untracked files listed one by one.
    fn status(&self) -> String {
        self.repo
            .git(["status", "--porcelain", "--untracked-files=all"])
            .unwrap()
    }

    /// Writes hook `name` of the repo: a shell script.
    fn hook(&self, name: &str, script: &str) {
        let path = self.repo.path().join(".git").join("hooks").join(name);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, format!("#!/bin/sh\n{script}")).unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
        }
    }
}

fn new_repo(fixture: &Fixture) -> Repo {
    fixture.repo("app").unwrap()
}

fn refused(result: Result<JobId, ChangesError>) -> &'static str {
    match result {
        Err(ChangesError::Refused(why)) => why,
        other => panic!("not refused: {other:?}"),
    }
}

#[tokio::test]
async fn ui_15_commit_staged_changes() {
    let fixture = Fixture::new().unwrap();
    let repo = new_repo(&fixture);
    repo.write("staged.txt", "one\n").unwrap();
    repo.write("unstaged.txt", "one\n").unwrap();
    repo.commit_all("First").unwrap();
    repo.write("staged.txt", "two\n").unwrap();
    repo.git(["add", "staged.txt"]).unwrap();
    repo.write("unstaged.txt", "two\n").unwrap();
    repo.write("untracked.txt", "new\n").unwrap();
    let first = repo.head().unwrap();
    let mut run = Run::new(repo, true).await;

    assert_eq!(
        refused(run.ask("  ", "A body alone", false).await),
        "the commit message has no subject"
    );
    let ended = run
        .commit(
            "Change staged.txt  ",
            "Why it changed.\n\nAnd how. \n\n",
            false,
        )
        .await;
    assert_eq!(ended.outcome, Some(Outcome::Succeeded), "{ended:?}");
    assert_eq!(ended.label, "Commit");
    assert_eq!(
        run.message(),
        "Change staged.txt\n\nWhy it changed.\n\nAnd how.\n"
    );
    assert_eq!(run.repo.rev_parse("HEAD^").unwrap(), first);
    assert_eq!(run.repo.git(["show", "HEAD:staged.txt"]).unwrap(), "two\n");
    assert_eq!(run.status(), " M unstaged.txt\n?? untracked.txt\n");
    assert_eq!(
        run.repo.git(["log", "-1", "--format=%an <%ae>"]).unwrap(),
        format!("{NAME} <{EMAIL}>\n")
    );

    // Nothing is staged now.
    assert_eq!(
        refused(run.ask("Again", "", false).await),
        "nothing is staged"
    );
}

#[tokio::test]
async fn ui_15_amend_last_commit() {
    let fixture = Fixture::new().unwrap();
    let repo = new_repo(&fixture);
    let first = repo.commit_file("a.txt", "a\n", "First").unwrap();
    repo.commit_file("b.txt", "b\n", "Second\n\nIts body.")
        .unwrap();
    repo.write("c.txt", "c\n").unwrap();
    repo.git(["add", "c.txt"]).unwrap();
    let mut run = Run::new(repo, true).await;

    // Amend starts from the last commit's message.
    let view = run.view().await;
    let ChangesContent::Uncommitted { last_message, .. } = &view.content else {
        panic!("not uncommitted changes");
    };
    assert_eq!(
        *last_message,
        Some(CommitMessage {
            subject: "Second".to_owned(),
            body: "Its body.".to_owned(),
        })
    );

    let ended = run.commit("Second, amended", "", true).await;
    assert_eq!(ended.outcome, Some(Outcome::Succeeded), "{ended:?}");
    assert_eq!(ended.label, "Amend the last commit");
    assert_eq!(run.message(), "Second, amended\n");
    assert_eq!(run.repo.rev_parse("HEAD^").unwrap(), first);
    let files = run
        .repo
        .git(["show", "--name-only", "--format=", "HEAD"])
        .unwrap();
    assert_eq!(files, "b.txt\nc.txt\n");
    assert_eq!(run.status(), "");

    // An amend needs no staged changes: the message alone is replaced.
    let ended = run.commit("Second, again", "", true).await;
    assert_eq!(ended.outcome, Some(Outcome::Succeeded), "{ended:?}");
    assert_eq!(run.message(), "Second, again\n");
    assert_eq!(run.repo.rev_parse("HEAD^").unwrap(), first);
}

#[tokio::test]
async fn ui_15_amend_needs_a_commit_and_a_commit_needs_no_conflict() {
    let fixture = Fixture::new().unwrap();
    let repo = new_repo(&fixture);
    repo.write("a.txt", "a\n").unwrap();
    repo.git(["add", "a.txt"]).unwrap();
    let mut run = Run::new(repo, true).await;

    // A branch with no commit yet: nothing to amend, but a first commit.
    let view = run.view().await;
    let ChangesContent::Uncommitted { last_message, .. } = &view.content else {
        panic!("not uncommitted changes");
    };
    assert_eq!(*last_message, None);
    assert_eq!(
        refused(run.ask("Amend", "", true).await),
        "the branch has no commit to amend"
    );
    let ended = run.commit("First", "", false).await;
    assert_eq!(ended.outcome, Some(Outcome::Succeeded), "{ended:?}");
    assert_eq!(run.message(), "First\n");

    // A merge stopped on a conflict: its commit is Continue's (M2-08).
    run.repo.branch("side").unwrap();
    run.repo.commit_file("a.txt", "main\n", "On main").unwrap();
    run.repo.switch("side").unwrap();
    run.repo.commit_file("a.txt", "side\n", "On side").unwrap();
    run.repo.switch("main").unwrap();
    run.repo.merge_with_conflicts("side").unwrap();
    run.repo.write("a.txt", "both\n").unwrap();
    run.repo.git(["add", "a.txt"]).unwrap();
    assert_eq!(
        refused(run.ask("Merge", "", false).await),
        "the repo is in conflict state; Continue makes the commit"
    );
    assert_eq!(
        refused(run.ask("Merge", "", true).await),
        "the repo is in conflict state; Continue makes the commit"
    );
}

#[tokio::test]
async fn sec_6_hooks_run_as_on_the_command_line() {
    let fixture = Fixture::new().unwrap();
    let repo = new_repo(&fixture);
    repo.commit_file("a.txt", "a\n", "First").unwrap();
    repo.write("a.txt", "b\n").unwrap();
    repo.git(["add", "a.txt"]).unwrap();
    let head = repo.head().unwrap();
    let mut run = Run::new(repo, true).await;

    run.hook("pre-commit", "echo 'pre-commit says no'\nexit 1\n");
    let ended = run.commit("Rejected", "", false).await;
    assert_eq!(ended.outcome, Some(Outcome::Failed), "{ended:?}");
    assert!(ended.output.contains("pre-commit says no"), "{ended:?}");
    assert_eq!(run.repo.head().unwrap(), head);
    assert_eq!(run.status(), "M  a.txt\n");

    std::fs::remove_file(run.repo.path().join(".git/hooks/pre-commit")).unwrap();
    run.hook("commit-msg", "echo 'Edited-by: commit-msg' >> \"$1\"\n");
    let ended = run.commit("Accepted", "", false).await;
    assert_eq!(ended.outcome, Some(Outcome::Succeeded), "{ended:?}");
    assert_eq!(run.message(), "Accepted\nEdited-by: commit-msg\n");
}

#[tokio::test]
async fn sec_6_sign_off_follows_git_config() {
    let fixture = Fixture::new().unwrap();
    let repo = new_repo(&fixture);
    repo.commit_file("a.txt", "a\n", "First").unwrap();
    let mut run = Run::new(repo, true).await;

    run.repo.git(["config", "format.signOff", "true"]).unwrap();
    run.repo.write("a.txt", "b\n").unwrap();
    run.repo.git(["add", "a.txt"]).unwrap();
    let ended = run.commit("Signed", "", false).await;
    assert_eq!(ended.outcome, Some(Outcome::Succeeded), "{ended:?}");
    assert_eq!(
        run.message(),
        format!("Signed\n\nSigned-off-by: {NAME} <{EMAIL}>\n")
    );

    run.repo.git(["config", "format.signOff", "false"]).unwrap();
    run.repo.write("a.txt", "c\n").unwrap();
    run.repo.git(["add", "a.txt"]).unwrap();
    let ended = run.commit("Not signed", "", false).await;
    assert_eq!(ended.outcome, Some(Outcome::Succeeded), "{ended:?}");
    assert_eq!(run.message(), "Not signed\n");
}

#[tokio::test]
async fn ui_15_commit_without_an_identity_fails_with_gits_message() {
    let fixture = Fixture::new().unwrap();
    let repo = new_repo(&fixture);
    repo.commit_file("a.txt", "a\n", "First").unwrap();
    repo.write("a.txt", "b\n").unwrap();
    repo.git(["add", "a.txt"]).unwrap();
    // git would otherwise guess an address from the user and host names.
    repo.git(["config", "user.useConfigOnly", "true"]).unwrap();
    let head = repo.head().unwrap();
    let mut run = Run::new(repo, false).await;

    let ended = run.commit("Nobody", "", false).await;
    assert_eq!(ended.outcome, Some(Outcome::Failed), "{ended:?}");
    assert!(
        ended.output.contains("Please tell me who you are"),
        "{ended:?}"
    );
    assert_eq!(run.repo.head().unwrap(), head);
    assert_eq!(run.status(), "M  a.txt\n");
}
