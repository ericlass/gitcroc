//! Merge, rebase, cherry-pick and revert, and abort and continue from
//! conflict state (section 6 "Integrate", OPS-1, OPS-2, GRP-8, GRP-12,
//! OPS-7) with real git, each as a job. Each test waits for the job's end
//! event; nothing waits for a time. A conflict is resolved the way the user
//! would in a terminal: the file written and `git add`ed by the fixture.

// Integration test crate: helpers outside #[test] fns are test code too (QA-4).
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::path::PathBuf;
use std::sync::Arc;

use gitcroc_core::{
    ConfigDir, ConflictDetails, GitRunner, IntegrateError, IntegrateOp, IntegrateRequest,
    IntegrateStart, JobEvents, JobId, JobView, Jobs, MergeFastForward, Operation, Outcome, RepoId,
    Step, Summaries, SummaryCache, SummaryView, WorkspaceId, Workspaces, add_path,
    conflict_details, merge_targets, op_integrate,
};
use repo_gen::{Fixture, Repo};
use tokio::sync::mpsc;

struct Dirs {
    root: tempfile::TempDir,
}

/// git with no user or system config, so no hook or setting of the machine
/// takes part, and an identity for the commits it makes.
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

    async fn ask(&self, op: IntegrateOp) -> Result<IntegrateStart, IntegrateError> {
        op_integrate(
            self.workspaces.clone(),
            self.summaries.clone(),
            self.jobs.clone(),
            git(&self.dirs),
            self.id.clone(),
            IntegrateRequest {
                repo_id: self.repo_id.clone(),
                op,
            },
        )
        .await
    }

    /// Runs `op` to its job's end.
    async fn run(&mut self, op: IntegrateOp) -> JobView {
        let start = self.ask(op).await.unwrap();
        let IntegrateStart::Started { job_id } = start else {
            panic!("not started: {start:?}");
        };
        let ended = self.ends.recv().await.unwrap();
        assert_eq!(ended.job_id, job_id);
        ended
    }

    async fn details(&self) -> ConflictDetails {
        conflict_details(
            self.workspaces.clone(),
            git(&self.dirs),
            self.id.clone(),
            self.repo_id.clone(),
        )
        .await
        .unwrap()
    }

    async fn targets(&self, filter: &str) -> Vec<String> {
        merge_targets(
            self.workspaces.clone(),
            self.id.clone(),
            self.repo_id.clone(),
            filter.to_owned(),
        )
        .await
        .unwrap()
    }
}

fn refused(result: Result<IntegrateStart, IntegrateError>) -> String {
    match result {
        Err(error @ IntegrateError::Refused(_)) => error.to_string(),
        other => panic!("not refused: {other:?}"),
    }
}

fn outcome(view: &JobView, expected: Outcome) {
    assert_eq!(view.outcome, Some(expected), "{view:#?}");
}

fn merge(revision: &str, fast_forward: MergeFastForward) -> IntegrateOp {
    IntegrateOp::Merge {
        revision: revision.to_owned(),
        fast_forward,
    }
}

fn rebase(onto: &str) -> IntegrateOp {
    IntegrateOp::Rebase {
        onto: onto.to_owned(),
    }
}

fn cherry_pick(commits: &[&String]) -> IntegrateOp {
    IntegrateOp::CherryPick {
        commits: commits.iter().map(|commit| (*commit).clone()).collect(),
    }
}

fn revert(commit: &str, mainline: Option<u32>) -> IntegrateOp {
    IntegrateOp::Revert {
        commit: commit.to_owned(),
        mainline,
    }
}

fn has_ref(repo: &Repo, name: &str) -> bool {
    repo.git_output(["rev-parse", "--verify", "--quiet", name])
        .unwrap()
        .status
        .success()
}

fn read(repo: &Repo, file: &str) -> String {
    std::fs::read_to_string(repo.path().join(file)).unwrap()
}

/// The parents of `revision`.
fn parents(repo: &Repo, revision: &str) -> usize {
    repo.git(["rev-list", "--parents", "-n", "1", revision])
        .unwrap()
        .split_whitespace()
        .count()
        - 1
}

