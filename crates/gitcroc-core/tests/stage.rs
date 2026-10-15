//! Stage, unstage and discard in "Uncommitted changes" (UI-15, GRP-9) on
//! fixture repos, with real git: whole files of the view's lists and single
//! hunks of the diff it showed, each as a job. Nothing here waits for a
//! time: each test waits for the job's end event.

// Integration test crate: helpers outside #[test] fns are test code too (QA-4).
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::path::PathBuf;
use std::sync::Arc;

use gitcroc_core::{
    ChangeAction, ChangeRequest, ChangeStart, ChangeTarget, ChangedFile, Changes, ChangesContent,
    ChangesError, ChangesTarget, ChangesView, ConfigDir, DiffRequest, FileDiff, FileList, FileRef,
    GitRunner, JobEvents, JobId, JobView, Jobs, Outcome, Summaries, SummaryCache, SummaryView,
    TextDiff, WorkspaceId, Workspaces, add_path, changes_apply, changes_get, diff_get,
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
    _dirs: Dirs,
    repo: Repo,
    workspaces: Arc<Workspaces>,
    changes: Arc<Changes>,
    jobs: Arc<Jobs>,
    ends: mpsc::UnboundedReceiver<JobView>,
    id: WorkspaceId,
}

fn git() -> GitRunner {
    GitRunner::new(PathBuf::from("git"))
}

impl Run {
    async fn new(repo: Repo) -> Self {
        // The fixture's git reads no system config, the app's git does: Git
        // for Windows sets `core.autocrlf` there. The repo's own setting
        // makes both read the working tree alike.
        repo.git(["config", "core.autocrlf", "false"]).unwrap();
        let dirs = Dirs {
            root: tempfile::tempdir().unwrap(),
        };
        let workspaces = Arc::new(Workspaces::load(&dirs));
        let id = workspaces.views()[0].id.clone();
        add_path(
            workspaces.clone(),
            id.clone(),
            repo.path().to_owned(),
            git(),
            JobId(0),
            |_| {},
        )
        .await
        .unwrap();
        let summaries = Arc::new(Summaries::load(SummaryCache::none(), &workspaces));
        let (sender, ends) = mpsc::unbounded_channel();
        let jobs = Jobs::new(workspaces.clone(), summaries, Arc::new(Ends(sender)));
        Self {
            _dirs: dirs,
            repo,
            workspaces,
            changes: Arc::new(Changes::new()),
            jobs,
            ends,
            id,
        }
    }

    /// Reads "Uncommitted changes" as a new view.
    async fn view(&self) -> ChangesView {
        let repo = self.workspaces.view(&self.id).unwrap().repos[0].id.clone();
        changes_get(
            self.changes.clone(),
            self.workspaces.clone(),
            git(),
            self.id.clone(),
            repo,
            ChangesTarget::Uncommitted,
        )
        .await
        .unwrap()
        .unwrap()
    }

    /// Reads the diff of `file` of `view`, as the pane shows it.
    async fn diff(&self, view: &ChangesView, file: FileRef) -> TextDiff {
        let diff = diff_get(
            self.changes.clone(),
            self.id.clone(),
            view.handle,
            file,
            DiffRequest::default(),
        )
        .await
        .unwrap()
        .unwrap();
        match diff {
            FileDiff::Text(text) => text,
            other => panic!("not a text diff: {other:?}"),
        }
    }

    async fn ask(
        &self,
        view: &ChangesView,
        action: ChangeAction,
        target: ChangeTarget,
        confirmed: bool,
    ) -> Result<Option<ChangeStart>, ChangesError> {
        let request = ChangeRequest {
            handle: view.handle,
            action,
            target,
            confirmed,
        };
        changes_apply(
            self.changes.clone(),
            self.jobs.clone(),
            self.workspaces.clone(),
            git(),
            self.id.clone(),
            request,
        )
        .await
    }

