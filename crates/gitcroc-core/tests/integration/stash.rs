//! Stash, apply, pop and drop (section 6 "Stash", GRP-7, GRP-8, GRP-12)
//! and the conflict state a stash apply leaves (OPS-1, OPS-2, OPS-5) with
//! real git, each as a job. Each test waits for the job's end event;
//! nothing waits for a time.

use std::path::PathBuf;
use std::sync::Arc;

use gitcroc_core::{
    Changes, ChangesContent, ChangesTarget, ConfigDir, ConflictStart, GitRepo, GitRunner,
    IntegrateError, IntegrateOp, IntegrateRequest, IntegrateStart, JobEvents, JobId, JobView, Jobs,
    Operation, Outcome, RepoId, ResolveRequest, StashEntry, StashError, StashOp, StashRef,
    StashRequest, StashStart, StatusRead, StatusReader, Summaries, SummaryCache, SummaryView,
    WorkspaceId, Workspaces, add_path, changes_get, conflict_details, conflict_resolve,
    op_integrate, op_stash,
};
use repo_gen::{Fixture, Repo};
use tokio::sync::mpsc;

struct Dirs {
    root: tempfile::TempDir,
}

/// git with no user or system config, so no hook or setting of the machine
/// takes part, and an identity for the stash commits it makes.
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

/// A workspace with one repo, its changes view and the job engine.
struct Run {
    dirs: Dirs,
    workspaces: Arc<Workspaces>,
    summaries: Arc<Summaries>,
    changes: Arc<Changes>,
    jobs: Arc<Jobs>,
    ends: mpsc::UnboundedReceiver<JobView>,
    id: WorkspaceId,
    repo_id: RepoId,
    path: PathBuf,
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
            changes: Arc::new(Changes::new()),
            jobs,
            ends,
            id,
            repo_id,
            path: repo.path().to_owned(),
        }
    }

    async fn ask(&self, op: StashOp, confirmed: bool) -> Result<StashStart, StashError> {
        op_stash(
            self.workspaces.clone(),
            self.summaries.clone(),
            self.jobs.clone(),
            git(&self.dirs),
            self.id.clone(),
            StashRequest {
                repo_id: self.repo_id.clone(),
                op,
                confirmed,
            },
        )
        .await
    }

    async fn ended(&mut self, job_id: JobId) -> JobView {
        let ended = self.ends.recv().await.unwrap();
        assert_eq!(ended.job_id, job_id);
        ended
    }

    /// Runs `op`, confirmed, to its job's end.
    async fn run(&mut self, op: StashOp) -> JobView {
        let start = self.ask(op, true).await.unwrap();
        let StashStart::Started { job_id } = start else {
            panic!("not started: {start:?}");
        };
        self.ended(job_id).await
    }

    async fn integrate(
        &self,
        op: IntegrateOp,
        confirmed: bool,
    ) -> Result<IntegrateStart, IntegrateError> {
        op_integrate(
            self.workspaces.clone(),
            self.summaries.clone(),
            self.jobs.clone(),
            git(&self.dirs),
            self.id.clone(),
            IntegrateRequest {
                repo_id: self.repo_id.clone(),
                op,
                confirmed,
            },
        )
        .await
    }

    /// The stash list as the tree reads it.
    fn stashes(&self) -> Vec<StashEntry> {
        GitRepo::open(&self.path).unwrap().stashes()
    }

    /// `stash@{index}` as the tree showed it.
    fn stash(&self, index: u32) -> StashRef {
        let entry = &self.stashes()[usize::try_from(index).unwrap()];
        StashRef {
            index,
            hash: entry.hash.clone(),
        }
    }

    fn state(&self) -> gitcroc_core::RepoState {
        StatusReader::new(GitRepo::open(&self.path).unwrap(), git(&self.dirs))
            .repo_state()
            .unwrap()
    }

    /// Mark resolved of unmerged `path` with `text` (OPS-5), to its job's
    /// end.
    async fn mark_resolved(&mut self, path: &str, text: &str) -> JobView {
        let view = changes_get(
            self.changes.clone(),
            self.workspaces.clone(),
            git(&self.dirs),
            self.id.clone(),
            self.repo_id.clone(),
            ChangesTarget::Uncommitted,
        )
        .await
        .unwrap()
        .unwrap();
        let ChangesContent::Uncommitted { unmerged, .. } = view.content else {
            panic!("not the working copy");
        };
        let file = unmerged.iter().find(|file| file.path == path).unwrap();
        let start = conflict_resolve(
            self.changes.clone(),
            self.jobs.clone(),
            git(&self.dirs),
            self.id.clone(),
            ResolveRequest {
                handle: view.handle,
                file: u32::try_from(file.id).unwrap(),
                text: text.to_owned(),
                override_markers: false,
            },
        )
        .await
        .unwrap()
        .unwrap();
        let ConflictStart::Started { job_id } = start else {
            panic!("not started: {start:?}");
        };
        self.ended(job_id).await
    }
}