/// The subjects of the first `count` commits of `HEAD`, newest first.
fn subjects(repo: &Repo, count: usize) -> Vec<String> {
    repo.git(["log", "--format=%s", "-n", &count.to_string()])
        .unwrap()
        .lines()
        .map(str::to_owned)
        .collect()
}

/// The user resolves `file` in a terminal: writes it and stages it.
fn resolve(repo: &Repo, file: &str, contents: &str) {
    repo.write(file, contents).unwrap();
    repo.git(["add", "--", file]).unwrap();
}

/// `main` at "Base" with `a.txt`, and `feature` one commit ahead of it with
/// `b.txt`; `main` checked out.
fn ahead() -> (Fixture, Repo) {
    let fixture = Fixture::new().unwrap();
    let repo = fixture.repo("app").unwrap();
    repo.commit_file("a.txt", "a\n", "Base").unwrap();
    repo.branch("feature").unwrap();
    repo.switch("feature").unwrap();
    repo.commit_file("b.txt", "b\n", "Feature").unwrap();
    repo.switch("main").unwrap();
    (fixture, repo)
}

/// `main` and `feature` both changed `a.txt` since "Base"; `main` checked
/// out.
fn conflicting() -> (Fixture, Repo) {
    let fixture = Fixture::new().unwrap();
    let repo = fixture.repo("app").unwrap();
    repo.commit_file("a.txt", "a\n", "Base").unwrap();
    repo.branch("feature").unwrap();
    repo.commit_file("a.txt", "main\n", "Main").unwrap();
    repo.switch("feature").unwrap();
    repo.commit_file("a.txt", "feature\n", "Feature").unwrap();
    repo.switch("main").unwrap();
    (fixture, repo)
}

// Merge.

#[tokio::test]
async fn merge_fast_forwards() {
    let (_fixture, repo) = ahead();
    let feature = repo.rev_parse("feature").unwrap();
    let mut run = Run::new(&repo).await;

    let ended = run.run(merge("feature", MergeFastForward::Config)).await;
    outcome(&ended, Outcome::Succeeded);
    assert_eq!(ended.label, "Merge feature into main");
    assert_eq!(repo.head().unwrap(), feature);
}

#[tokio::test]
async fn merge_no_ff_makes_a_merge_commit() {
    let (_fixture, repo) = ahead();
    let mut run = Run::new(&repo).await;

    outcome(
        &run.run(merge("feature", MergeFastForward::NoFf)).await,
        Outcome::Succeeded,
    );
    assert_eq!(parents(&repo, "HEAD"), 2);
    // git's prepared message (`--no-edit`).
    assert_eq!(subjects(&repo, 1), ["Merge branch 'feature'"]);
}

#[tokio::test]
async fn merge_ff_only_fails_on_diverged_history() {
    let (_fixture, repo) = ahead();
    repo.commit_file("c.txt", "c\n", "Main").unwrap();
    let before = repo.head().unwrap();
    let mut run = Run::new(&repo).await;

    let ended = run.run(merge("feature", MergeFastForward::FfOnly)).await;
    outcome(&ended, Outcome::Failed);
    assert!(ended.message.is_some(), "git's message: {ended:#?}");
    assert_eq!(repo.head().unwrap(), before);
}

#[tokio::test]
async fn merge_of_a_contained_ref_is_nothing_to_do() {
    let (_fixture, repo) = ahead();
    repo.switch("feature").unwrap();
    let before = repo.head().unwrap();
    let mut run = Run::new(&repo).await;

    for fast_forward in [MergeFastForward::Config, MergeFastForward::NoFf] {
        outcome(
            &run.run(merge("main", fast_forward)).await,
            Outcome::NothingToDo,
        );
        assert_eq!(repo.head().unwrap(), before);
    }
}

