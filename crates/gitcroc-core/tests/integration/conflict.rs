//! Resolving a conflicted file from the changes view (OPS-5, OPS-6, OPS-7,
//! DATA-3) with real git, each resolution a job. Each test waits for the
//! job's end event; nothing waits for a time. Conflicts are made with `git`
//! as a terminal user would.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use gitcroc_core::{
    Changes, ChangesContent, ChangesTarget, ChoiceKind, ChooseRequest, ConfigDir, Conflict,
    ConflictChoice, ConflictContent, ConflictRead, ConflictStart, GitRunner, JobEvents, JobId,
    JobView, Jobs, Outcome, RepoId, ResolveRequest, SideHolds, Summaries, SummaryCache,
    SummaryView, WorkspaceId, Workspaces, add_path, changes_get, conflict_choose, conflict_get,
    conflict_resolve,
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
    changes: Arc<Changes>,
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
            summaries,
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
            changes: Arc::new(Changes::new()),
            jobs,
            ends,
            id,
            repo_id,
        }
    }

    /// Reads the working copy as a new view: its handle and the id of each
    /// unmerged file by path.
    async fn view(&self) -> (u32, Vec<(String, u32)>) {
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
        let files = unmerged
            .into_iter()
            .map(|file| (file.path, u32::try_from(file.id).unwrap()))
            .collect();
        (view.handle, files)
    }

    /// The view's handle and the id of unmerged `path` in it.
    async fn file(&self, path: &str) -> (u32, u32) {
        let (handle, files) = self.view().await;
        let id = files
            .iter()
            .find(|(name, _)| name == path)
            .unwrap_or_else(|| panic!("{path} is not unmerged: {files:?}"))
            .1;
        (handle, id)
    }

    async fn read(&self, handle: u32, file: u32) -> ConflictRead {
        conflict_get(self.changes.clone(), self.id.clone(), handle, file)
            .await
            .unwrap()
            .unwrap()
    }

    async fn conflict(&self, path: &str) -> Conflict {
        let (handle, file) = self.file(path).await;
        match self.read(handle, file).await {
            ConflictRead::Conflict { conflict } => *conflict,
            ConflictRead::AlreadyResolved => panic!("{path} is resolved"),
        }
    }

    async fn resolve(
        &self,
        handle: u32,
        file: u32,
        text: &str,
        override_markers: bool,
    ) -> ConflictStart {
        let request = ResolveRequest {
            handle,
            file,
            text: text.to_owned(),
            override_markers,
        };
        conflict_resolve(
            self.changes.clone(),
            self.jobs.clone(),
            git(&self.dirs),
            self.id.clone(),
            request,
        )
        .await
        .unwrap()
        .unwrap()
    }

    async fn choose(&self, handle: u32, file: u32, side: ConflictChoice) -> ConflictStart {
        let request = ChooseRequest { handle, file, side };
        conflict_choose(
            self.changes.clone(),
            self.jobs.clone(),
            git(&self.dirs),
            self.id.clone(),
            request,
        )
        .await
        .unwrap()
        .unwrap()
    }

    /// Waits for the end of the job `start` started; it must succeed.
    async fn ended(&mut self, start: ConflictStart) -> JobView {
        let ConflictStart::Started { job_id } = start else {
            panic!("not started: {start:?}");
        };
        let ended = self.ends.recv().await.unwrap();
        assert_eq!(ended.job_id, job_id);
        assert_eq!(ended.outcome, Some(Outcome::Succeeded), "{ended:#?}");
        ended
    }
}

/// `main` and `feature` changed `a.txt` differently; `main` merging
/// `feature` stopped on it.
fn merging(fixture: &Fixture) -> Repo {
    let repo = fixture.repo("app").unwrap();
    repo.commit_file("a.txt", "a\n", "Base").unwrap();
    repo.branch("feature").unwrap();
    repo.commit_file("a.txt", "main\n", "Main").unwrap();
    repo.switch("feature").unwrap();
    repo.commit_file("a.txt", "feature\n", "Feature").unwrap();
    repo.switch("main").unwrap();
    repo.merge_with_conflicts("feature").unwrap();
    repo
}

