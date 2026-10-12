//! Working copy status, conflict state and summary on fixture repos made
//! with git, as a terminal user would (ARCH-2, OPS-1, UI-3, UI-15).

// Integration test crate: helpers outside #[test] fns are test code too (QA-4).
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::path::PathBuf;

use gitcroc_git::{
    ChangedFile, Conflict, DiffOptions, DiffRead, DiffSource, Engine, Fallback, FileDiff,
    FileStatus, GitRepo, GitRunner, Head, LineKind, Operation, RepoState, StatusRead, StatusReader,
    Upstream, WorkingCopy,
};
use repo_gen::{Fixture, Repo};

fn reader(git: &Repo) -> StatusReader {
    StatusReader::new(
        GitRepo::open(git.path()).unwrap(),
        GitRunner::new(PathBuf::from("git")),
    )
}

fn status(git: &Repo, engine: Engine) -> WorkingCopy {
    reader(git).with_engine(engine).status().unwrap()
}

/// (status, path, old path, mode change) of each entry.
fn summary(files: &[ChangedFile]) -> Vec<(FileStatus, &str, Option<&str>, Option<String>)> {
    files
        .iter()
        .map(|f| {
            (
                f.status,
                f.path.as_str(),
                f.old_path.as_deref(),
                f.mode_change
                    .as_ref()
                    .map(|m| format!("{}>{}", m.old, m.new)),
            )
        })
        .collect()
}

/// A repo with every kind of working copy change git reports by default.
fn busy_repo(fixture: &Fixture, name: &str) -> Repo {
    let git = fixture.repo(name).unwrap();
    git.write(".gitignore", "*.log\n").unwrap();
    git.write("modified.txt", "one\ntwo\n").unwrap();
    git.write("both.txt", "one\n").unwrap();
    git.write("deleted.txt", "gone\n").unwrap();
    git.write("removed.txt", "gone too\n").unwrap();
    git.write(
        "renamed-from.txt",
        "a file long enough to be found as a rename\n",
    )
    .unwrap();
    git.write("script.sh", "echo hi\n").unwrap();
    git.write("name with spaces ü.txt", "spaces\n").unwrap();
    git.commit_all("First").unwrap();

    git.write("modified.txt", "one\ntwo\nthree\n").unwrap();
    git.write("both.txt", "one\nstaged\n").unwrap();
    git.git(["add", "both.txt"]).unwrap();
    git.write("both.txt", "one\nstaged\nunstaged\n").unwrap();
    git.write("added.txt", "new\n").unwrap();
    git.git(["add", "added.txt"]).unwrap();
    git.remove("deleted.txt").unwrap();
    git.git(["rm", "--quiet", "removed.txt"]).unwrap();
    git.git(["mv", "renamed-from.txt", "renamed-to.txt"])
        .unwrap();
    git.git(["update-index", "--chmod=+x", "script.sh"])
        .unwrap();
    // Without this, the file would also show as unstaged on platforms
    // with an executable bit.
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let script = git.path().join("script.sh");
        std::fs::set_permissions(script, std::fs::Permissions::from_mode(0o755)).unwrap();
    }
    git.write("name with spaces ü.txt", "spaces\nchanged\n")
        .unwrap();
    git.write("untracked.txt", "untracked\n").unwrap();
    git.write("new-folder/a.txt", "a\n").unwrap();
    git.write("new-folder/b.txt", "b\n").unwrap();
    git.write("debug.log", "ignored\n").unwrap();
    git.write("intent.txt", "intent\n").unwrap();
    git.git(["add", "--intent-to-add", "intent.txt"]).unwrap();
    git
}