#[tokio::test]
async fn merge_and_rebase_are_refused_on_a_detached_head_or_an_unknown_ref() {
    let (_fixture, repo) = ahead();
    let run = Run::new(&repo).await;

    for op in [
        merge("nope", MergeFastForward::Config),
        merge("--help", MergeFastForward::Config),
        rebase("nope"),
    ] {
        assert!(refused(run.ask(op).await).contains("names no commit"));
    }
    repo.git(["switch", "--quiet", "--detach", "main"]).unwrap();
    for op in [
        merge("feature", MergeFastForward::Config),
        rebase("feature"),
    ] {
        assert!(refused(run.ask(op).await).contains("HEAD is detached"));
    }
}

#[tokio::test]
async fn ops_1_a_conflicting_merge_ends_conflict_and_abort_restores_the_branch() {
    let (_fixture, repo) = conflicting();
    let before = repo.head().unwrap();
    let mut run = Run::new(&repo).await;

    let ended = run.run(merge("feature", MergeFastForward::Config)).await;
    outcome(&ended, Outcome::Conflict);
    assert!(has_ref(&repo, "MERGE_HEAD"));
    assert_eq!(
        run.details().await,
        ConflictDetails {
            operation: Some(Operation::Merge),
            applying: Some("feature".to_owned()),
            onto: Some("main".to_owned()),
            step: None,
            unmerged_paths: 1,
        }
    );
    // GRP-8: nothing else runs in conflict state; Continue waits (OPS-2).
    assert!(refused(run.ask(rebase("feature")).await).contains("conflict state"));
    assert!(refused(run.ask(IntegrateOp::Continue).await).contains("1 path is still unmerged"));

    let ended = run.run(IntegrateOp::Abort).await;
    outcome(&ended, Outcome::Succeeded);
    assert_eq!(ended.label, "Abort the merge");
    assert!(!has_ref(&repo, "MERGE_HEAD"));
    assert_eq!(repo.head().unwrap(), before);
    assert_eq!(read(&repo, "a.txt"), "main\n");
    assert_eq!(run.details().await.operation, None);
}

#[tokio::test]
async fn merge_continue_commits_the_resolved_merge() {
    let (_fixture, repo) = conflicting();
    let mut run = Run::new(&repo).await;
    outcome(
        &run.run(merge("feature", MergeFastForward::Config)).await,
        Outcome::Conflict,
    );

    resolve(&repo, "a.txt", "both\n");
    assert_eq!(run.details().await.unmerged_paths, 0);
    let ended = run.run(IntegrateOp::Continue).await;
    outcome(&ended, Outcome::Succeeded);
    assert_eq!(ended.label, "Continue the merge");
    assert!(!has_ref(&repo, "MERGE_HEAD"));
    assert_eq!(parents(&repo, "HEAD"), 2);
    assert_eq!(subjects(&repo, 1), ["Merge branch 'feature'"]);
    assert_eq!(read(&repo, "a.txt"), "both\n");
}

#[tokio::test]
async fn grp_12_merge_never_stashes() {
    let (_fixture, repo) = conflicting();
    repo.switch("feature").unwrap();
    repo.git(["reset", "--quiet", "--hard", "HEAD~1"]).unwrap();
    // `feature` is behind `main` now; `main` changes a.txt, edited here.
    repo.write("a.txt", "local edit\n").unwrap();
    repo.git(["config", "merge.autoStash", "true"]).unwrap();
    repo.git(["config", "rebase.autoStash", "true"]).unwrap();
    let before = repo.head().unwrap();
    let mut run = Run::new(&repo).await;

    for op in [merge("main", MergeFastForward::Config), rebase("main")] {
        let ended = run.run(op).await;
        outcome(&ended, Outcome::Failed);
        assert!(ended.message.is_some(), "git's message: {ended:#?}");
        assert_eq!(repo.head().unwrap(), before);
        assert_eq!(read(&repo, "a.txt"), "local edit\n");
        assert_eq!(repo.git(["stash", "list"]).unwrap(), "");
    }
}

#[tokio::test]
async fn merge_targets_filter_in_the_core() {
    let (_fixture, repo) = ahead();
    repo.branch("fix/Feature-two").unwrap();
    repo.tag("v1").unwrap();
    let run = Run::new(&repo).await;

    // The current branch is not offered.
    assert_eq!(run.targets("").await, ["feature", "fix/Feature-two", "v1"]);
    assert_eq!(run.targets("FEAT").await, ["feature", "fix/Feature-two"]);
    assert!(run.targets("zzz").await.is_empty());
}