/// `git ls-files --stage` of `path`: one line per stage.
fn stages(repo: &Repo, path: &str) -> Vec<String> {
    repo.git(["ls-files", "--stage", "--", path])
        .unwrap()
        .lines()
        .map(str::to_owned)
        .collect()
}

/// The single stage-0 entry of `path`: its mode and object.
fn resolved_entry(repo: &Repo, path: &str) -> (String, String) {
    let lines = stages(repo, path);
    assert_eq!(lines.len(), 1, "{path} is not resolved: {lines:?}");
    let mut parts = lines[0].split_whitespace();
    let (mode, id, stage) = (
        parts.next().unwrap(),
        parts.next().unwrap(),
        parts.next().unwrap(),
    );
    assert_eq!(stage, "0", "{lines:?}");
    (mode.to_owned(), id.to_owned())
}

fn blob(repo: &Repo, revision: &str) -> String {
    repo.git(["rev-parse", revision]).unwrap().trim().to_owned()
}

/// Every path under `root`, the git directory included, except git's
/// object store, where `git add` writes the blob.
fn all_paths(root: &Path) -> BTreeSet<PathBuf> {
    let mut found = BTreeSet::new();
    let mut dirs = vec![root.to_path_buf()];
    while let Some(dir) = dirs.pop() {
        for entry in std::fs::read_dir(&dir).unwrap() {
            let path = entry.unwrap().path();
            let relative = path.strip_prefix(root).unwrap().to_path_buf();
            if relative.starts_with(Path::new(".git").join("objects")) {
                continue;
            }
            if path.is_dir() {
                dirs.push(path);
            }
            found.insert(relative);
        }
    }
    found
}

#[tokio::test]
async fn ops_5_mark_resolved_refuses_markers() {
    let fixture = Fixture::new().unwrap();
    let repo = merging(&fixture);
    let mut run = Run::new(&repo).await;
    let (handle, file) = run.file("a.txt").await;

    for (text, line) in [
        ("<<<<<<< HEAD\nmain\n", 1),
        ("main\n=======\nfeature\n", 2),
        ("main\n|||||||\n", 2),
        ("main\n>>>>>>> feature", 2),
    ] {
        assert_eq!(
            run.resolve(handle, file, text, false).await,
            ConflictStart::MarkersRemain { line },
            "{text:?}"
        );
    }
    assert_eq!(stages(&repo, "a.txt").len(), 3, "nothing ran");

    // Marker-like text that is no marker line.
    for text in ["<<<<<<<< eight\n", "  =======\n", "a <<<<<<< b\n"] {
        let (handle, file) = run.file("a.txt").await;
        let start = run.resolve(handle, file, text, false).await;
        run.ended(start).await;
        assert_eq!(
            std::fs::read_to_string(repo.path().join("a.txt")).unwrap(),
            text
        );
        resolved_entry(&repo, "a.txt");
        repo.git(["checkout", "--merge", "--", "a.txt"]).unwrap();
    }

    // The override, for a file that legitimately has marker lines.
    let (handle, file) = run.file("a.txt").await;
    let text = "=======\n";
    let start = run.resolve(handle, file, text, true).await;
    run.ended(start).await;
    assert_eq!(
        std::fs::read_to_string(repo.path().join("a.txt")).unwrap(),
        text
    );
    resolved_entry(&repo, "a.txt");
}