#[test]
fn arch_2_status_matches_git() {
    let fixture = Fixture::new().unwrap();
    let git = busy_repo(&fixture, "busy");
    let by_gix = status(&git, Engine::Gix);
    let by_git = status(&git, Engine::Git);
    assert_eq!(by_gix, by_git);

    assert_eq!(
        summary(&by_gix.staged),
        [
            (FileStatus::Added, "added.txt", None, None),
            (FileStatus::Modified, "both.txt", None, None),
            (FileStatus::Deleted, "removed.txt", None, None),
            (
                FileStatus::Renamed,
                "renamed-to.txt",
                Some("renamed-from.txt"),
                None
            ),
            (
                FileStatus::Modified,
                "script.sh",
                None,
                Some("100644>100755".into())
            ),
        ]
    );
    assert_eq!(
        summary(&by_gix.unstaged),
        [
            (FileStatus::Modified, "both.txt", None, None),
            (FileStatus::Deleted, "deleted.txt", None, None),
            (FileStatus::Added, "intent.txt", None, None),
            (FileStatus::Modified, "modified.txt", None, None),
            (FileStatus::Modified, "name with spaces ü.txt", None, None),
            (FileStatus::Untracked, "new-folder/", None, None),
            (FileStatus::Untracked, "untracked.txt", None, None),
        ]
    );
    assert!(by_gix.unmerged.is_empty());
    let ids: Vec<usize> = by_gix.unstaged.iter().map(|f| f.id).collect();
    assert_eq!(ids, [0, 1, 2, 3, 4, 5, 6]);

    // The repo's config for untracked files is followed.
    git.git(["config", "status.showUntrackedFiles", "all"])
        .unwrap();
    let all = status(&git, Engine::Gix);
    assert_eq!(all, status(&git, Engine::Git));
    let untracked: Vec<&str> = all
        .unstaged
        .iter()
        .filter(|f| f.status == FileStatus::Untracked)
        .map(|f| f.path.as_str())
        .collect();
    assert_eq!(
        untracked,
        ["new-folder/a.txt", "new-folder/b.txt", "untracked.txt"]
    );
    git.git(["config", "status.showUntrackedFiles", "no"])
        .unwrap();
    let none = status(&git, Engine::Gix);
    assert_eq!(none, status(&git, Engine::Git));
    assert!(
        none.unstaged
            .iter()
            .all(|f| f.status != FileStatus::Untracked)
    );

    // And its config for renames. Copies count as renames, so both engines
    // pair the same files.
    let renamed = |working_copy: &WorkingCopy| {
        working_copy
            .staged
            .iter()
            .filter(|f| f.status == FileStatus::Renamed)
            .count()
    };
    git.git(["config", "status.renames", "false"]).unwrap();
    let off = status(&git, Engine::Gix);
    assert_eq!(off, status(&git, Engine::Git));
    assert_eq!(renamed(&off), 0);
    git.git(["config", "status.renames", "copies"]).unwrap();
    git.write("copy.txt", "a file long enough to be found as a rename\n")
        .unwrap();
    git.git(["add", "copy.txt"]).unwrap();
    let copies = status(&git, Engine::Gix);
    assert_eq!(copies, status(&git, Engine::Git));
    assert_eq!(renamed(&copies), 1);
}

#[cfg(unix)]
#[test]
fn arch_2_status_matches_git_for_modes_and_links() {
    use std::os::unix::fs::PermissionsExt;

    let fixture = Fixture::new().unwrap();
    let git = fixture.repo("modes").unwrap();
    git.write("tool.sh", "echo\n").unwrap();
    git.write("link-me.txt", "target\n").unwrap();
    git.commit_all("First").unwrap();
    let tool = git.path().join("tool.sh");
    std::fs::set_permissions(&tool, std::fs::Permissions::from_mode(0o755)).unwrap();
    git.remove("link-me.txt").unwrap();
    std::os::unix::fs::symlink("tool.sh", git.path().join("link-me.txt")).unwrap();
    std::os::unix::fs::symlink("tool.sh", git.path().join("new-link")).unwrap();

    let by_gix = status(&git, Engine::Gix);
    assert_eq!(by_gix, status(&git, Engine::Git));
    assert_eq!(
        summary(&by_gix.unstaged),
        [
            (
                FileStatus::TypeChanged,
                "link-me.txt",
                None,
                Some("100644>120000".into())
            ),
            (FileStatus::Untracked, "new-link", None, None),
            (
                FileStatus::Modified,
                "tool.sh",
                None,
                Some("100644>100755".into())
            ),
        ]
    );
}

