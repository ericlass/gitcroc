//! Reset of the current branch to a commit (section 6 "Reset", UI-13,
//! GRP-8, GRP-9) with real git, as a job: the index and the working tree
//! after a soft, mixed and hard reset as git defines each, and the
//! confirmation a hard reset needs. Each test waits for the job's end
//! event; nothing waits for a time.

// Integration test crate: helpers outside #[test] fns are test code too (QA-4).
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::path::PathBuf;
use std::sync::Arc;

use gitcroc_core::{
    ConfigDir, GitRunner, JobEvents, JobId, JobView, Jobs, Outcome, RepoId, ResetError, ResetMode,
    ResetRequest, ResetStart, Summaries, SummaryCache, SummaryView, WorkspaceId, Workspaces,
    add_path, op_reset,
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

    async fn ask(
        &self,
        commit: &str,
        mode: ResetMode,
        confirmed: bool,
    ) -> Result<ResetStart, ResetError> {
        op_reset(
            self.workspaces.clone(),
            self.summaries.clone(),
            self.jobs.clone(),
            git(&self.dirs),
            self.id.clone(),
            ResetRequest {
                repo_id: self.repo_id.clone(),
                commit: commit.to_owned(),
                mode,
                confirmed,
            },
        )
        .await
    }

    /// Resets to `commit`, confirmed, to its job's end.
    async fn run(&mut self, commit: &str, mode: ResetMode) -> JobView {
        let start = self.ask(commit, mode, true).await.unwrap();
        let ResetStart::Started { job_id } = start else {
            panic!("not started: {start:?}");
        };
        let ended = self.ends.recv().await.unwrap();
        assert_eq!(ended.job_id, job_id);
        ended
    }
}

fn refused(result: Result<ResetStart, ResetError>) -> String {
    match result {
        Err(error @ ResetError::Refused(_)) => error.to_string(),
        other => panic!("not refused: {other:?}"),
    }
}

fn succeeded(view: &JobView) {
    assert_eq!(view.outcome, Some(Outcome::Succeeded), "{view:#?}");
}

/// `app` on `main` with three commits, each adding a file; returns it and
/// the first commit.
fn three_commits(fixture: &Fixture) -> (Repo, String) {
    let app = fixture.repo("app").unwrap();
    let first = app.commit_file("a.txt", "a\n", "First").unwrap();
    app.commit_file("b.txt", "b\n", "Second").unwrap();
    app.commit_file("c.txt", "c\n", "Third").unwrap();
    (app, first)
}

/// `git status --porcelain`, one line per path.
fn status(repo: &Repo) -> String {
    repo.git(["status", "--porcelain", "--untracked-files=all"])
        .unwrap()
}

fn short(repo: &Repo, commit: &str) -> String {
    repo.git(["rev-parse", "--short", commit])
        .unwrap()
        .trim()
        .to_owned()
}

#[tokio::test]
async fn reset_soft_moves_the_branch_and_keeps_the_index_and_the_working_tree() {
    let fixture = Fixture::new().unwrap();
    let (app, first) = three_commits(&fixture);
    app.write("a.txt", "changed\n").unwrap();
    let mut run = Run::new(&app).await;
    let ended = run.run(&first, ResetMode::Soft).await;
    succeeded(&ended);
    assert_eq!(
        ended.label,
        format!("Reset main to {} (soft)", short(&app, &first))
    );
    assert_eq!(app.head().unwrap(), first);
    // The later commits' files are staged; the edit stays unstaged.
    assert_eq!(status(&app), " M a.txt\nA  b.txt\nA  c.txt\n");
}

#[tokio::test]
async fn reset_mixed_resets_the_index_and_keeps_the_working_tree() {
    let fixture = Fixture::new().unwrap();
    let (app, first) = three_commits(&fixture);
    app.write("a.txt", "changed\n").unwrap();
    app.git(["add", "a.txt"]).unwrap();
    let mut run = Run::new(&app).await;
    succeeded(&run.run(&first, ResetMode::Mixed).await);
    assert_eq!(app.head().unwrap(), first);
    // Nothing staged; the later commits' files are untracked now.
    assert_eq!(status(&app), " M a.txt\n?? b.txt\n?? c.txt\n");
    assert_eq!(
        std::fs::read_to_string(app.path().join("a.txt")).unwrap(),
        "changed\n"
    );
}