fn push(message: Option<&str>, include_untracked: bool) -> StashOp {
    StashOp::Push {
        message: message.map(str::to_owned),
        include_untracked,
    }
}

fn refused(result: Result<StashStart, StashError>) -> String {
    match result {
        Err(error @ StashError::Refused(_)) => error.to_string(),
        other => panic!("not refused: {other:?}"),
    }
}

fn outcome(view: &JobView, expected: Outcome) {
    assert_eq!(view.outcome, Some(expected), "{view:#?}");
}

fn read(repo: &Repo, file: &str) -> String {
    std::fs::read_to_string(repo.path().join(file)).unwrap()
}

fn messages(run: &Run) -> Vec<String> {
    run.stashes()
        .into_iter()
        .map(|entry| entry.message)
        .collect()
}

/// `main` at "Base" with `a.txt` and `b.txt`.
fn base() -> (Fixture, Repo) {
    let fixture = Fixture::new().unwrap();
    let repo = fixture.repo("app").unwrap();
    repo.write("a.txt", "a\n").unwrap();
    repo.write("b.txt", "b\n").unwrap();
    repo.commit_all("Base").unwrap();
    (fixture, repo)
}

/// [`base`] with a stash that changes `a.txt` to "stashed" and `c.txt`
/// (new, staged), then a commit that changes `a.txt` to "committed": an
/// apply conflicts in `a.txt` and brings `c.txt` in cleanly.
fn conflicting() -> (Fixture, Repo) {
    let (fixture, repo) = base();
    repo.write("a.txt", "stashed\n").unwrap();
    repo.write("c.txt", "c\n").unwrap();
    repo.git(["add", "c.txt"]).unwrap();
    repo.stash("work").unwrap();
    repo.commit_file("a.txt", "committed\n", "Committed")
        .unwrap();
    (fixture, repo)
}

// Stash.

#[tokio::test]
async fn stash_with_a_message_and_untracked_files() {
    let (_fixture, repo) = base();
    repo.write("a.txt", "changed\n").unwrap();
    repo.write("new.txt", "new\n").unwrap();
    let mut run = Run::new(&repo).await;

    let ended = run.run(push(Some("  my work  "), true)).await;
    outcome(&ended, Outcome::Succeeded);
    assert_eq!(ended.label, "Stash changes as \"my work\"");
    assert_eq!(messages(&run), ["On main: my work"]);
    assert_eq!(read(&repo, "a.txt"), "a\n");
    assert!(!repo.path().join("new.txt").exists());
    // The untracked file is in the stash's third parent.
    assert_eq!(repo.git(["show", "stash@{0}^3:new.txt"]).unwrap(), "new\n");
}

#[tokio::test]
async fn stash_without_a_message_leaves_untracked_files() {
    let (_fixture, repo) = base();
    repo.write("a.txt", "changed\n").unwrap();
    repo.write("new.txt", "new\n").unwrap();
    let mut run = Run::new(&repo).await;

    // A blank message is no message: git's own.
    outcome(&run.run(push(Some("   "), false)).await, Outcome::Succeeded);
    let listed = messages(&run);
    assert_eq!(listed.len(), 1);
    assert!(listed[0].starts_with("WIP on main: "), "{listed:?}");
    assert_eq!(read(&repo, "a.txt"), "a\n");
    assert_eq!(read(&repo, "new.txt"), "new\n");
}

#[tokio::test]
async fn grp_7_stash_of_a_clean_working_copy_is_nothing_to_do() {
    let (_fixture, repo) = base();
    let mut run = Run::new(&repo).await;
    outcome(&run.run(push(None, true)).await, Outcome::NothingToDo);

    // Untracked files alone, left out, are nothing to stash either.
    repo.write("new.txt", "new\n").unwrap();
    outcome(&run.run(push(None, false)).await, Outcome::NothingToDo);
    assert!(run.stashes().is_empty());
    assert_eq!(read(&repo, "new.txt"), "new\n");
}