// Rebase.

#[tokio::test]
async fn rebase_replays_the_branch_onto_the_ref() {
    let (_fixture, repo) = ahead();
    let main = repo.commit_file("c.txt", "c\n", "Main").unwrap();
    repo.switch("feature").unwrap();
    let mut run = Run::new(&repo).await;

    let ended = run.run(rebase("main")).await;
    outcome(&ended, Outcome::Succeeded);
    assert_eq!(ended.label, "Rebase feature onto main");
    assert_eq!(repo.rev_parse("HEAD~1").unwrap(), main);
    assert_eq!(subjects(&repo, 1), ["Feature"]);
}

#[tokio::test]
async fn rebase_onto_a_contained_ref_is_nothing_to_do() {
    let (_fixture, repo) = ahead();
    repo.switch("feature").unwrap();
    let before = repo.head().unwrap();
    let mut run = Run::new(&repo).await;

    outcome(&run.run(rebase("main")).await, Outcome::NothingToDo);
    assert_eq!(repo.head().unwrap(), before);
}

#[tokio::test]
async fn rebase_stops_at_step_2_of_3_continues_into_a_further_conflict_and_aborts() {
    let fixture = Fixture::new().unwrap();
    let repo = fixture.repo("app").unwrap();
    repo.commit_file("a.txt", "a\n", "Base").unwrap();
    repo.branch("feature").unwrap();
    repo.commit_file("a.txt", "main\n", "Main").unwrap();
    repo.switch("feature").unwrap();
    repo.commit_file("b.txt", "b\n", "One").unwrap();
    repo.commit_file("a.txt", "two\n", "Two").unwrap();
    let before = repo.commit_file("a.txt", "three\n", "Three").unwrap();
    let mut run = Run::new(&repo).await;

    let ended = run.run(rebase("main")).await;
    outcome(&ended, Outcome::Conflict);
    let details = run.details().await;
    assert_eq!(details.operation, Some(Operation::Rebase));
    assert_eq!(details.applying.as_deref(), Some("feature"));
    assert_eq!(details.onto.as_deref(), Some("main"));
    assert_eq!(
        details.step,
        Some(Step {
            current: 2,
            total: 3
        })
    );

    // Resolved unlike "Two", step 3's change to a.txt conflicts in turn.
    resolve(&repo, "a.txt", "resolved\n");
    let ended = run.run(IntegrateOp::Continue).await;
    outcome(&ended, Outcome::Conflict);
    assert_eq!(ended.label, "Continue the rebase");
    let details = run.details().await;
    assert_eq!(
        details.step,
        Some(Step {
            current: 3,
            total: 3
        })
    );
    assert_eq!(details.unmerged_paths, 1);

    let ended = run.run(IntegrateOp::Abort).await;
    outcome(&ended, Outcome::Succeeded);
    assert_eq!(ended.label, "Abort the rebase");
    assert_eq!(run.details().await.operation, None);
    assert_eq!(repo.head().unwrap(), before);
    assert_eq!(
        repo.git(["symbolic-ref", "--short", "HEAD"])
            .unwrap()
            .trim(),
        "feature"
    );
}

#[tokio::test]
async fn rebase_continues_to_the_end() {
    let (_fixture, repo) = conflicting();
    repo.switch("feature").unwrap();
    repo.commit_file("c.txt", "c\n", "After").unwrap();
    let main = repo.rev_parse("main").unwrap();
    let mut run = Run::new(&repo).await;
    outcome(&run.run(rebase("main")).await, Outcome::Conflict);

    resolve(&repo, "a.txt", "resolved\n");
    outcome(&run.run(IntegrateOp::Continue).await, Outcome::Succeeded);
    assert_eq!(run.details().await.operation, None);
    assert_eq!(subjects(&repo, 3), ["After", "Feature", "Main"]);
    assert_eq!(repo.rev_parse("HEAD~2").unwrap(), main);
}