#[tokio::test]
async fn grp_9_hard_reset_needs_confirmation() {
    let fixture = Fixture::new().unwrap();
    let (app, first) = three_commits(&fixture);
    let third = app.head().unwrap();
    // One unstaged edit, one staged new file, one untracked file.
    app.write("a.txt", "changed\n").unwrap();
    app.write("new.txt", "new\n").unwrap();
    app.git(["add", "new.txt"]).unwrap();
    app.write("scratch.txt", "mine\n").unwrap();
    let mut run = Run::new(&app).await;

    // Not confirmed: the repo, the branch, the commit and how many files'
    // changes are lost; nothing runs.
    assert_eq!(
        run.ask(&first, ResetMode::Hard, false).await.unwrap(),
        ResetStart::Confirm {
            repo: "app".to_owned(),
            actions: vec![
                format!("Reset branch main to {} First", short(&app, &first)),
                "Discard the uncommitted changes to 2 files; untracked files stay".to_owned(),
            ],
        }
    );
    assert!(run.jobs.all_views().is_empty());
    assert_eq!(app.head().unwrap(), third);

    let ended = run.run(&first, ResetMode::Hard).await;
    succeeded(&ended);
    assert_eq!(
        ended.label,
        format!("Reset main to {} (hard)", short(&app, &first))
    );
    assert_eq!(app.head().unwrap(), first);
    // Only the untracked file is left.
    assert_eq!(status(&app), "?? scratch.txt\n");
    assert_eq!(
        std::fs::read_to_string(app.path().join("a.txt")).unwrap(),
        "a\n"
    );

    // Soft and mixed need none.
    assert!(matches!(
        run.ask(&third, ResetMode::Soft, false).await.unwrap(),
        ResetStart::Started { .. }
    ));
}

#[tokio::test]
async fn hard_reset_of_a_clean_working_copy_still_asks_and_says_nothing_is_lost() {
    let fixture = Fixture::new().unwrap();
    let (app, first) = three_commits(&fixture);
    let run = Run::new(&app).await;
    let ResetStart::Confirm { actions, .. } =
        run.ask(&first, ResetMode::Hard, false).await.unwrap()
    else {
        panic!("no confirmation");
    };
    assert_eq!(
        actions[1],
        "No uncommitted changes are lost; untracked files stay"
    );
}

#[tokio::test]
async fn reset_of_a_detached_head_resets_head_and_says_so() {
    let fixture = Fixture::new().unwrap();
    let (app, first) = three_commits(&fixture);
    let third = app.head().unwrap();
    app.git(["switch", "--quiet", "--detach", "HEAD~1"])
        .unwrap();
    let mut run = Run::new(&app).await;

    let ResetStart::Confirm { actions, .. } =
        run.ask(&first, ResetMode::Hard, false).await.unwrap()
    else {
        panic!("no confirmation");
    };
    assert_eq!(
        actions[0],
        format!(
            "HEAD is detached: reset HEAD, not a branch, to {} First",
            short(&app, &first)
        )
    );
    let ended = run.run(&first, ResetMode::Mixed).await;
    succeeded(&ended);
    assert_eq!(
        ended.label,
        format!("Reset HEAD (detached) to {} (mixed)", short(&app, &first))
    );
    assert_eq!(app.head().unwrap(), first);
    // No branch moved.
    assert_eq!(app.rev_parse("main").unwrap(), third);
}

#[tokio::test]
async fn reset_refuses_what_names_no_commit_and_a_repo_in_conflict_state() {
    let fixture = Fixture::new().unwrap();
    let (app, _) = three_commits(&fixture);
    let run = Run::new(&app).await;
    for commit in ["", "--hard", "0123abc"] {
        assert_eq!(
            refused(run.ask(commit, ResetMode::Soft, false).await),
            format!("'{commit}' names no commit")
        );
    }

    app.git(["switch", "--quiet", "-c", "side"]).unwrap();
    app.commit_file("a.txt", "side\n", "Side").unwrap();
    app.switch("main").unwrap();
    app.commit_file("a.txt", "main\n", "Main").unwrap();
    app.merge_with_conflicts("side").unwrap();
    let head = app.head().unwrap();
    assert_eq!(
        refused(run.ask(&head, ResetMode::Hard, true).await),
        "the repo is in conflict state; continue or abort first"
    );
    assert!(run.jobs.all_views().is_empty());
}
