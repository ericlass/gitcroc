//! The history the pane shows (UI-9, UI-12, ARCH-8, ARCH-9) and the offer
//! to write a commit-graph file (PERF-2, ARCH-1).

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use gitcroc_core::{
    ConfigDir, GitRepo, GitRunner, Histories, HistoryStart, HistoryTarget, HistoryView, JobEvents,
    JobId, JobState, JobView, Jobs, Outcome, PAGE_SIZE, RepoId, RepoRead, Summaries, SummaryCache,
    SummaryView, WorkspaceId, Workspaces, add_path, commit_graph_write, history_get, history_more,
};
use repo_gen::{Fixture, Repo, RepoSpec};

struct Dirs {
    root: tempfile::TempDir,
}

impl ConfigDir for Dirs {
    fn config_dir(&self) -> Option<PathBuf> {
        Some(self.root.path().join("config"))
    }
}

struct Run {
    dirs: Dirs,
    workspaces: Arc<Workspaces>,
    histories: Arc<Histories>,
    jobs: Arc<Jobs>,
    progress: Arc<Progress>,
    id: WorkspaceId,
}

/// The progress the engine sent.
#[derive(Default)]
struct Progress(std::sync::Mutex<Vec<(JobId, u32, u32)>>);

impl JobEvents for Progress {
    fn progress(&self, job: &JobView) {
        if job.state == JobState::Running {
            let step = (job.job_id, job.done, job.total);
            self.0.lock().unwrap().push(step);
        }
    }

    fn ended(&self, _: &JobView) {}

    fn summary(&self, _: SummaryView, _: bool) {}
}

impl Run {
    async fn with_repos_in(path: &Path, threshold: Duration) -> Self {
        let dirs = Dirs {
            root: tempfile::tempdir().unwrap(),
        };
        let workspaces = Arc::new(Workspaces::load(&dirs));
        let id = workspaces.views()[0].id.clone();
        add_path(
            workspaces.clone(),
            id.clone(),
            path.to_owned(),
            git(),
            JobId(1),
            |_| {},
        )
        .await
        .unwrap();
        let summaries = Arc::new(Summaries::load(SummaryCache::none(), &workspaces));
        let progress = Arc::new(Progress::default());
        let jobs = Jobs::new(workspaces.clone(), summaries, progress.clone());
        Self {
            dirs,
            workspaces,
            histories: Arc::new(Histories::new(threshold)),
            jobs,
            progress,
            id,
        }
    }

    fn repo(&self, name: &str) -> RepoId {
        let view = self.workspaces.view(&self.id).unwrap();
        view.repos
            .into_iter()
            .find(|repo| repo.name == name)
            .unwrap()
            .id
    }

    async fn get(&self, repo: &RepoId, target: HistoryTarget, text: &str) -> Option<HistoryView> {
        history_get(
            self.histories.clone(),
            self.workspaces.clone(),
            self.id.clone(),
            repo.clone(),
            target,
            text.to_owned(),
        )
        .await
        .unwrap()
    }

    fn write_commit_graph(&self, repo: &RepoId, git: GitRunner) -> gitcroc_core::CommitGraphWrite {
        commit_graph_write(
            &self.histories,
            &self.jobs,
            &self.workspaces,
            &self.id,
            repo,
            git,
        )
        .unwrap()
    }

    async fn more(&self, handle: u32) -> Option<HistoryView> {
        history_more(self.histories.clone(), self.id.clone(), handle)
            .await
            .unwrap()
    }
}

fn git() -> GitRunner {
    GitRunner::new(PathBuf::from("git"))
}

fn subjects(view: &HistoryView) -> Vec<&str> {
    view.rows.iter().map(|row| row.subject.as_str()).collect()
}

fn empty_commit(repo: &Repo, message: &str) -> String {
    repo.git(["commit", "--quiet", "--allow-empty", "-m", message])
        .unwrap();
    repo.head().unwrap()
}