    /// Runs a request to its job's end; a discard is confirmed.
    async fn apply(
        &mut self,
        view: &ChangesView,
        action: ChangeAction,
        target: ChangeTarget,
    ) -> JobView {
        let started = self
            .ask(view, action, target, action == ChangeAction::Discard)
            .await
            .unwrap()
            .unwrap();
        let ChangeStart::Started { job_id } = started else {
            panic!("the job did not start: {started:?}");
        };
        let ended = self.ends.recv().await.unwrap();
        assert_eq!(ended.job_id, job_id);
        ended
    }

    /// Applies `action` to the file at `path` of a fresh view, to its end.
    async fn file(&mut self, action: ChangeAction, path: &str) -> JobView {
        let view = self.view().await;
        let list = match action {
            ChangeAction::Unstage => FileList::Staged,
            ChangeAction::Stage | ChangeAction::Discard => FileList::Unstaged,
        };
        let file = find(&view, list, path);
        let target = ChangeTarget::Files { files: vec![file] };
        let ended = self.apply(&view, action, target).await;
        assert_eq!(ended.outcome, Some(Outcome::Succeeded), "{ended:?}");
        ended
    }

    /// `git status --porcelain`, untracked files listed one by one.
    fn status(&self) -> String {
        self.repo
            .git(["status", "--porcelain", "--untracked-files=all"])
            .unwrap()
    }

    fn cached_diff(&self) -> String {
        self.repo.git(["diff", "--cached", "--no-color"]).unwrap()
    }
}

/// The ref of the file of `list` at `path`.
fn find(view: &ChangesView, list: FileList, path: &str) -> FileRef {
    let ChangesContent::Uncommitted {
        staged, unstaged, ..
    } = &view.content
    else {
        panic!("not uncommitted changes");
    };
    let files: &[ChangedFile] = match list {
        FileList::Staged => staged,
        FileList::Unstaged => unstaged,
        FileList::Commit => panic!("no commit list here"),
    };
    let file = files
        .iter()
        .find(|file| file.path == path)
        .unwrap_or_else(|| panic!("{path} is not in {list:?}: {files:?}"));
    FileRef {
        list,
        id: u32::try_from(file.id).unwrap(),
    }
}

fn numbered(lines: std::ops::RangeInclusive<u32>) -> String {
    lines.map(|n| format!("line {n}\n")).collect()
}

/// A 30-line file with lines 2, 15 and 28 changed: three hunks.
fn three_hunks(repo: &Repo) -> String {
    let old = numbered(1..=30);
    repo.commit_file("f.txt", &old, "First").unwrap();
    let new = old
        .replace("line 2\n", "line two\n")
        .replace("line 15\n", "line fifteen\n")
        .replace("line 28\n", "line twenty-eight\n");
    repo.write("f.txt", &new).unwrap();
    new
}

fn new_repo(fixture: &Fixture) -> Repo {
    fixture.repo("app").unwrap()
}