/// A repo whose merge leaves every kind of conflict, among them files that
/// exist only in stage 2 or 3.
fn every_conflict(fixture: &Fixture) -> Repo {
    let git = fixture.repo("conflicts").unwrap();
    git.write("delete-modify.txt", "1\n2\n").unwrap();
    git.write("modify-delete.txt", "1\n2\n").unwrap();
    git.write("renamed.txt", "1\n2\n3\n4\n5\n6\n7\n8\n")
        .unwrap();
    git.commit_all("Base").unwrap();
    git.branch("other").unwrap();
    git.switch("other").unwrap();
    git.write("add-add.txt", "theirs\n").unwrap();
    git.remove("modify-delete.txt").unwrap();
    git.write("delete-modify.txt", "1\nthem\n").unwrap();
    git.git(["mv", "renamed.txt", "their-name.txt"]).unwrap();
    git.commit_all("Theirs").unwrap();
    git.switch("main").unwrap();
    git.write("add-add.txt", "ours\n").unwrap();
    git.remove("delete-modify.txt").unwrap();
    git.write("modify-delete.txt", "1\nus\n").unwrap();
    git.git(["mv", "renamed.txt", "our-name.txt"]).unwrap();
    git.commit_all("Ours").unwrap();
    git.merge_with_conflicts("other").unwrap();
    git
}

#[test]
fn arch_2_status_of_a_fallback_repo_matches_git() {
    let fixture = Fixture::new().unwrap();

    let fsmonitor = busy_repo(&fixture, "fsmonitor");
    // A hook that cannot run: git warns and checks every file, on every
    // platform, without starting a monitor daemon.
    fsmonitor
        .git(["config", "core.fsmonitor", "gitcroc-missing-fsmonitor-hook"])
        .unwrap();

    let sparse = fixture.repo("sparse").unwrap();
    sparse.write("inside/a.txt", "a\n").unwrap();
    sparse.write("outside/b.txt", "b\n").unwrap();
    sparse.commit_all("First").unwrap();
    sparse
        .git([
            "sparse-checkout",
            "set",
            "--cone",
            "--sparse-index",
            "inside",
        ])
        .unwrap();
    sparse.write("inside/a.txt", "changed\n").unwrap();

    let untracked_cache = busy_repo(&fixture, "untracked-cache");
    untracked_cache
        .git(["update-index", "--untracked-cache"])
        .unwrap();
    untracked_cache.git(["status", "--porcelain"]).unwrap();

    let conflicts = every_conflict(&fixture);

    for (git, fallback) in [
        (&fsmonitor, Fallback::Fsmonitor),
        (&sparse, Fallback::SparseIndex),
        (&untracked_cache, Fallback::UntrackedCache),
        (&conflicts, Fallback::Unmerged),
    ] {
        let opened = reader(git);
        assert_eq!(opened.fallback().unwrap(), Some(fallback));
        assert_eq!(
            opened.status().unwrap(),
            status(git, Engine::Git),
            "{fallback:?}"
        );
    }

    let unmerged: Vec<_> = reader(&conflicts)
        .status()
        .unwrap()
        .unmerged
        .iter()
        .map(|f| (f.path.clone(), f.conflict))
        .collect();
    assert_eq!(
        unmerged,
        [
            ("add-add.txt".to_owned(), Conflict::BothAdded),
            ("delete-modify.txt".to_owned(), Conflict::DeletedByUs),
            ("modify-delete.txt".to_owned(), Conflict::DeletedByThem),
            ("our-name.txt".to_owned(), Conflict::AddedByUs),
            ("renamed.txt".to_owned(), Conflict::BothDeleted),
            ("their-name.txt".to_owned(), Conflict::AddedByThem),
        ]
    );

    // Config is read when the repo is opened.
    let opened = reader(&fsmonitor);
    fsmonitor
        .git(["config", "core.fsmonitor", "false"])
        .unwrap();
    assert_eq!(opened.fallback().unwrap(), Some(Fallback::Fsmonitor));
    assert_eq!(reader(&fsmonitor).fallback().unwrap(), None);
}