fn has_commit_graph(repo: &Path) -> bool {
    let info = repo.join(".git/objects/info");
    info.join("commit-graph").is_file() || info.join("commit-graphs").is_dir()
}

/// A repo with three commits on `main`, one on `topic` and a tag.
fn small_repo(fixture: &Fixture) -> Repo {
    let repo = fixture.repo("app").unwrap();
    empty_commit(&repo, "First");
    empty_commit(&repo, "Second");
    repo.git(["tag", "v1"]).unwrap();
    repo.git(["checkout", "--quiet", "-b", "topic"]).unwrap();
    empty_commit(&repo, "On topic");
    repo.git(["checkout", "--quiet", "-"]).unwrap();
    empty_commit(&repo, "Third");
    repo
}

#[tokio::test]
async fn ui_9_history_follows_the_selected_ref() {
    let fixture = Fixture::new().unwrap();
    small_repo(&fixture);
    let run = Run::with_repos_in(fixture.path(), Duration::MAX).await;
    let app = run.repo("app");

    let all = run.get(&app, HistoryTarget::Repo, "").await.unwrap();
    assert_eq!(subjects(&all), ["Third", "On topic", "Second", "First"]);
    assert!(all.done);
    assert!(all.rows.iter().all(|row| row.graph.is_some()));

    let topic = HistoryTarget::Branch {
        name: "topic".into(),
    };
    let topic = run.get(&app, topic, "").await.unwrap();
    assert_eq!(subjects(&topic), ["On topic", "Second", "First"]);

    let tag = HistoryTarget::Tag { name: "v1".into() };
    assert_eq!(
        subjects(&run.get(&app, tag, "").await.unwrap()),
        ["Second", "First"]
    );
}

#[tokio::test]
async fn ui_12_filter_by_subject_author_and_hash() {
    let fixture = Fixture::new().unwrap();
    let repo = small_repo(&fixture);
    repo.git([
        "commit",
        "--quiet",
        "--allow-empty",
        "--author=Grace Hopper <grace@example.com>",
        "-m",
        "Fix a bug",
    ])
    .unwrap();
    let grace = repo.head().unwrap();
    let run = Run::with_repos_in(fixture.path(), Duration::MAX).await;
    let app = run.repo("app");

    for (text, expected) in [
        ("TOPIC", vec!["On topic"]),
        ("hopper", vec!["Fix a bug"]),
        ("grace@", vec!["Fix a bug"]),
        (&grace[..7], vec!["Fix a bug"]),
        ("nothing like this", vec![]),
    ] {
        let view = run.get(&app, HistoryTarget::Repo, text).await.unwrap();
        assert_eq!(subjects(&view), expected, "{text}");
        // Filtered rows are not contiguous: no graph lines.
        assert!(view.rows.iter().all(|row| row.graph.is_none()));
    }
}

#[tokio::test]
async fn arch_8_history_continues_in_pages_and_a_newer_walk_supersedes() {
    let dir = tempfile::tempdir().unwrap();
    let spec = RepoSpec {
        name: "long".into(),
        files: 4,
        lines: 10,
        commits: 1200,
        branches: 2,
    };
    repo_gen::generate_repo(&spec, 7, &dir.path().join("long")).unwrap();
    let run = Run::with_repos_in(dir.path(), Duration::MAX).await;
    let long = run.repo("long");

    let first = run.get(&long, HistoryTarget::Repo, "").await.unwrap();
    assert_eq!(first.rows.len(), PAGE_SIZE);
    assert!(!first.done);
    let second = run.more(first.handle).await.unwrap();
    assert_eq!(second.rows.len(), PAGE_SIZE);
    assert_eq!(second.handle, first.handle);
    // Lanes continue across pages: the graph is never restarted.
    assert!(second.rows.iter().all(|row| row.graph.is_some()));

    // A newer walk of the workspace replaces the old one.
    let newer = run.get(&long, HistoryTarget::Repo, "").await.unwrap();
    assert_ne!(newer.handle, first.handle);
    assert!(run.more(first.handle).await.is_none());

    let mut pages = vec![newer];
    while !pages.last().unwrap().done {
        let handle = pages.last().unwrap().handle;
        pages.push(run.more(handle).await.unwrap());
    }
    let total: usize = pages.iter().map(|page| page.rows.len()).sum();
    assert_eq!(total, 1200);
    // Past the end there is no page to read.
    let handle = pages.last().unwrap().handle;
    assert!(
        history_more(run.histories.clone(), run.id.clone(), handle)
            .await
            .is_err()
    );
}