#[tokio::test]
async fn ui_15_stage_and_unstage_a_file() {
    let fixture = Fixture::new().unwrap();
    let repo = new_repo(&fixture);
    repo.write("modified.txt", "one\n").unwrap();
    repo.write("deleted.txt", "gone\n").unwrap();
    repo.write("old name.txt", "renamed\ncontent\nkept\n")
        .unwrap();
    repo.write("mode.sh", "echo hi\n").unwrap();
    repo.commit_all("First").unwrap();
    repo.write("modified.txt", "two\n").unwrap();
    repo.remove("deleted.txt").unwrap();
    // `[a]` is a glob that would match `a.txt` too, were it not literal.
    repo.write("[a].txt", "new\n").unwrap();
    repo.write("a.txt", "other\n").unwrap();
    let mut run = Run::new(repo).await;

    // Modified, deleted and untracked: staged, then unstaged again.
    for (path, staged, unstaged) in [
        ("modified.txt", "M  modified.txt", " M modified.txt"),
        ("deleted.txt", "D  deleted.txt", " D deleted.txt"),
        ("[a].txt", "A  [a].txt", "?? [a].txt"),
    ] {
        run.file(ChangeAction::Stage, path).await;
        assert!(run.status().contains(staged), "{}", run.status());
        // `[a].txt` was taken literally: `a.txt` stays untracked.
        assert!(run.status().contains("?? a.txt"), "{}", run.status());
        run.file(ChangeAction::Unstage, path).await;
        assert!(run.status().contains(unstaged), "{}", run.status());
    }
    // The name with `[a]` was taken literally: nothing else was staged.
    assert_eq!(run.cached_diff(), "");

    // Added: unstaged back to untracked.
    run.repo.write("added.txt", "added\n").unwrap();
    run.repo.git(["add", "added.txt"]).unwrap();
    run.file(ChangeAction::Unstage, "added.txt").await;
    assert!(run.status().contains("?? added.txt"), "{}", run.status());

    // Renamed: both paths go back.
    run.repo
        .git(["mv", "old name.txt", "new name.txt"])
        .unwrap();
    assert!(
        run.status()
            .contains("R  \"old name.txt\" -> \"new name.txt\"")
    );
    run.file(ChangeAction::Unstage, "new name.txt").await;
    let status = run.status();
    assert!(status.contains(" D \"old name.txt\""), "{status}");
    assert!(status.contains("?? \"new name.txt\""), "{status}");
    assert_eq!(run.cached_diff(), "");
    run.repo.remove("new name.txt").unwrap();
    run.repo.git(["checkout", "--", "old name.txt"]).unwrap();

    // Mode changed: staged with update-index, which works on every file
    // system, and unstaged.
    run.repo
        .git(["update-index", "--chmod=+x", "mode.sh"])
        .unwrap();
    assert!(run.cached_diff().contains("new mode 100755"));
    run.file(ChangeAction::Unstage, "mode.sh").await;
    assert!(
        !run.cached_diff().contains("mode.sh"),
        "{}",
        run.cached_diff()
    );

    // Where the file system keeps the executable bit, a mode change in the
    // working tree is staged like any other change.
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let script = run.repo.path().join("mode.sh");
        std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
        run.file(ChangeAction::Stage, "mode.sh").await;
        assert!(run.cached_diff().contains("new mode 100755"));
        run.file(ChangeAction::Unstage, "mode.sh").await;
        assert!(!run.cached_diff().contains("mode.sh"));
    }
}

#[tokio::test]
async fn ui_15_unstage_on_an_unborn_branch() {
    let fixture = Fixture::new().unwrap();
    let repo = new_repo(&fixture);
    repo.write("first.txt", "first\n").unwrap();
    repo.git(["add", "first.txt"]).unwrap();
    let mut run = Run::new(repo).await;

    run.file(ChangeAction::Unstage, "first.txt").await;
    assert_eq!(run.status(), "?? first.txt\n");
    run.file(ChangeAction::Stage, "first.txt").await;
    assert_eq!(run.status(), "A  first.txt\n");
}

#[tokio::test]
async fn ui_15_stage_and_unstage_several_files_in_one_job() {
    let fixture = Fixture::new().unwrap();
    let repo = new_repo(&fixture);
    repo.commit_file("a.txt", "a\n", "First").unwrap();
    repo.write("a.txt", "A\n").unwrap();
    repo.write("b.txt", "b\n").unwrap();
    let mut run = Run::new(repo).await;

    let view = run.view().await;
    let files = vec![
        find(&view, FileList::Unstaged, "a.txt"),
        find(&view, FileList::Unstaged, "b.txt"),
    ];
    let ended = run
        .apply(&view, ChangeAction::Stage, ChangeTarget::Files { files })
        .await;
    assert_eq!(ended.outcome, Some(Outcome::Succeeded), "{ended:?}");
    assert_eq!(ended.label, "Stage 2 files");
    assert_eq!(run.status(), "M  a.txt\nA  b.txt\n");
}