#[test]
fn arch_2_callers_cannot_tell_which_ran() {
    let fixture = Fixture::new().unwrap();
    let git = busy_repo(&fixture, "busy");
    let repo = GitRepo::open(git.path()).unwrap();
    let options = DiffOptions::default();
    let diffs = |working_copy: &WorkingCopy| -> Vec<FileDiff> {
        let staged = working_copy
            .staged
            .iter()
            .map(|f| repo.file_diff(&DiffSource::Staged, f, options).unwrap());
        let unstaged = working_copy
            .unstaged
            .iter()
            .map(|f| repo.file_diff(&DiffSource::Unstaged, f, options).unwrap());
        staged.chain(unstaged).collect()
    };
    let by_gix = status(&git, Engine::Gix);
    let by_git = status(&git, Engine::Git);
    assert_eq!(by_gix, by_git);
    assert_eq!(diffs(&by_gix), diffs(&by_git));
    assert_eq!(
        serde_json::to_value(&by_gix).unwrap(),
        serde_json::to_value(&by_git).unwrap()
    );
}

/// The text of the lines of a text diff, marked `+`, `-` or ` `.
fn lines(diff: &FileDiff) -> Vec<String> {
    let FileDiff::Text(text) = diff else {
        panic!("not a text diff: {diff:?}");
    };
    text.hunks
        .iter()
        .flat_map(|h| &h.lines)
        .map(|line| {
            let mark = match line.kind {
                LineKind::Added => '+',
                LineKind::Removed => '-',
                LineKind::Context => ' ',
            };
            format!("{mark}{}", line.text)
        })
        .collect()
}

#[test]
fn ui_15_staged_and_unstaged_diffs() {
    let fixture = Fixture::new().unwrap();
    let git = fixture.repo("diffs").unwrap();
    git.write("changed.txt", "one\n").unwrap();
    git.write("staged-delete.txt", "bye\n").unwrap();
    git.write("unstaged-delete.txt", "bye too\n").unwrap();
    git.commit_all("First").unwrap();
    git.write("changed.txt", "one\nstaged\n").unwrap();
    git.git(["add", "changed.txt"]).unwrap();
    git.write("changed.txt", "one\nstaged\nunstaged\n").unwrap();
    git.write("staged-new.txt", "fresh\n").unwrap();
    git.git(["add", "staged-new.txt"]).unwrap();
    git.git(["rm", "--quiet", "staged-delete.txt"]).unwrap();
    git.remove("unstaged-delete.txt").unwrap();
    git.write("untracked.txt", "brand new\n").unwrap();

    let repo = GitRepo::open(git.path()).unwrap();
    let working_copy = reader(&git).status().unwrap();
    let diff = |source: DiffSource, path: &str| {
        let list = match source {
            DiffSource::Staged => &working_copy.staged,
            _ => &working_copy.unstaged,
        };
        let file = list.iter().find(|f| f.path == path).unwrap();
        lines(
            &repo
                .file_diff(&source, file, DiffOptions::default())
                .unwrap(),
        )
    };
    assert_eq!(diff(DiffSource::Staged, "changed.txt"), [" one", "+staged"]);
    assert_eq!(
        diff(DiffSource::Unstaged, "changed.txt"),
        [" one", " staged", "+unstaged"]
    );
    assert_eq!(diff(DiffSource::Staged, "staged-new.txt"), ["+fresh"]);
    assert_eq!(diff(DiffSource::Staged, "staged-delete.txt"), ["-bye"]);
    assert_eq!(
        diff(DiffSource::Unstaged, "unstaged-delete.txt"),
        ["-bye too"]
    );
    assert_eq!(diff(DiffSource::Unstaged, "untracked.txt"), ["+brand new"]);

    // A file of one list cannot be diffed as another's.
    let staged = &working_copy.staged[0];
    assert!(
        repo.file_diff(&DiffSource::Unstaged, staged, DiffOptions::default())
            .is_err()
    );
}