// Cherry-pick.

/// `main` at "Base", `side` with "One" (`b.txt`), "Two" (`a.txt`) and
/// "Three" (`c.txt`); `main` checked out. Returns the three commits.
fn side() -> (Fixture, Repo, [String; 3]) {
    let fixture = Fixture::new().unwrap();
    let repo = fixture.repo("app").unwrap();
    repo.commit_file("a.txt", "a\n", "Base").unwrap();
    repo.branch("side").unwrap();
    repo.switch("side").unwrap();
    let one = repo.commit_file("b.txt", "b\n", "One").unwrap();
    let two = repo.commit_file("a.txt", "two\n", "Two").unwrap();
    let three = repo.commit_file("c.txt", "c\n", "Three").unwrap();
    repo.switch("main").unwrap();
    (fixture, repo, [one, two, three])
}

#[tokio::test]
async fn cherry_pick_applies_one_commit() {
    let (_fixture, repo, [one, ..]) = side();
    let mut run = Run::new(&repo).await;

    let ended = run.run(cherry_pick(&[&one])).await;
    outcome(&ended, Outcome::Succeeded);
    assert_eq!(ended.label, format!("Cherry-pick {}", &one[..7]));
    assert_eq!(subjects(&repo, 2), ["One", "Base"]);
}

#[tokio::test]
async fn cherry_pick_applies_three_commits_oldest_first() {
    let (_fixture, repo, [one, two, three]) = side();
    let mut run = Run::new(&repo).await;

    // As a multi-selection of the history gives them: newest first, or in
    // any order the user clicked.
    let ended = run.run(cherry_pick(&[&three, &one, &two])).await;
    outcome(&ended, Outcome::Succeeded);
    assert_eq!(ended.label, "Cherry-pick 3 commits");
    assert_eq!(subjects(&repo, 4), ["Three", "Two", "One", "Base"]);
}

#[tokio::test]
async fn cherry_pick_conflicts_in_the_second_then_continues() {
    let (_fixture, repo, [one, two, three]) = side();
    repo.commit_file("a.txt", "main\n", "Main").unwrap();
    let mut run = Run::new(&repo).await;

    let ended = run.run(cherry_pick(&[&one, &two, &three])).await;
    outcome(&ended, Outcome::Conflict);
    assert!(has_ref(&repo, "CHERRY_PICK_HEAD"));
    let details = run.details().await;
    assert_eq!(details.operation, Some(Operation::CherryPick));
    assert_eq!(details.applying, Some(format!("{} Two", &two[..7])));
    assert_eq!(details.onto.as_deref(), Some("main"));

    resolve(&repo, "a.txt", "resolved\n");
    let ended = run.run(IntegrateOp::Continue).await;
    outcome(&ended, Outcome::Succeeded);
    assert_eq!(ended.label, "Continue the cherry-pick");
    assert!(!has_ref(&repo, "CHERRY_PICK_HEAD"));
    assert_eq!(subjects(&repo, 4), ["Three", "Two", "One", "Main"]);
}

#[tokio::test]
async fn cherry_pick_abort_restores_the_branch() {
    let (_fixture, repo, [_, two, _]) = side();
    let before = repo.commit_file("a.txt", "main\n", "Main").unwrap();
    let mut run = Run::new(&repo).await;
    outcome(&run.run(cherry_pick(&[&two])).await, Outcome::Conflict);

    let ended = run.run(IntegrateOp::Abort).await;
    outcome(&ended, Outcome::Succeeded);
    assert_eq!(ended.label, "Abort the cherry-pick");
    assert!(!has_ref(&repo, "CHERRY_PICK_HEAD"));
    assert_eq!(repo.head().unwrap(), before);
    assert_eq!(read(&repo, "a.txt"), "main\n");
}