#[tokio::test]
async fn ui_15_stage_and_unstage_a_hunk() {
    let fixture = Fixture::new().unwrap();
    let repo = new_repo(&fixture);
    let new = three_hunks(&repo);
    let mut run = Run::new(repo).await;

    let view = run.view().await;
    let file = find(&view, FileList::Unstaged, "f.txt");
    let diff = run.diff(&view, file).await;
    assert_eq!(diff.hunks.len(), 3);
    let target = ChangeTarget::Hunk { file, hunk: 1 };
    let ended = run.apply(&view, ChangeAction::Stage, target).await;
    assert_eq!(ended.outcome, Some(Outcome::Succeeded), "{ended:?}");
    assert_eq!(ended.label, "Stage a hunk of f.txt");

    // The index has exactly the second change.
    let staged = run.cached_diff();
    let changed: Vec<&str> = staged
        .lines()
        .filter(|line| {
            (line.starts_with('+') || line.starts_with('-'))
                && !line.starts_with("+++")
                && !line.starts_with("---")
        })
        .collect();
    assert_eq!(changed, ["-line 15", "+line fifteen"], "{staged}");
    // The working tree is untouched.
    let on_disk = std::fs::read_to_string(run.repo.path().join("f.txt")).unwrap();
    assert_eq!(on_disk, new);

    // Unstaging that hunk from the staged list restores the index.
    let view = run.view().await;
    let file = find(&view, FileList::Staged, "f.txt");
    let diff = run.diff(&view, file).await;
    assert_eq!(diff.hunks.len(), 1);
    let target = ChangeTarget::Hunk { file, hunk: 0 };
    let ended = run.apply(&view, ChangeAction::Unstage, target).await;
    assert_eq!(ended.outcome, Some(Outcome::Succeeded), "{ended:?}");
    assert_eq!(run.cached_diff(), "");
    assert_eq!(run.status(), " M f.txt\n");
}

#[tokio::test]
async fn ui_15_a_hunk_at_the_end_without_a_newline() {
    let fixture = Fixture::new().unwrap();
    let repo = new_repo(&fixture);
    repo.commit_file("f.txt", numbered(1..=20), "First")
        .unwrap();
    let new = numbered(1..=20).replace("line 2\n", "line two\n") + "no newline";
    repo.write("f.txt", &new).unwrap();
    let mut run = Run::new(repo).await;

    let view = run.view().await;
    let file = find(&view, FileList::Unstaged, "f.txt");
    assert_eq!(run.diff(&view, file).await.hunks.len(), 2);
    let target = ChangeTarget::Hunk { file, hunk: 1 };
    let ended = run.apply(&view, ChangeAction::Stage, target).await;
    assert_eq!(ended.outcome, Some(Outcome::Succeeded), "{ended:?}");
    let staged = run.repo.git(["show", ":f.txt"]).unwrap();
    assert_eq!(staged, numbered(1..=20) + "no newline");
}

#[tokio::test]
async fn ui_15_a_hunk_of_a_file_whose_name_git_quotes() {
    let fixture = Fixture::new().unwrap();
    let repo = new_repo(&fixture);
    let name = "two words é.txt";
    repo.commit_file(name, numbered(1..=20), "First").unwrap();
    repo.write(name, numbered(1..=20).replace("line 2\n", "line two\n"))
        .unwrap();
    let mut run = Run::new(repo).await;

    let view = run.view().await;
    let file = find(&view, FileList::Unstaged, name);
    run.diff(&view, file).await;
    let target = ChangeTarget::Hunk { file, hunk: 0 };
    let ended = run.apply(&view, ChangeAction::Stage, target).await;
    assert_eq!(ended.outcome, Some(Outcome::Succeeded), "{ended:?}");
    let status = run
        .repo
        .git(["-c", "core.quotePath=false", "status", "--porcelain"])
        .unwrap();
    assert_eq!(status, "M  \"two words é.txt\"\n");
}