#[tokio::test]
async fn arch_9_walks_are_kept_per_workspace() {
    let fixture = Fixture::new().unwrap();
    small_repo(&fixture);
    let run = Run::with_repos_in(fixture.path(), Duration::MAX).await;
    let app = run.repo("app");
    let view = run.get(&app, HistoryTarget::Repo, "").await.unwrap();

    let elsewhere = WorkspaceId("elsewhere".into());
    assert!(
        history_more(run.histories.clone(), elsewhere, view.handle)
            .await
            .unwrap()
            .is_none()
    );
    let unknown = history_get(
        run.histories.clone(),
        run.workspaces.clone(),
        WorkspaceId("elsewhere".into()),
        app,
        HistoryTarget::Repo,
        String::new(),
    )
    .await;
    assert!(unknown.is_err());
}

#[tokio::test]
async fn perf_2_slow_walk_without_commit_graph_offers_to_write_it() {
    let fixture = Fixture::new().unwrap();
    let with_graph = fixture.repo("indexed").unwrap();
    empty_commit(&with_graph, "First");
    with_graph
        .git(["commit-graph", "write", "--reachable"])
        .unwrap();
    small_repo(&fixture);
    // Every walk is "slow" against a zero threshold.
    let run = Run::with_repos_in(fixture.path(), Duration::ZERO).await;
    let app = run.repo("app");
    let indexed = run.repo("indexed");
    assert!(!has_commit_graph(&fixture.path().join("app")));

    let first = run.get(&app, HistoryTarget::Repo, "").await.unwrap();
    assert!(first.offer_commit_graph);
    // Under the threshold there is no offer.
    let fast = Run::with_repos_in(fixture.path(), Duration::MAX).await;
    let fast_app = fast.repo("app");
    assert!(
        !fast
            .get(&fast_app, HistoryTarget::Repo, "")
            .await
            .unwrap()
            .offer_commit_graph
    );
    // With the file there is nothing to offer.
    let view = run.get(&indexed, HistoryTarget::Repo, "").await.unwrap();
    assert!(!view.offer_commit_graph);

    // Declined, the offer never comes back, and the answer is kept in the
    // workspace file.
    run.workspaces.decline_commit_graph(&run.id, &app).unwrap();
    let again = run.get(&app, HistoryTarget::Repo, "").await.unwrap();
    assert!(!again.offer_commit_graph);
    assert!(run.workspaces.commit_graph_declined(&run.id, &app).unwrap());
    let config = run.dirs.root.path().join("config/workspaces");
    let file = std::fs::read_dir(config).unwrap().next().unwrap().unwrap();
    let text = std::fs::read_to_string(file.path()).unwrap();
    assert!(text.contains("commit_graph = \"declined\""), "{text}");
    // Nothing was written into the repo.
    assert!(!has_commit_graph(&fixture.path().join("app")));
}