#[tokio::test]
async fn stash_message_with_a_nul_character_is_refused() {
    let (_fixture, repo) = base();
    repo.write("a.txt", "changed\n").unwrap();
    let run = Run::new(&repo).await;
    let why = refused(run.ask(push(Some("a\0b"), false), false).await);
    assert!(why.contains("NUL"), "{why}");
    assert!(run.stashes().is_empty());
}

// Apply and pop.

#[tokio::test]
async fn stash_apply_keeps_the_entry_and_pop_drops_it() {
    let (_fixture, repo) = base();
    repo.write("a.txt", "first\n").unwrap();
    repo.stash("first").unwrap();
    repo.write("b.txt", "second\n").unwrap();
    repo.stash("second").unwrap();
    let mut run = Run::new(&repo).await;

    let first = run.stash(1);
    let ended = run.run(StashOp::Apply { stash: first }).await;
    outcome(&ended, Outcome::Succeeded);
    assert_eq!(ended.label, "Apply stash@{1} (On main: first)");
    assert_eq!(read(&repo, "a.txt"), "first\n");
    assert_eq!(messages(&run), ["On main: second", "On main: first"]);

    let second = run.stash(0);
    let ended = run.run(StashOp::Pop { stash: second }).await;
    outcome(&ended, Outcome::Succeeded);
    assert_eq!(read(&repo, "b.txt"), "second\n");
    assert_eq!(messages(&run), ["On main: first"]);
}

#[tokio::test]
async fn grp_12_apply_over_conflicting_local_changes_fails_and_touches_nothing() {
    let (_fixture, repo) = base();
    repo.write("a.txt", "stashed\n").unwrap();
    repo.stash("work").unwrap();
    repo.write("a.txt", "local\n").unwrap();
    let mut run = Run::new(&repo).await;

    for op in [
        StashOp::Apply {
            stash: run.stash(0),
        },
        StashOp::Pop {
            stash: run.stash(0),
        },
    ] {
        let ended = run.run(op).await;
        outcome(&ended, Outcome::Failed);
        assert!(ended.output.contains("a.txt"), "{}", ended.output);
        assert_eq!(read(&repo, "a.txt"), "local\n");
        assert_eq!(messages(&run), ["On main: work"]);
        assert!(!run.state().in_conflict());
    }
}

#[tokio::test]
async fn ops_1_a_conflicting_apply_ends_conflict_and_mark_resolved_ends_the_state() {
    let (_fixture, repo) = conflicting();
    let mut run = Run::new(&repo).await;

    let stash = run.stash(0);
    outcome(&run.run(StashOp::Apply { stash }).await, Outcome::Conflict);
    let state = run.state();
    assert_eq!(state.operation, Some(Operation::StashApply));
    assert_eq!(state.unmerged_paths, 1);
    let details = conflict_details(
        run.workspaces.clone(),
        git(&run.dirs),
        run.id.clone(),
        run.repo_id.clone(),
    )
    .await
    .unwrap();
    assert_eq!(details.operation, Some(Operation::StashApply));
    assert_eq!(details.onto.as_deref(), Some("main"));

    // GRP-8: no other write while in conflict state.
    repo.write("b.txt", "more\n").unwrap();
    let why = refused(run.ask(push(None, false), false).await);
    assert!(why.contains("conflict state"), "{why}");
    // Nothing to continue: the state ends with the last Mark resolved.
    let start = run.integrate(IntegrateOp::Continue, true).await;
    assert!(
        matches!(start, Err(IntegrateError::Refused(_))),
        "{start:?}"
    );

    outcome(
        &run.mark_resolved("a.txt", "resolved\n").await,
        Outcome::Succeeded,
    );
    assert_eq!(run.state(), gitcroc_core::RepoState::default());
    assert_eq!(read(&repo, "a.txt"), "resolved\n");
    assert_eq!(read(&repo, "c.txt"), "c\n");
    // An apply keeps the entry in any case.
    assert_eq!(messages(&run), ["On main: work"]);
}