#[test]
fn ui_15_untracked_folder_is_one_entry_without_content() {
    let fixture = Fixture::new().unwrap();
    let git = fixture.repo("folder").unwrap();
    git.commit_file("a.txt", "a\n", "First").unwrap();
    git.write("folder/inner/b.txt", "b\n").unwrap();
    let repo = GitRepo::open(git.path()).unwrap();
    let working_copy = reader(&git).status().unwrap();
    let folder = &working_copy.unstaged[0];
    assert_eq!(folder.path, "folder/");
    assert_eq!(
        repo.file_diff(&DiffSource::Unstaged, folder, DiffOptions::default())
            .unwrap(),
        FileDiff::Directory
    );
}

/// A repo on `main` with `file.txt` changed differently on `main` and on
/// `other`, so merging, rebasing or picking one into the other conflicts.
fn diverged(fixture: &Fixture, name: &str) -> Repo {
    let git = fixture.repo(name).unwrap();
    git.commit_file("file.txt", "base\n", "Base").unwrap();
    git.branch("other").unwrap();
    git.switch("other").unwrap();
    git.commit_file("file.txt", "theirs\n", "Theirs").unwrap();
    git.switch("main").unwrap();
    git.commit_file("file.txt", "ours\n", "Ours").unwrap();
    git
}

/// Runs a git command that is expected to stop with a conflict.
fn conflict(git: &Repo, args: &[&str]) {
    let out = git.git_output(args).unwrap();
    assert!(!out.status.success(), "{args:?} did not conflict");
}

