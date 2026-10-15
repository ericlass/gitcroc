//! The offer to speed up the status of a large repo (PERF-7): made once a
//! repo tracks more files than the threshold, made small here, and never
//! with a setting on or after "don't ask again"; nothing writes git config
//! until the user accepts, and accepting writes exactly the offered keys.
//! Each job is awaited by its end event; nothing waits for a time.

use std::collections::BTreeSet;
use std::path::PathBuf;
use std::sync::Arc;

use gitcroc_core::{
    Changes, ChangesContent, ChangesTarget, ConfigDir, FsmonitorError, GitRunner, JobEvents, JobId,
    JobView, Jobs, Outcome, RepoId, Summaries, SummaryCache, SummaryView, WorkspaceId, Workspaces,
    add_path, changes_get, fsmonitor_enable, refresh_workspace,
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

/// The keys the offer turns on here: on Linux git has no built-in monitor.
fn offered() -> Vec<String> {
    let mut keys = Vec::new();
    if cfg!(any(target_os = "macos", target_os = "windows")) {
        keys.push("core.fsmonitor".to_owned());
    }
    keys.push("core.untrackedCache".to_owned());
    keys
}

/// A repo tracking five files.
fn five_files(fixture: &Fixture) -> Repo {
    let repo = fixture.repo("app").unwrap();
    for name in ["a", "b", "c", "d", "e"] {
        repo.write(&format!("{name}.txt"), name).unwrap();
    }
    repo.commit_all("Five files").unwrap();
    repo
}

/// The repo's local config as `git config --local --list` prints it.
fn local_config(repo: &Repo) -> BTreeSet<String> {
    repo.git(["config", "--local", "--list"])
        .unwrap()
        .lines()
        .map(str::to_owned)
        .collect()
}

/// A workspace with one repo, the job engine and the changes view, with
/// the offer made over `threshold` tracked files.
struct Run {
    dirs: Dirs,
    workspaces: Arc<Workspaces>,
    summaries: Arc<Summaries>,
    changes: Arc<Changes>,
    jobs: Arc<Jobs>,
    ends: mpsc::UnboundedReceiver<JobView>,
    id: WorkspaceId,
    repo_id: RepoId,
}

impl Run {
    async fn new(repo: &Repo, threshold: usize) -> Self {
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
            changes: Arc::new(Changes::with_fsmonitor_threshold(threshold)),
            jobs,
            ends,
            id,
            repo_id,
        }
    }

    /// What the changes pane offers for the working copy.
    async fn offer(&self) -> Option<Vec<String>> {
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
        let ChangesContent::Uncommitted {
            offer_fsmonitor, ..
        } = view.content
        else {
            panic!("not the working copy");
        };
        offer_fsmonitor
    }

    async fn enable(&self) -> Result<JobId, FsmonitorError> {
        fsmonitor_enable(
            self.workspaces.clone(),
            self.summaries.clone(),
            self.changes.clone(),
            self.jobs.clone(),
            git(&self.dirs),
            self.id.clone(),
            self.repo_id.clone(),
        )
        .await
    }

    fn workspace_file(&self) -> String {
        let dir = self.dirs.root.path().join("config").join("workspaces");
        let file = std::fs::read_dir(dir).unwrap().next().unwrap().unwrap();
        std::fs::read_to_string(file.path()).unwrap()
    }
}

#[tokio::test]
async fn perf_7_offers_fsmonitor_once_for_large_repos() {
    let fixture = Fixture::new().unwrap();
    let repo = five_files(&fixture);

    // At the threshold: no offer.
    assert_eq!(Run::new(&repo, 5).await.offer().await, None);

    // Over it: the offer, until the user answers "don't ask again".
    let run = Run::new(&repo, 4).await;
    assert_eq!(run.offer().await, Some(offered()));
    assert_eq!(run.offer().await, Some(offered()));
    run.workspaces
        .decline_fsmonitor(&run.id, &run.repo_id)
        .unwrap();
    assert_eq!(run.offer().await, None);
    assert!(run.workspace_file().contains("fsmonitor = \"declined\""));

    // A setting already on: no offer.
    repo.git(["config", "core.untrackedCache", "true"]).unwrap();
    assert_eq!(Run::new(&repo, 4).await.offer().await, None);
}

#[tokio::test]
async fn perf_7_never_changes_git_config_without_asking() {
    let fixture = Fixture::new().unwrap();
    let repo = five_files(&fixture);
    let before = local_config(&repo);
    let config = repo.path().join(".git").join("config");
    let bytes = std::fs::read(&config).unwrap();
    let mut run = Run::new(&repo, 4).await;

    // The status read and the refresh write no config.
    assert_eq!(run.offer().await, Some(offered()));
    let summaries = run.summaries.clone();
    refresh_workspace(
        run.workspaces.clone(),
        summaries,
        run.id.clone(),
        git(&run.dirs),
        |_| {},
    )
    .await
    .unwrap();
    assert_eq!(std::fs::read(&config).unwrap(), bytes);
    assert!(!run.workspace_file().contains("fsmonitor"));

    // Accepting writes exactly the offered keys to the local config.
    let job_id = run.enable().await.unwrap();
    let ended = run.ends.recv().await.unwrap();
    assert_eq!(ended.job_id, job_id);
    assert_eq!(ended.outcome, Some(Outcome::Succeeded), "{ended:#?}");
    let after = local_config(&repo);
    let added: Vec<String> = after.difference(&before).cloned().collect();
    let mut expected: Vec<String> = offered()
        .iter()
        .map(|key| format!("{}=true", key.to_lowercase()))
        .collect();
    expected.sort();
    assert_eq!(added, expected);
    assert!(before.is_subset(&after));
}

#[tokio::test]
async fn grp_8_turning_the_settings_on_is_refused_in_conflict_state() {
    let fixture = Fixture::new().unwrap();
    let repo = five_files(&fixture);
    repo.git(["switch", "--quiet", "-c", "side"]).unwrap();
    repo.commit_file("a.txt", "side\n", "Side").unwrap();
    repo.switch("main").unwrap();
    repo.commit_file("a.txt", "main\n", "Main").unwrap();
    repo.merge_with_conflicts("side").unwrap();
    let before = local_config(&repo);
    let run = Run::new(&repo, 4).await;
    // No offer in conflict state, and no write when asked anyway.
    assert_eq!(run.offer().await, None);
    let refused = run.enable().await;
    assert!(
        matches!(&refused, Err(FsmonitorError::Refused(why)) if why.contains("conflict state")),
        "{refused:?}"
    );
    assert_eq!(local_config(&repo), before);
}
