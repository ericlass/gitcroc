//! What the changes pane shows (UI-14, UI-15, UI-16, UI-18) and how its
//! files cross IPC: one view per workspace, one file's diff at a time
//! (ARCH-8, ARCH-9).

// Integration test crate: helpers outside #[test] fns are test code too (QA-4).
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::path::{Path, PathBuf};
use std::sync::Arc;

use gitcroc_core::{
    Changes, ChangesContent, ChangesError, ChangesTarget, ChangesView, ConfigDir, Context,
    DiffRequest, FileDiff, FileList, FileRef, FileStatus, GitRunner, ImageSide, JobId, RepoId,
    WorkspaceId, Workspaces, add_path, changes_get, diff_get, diff_image,
};
use repo_gen::{Fixture, Repo};

struct Dirs {
    root: tempfile::TempDir,
}

impl ConfigDir for Dirs {
    fn config_dir(&self) -> Option<PathBuf> {
        Some(self.root.path().join("config"))
    }
}

struct Run {
    _dirs: Dirs,
    workspaces: Arc<Workspaces>,
    changes: Arc<Changes>,
    id: WorkspaceId,
    repo: RepoId,
}

impl Run {
    async fn with_repo(path: &Path) -> Self {
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
        let repo = workspaces.view(&id).unwrap().repos[0].id.clone();
        Self {
            _dirs: dirs,
            workspaces,
            changes: Arc::new(Changes::new()),
            id,
            repo,
        }
    }

    async fn get(&self, target: ChangesTarget) -> Option<ChangesView> {
        changes_get(
            self.changes.clone(),
            self.workspaces.clone(),
            git(),
            self.id.clone(),
            self.repo.clone(),
            target,
        )
        .await
        .unwrap()
    }

    async fn commit(&self, hash: &str, parent: u32) -> ChangesView {
        let target = ChangesTarget::Commit {
            hash: hash.to_owned(),
            parent,
        };
        self.get(target).await.unwrap()
    }

    async fn diff(&self, handle: u32, list: FileList, id: u32) -> Option<FileDiff> {
        let file = FileRef { list, id };
        diff_get(
            self.changes.clone(),
            self.id.clone(),
            handle,
            file,
            DiffRequest::default(),
        )
        .await
        .unwrap()
    }
}

fn git() -> GitRunner {
    GitRunner::new(PathBuf::from("git"))
}

fn paths(files: &[gitcroc_core::ChangedFile]) -> Vec<&str> {
    files.iter().map(|file| file.path.as_str()).collect()
}

fn new_repo(fixture: &Fixture) -> Repo {
    fixture.repo("app").unwrap()
}

#[tokio::test]
async fn ui_14_commit_header_files_and_one_diff() {
    let fixture = Fixture::new().unwrap();
    let repo = new_repo(&fixture);
    let first = repo.commit_file("a.txt", "one\n", "First").unwrap();
    repo.write("b.txt", "b\n").unwrap();
    let second = repo
        .commit_file("a.txt", "one\ntwo\n", "Second\n\nWith a body.")
        .unwrap();
    let run = Run::with_repo(repo.path()).await;

    let view = run.commit(&second, 0).await;
    let ChangesContent::Commit {
        details,
        parent,
        files,
    } = view.content
    else {
        panic!("not a commit view");
    };
    assert_eq!(details.hash, second);
    assert_eq!(details.message, "Second\n\nWith a body.\n");
    assert_eq!(details.parents, [first]);
    assert_eq!(parent, 0);
    assert_eq!(paths(&files), ["a.txt", "b.txt"]);

    let diff = run.diff(view.handle, FileList::Commit, 0).await.unwrap();
    let FileDiff::Text(text) = diff else {
        panic!("not a text diff: {diff:?}");
    };
    assert_eq!((text.added, text.removed), (1, 0));

    // A list the view does not have, or an id past its end, is refused.
    let file = FileRef {
        list: FileList::Staged,
        id: 0,
    };
    let missing = diff_get(
        run.changes.clone(),
        run.id.clone(),
        view.handle,
        file,
        DiffRequest::default(),
    )
    .await;
    assert!(matches!(missing, Err(ChangesError::UnknownFile(0))));
    let file = FileRef {
        list: FileList::Commit,
        id: 2,
    };
    let past = diff_get(
        run.changes.clone(),
        run.id.clone(),
        view.handle,
        file,
        DiffRequest::default(),
    )
    .await;
    assert!(matches!(past, Err(ChangesError::UnknownFile(2))));
}