#[tokio::test]
async fn ui_15_hunks_of_a_file_checked_out_with_crlf() {
    let fixture = Fixture::new().unwrap();
    let repo = new_repo(&fixture);
    let _ = three_hunks(&repo);
    let mut run = Run::new(repo).await;
    // As on Windows: the working tree has CRLF, the index LF.
    run.repo.git(["config", "core.autocrlf", "true"]).unwrap();
    let crlf = |text: &str| text.replace('\n', "\r\n");
    let changed = numbered(1..=30)
        .replace("line 2\n", "line two\n")
        .replace("line 15\n", "line fifteen\n")
        .replace("line 28\n", "line twenty-eight\n");
    run.repo.write("f.txt", crlf(&changed)).unwrap();

    let view = run.view().await;
    let file = find(&view, FileList::Unstaged, "f.txt");
    assert_eq!(run.diff(&view, file).await.hunks.len(), 3);
    let target = ChangeTarget::Hunk { file, hunk: 1 };
    let ended = run.apply(&view, ChangeAction::Stage, target).await;
    assert_eq!(ended.outcome, Some(Outcome::Succeeded), "{ended:?}");
    let staged = run.repo.git(["show", ":f.txt"]).unwrap();
    assert_eq!(
        staged,
        numbered(1..=30).replace("line 15\n", "line fifteen\n")
    );

    let view = run.view().await;
    let file = find(&view, FileList::Unstaged, "f.txt");
    run.diff(&view, file).await;
    let target = ChangeTarget::Hunk { file, hunk: 0 };
    let ended = run.apply(&view, ChangeAction::Discard, target).await;
    assert_eq!(ended.outcome, Some(Outcome::Succeeded), "{ended:?}");
    let on_disk = std::fs::read_to_string(run.repo.path().join("f.txt")).unwrap();
    assert_eq!(on_disk, crlf(&changed.replace("line two\n", "line 2\n")));
}

#[tokio::test]
async fn ui_15_discard_a_file_and_a_hunk() {
    let fixture = Fixture::new().unwrap();
    let repo = new_repo(&fixture);
    repo.write("other.txt", "kept\n").unwrap();
    let _ = three_hunks(&repo);
    repo.write("other.txt", "changed\n").unwrap();
    repo.write("untracked.txt", "new\n").unwrap();
    repo.write("folder/inner/a.txt", "a\n").unwrap();
    repo.write("folder/b.txt", "b\n").unwrap();
    let mut run = Run::new(repo).await;

    // A hunk: the first change goes, the others stay.
    let view = run.view().await;
    let file = find(&view, FileList::Unstaged, "f.txt");
    assert_eq!(run.diff(&view, file).await.hunks.len(), 3);
    let target = ChangeTarget::Hunk { file, hunk: 0 };
    let ended = run.apply(&view, ChangeAction::Discard, target).await;
    assert_eq!(ended.outcome, Some(Outcome::Succeeded), "{ended:?}");
    let on_disk = std::fs::read_to_string(run.repo.path().join("f.txt")).unwrap();
    let expected = numbered(1..=30)
        .replace("line 15\n", "line fifteen\n")
        .replace("line 28\n", "line twenty-eight\n");
    assert_eq!(on_disk, expected);

    // A tracked file, an untracked file and an untracked folder.
    run.file(ChangeAction::Discard, "other.txt").await;
    run.file(ChangeAction::Discard, "untracked.txt").await;
    // git lists the untracked folder as one entry.
    run.file(ChangeAction::Discard, "folder/").await;
    assert_eq!(run.status(), " M f.txt\n");
    assert!(!run.repo.path().join("folder").exists());
    assert_eq!(
        std::fs::read_to_string(run.repo.path().join("other.txt")).unwrap(),
        "kept\n"
    );
}