#[tokio::test]
async fn arch_1_commit_graph_is_written_by_the_git_cli() {
    let fixture = Fixture::new().unwrap();
    small_repo(&fixture);
    let path = fixture.path().join("app");
    let run = Run::with_repos_in(fixture.path(), Duration::ZERO).await;
    let app = run.repo("app");
    assert!(
        run.get(&app, HistoryTarget::Repo, "")
            .await
            .unwrap()
            .offer_commit_graph
    );
    let before = GitRepo::open(&path)
        .unwrap()
        .history(HistoryStart::All, None, None)
        .unwrap();
    assert!(!before.commit_graph);

    let write = run.write_commit_graph(&app, git());
    // A second request while it runs joins the running job.
    let joined = run.write_commit_graph(&app, git());
    assert_eq!(joined.job_id, write.job_id);
    assert!(joined.ended.is_none());
    // No offer while the file is being written.
    assert!(
        !run.get(&app, HistoryTarget::Repo, "")
            .await
            .unwrap()
            .offer_commit_graph
    );
    let view = write.ended.unwrap().await.unwrap();
    assert_eq!(view.outcome, Some(Outcome::Succeeded));
    // A job of the engine, in the job list.
    assert_eq!(run.jobs.views(&run.id)[0].job_id, view.job_id);
    assert_eq!(view.label, "Writing the commit-graph file of app");
    assert_eq!(*run.progress.0.lock().unwrap(), [(view.job_id, 0, 0)]);

    assert!(has_commit_graph(&path));
    let after = GitRepo::open(&path)
        .unwrap()
        .history(HistoryStart::All, None, None)
        .unwrap();
    assert!(after.commit_graph);
    assert!(
        !run.get(&app, HistoryTarget::Repo, "")
            .await
            .unwrap()
            .offer_commit_graph
    );
    // The job is over: a new request starts a new one.
    let again = run.write_commit_graph(&app, git());
    assert_ne!(again.job_id, view.job_id);
    again.ended.unwrap().await.unwrap();
}

#[tokio::test]
async fn arch_1_a_failed_commit_graph_write_is_reported() {
    let fixture = Fixture::new().unwrap();
    small_repo(&fixture);
    let run = Run::with_repos_in(fixture.path(), Duration::ZERO).await;
    let app = run.repo("app");
    let missing = GitRunner::new(PathBuf::from("/no/such/git"));
    let write = run.write_commit_graph(&app, missing);
    let view = write.ended.unwrap().await.unwrap();
    assert_eq!(view.outcome, Some(Outcome::Failed));
    assert!(view.message.unwrap().contains("/no/such/git"));
    // History keeps working without the file.
    let view = run.get(&app, HistoryTarget::Repo, "").await.unwrap();
    assert_eq!(view.rows.len(), 4);
}

/// How many of `repo`'s pack files this process has mapped into memory.
#[cfg(target_os = "linux")]
fn mapped_packs(repo: &Path) -> usize {
    let packs = repo.canonicalize().unwrap().join(".git/objects/pack/");
    let maps = std::fs::read_to_string("/proc/self/maps").unwrap();
    maps.lines()
        .filter(|line| line.contains(packs.to_str().unwrap()))
        .count()
}

#[tokio::test]
async fn perf_2_a_walk_keeps_no_repo_open_between_pages() {
    let dir = tempfile::tempdir().unwrap();
    let spec = RepoSpec {
        name: "long".into(),
        files: 4,
        lines: 10,
        commits: 1200,
        branches: 2,
    };
    let path = dir.path().join("long");
    repo_gen::generate_repo(&spec, 7, &path).unwrap();
    let run = Run::with_repos_in(dir.path(), Duration::MAX).await;
    let long = run.repo("long");

    // An open repo would keep the pack parts the walk read in memory for
    // as long as the history is shown (M1-14).
    let first = run.get(&long, HistoryTarget::Repo, "").await.unwrap();
    assert!(!first.done);
    #[cfg(target_os = "linux")]
    assert_eq!(mapped_packs(&path), 0);
    let second = run.more(first.handle).await.unwrap();
    assert_eq!(second.rows.len(), PAGE_SIZE);
    assert!(second.rows.iter().all(|row| row.graph.is_some()));
    #[cfg(target_os = "linux")]
    assert_eq!(mapped_packs(&path), 0);
}