#[tokio::test]
async fn ui_16_merge_commit_against_either_parent() {
    let fixture = Fixture::new().unwrap();
    let repo = new_repo(&fixture);
    repo.commit_file("base.txt", "base\n", "Base").unwrap();
    repo.branch("side").unwrap();
    repo.commit_file("main.txt", "main\n", "On main").unwrap();
    repo.switch("side").unwrap();
    repo.commit_file("side.txt", "side\n", "On side").unwrap();
    repo.switch("main").unwrap();
    let merge = repo.merge("side", "Merge side").unwrap();
    let run = Run::with_repo(repo.path()).await;

    // Against the first parent, the merge brought in the side branch's file.
    let first = run.commit(&merge, 0).await;
    let ChangesContent::Commit { details, files, .. } = &first.content else {
        panic!("not a commit view");
    };
    assert_eq!(details.parents.len(), 2);
    assert_eq!(paths(files), ["side.txt"]);

    let second = run.commit(&merge, 1).await;
    let ChangesContent::Commit { parent, files, .. } = &second.content else {
        panic!("not a commit view");
    };
    assert_eq!(*parent, 1);
    assert_eq!(paths(files), ["main.txt"]);
    let diff = run.diff(second.handle, FileList::Commit, 0).await.unwrap();
    assert!(matches!(diff, FileDiff::Text(text) if text.added == 1));
}

#[tokio::test]
async fn ui_15_uncommitted_lists_and_their_diffs() {
    let fixture = Fixture::new().unwrap();
    let repo = new_repo(&fixture);
    repo.commit_file("a.txt", "a\n", "First").unwrap();
    repo.write("a.txt", "a\nstaged\n").unwrap();
    repo.git(["add", "a.txt"]).unwrap();
    repo.write("a.txt", "a\nstaged\nunstaged\n").unwrap();
    repo.write("new.txt", "new\n").unwrap();
    let run = Run::with_repo(repo.path()).await;

    let view = run.get(ChangesTarget::Uncommitted).await.unwrap();
    let ChangesContent::Uncommitted {
        staged,
        unstaged,
        unmerged,
        last_message,
        in_conflict,
    } = &view.content
    else {
        panic!("not an uncommitted view");
    };
    assert_eq!(
        last_message.as_ref().map(|m| m.subject.as_str()),
        Some("First")
    );
    assert!(!in_conflict);
    assert_eq!(paths(staged), ["a.txt"]);
    assert_eq!(paths(unstaged), ["a.txt", "new.txt"]);
    assert_eq!(unstaged[1].status, FileStatus::Untracked);
    assert!(unmerged.is_empty());

    let staged_diff = run.diff(view.handle, FileList::Staged, 0).await.unwrap();
    let FileDiff::Text(text) = staged_diff else {
        panic!("not a text diff");
    };
    assert_eq!(text.hunks[0].lines.last().unwrap().text, "staged");
    let unstaged_diff = run.diff(view.handle, FileList::Unstaged, 0).await.unwrap();
    let FileDiff::Text(text) = unstaged_diff else {
        panic!("not a text diff");
    };
    assert_eq!(text.hunks[0].lines.last().unwrap().text, "unstaged");
    let untracked = run.diff(view.handle, FileList::Unstaged, 1).await.unwrap();
    assert!(matches!(untracked, FileDiff::Text(text) if text.added == 1));
}

#[tokio::test]
async fn ui_15_unmerged_files_are_listed_apart() {
    let fixture = Fixture::new().unwrap();
    let repo = new_repo(&fixture);
    repo.commit_file("a.txt", "base\n", "Base").unwrap();
    repo.branch("side").unwrap();
    repo.commit_file("a.txt", "main\n", "On main").unwrap();
    repo.switch("side").unwrap();
    repo.commit_file("a.txt", "side\n", "On side").unwrap();
    repo.switch("main").unwrap();
    repo.merge_with_conflicts("side").unwrap();
    let run = Run::with_repo(repo.path()).await;

    let view = run.get(ChangesTarget::Uncommitted).await.unwrap();
    let ChangesContent::Uncommitted {
        staged,
        unstaged,
        unmerged,
        in_conflict,
        ..
    } = &view.content
    else {
        panic!("not an uncommitted view");
    };
    // The merge's commit is Continue's (M2-08): no commit box.
    assert!(in_conflict);
    assert!(staged.is_empty() && unstaged.is_empty());
    assert_eq!(unmerged.len(), 1);
    assert_eq!(unmerged[0].path, "a.txt");
}

#[tokio::test]
async fn arch_8_a_newer_view_supersedes_the_older() {
    let fixture = Fixture::new().unwrap();
    let repo = new_repo(&fixture);
    let first = repo.commit_file("a.txt", "a\n", "First").unwrap();
    let second = repo.commit_file("a.txt", "b\n", "Second").unwrap();
    let run = Run::with_repo(repo.path()).await;

    let older = run.commit(&first, 0).await;
    let newer = run.commit(&second, 0).await;
    assert_ne!(older.handle, newer.handle);
    assert!(run.diff(older.handle, FileList::Commit, 0).await.is_none());
    assert!(run.diff(newer.handle, FileList::Commit, 0).await.is_some());

    // Another workspace has its own view.
    let other = changes_get(
        run.changes.clone(),
        run.workspaces.clone(),
        git(),
        WorkspaceId("elsewhere".to_owned()),
        run.repo.clone(),
        ChangesTarget::Uncommitted,
    )
    .await;
    assert!(matches!(other, Err(ChangesError::Workspace(_))));
    assert!(run.diff(newer.handle, FileList::Commit, 0).await.is_some());
}