#[tokio::test]
async fn grp_9_discard_needs_confirmation() {
    let fixture = Fixture::new().unwrap();
    let repo = new_repo(&fixture);
    repo.commit_file("a.txt", "a\n", "First").unwrap();
    repo.write("a.txt", "changed\n").unwrap();
    let mut run = Run::new(repo).await;

    let view = run.view().await;
    let file = find(&view, FileList::Unstaged, "a.txt");
    let target = ChangeTarget::Files { files: vec![file] };
    let answer = run
        .ask(&view, ChangeAction::Discard, target.clone(), false)
        .await
        .unwrap();
    assert_eq!(
        answer,
        Some(ChangeStart::Confirm {
            repo: "app".to_owned(),
            files: vec!["a.txt".to_owned()],
        })
    );
    // Nothing ran.
    assert_eq!(run.status(), " M a.txt\n");
    assert!(run.jobs.all_views().is_empty());

    // A hunk too.
    run.diff(&view, file).await;
    let hunk = ChangeTarget::Hunk { file, hunk: 0 };
    let answer = run
        .ask(&view, ChangeAction::Discard, hunk, false)
        .await
        .unwrap();
    assert!(matches!(answer, Some(ChangeStart::Confirm { .. })));

    // Stage and unstage need none.
    let ended = run.apply(&view, ChangeAction::Stage, target.clone()).await;
    assert_eq!(ended.outcome, Some(Outcome::Succeeded));
    run.repo.git(["reset", "--quiet"]).unwrap();

    let view = run.view().await;
    let file = find(&view, FileList::Unstaged, "a.txt");
    let target = ChangeTarget::Files { files: vec![file] };
    let ended = run.apply(&view, ChangeAction::Discard, target).await;
    assert_eq!(ended.outcome, Some(Outcome::Succeeded));
    assert_eq!(run.status(), "");
}

#[tokio::test]
async fn ui_15_stale_hunk_is_refused() {
    let fixture = Fixture::new().unwrap();
    let repo = new_repo(&fixture);
    let new = three_hunks(&repo);
    let run = Run::new(repo).await;

    let view = run.view().await;
    let file = find(&view, FileList::Unstaged, "f.txt");
    run.diff(&view, file).await;
    // The user's editor saves while the diff is shown.
    run.repo
        .write("f.txt", new.replace("line 20\n", "line twenty\n"))
        .unwrap();
    let target = ChangeTarget::Hunk { file, hunk: 1 };
    let answer = run
        .ask(&view, ChangeAction::Stage, target.clone(), false)
        .await
        .unwrap();
    assert_eq!(answer, Some(ChangeStart::FileChanged));
    // A discard of that hunk too, confirmed: the edit is never lost.
    let answer = run
        .ask(&view, ChangeAction::Discard, target.clone(), true)
        .await
        .unwrap();
    assert_eq!(answer, Some(ChangeStart::FileChanged));
    let on_disk = std::fs::read_to_string(run.repo.path().join("f.txt")).unwrap();
    assert!(on_disk.contains("line twenty\n"), "{on_disk}");
    assert_eq!(run.cached_diff(), "");
    assert!(run.jobs.all_views().is_empty());

    // So does a change to the index side.
    run.repo.write("f.txt", &new).unwrap();
    let view = run.view().await;
    let file = find(&view, FileList::Unstaged, "f.txt");
    run.diff(&view, file).await;
    run.repo.git(["add", "f.txt"]).unwrap();
    run.repo.write("f.txt", numbered(1..=31)).unwrap();
    let target = ChangeTarget::Hunk { file, hunk: 1 };
    let answer = run
        .ask(&view, ChangeAction::Stage, target, false)
        .await
        .unwrap();
    assert_eq!(answer, Some(ChangeStart::FileChanged));

    // Read again, the hunk applies.
    let mut run = run;
    let view = run.view().await;
    let file = find(&view, FileList::Unstaged, "f.txt");
    run.diff(&view, file).await;
    let target = ChangeTarget::Hunk { file, hunk: 0 };
    let ended = run.apply(&view, ChangeAction::Stage, target).await;
    assert_eq!(ended.outcome, Some(Outcome::Succeeded), "{ended:?}");

    // Unstage: the index changed in a terminal while the staged diff is shown.
    let view = run.view().await;
    let file = find(&view, FileList::Staged, "f.txt");
    run.diff(&view, file).await;
    let staged = run.cached_diff();
    run.repo.git(["add", "f.txt"]).unwrap();
    let target = ChangeTarget::Hunk { file, hunk: 0 };
    let answer = run
        .ask(&view, ChangeAction::Unstage, target, false)
        .await
        .unwrap();
    assert_eq!(answer, Some(ChangeStart::FileChanged));
    assert_ne!(run.cached_diff(), staged);
}