/// A name, how to get there from [`diverged`], and the state expected.
type Case = (&'static str, fn(&Repo), Option<Operation>, u32);

#[test]
fn ops_1_detects_conflict_state() {
    let fixture = Fixture::new().unwrap();
    let cases: [Case; 7] = [
        ("clean", |_| {}, None, 0),
        (
            "merge",
            |git| conflict(git, &["merge", "--no-edit", "other"]),
            Some(Operation::Merge),
            1,
        ),
        (
            "rebase-merge",
            |git| conflict(git, &["rebase", "--merge", "other"]),
            Some(Operation::Rebase),
            1,
        ),
        (
            "rebase-apply",
            |git| conflict(git, &["rebase", "--apply", "other"]),
            Some(Operation::Rebase),
            1,
        ),
        (
            "cherry-pick",
            |git| conflict(git, &["cherry-pick", "other"]),
            Some(Operation::CherryPick),
            1,
        ),
        (
            "revert",
            |git| {
                git.commit_file("file.txt", "ours again\n", "Again")
                    .unwrap();
                conflict(git, &["revert", "--no-edit", "HEAD~1"]);
            },
            Some(Operation::Revert),
            1,
        ),
        (
            "stash-apply",
            |git| {
                git.write("file.txt", "stashed\n").unwrap();
                git.stash("Stashed").unwrap();
                git.commit_file("file.txt", "committed\n", "Committed")
                    .unwrap();
                conflict(git, &["stash", "apply"]);
            },
            None,
            1,
        ),
    ];
    for (name, make, operation, unmerged_paths) in cases {
        let git = diverged(&fixture, name);
        make(&git);
        let state = reader(&git).repo_state().unwrap();
        assert_eq!(
            state,
            RepoState {
                operation,
                unmerged_paths
            },
            "{name}"
        );
        assert_eq!(state.in_conflict(), name != "clean", "{name}");

        // The conflicted file is listed as unmerged by both paths, and in
        // neither the staged nor the unstaged list.
        let by_gix = reader(&git).status().unwrap();
        assert_eq!(by_gix, status(&git, Engine::Git), "{name}");
        let unmerged: Vec<_> = by_gix
            .unmerged
            .iter()
            .map(|f| (f.path.as_str(), f.conflict))
            .collect();
        let expected: &[(&str, Conflict)] = if unmerged_paths == 0 {
            &[]
        } else {
            &[("file.txt", Conflict::BothModified)]
        };
        assert_eq!(unmerged, expected, "{name}");
        assert!(by_gix.staged.iter().all(|f| f.path != "file.txt"), "{name}");
        assert!(
            by_gix.unstaged.iter().all(|f| f.path != "file.txt"),
            "{name}"
        );
    }
}

#[test]
fn ui_3_summary_of_branches_upstreams_and_detached_head() {
    let fixture = Fixture::new().unwrap();
    let upstream = fixture.repo("upstream").unwrap();
    upstream.commit_file("a.txt", "1\n", "One").unwrap();
    upstream.commit_file("a.txt", "2\n", "Two").unwrap();
    let clone = upstream.clone_to(fixture.path().join("clone")).unwrap();
    upstream.commit_file("b.txt", "3\n", "Three").unwrap();
    clone.git(["fetch", "--quiet"]).unwrap();
    clone.git(["reset", "--quiet", "--hard", "HEAD~1"]).unwrap();
    clone.commit_file("c.txt", "c\n", "Mine 1").unwrap();
    let head = clone.commit_file("c.txt", "cc\n", "Mine 2").unwrap();

    // Ahead 2 and behind 2 against origin/main, which has Two and Three.
    let summary = reader(&clone).summary().unwrap();
    assert_eq!(
        summary.head,
        Head::Branch {
            name: "main".into(),
            hash: Some(head.clone())
        }
    );
    assert_eq!(
        summary.upstream,
        Some(Upstream {
            name: "origin/main".into(),
            ahead: 2,
            behind: 2,
            gone: false
        })
    );
    assert!(!summary.dirty);
    assert!(!summary.state.in_conflict());

    // Ahead 2, behind 1.
    clone.git(["reset", "--quiet", "--hard", "HEAD~2"]).unwrap();
    clone
        .git(["merge", "--quiet", "--ff-only", "origin/main"])
        .unwrap();
    clone.git(["reset", "--quiet", "--hard", "HEAD~1"]).unwrap();
    clone.commit_file("c.txt", "c\n", "Mine 1").unwrap();
    clone.commit_file("c.txt", "cc\n", "Mine 2").unwrap();
    clone.write("dirty.txt", "x\n").unwrap();
    let summary = reader(&clone).summary().unwrap();
    let upstream = summary.upstream.unwrap();
    assert_eq!((upstream.ahead, upstream.behind), (2, 1));
    assert!(summary.dirty);

    // A branch without upstream.
    clone.git(["switch", "--quiet", "-c", "local"]).unwrap();
    let summary = reader(&clone).summary().unwrap();
    assert!(matches!(&summary.head, Head::Branch { name, .. } if name == "local"));
    assert_eq!(summary.upstream, None);

    // An upstream whose ref is gone.
    clone
        .git(["branch", "--quiet", "--set-upstream-to=origin/main"])
        .unwrap();
    clone
        .git(["update-ref", "-d", "refs/remotes/origin/main"])
        .unwrap();
    let summary = reader(&clone).summary().unwrap();
    assert_eq!(
        summary.upstream,
        Some(Upstream {
            name: "origin/main".into(),
            ahead: 0,
            behind: 0,
            gone: true
        })
    );

    // Detached HEAD.
    clone
        .git(["switch", "--quiet", "--detach", "HEAD~1"])
        .unwrap();
    let detached = clone.head().unwrap();
    let summary = reader(&clone).summary().unwrap();
    assert_eq!(summary.head, Head::Detached { hash: detached });
    assert_eq!(summary.upstream, None);
}

#[test]
fn ui_3_summary_of_an_unborn_branch() {
    let fixture = Fixture::new().unwrap();
    let git = fixture.repo("new").unwrap();
    let summary = reader(&git).summary().unwrap();
    assert_eq!(
        summary.head,
        Head::Branch {
            name: "main".into(),
            hash: None
        }
    );
    assert_eq!(summary.upstream, None);
    assert!(!summary.dirty);

    git.write("first.txt", "first\n").unwrap();
    git.git(["add", "first.txt"]).unwrap();
    let by_gix = status(&git, Engine::Gix);
    assert_eq!(by_gix, status(&git, Engine::Git));
    assert_eq!(by_gix.staged.len(), 1);
    assert_eq!(by_gix.staged[0].status, FileStatus::Added);
    assert!(reader(&git).summary().unwrap().dirty);
}
