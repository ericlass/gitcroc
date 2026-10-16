//! Clone (section 6 "Repos", GRP-1, GRP-7, GRP-10) with real git, as a
//! job. The remote is a bare repo on disk reached over `file://`, or an
//! ssh remote whose ssh never answers. Nothing here waits for a time: each
//! test waits for the clone's report, and the cancel test for the fake ssh
//! to start.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use gitcroc_core::{
    CancelResult, CloneError, CloneReport, CloneRequest, ConfigDir, GitRunner, JobEvents, JobView,
    Jobs, Outcome, SkipReason, Summaries, SummaryCache, SummaryView, WorkspaceId, Workspaces,
    clone_start, clone_target_check,
};
use repo_gen::Fixture;

struct Dirs {
    root: tempfile::TempDir,
}

impl ConfigDir for Dirs {
    fn config_dir(&self) -> Option<PathBuf> {
        Some(self.root.path().join("config"))
    }
}

/// git with no user or system config, so no setting of the machine takes
/// part.
fn git(dirs: &Dirs) -> GitRunner {
    let home = dirs.root.path().join("home");
    GitRunner::new(PathBuf::from("git")).with_env([
        ("HOME", home.clone().into_os_string()),
        ("XDG_CONFIG_HOME", home.into_os_string()),
        ("GIT_CONFIG_NOSYSTEM", "1".into()),
    ])
}

/// Keeps every job's last view.
#[derive(Default)]
struct Events {
    seen: Mutex<Vec<JobView>>,
}

impl JobEvents for Events {
    fn progress(&self, job: &JobView) {
        self.seen.lock().unwrap().push(job.clone());
    }

    fn ended(&self, job: &JobView) {
        self.seen.lock().unwrap().push(job.clone());
    }

    fn summary(&self, _: SummaryView, _: bool) {}
}

fn file_url(path: &Path) -> String {
    let path = path.to_string_lossy().replace('\\', "/");
    if path.starts_with('/') {
        format!("file://{path}")
    } else {
        format!("file:///{path}")
    }
}

/// An empty workspace, the job engine, a bare repo with one commit to
/// clone, and `parent`, the folder clones go to.
struct Run {
    dirs: Dirs,
    fixture: Fixture,
    workspaces: Arc<Workspaces>,
    jobs: Arc<Jobs>,
    events: Arc<Events>,
    id: WorkspaceId,
    bare: PathBuf,
    parent: PathBuf,
}

impl Run {
    fn new() -> Self {
        let dirs = Dirs {
            root: tempfile::tempdir().unwrap(),
        };
        std::fs::create_dir_all(dirs.root.path().join("home")).unwrap();
        let fixture = Fixture::new().unwrap();
        let seed = fixture.repo("seed").unwrap();
        seed.commit_file("a.txt", "a\n", "First").unwrap();
        let bare = fixture.path().join("origin.git");
        let mut args: Vec<std::ffi::OsString> = vec!["clone".into(), "--bare".into()];
        args.extend(["--quiet".into(), "--".into()]);
        args.extend([seed.path().into(), bare.clone().into()]);
        seed.git(args).unwrap();
        let parent = fixture.path().join("clones");
        std::fs::create_dir(&parent).unwrap();
        // Beside the clone: it must stay.
        std::fs::write(parent.join("keep.txt"), "keep\n").unwrap();
        let workspaces = Arc::new(Workspaces::load(&dirs));
        let id = workspaces.views()[0].id.clone();
        let summaries = Arc::new(Summaries::load(SummaryCache::none(), &workspaces));
        let events = Arc::new(Events::default());
        let jobs = Jobs::new(workspaces.clone(), summaries, events.clone());
        Self {
            dirs,
            fixture,
            workspaces,
            jobs,
            events,
            id,
            bare,
            parent,
        }
    }

    async fn start(
        &self,
        git: GitRunner,
        url: &str,
        name: &str,
    ) -> Result<gitcroc_core::CloneHandle, CloneError> {
        clone_start(
            self.workspaces.clone(),
            self.jobs.clone(),
            git,
            self.id.clone(),
            CloneRequest {
                url: url.to_owned(),
                parent: self.parent.clone(),
                name: name.to_owned(),
                repo: None,
                group: None,
            },
        )
        .await
    }

    /// Clones `url` into `name` to the end: its report and its job's end.
    async fn clone(&self, url: &str, name: &str) -> (CloneReport, JobView) {
        let handle = self.start(git(&self.dirs), url, name).await.unwrap();
        let report = handle.ended.await.unwrap();
        assert_eq!(report.job_id, handle.job_id);
        (report, self.ended(handle.job_id.0))
    }

    fn ended(&self, job: u32) -> JobView {
        self.events
            .seen
            .lock()
            .unwrap()
            .iter()
            .rev()
            .find(|view| view.job_id.0 == job)
            .cloned()
            .unwrap()
    }

    /// The names in `parent`, sorted.
    fn listed(&self) -> Vec<String> {
        let mut names: Vec<String> = std::fs::read_dir(&self.parent)
            .unwrap()
            .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        names.sort();
        names
    }
}