#[tokio::test]
async fn ui_15_hunks_need_the_diff_shown_with_whitespace() {
    let fixture = Fixture::new().unwrap();
    let repo = new_repo(&fixture);
    let _ = three_hunks(&repo);
    let run = Run::new(repo).await;

    let view = run.view().await;
    let file = find(&view, FileList::Unstaged, "f.txt");
    // No diff read yet.
    let target = ChangeTarget::Hunk { file, hunk: 0 };
    let refused = run
        .ask(&view, ChangeAction::Stage, target.clone(), false)
        .await;
    assert!(
        matches!(refused, Err(ChangesError::Refused(_))),
        "{refused:?}"
    );

    let request = DiffRequest {
        ignore_whitespace: true,
        ..DiffRequest::default()
    };
    diff_get(
        run.changes.clone(),
        run.id.clone(),
        view.handle,
        file,
        request,
    )
    .await
    .unwrap();
    let refused = run.ask(&view, ChangeAction::Stage, target, false).await;
    assert!(
        matches!(refused, Err(ChangesError::Refused(_))),
        "{refused:?}"
    );
    // A staged file cannot be staged.
    let wrong = ChangeTarget::Files {
        files: vec![FileRef {
            list: FileList::Staged,
            id: 0,
        }],
    };
    let refused = run.ask(&view, ChangeAction::Stage, wrong, false).await;
    assert!(refused.is_err(), "{refused:?}");
}

#[tokio::test]
async fn ui_15_a_locked_index_fails_the_job_with_gits_message() {
    let fixture = Fixture::new().unwrap();
    let repo = new_repo(&fixture);
    repo.commit_file("a.txt", "a\n", "First").unwrap();
    repo.commit_file("b.txt", "b\n", "Second").unwrap();
    repo.write("a.txt", "changed\n").unwrap();
    repo.write("b.txt", "staged\n").unwrap();
    repo.git(["add", "b.txt"]).unwrap();
    let mut run = Run::new(repo).await;

    let lock = run.repo.path().join(".git").join("index.lock");
    std::fs::write(&lock, "").unwrap();
    let view = run.view().await;
    // Stage, unstage and discard each fail and change nothing.
    for (action, list, path) in [
        (ChangeAction::Stage, FileList::Unstaged, "a.txt"),
        (ChangeAction::Unstage, FileList::Staged, "b.txt"),
        (ChangeAction::Discard, FileList::Unstaged, "a.txt"),
    ] {
        let file = find(&view, list, path);
        let target = ChangeTarget::Files { files: vec![file] };
        let ended = run.apply(&view, action, target).await;
        assert_eq!(ended.outcome, Some(Outcome::Failed), "{action:?}");
        assert!(ended.output.contains("index.lock"), "{ended:?}");
    }
    std::fs::remove_file(&lock).unwrap();
    assert_eq!(run.status(), " M a.txt\nM  b.txt\n");
}