#[tokio::test]
async fn ops_5_mark_resolved_writes_and_stages() {
    let fixture = Fixture::new().unwrap();
    let repo = fixture.repo("app").unwrap();
    // CRLF in the repo and on disk, and an executable file.
    let commit = |text: &str, message: &str| {
        add(&repo, "run.sh", text.as_bytes());
        executable(&repo, "run.sh");
        repo.git(["commit", "--quiet", "-m", message]).unwrap();
    };
    commit("#!/bin/sh\r\nbase\r\n", "Base");
    repo.branch("feature").unwrap();
    commit("#!/bin/sh\r\nmain\r\n", "Main");
    repo.switch("feature").unwrap();
    commit("#!/bin/sh\r\nfeature\r\n", "Feature");
    repo.switch("main").unwrap();
    repo.merge_with_conflicts("feature").unwrap();
    let mut run = Run::new(&repo).await;

    let conflict = run.conflict("run.sh").await;
    assert!(!conflict.edited_outside);
    assert!(matches!(conflict.content, ConflictContent::Text { .. }));
    let before = all_paths(repo.path());
    let (handle, file) = run.file("run.sh").await;
    let start = run.resolve(handle, file, "#!/bin/sh\nboth\n", false).await;
    let ended = run.ended(start).await;
    assert_eq!(ended.label, "Resolve run.sh");

    assert_eq!(
        std::fs::read(repo.path().join("run.sh")).unwrap(),
        b"#!/bin/sh\r\nboth\r\n"
    );
    let (mode, id) = resolved_entry(&repo, "run.sh");
    assert_eq!(mode, "100755");
    assert_eq!(
        repo.git(["cat-file", "blob", &id]).unwrap(),
        "#!/bin/sh\r\nboth\r\n"
    );
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = std::fs::metadata(repo.path().join("run.sh"))
            .unwrap()
            .permissions()
            .mode();
        assert_ne!(mode & 0o111, 0, "the file is no longer executable");
    }
    // Nothing else was created in the repo or its git directory (DATA-3).
    let created: Vec<_> = all_paths(repo.path())
        .difference(&before)
        .cloned()
        .collect();
    assert_eq!(created, Vec::<PathBuf>::new());
}

#[tokio::test]
async fn ops_5_a_conflict_resolved_meanwhile_is_refused_as_already_resolved() {
    let fixture = Fixture::new().unwrap();
    let repo = merging(&fixture);
    let run = Run::new(&repo).await;
    let (handle, file) = run.file("a.txt").await;

    // Resolved in a terminal after the view was read.
    repo.write("a.txt", "terminal\n").unwrap();
    repo.git(["add", "a.txt"]).unwrap();

    assert_eq!(run.read(handle, file).await, ConflictRead::AlreadyResolved);
    assert_eq!(
        run.resolve(handle, file, "app\n", false).await,
        ConflictStart::AlreadyResolved
    );
    assert_eq!(
        run.choose(handle, file, ConflictChoice::Theirs).await,
        ConflictStart::AlreadyResolved
    );
    assert_eq!(
        std::fs::read_to_string(repo.path().join("a.txt")).unwrap(),
        "terminal\n"
    );
    // Read again, the lists no longer have it.
    assert_eq!(run.view().await.1, Vec::<(String, u32)>::new());
}

/// A branch `feature` from "Base"; `main` then `feature` get a commit each
/// from `on_main` and `on_feature`, then `main` merges `feature` and stops.
fn conflict_of(
    fixture: &Fixture,
    name: &str,
    base: impl Fn(&Repo),
    on_main: impl Fn(&Repo),
    on_feature: impl Fn(&Repo),
) -> Repo {
    let repo = fixture.repo(name).unwrap();
    base(&repo);
    repo.git(["commit", "--quiet", "--allow-empty", "-m", "Base"])
        .unwrap();
    repo.branch("feature").unwrap();
    on_main(&repo);
    repo.git(["commit", "--quiet", "-m", "Main"]).unwrap();
    repo.switch("feature").unwrap();
    on_feature(&repo);
    repo.git(["commit", "--quiet", "-m", "Feature"]).unwrap();
    repo.switch("main").unwrap();
    repo.merge_with_conflicts("feature").unwrap();
    repo
}

fn add(repo: &Repo, path: &str, contents: &[u8]) {
    repo.write(path, contents).unwrap();
    repo.git(["add", "--", path]).unwrap();
}

/// Makes `path` executable in the index and, where the file system has
/// the bit, on disk.
fn executable(repo: &Repo, path: &str) {
    repo.git(["update-index", "--chmod=+x", "--", path])
        .unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let file = repo.path().join(path);
        std::fs::set_permissions(file, std::fs::Permissions::from_mode(0o755)).unwrap();
    }
}

/// A submodule commit without a submodule: a gitlink in the index.
fn gitlink(repo: &Repo, path: &str, commit: &str) {
    repo.git([
        "update-index",
        "--add",
        "--cacheinfo",
        &format!("160000,{commit},{path}"),
    ])
    .unwrap();
}