/// A PNG of the given size: its signature and padding.
fn png(size: usize) -> Vec<u8> {
    let mut bytes = b"\x89PNG\r\n\x1a\n".to_vec();
    bytes.resize(size, 0);
    bytes
}

#[tokio::test]
async fn ui_18_image_bytes_come_from_the_last_image_diff() {
    let fixture = Fixture::new().unwrap();
    let repo = new_repo(&fixture);
    repo.commit_file("logo.png", png(64), "Add").unwrap();
    repo.write("note.txt", "note\n").unwrap();
    let hash = repo.commit_file("logo.png", png(80), "Grow").unwrap();
    let run = Run::with_repo(repo.path()).await;

    let view = run.commit(&hash, 0).await;
    let image = FileRef {
        list: FileList::Commit,
        id: 0,
    };
    let side = |file, side| diff_image(&run.changes, &run.id, view.handle, file, side);
    // Nothing until the diff was read.
    assert_eq!(side(image, ImageSide::New), None);

    let diff = run.diff(view.handle, FileList::Commit, 0).await.unwrap();
    let FileDiff::Image { old, new } = &diff else {
        panic!("not an image diff: {diff:?}");
    };
    assert_eq!(old.as_ref().unwrap().size, 64);
    assert_eq!(new.as_ref().unwrap().size, 80);
    // The bytes are not part of the diff's JSON form.
    let json = serde_json::to_value(&diff).unwrap();
    assert!(json["new"].get("bytes").is_none(), "{json}");
    assert_eq!(side(image, ImageSide::Old), Some(png(64)));
    assert_eq!(side(image, ImageSide::New), Some(png(80)));

    // Another file's diff replaces them.
    run.diff(view.handle, FileList::Commit, 1).await.unwrap();
    assert_eq!(side(image, ImageSide::New), None);
}

#[tokio::test]
async fn ui_17_whitespace_and_context_are_applied_by_the_core() {
    let fixture = Fixture::new().unwrap();
    let repo = new_repo(&fixture);
    let lines: String = (1..=30).map(|n| format!("line {n}\n")).collect();
    repo.commit_file("a.txt", &lines, "First").unwrap();
    let changed = lines
        .replace("line 15\n", "line  15\n")
        .replace("line 16\n", "line sixteen\n");
    let hash = repo.commit_file("a.txt", &changed, "Second").unwrap();
    let run = Run::with_repo(repo.path()).await;
    let view = run.commit(&hash, 0).await;

    let read = |ignore_whitespace, context| {
        let file = FileRef {
            list: FileList::Commit,
            id: 0,
        };
        let request = DiffRequest {
            ignore_whitespace,
            context,
            ..DiffRequest::default()
        };
        let changes = run.changes.clone();
        let id = run.id.clone();
        async move {
            match diff_get(changes, id, view.handle, file, request).await {
                Ok(Some(FileDiff::Text(text))) => text,
                other => panic!("not a text diff: {other:?}"),
            }
        }
    };
    let text = read(false, Context::Three).await;
    assert_eq!((text.added, text.removed), (2, 2));
    assert_eq!(text.hunks[0].lines.len(), 3 + 4 + 3);
    assert!(text.new_text.is_none());
    let text = read(true, Context::Three).await;
    assert_eq!((text.added, text.removed), (1, 1));
    // Side-by-side asks for the whole file: both texts come with it.
    let text = read(false, Context::WholeFile).await;
    assert_eq!(text.new_text.as_deref(), Some(changed.as_str()));
    assert_eq!(text.old_text.as_deref(), Some(lines.as_str()));
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
async fn arch_8_a_view_keeps_no_repo_open_between_diffs() {
    let fixture = Fixture::new().unwrap();
    let repo = new_repo(&fixture);
    repo.commit_file("a.txt", "a\n", "First").unwrap();
    repo.write("b.txt", "b\n").unwrap();
    let second = repo.commit_file("a.txt", "a\nb\n", "Second").unwrap();
    repo.git(["gc", "--quiet"]).unwrap();
    let run = Run::with_repo(repo.path()).await;

    // An open repo would keep the pack parts the reads used in memory for
    // as long as the view is shown (M1-14).
    let view = run.commit(&second, 0).await;
    #[cfg(target_os = "linux")]
    assert_eq!(mapped_packs(repo.path()), 0);
    for id in [0, 1] {
        assert!(run.diff(view.handle, FileList::Commit, id).await.is_some());
        #[cfg(target_os = "linux")]
        assert_eq!(mapped_packs(repo.path()), 0);
    }
}