#[tokio::test]
async fn grp_1_a_clone_is_added_to_the_workspace() {
    let run = Run::new();
    let (report, ended) = run.clone(&file_url(&run.bare), "copy").await;

    assert_eq!(ended.outcome, Some(Outcome::Succeeded), "{ended:#?}");
    assert_eq!(ended.label, "Clone copy");
    let repo = report.repo.expect("no repo to select");
    assert_eq!(repo.name, "copy");
    let view = run.workspaces.view(&run.id).unwrap();
    assert_eq!(view.repos, vec![repo]);
    assert!(run.parent.join("copy").join("a.txt").is_file());
    assert_eq!(run.listed(), ["copy", "keep.txt"]);
}

#[tokio::test]
async fn grp_1_a_clone_into_an_empty_folder_is_added() {
    let run = Run::new();
    std::fs::create_dir(run.parent.join("empty")).unwrap();
    assert_eq!(
        clone_target_check(run.parent.clone(), "empty".into(), false).await,
        None
    );

    let (report, ended) = run.clone(&file_url(&run.bare), "empty").await;
    assert_eq!(ended.outcome, Some(Outcome::Succeeded), "{ended:#?}");
    assert_eq!(report.repo.unwrap().name, "empty");
}

#[tokio::test]
async fn grp_1_a_folder_that_is_not_empty_is_refused_before_git_runs() {
    let run = Run::new();
    let full = run.parent.join("full");
    std::fs::create_dir(&full).unwrap();
    std::fs::write(full.join("mine.txt"), "mine\n").unwrap();
    let why = format!("the folder {} exists and is not empty", full.display());
    assert_eq!(
        clone_target_check(run.parent.clone(), "full".into(), false).await,
        Some(why.clone())
    );

    let refused = run
        .start(git(&run.dirs), &file_url(&run.bare), "full")
        .await;
    assert!(
        matches!(&refused, Err(CloneError::Refused(text)) if *text == why),
        "{refused:?}"
    );
    // A file, a bad name and a bad URL are refused too, and no job started.
    let refused = run
        .start(git(&run.dirs), &file_url(&run.bare), "keep.txt")
        .await;
    assert!(
        matches!(refused, Err(CloneError::Refused(_))),
        "{refused:?}"
    );
    let refused = run.start(git(&run.dirs), &file_url(&run.bare), "a/b").await;
    assert!(
        matches!(refused, Err(CloneError::Refused(_))),
        "{refused:?}"
    );
    let refused = run.start(git(&run.dirs), "--upload-pack=x", "copy").await;
    assert!(
        matches!(refused, Err(CloneError::Refused(_))),
        "{refused:?}"
    );
    assert!(run.jobs.all_views().is_empty());
    assert_eq!(
        std::fs::read_to_string(full.join("mine.txt")).unwrap(),
        "mine\n"
    );
    assert_eq!(run.listed(), ["full", "keep.txt"]);
}

#[tokio::test]
async fn grp_7_a_failed_clone_is_failed_and_leaves_no_folder() {
    let run = Run::new();
    let missing = run.fixture.path().join("missing.git");
    let (report, ended) = run.clone(&file_url(&missing), "copy").await;

    assert_eq!(ended.outcome, Some(Outcome::Failed), "{ended:#?}");
    assert!(ended.message.is_some());
    assert!(ended.output.contains("fatal:"), "{}", ended.output);
    assert_eq!(report.repo, None);
    assert!(run.workspaces.view(&run.id).unwrap().repos.is_empty());
    assert_eq!(run.listed(), ["keep.txt"]);

    // A folder that was empty stays, empty.
    std::fs::create_dir(run.parent.join("empty")).unwrap();
    let (_, ended) = run.clone(&file_url(&missing), "empty").await;
    assert_eq!(ended.outcome, Some(Outcome::Failed), "{ended:#?}");
    assert_eq!(run.listed(), ["empty", "keep.txt"]);
    assert_eq!(
        std::fs::read_dir(run.parent.join("empty")).unwrap().count(),
        0
    );
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
async fn grp_10_cancel_kills_a_clone_and_removes_its_folder() {
    let run = Run::new();
    let (script, started) = fake_ssh(run.fixture.path());
    let command = format!("sh '{}'", script.to_string_lossy().replace('\\', "/"));
    let git = git(&run.dirs).with_env([
        ("GIT_SSH_COMMAND", command.as_str()),
        ("GIT_SSH_VARIANT", "simple"),
    ]);

    let handle = run
        .start(git, "ssh://gitcroc.invalid/repo.git", "copy")
        .await
        .unwrap();
    let mut ended = handle.ended;
    // Polls a file: the wait ends when it appears, however long that takes.
    let ssh_runs = async {
        while !started.exists() {
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
    };
    tokio::select! {
        report = &mut ended => panic!("ended before ssh ran: {report:#?}"),
        () = ssh_runs => {}
    }
    assert!(run.parent.join("copy").exists(), "git made no folder");
    assert_eq!(run.jobs.cancel(handle.job_id), CancelResult::Killing);
    let report = ended.await.unwrap();

    assert_eq!(report.repo, None);
    let view = run.ended(handle.job_id.0);
    assert_eq!(
        view.outcome,
        Some(Outcome::Skipped {
            reason: SkipReason::Cancelled
        })
    );
    assert_eq!(run.listed(), ["keep.txt"]);
    assert!(run.workspaces.view(&run.id).unwrap().repos.is_empty());
}