const SUB_BASE: &str = "1111111111111111111111111111111111111111";
const SUB_MAIN: &str = "2222222222222222222222222222222222222222";
const SUB_FEATURE: &str = "3333333333333333333333333333333333333333";

#[tokio::test]
async fn ops_6_choice_conflicts() {
    let fixture = Fixture::new().unwrap();
    let none = |_: &Repo| {};
    let binary = conflict_of(
        &fixture,
        "binary",
        |repo| add(repo, "f.bin", b"\0base"),
        |repo| add(repo, "f.bin", b"\0main"),
        |repo| add(repo, "f.bin", b"\0feature"),
    );
    let deleted_by_us = conflict_of(
        &fixture,
        "deleted-by-us",
        |repo| add(repo, "f.txt", b"base\n"),
        |repo| {
            repo.git(["rm", "--quiet", "f.txt"]).unwrap();
        },
        |repo| add(repo, "f.txt", b"feature\n"),
    );
    let deleted_by_them = conflict_of(
        &fixture,
        "deleted-by-them",
        |repo| add(repo, "f.txt", b"base\n"),
        |repo| add(repo, "f.txt", b"main\n"),
        |repo| {
            repo.git(["rm", "--quiet", "f.txt"]).unwrap();
        },
    );
    let mode = conflict_of(
        &fixture,
        "mode",
        none,
        |repo| {
            add(repo, "f.sh", b"same\n");
            executable(repo, "f.sh");
        },
        |repo| add(repo, "f.sh", b"same\n"),
    );
    let submodule = conflict_of(
        &fixture,
        "submodule",
        |repo| gitlink(repo, "sub", SUB_BASE),
        |repo| gitlink(repo, "sub", SUB_MAIN),
        |repo| gitlink(repo, "sub", SUB_FEATURE),
    );

    let file = |size: u64, executable: bool| SideHolds::File { size, executable };
    let submodule_at = |commit: &str| SideHolds::Submodule {
        commit: commit.to_owned(),
    };
    let cases: [(&Repo, &str, ChoiceKind, SideHolds, SideHolds); 5] = [
        (
            &binary,
            "f.bin",
            ChoiceKind::Binary,
            file(5, false),
            file(8, false),
        ),
        (
            &deleted_by_us,
            "f.txt",
            ChoiceKind::DeletedByUs,
            SideHolds::Deleted,
            file(8, false),
        ),
        (
            &deleted_by_them,
            "f.txt",
            ChoiceKind::DeletedByThem,
            file(5, false),
            SideHolds::Deleted,
        ),
        (
            &mode,
            "f.sh",
            ChoiceKind::Mode,
            file(5, true),
            file(5, false),
        ),
        (
            &submodule,
            "sub",
            ChoiceKind::Submodule,
            submodule_at(SUB_MAIN),
            submodule_at(SUB_FEATURE),
        ),
    ];
    for (repo, path, kind, ours, theirs) in cases {
        let run = Run::new(repo).await;
        let conflict = run.conflict(path).await;
        assert_eq!(
            conflict.content,
            ConflictContent::Choice {
                reason: kind,
                ours,
                theirs,
            },
            "{path} in {}",
            repo.path().display()
        );
        assert!(!conflict.edited_outside);
    }

    // Each choice leaves the index resolved with the chosen side.
    for (repo, path, side, expected) in [
        (
            &binary,
            "f.bin",
            ConflictChoice::Theirs,
            Some(("100644", blob(&binary, "feature:f.bin"))),
        ),
        (&deleted_by_us, "f.txt", ConflictChoice::Ours, None),
        (
            &deleted_by_them,
            "f.txt",
            ConflictChoice::Ours,
            Some(("100644", blob(&deleted_by_them, "main:f.txt"))),
        ),
        (
            &mode,
            "f.sh",
            ConflictChoice::Theirs,
            Some(("100644", blob(&mode, "feature:f.sh"))),
        ),
        (
            &submodule,
            "sub",
            ConflictChoice::Theirs,
            Some(("160000", SUB_FEATURE.to_owned())),
        ),
    ] {
        let mut run = Run::new(repo).await;
        let (handle, file) = run.file(path).await;
        let start = run.choose(handle, file, side).await;
        run.ended(start).await;
        let on_disk = repo.path().join(path);
        match expected {
            Some((mode, id)) => {
                assert_eq!(
                    resolved_entry(repo, path),
                    (mode.to_owned(), id.clone()),
                    "{path}"
                );
                if mode != "160000" {
                    let blob = repo.git_output(["cat-file", "blob", &id]).unwrap().stdout;
                    assert_eq!(std::fs::read(&on_disk).unwrap(), blob, "{path} on disk");
                }
            }
            None => {
                assert_eq!(stages(repo, path), Vec::<String>::new(), "{path}");
                assert!(!on_disk.exists(), "{path} is still on disk");
            }
        }
        assert_eq!(run.view().await.1, Vec::<(String, u32)>::new(), "{path}");
    }

    // The other side of a deletion: the file is kept with their content.
    let kept = conflict_of(
        &fixture,
        "kept",
        |repo| add(repo, "f.txt", b"base\n"),
        |repo| {
            repo.git(["rm", "--quiet", "f.txt"]).unwrap();
        },
        |repo| add(repo, "f.txt", b"feature\n"),
    );
    let mut run = Run::new(&kept).await;
    let (handle, file) = run.file("f.txt").await;
    let start = run.choose(handle, file, ConflictChoice::Theirs).await;
    let ended = run.ended(start).await;
    assert_eq!(ended.label, "Resolve f.txt with feature");
    assert_eq!(
        resolved_entry(&kept, "f.txt").1,
        blob(&kept, "feature:f.txt")
    );
    assert_eq!(
        std::fs::read_to_string(kept.path().join("f.txt")).unwrap(),
        "feature\n"
    );
}