#[tokio::test]
async fn ops_2_abort_follows_the_repo_state() {
    let (_fixture, repo, [_, two, _]) = side();
    let before = repo.commit_file("a.txt", "main\n", "Main").unwrap();
    // Started in a terminal, not by the app (OPS-1).
    let output = repo
        .git_output(["cherry-pick", "--end-of-options", &two])
        .unwrap();
    assert!(!output.status.success());
    assert!(has_ref(&repo, "CHERRY_PICK_HEAD"));
    let mut run = Run::new(&repo).await;
    assert_eq!(run.details().await.operation, Some(Operation::CherryPick));

    let ended = run.run(IntegrateOp::Abort).await;
    outcome(&ended, Outcome::Succeeded);
    assert_eq!(ended.label, "Abort the cherry-pick");
    assert!(!has_ref(&repo, "CHERRY_PICK_HEAD"));
    assert_eq!(repo.head().unwrap(), before);
}

#[tokio::test]
async fn abort_and_continue_are_refused_without_an_operation() {
    let (_fixture, repo) = ahead();
    let run = Run::new(&repo).await;

    assert!(refused(run.ask(IntegrateOp::Abort).await).contains("no operation to abort"));
    assert!(refused(run.ask(IntegrateOp::Continue).await).contains("no operation to continue"));
    assert!(refused(run.ask(cherry_pick(&[])).await).contains("no commit"));
}

// Revert.

#[tokio::test]
async fn revert_undoes_a_commit() {
    let (_fixture, repo) = ahead();
    repo.switch("feature").unwrap();
    let feature = repo.head().unwrap();
    let mut run = Run::new(&repo).await;

    let ended = run.run(revert(&feature, None)).await;
    outcome(&ended, Outcome::Succeeded);
    assert_eq!(ended.label, format!("Revert {}", &feature[..7]));
    assert_eq!(subjects(&repo, 1), ["Revert \"Feature\""]);
    assert!(!repo.path().join("b.txt").exists());
    // Only a merge commit takes a mainline.
    assert!(refused(run.ask(revert(&feature, Some(1))).await).contains("not a merge commit"));
}

#[tokio::test]
async fn revert_of_a_merge_commit_asks_for_the_mainline() {
    let (_fixture, repo) = ahead();
    let base = repo.head().unwrap();
    let merged = repo.merge("feature", "Merge feature").unwrap();
    let mut run = Run::new(&repo).await;

    let start = run.ask(revert(&merged, None)).await.unwrap();
    assert_eq!(
        start,
        IntegrateStart::ChooseMainline {
            commit: merged.clone(),
            parents: vec![
                format!("{} Base", &base[..7]),
                format!("{} Feature", &repo.rev_parse("feature").unwrap()[..7]),
            ],
        }
    );
    assert!(refused(run.ask(revert(&merged, Some(3))).await).contains("has no parent 3"));
    assert!(refused(run.ask(revert(&merged, Some(0))).await).contains("has no parent 0"));

    outcome(&run.run(revert(&merged, Some(1))).await, Outcome::Succeeded);
    assert!(!repo.path().join("b.txt").exists());
    assert_eq!(parents(&repo, "HEAD"), 1);
}

#[tokio::test]
async fn a_conflicting_revert_ends_conflict_and_aborts() {
    let fixture = Fixture::new().unwrap();
    let repo = fixture.repo("app").unwrap();
    repo.commit_file("a.txt", "a\n", "Base").unwrap();
    let changed = repo.commit_file("a.txt", "b\n", "Change").unwrap();
    let before = repo.commit_file("a.txt", "c\n", "Again").unwrap();
    let mut run = Run::new(&repo).await;

    let ended = run.run(revert(&changed, None)).await;
    outcome(&ended, Outcome::Conflict);
    assert!(has_ref(&repo, "REVERT_HEAD"));
    let details = run.details().await;
    assert_eq!(details.operation, Some(Operation::Revert));
    assert_eq!(details.applying, Some(format!("{} Change", &changed[..7])));

    let ended = run.run(IntegrateOp::Abort).await;
    outcome(&ended, Outcome::Succeeded);
    assert_eq!(ended.label, "Abort the revert");
    assert!(!has_ref(&repo, "REVERT_HEAD"));
    assert_eq!(repo.head().unwrap(), before);
}