#[tokio::test]
async fn a_conflicting_pop_keeps_the_stash() {
    let (_fixture, repo) = conflicting();
    let mut run = Run::new(&repo).await;

    let stash = run.stash(0);
    let ended = run
        .run(StashOp::Pop {
            stash: stash.clone(),
        })
        .await;
    outcome(&ended, Outcome::Conflict);
    assert_eq!(run.state().operation, Some(Operation::StashApply));
    assert_eq!(run.stash(0), stash);

    outcome(
        &run.mark_resolved("a.txt", "resolved\n").await,
        Outcome::Succeeded,
    );
    assert!(!run.state().in_conflict());
    // What the app offers next: dropping the stash git kept.
    let start = run.ask(StashOp::Drop { stash }, false).await.unwrap();
    assert!(matches!(start, StashStart::Confirm { .. }), "{start:?}");
}

// Drop.

#[tokio::test]
async fn stash_drop_needs_confirmation() {
    let (_fixture, repo) = base();
    repo.write("a.txt", "first\n").unwrap();
    repo.stash("first").unwrap();
    repo.write("a.txt", "second\n").unwrap();
    repo.stash("second").unwrap();
    let mut run = Run::new(&repo).await;

    let first = run.stash(1);
    let start = run
        .ask(
            StashOp::Drop {
                stash: first.clone(),
            },
            false,
        )
        .await
        .unwrap();
    let StashStart::Confirm {
        repo: name,
        actions,
    } = start
    else {
        panic!("not asked: {start:?}");
    };
    assert_eq!(name, "app");
    assert_eq!(actions.len(), 1);
    assert!(
        actions[0].contains("stash@{1} (On main: first)"),
        "{actions:?}"
    );
    // Nothing ran.
    assert_eq!(messages(&run), ["On main: second", "On main: first"]);

    let ended = run.run(StashOp::Drop { stash: first }).await;
    outcome(&ended, Outcome::Succeeded);
    assert_eq!(messages(&run), ["On main: second"]);
}

#[tokio::test]
async fn a_stash_list_changed_since_it_was_read_is_refused() {
    let (_fixture, repo) = base();
    repo.write("a.txt", "first\n").unwrap();
    repo.stash("first").unwrap();
    let run = Run::new(&repo).await;
    let read_before = run.stash(0);

    // A stash made in a terminal meanwhile: stash@{0} is another entry.
    repo.write("b.txt", "second\n").unwrap();
    repo.stash("second").unwrap();
    for op in [
        StashOp::Apply {
            stash: read_before.clone(),
        },
        StashOp::Pop {
            stash: read_before.clone(),
        },
        StashOp::Drop {
            stash: read_before.clone(),
        },
    ] {
        let why = refused(run.ask(op, true).await);
        assert!(why.contains("stash list changed"), "{why}");
    }
    let gone = StashRef {
        index: 5,
        hash: read_before.hash,
    };
    refused(run.ask(StashOp::Drop { stash: gone }, true).await);
    assert_eq!(messages(&run), ["On main: second", "On main: first"]);
    assert_eq!(read(&repo, "a.txt"), "a\n");
}

// Abort.

#[tokio::test]
async fn ops_2_abort_of_a_stash_apply_restores_the_touched_files_and_keeps_other_changes() {
    let (_fixture, repo) = conflicting();
    // An unrelated local change the apply does not touch.
    repo.write("b.txt", "local\n").unwrap();
    let mut run = Run::new(&repo).await;

    let stash = run.stash(0);
    outcome(&run.run(StashOp::Pop { stash }).await, Outcome::Conflict);
    assert_eq!(read(&repo, "c.txt"), "c\n");

    // It cannot be undone: the core asks first, and nothing runs.
    let start = run.integrate(IntegrateOp::Abort, false).await.unwrap();
    let IntegrateStart::Confirm { actions, .. } = start else {
        panic!("not asked: {start:?}");
    };
    assert!(actions[0].contains("back to HEAD"), "{actions:?}");
    assert_eq!(run.state().operation, Some(Operation::StashApply));

    let start = run.integrate(IntegrateOp::Abort, true).await.unwrap();
    let IntegrateStart::Started { job_id } = start else {
        panic!("not started: {start:?}");
    };
    let ended = run.ended(job_id).await;
    outcome(&ended, Outcome::Succeeded);
    assert_eq!(ended.label, "Abort the stash apply");
    assert!(!run.state().in_conflict());
    assert_eq!(read(&repo, "a.txt"), "committed\n");
    assert!(!repo.path().join("c.txt").exists());
    assert_eq!(read(&repo, "b.txt"), "local\n");
    // git kept the stash of the conflicting pop.
    assert_eq!(messages(&run), ["On main: work"]);
}