/// Every file of the work tree outside `.git` with its bytes and
/// modification time.
fn work_tree(root: &Path) -> Vec<(PathBuf, Vec<u8>, std::time::SystemTime)> {
    all_paths(root)
        .into_iter()
        .filter(|path| !path.starts_with(".git") && root.join(path).is_file())
        .map(|path| {
            let full = root.join(&path);
            let modified = std::fs::metadata(&full).unwrap().modified().unwrap();
            (path, std::fs::read(&full).unwrap(), modified)
        })
        .collect()
}

#[tokio::test]
async fn ops_9_nothing_is_written_before_mark_resolved() {
    let fixture = Fixture::new().unwrap();
    let repo = fixture.repo("app").unwrap();
    repo.commit_file("a.txt", "a\n", "Base a").unwrap();
    repo.commit_file("b.txt", "b\n", "Base b").unwrap();
    repo.branch("feature").unwrap();
    repo.commit_file("a.txt", "main a\n", "Main a").unwrap();
    repo.commit_file("b.txt", "main b\n", "Main b").unwrap();
    repo.switch("feature").unwrap();
    repo.commit_file("a.txt", "feature a\n", "Feature a")
        .unwrap();
    repo.commit_file("b.txt", "feature b\n", "Feature b")
        .unwrap();
    repo.switch("main").unwrap();
    repo.merge_with_conflicts("feature").unwrap();
    let mut run = Run::new(&repo).await;
    let files = work_tree(repo.path());
    let index = (stages(&repo, "a.txt"), stages(&repo, "b.txt"));

    // The editor opens a file, the user edits it (in the UI alone),
    // switches to the other file and to a new view and back: only reads.
    for path in ["a.txt", "b.txt", "a.txt"] {
        let (handle, file) = run.file(path).await;
        assert!(matches!(
            run.read(handle, file).await,
            ConflictRead::Conflict { .. }
        ));
    }

    assert_eq!(work_tree(repo.path()), files);
    assert_eq!((stages(&repo, "a.txt"), stages(&repo, "b.txt")), index);
    assert!(run.jobs.all_views().is_empty(), "a job ran");
    assert!(run.ends.try_recv().is_err(), "a job ended");

    // Mark resolved is the first write.
    let (handle, file) = run.file("a.txt").await;
    let start = run.resolve(handle, file, "both a\n", false).await;
    run.ended(start).await;
    assert_eq!(
        std::fs::read_to_string(repo.path().join("a.txt")).unwrap(),
        "both a\n"
    );
    assert_eq!(stages(&repo, "b.txt"), index.1);
}
