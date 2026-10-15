//! Commit headers, changed files and file diffs on fixture repos (UI-14,
//! UI-16, UI-17, UI-18, ARCH-8, PLAT-2, SEC-3).

use std::ffi::OsString;

use gitcroc_git::GitRepo;
use gitcroc_git::{
    ChangedFile, Context, DiffError, DiffOptions, DiffRead, DiffSource, FileDiff, FileStatus,
    ImageFormat, LineKind, MAX_DIFF_LINES, TextDiff,
};
use repo_gen::{Fixture, Repo};

fn open(git: &Repo) -> GitRepo {
    GitRepo::open(git.path()).unwrap()
}

/// Commits what is staged, without `git add`, so index-only entries stay.
fn commit_index(git: &Repo, message: &str) -> String {
    git.git(["commit", "--quiet", "--no-verify", "-m", message])
        .unwrap();
    git.head().unwrap()
}

/// Stages `content` at `path` with `mode` without touching the working
/// tree, so any name and any mode work on every platform.
fn stage(git: &Repo, mode: &str, path: impl Into<OsString>, content: &[u8]) {
    git.write("blob.tmp", content).unwrap();
    let id = git.git(["hash-object", "-w", "blob.tmp"]).unwrap();
    git.remove("blob.tmp").unwrap();
    let args: Vec<OsString> = vec![
        "update-index".into(),
        "--add".into(),
        "--cacheinfo".into(),
        mode.into(),
        id.trim().into(),
        path.into(),
    ];
    git.git(args).unwrap();
}

/// (status, path, old path, added, removed) of a changed file.
type Summary<'a> = (
    FileStatus,
    &'a str,
    Option<&'a str>,
    Option<u32>,
    Option<u32>,
);

fn summary(files: &[ChangedFile]) -> Vec<Summary<'_>> {
    files
        .iter()
        .map(|f| {
            (
                f.status,
                f.path.as_str(),
                f.old_path.as_deref(),
                f.added,
                f.removed,
            )
        })
        .collect()
}

fn diff_of(
    repo: &GitRepo,
    hash: &str,
    parent: usize,
    path: &str,
    options: DiffOptions,
) -> FileDiff {
    let files = repo.changed_files(hash, parent).unwrap();
    let file = files.iter().find(|f| f.path == path).unwrap();
    let source = DiffSource::Commit {
        hash: hash.into(),
        parent,
    };
    repo.file_diff(&source, file, options).unwrap()
}

fn text(diff: FileDiff) -> TextDiff {
    match diff {
        FileDiff::Text(text) => text,
        other => panic!("not a text diff: {other:?}"),
    }
}

/// Twelve lines, enough for rename detection to find a match.
fn body(tag: &str) -> String {
    (1..=12).map(|n| format!("{tag} line {n}\n")).collect()
}

#[test]
fn ui_14_commit_header() {
    let fixture = Fixture::new().unwrap();
    let git = fixture.repo("app").unwrap();
    let first = git.commit_file("a.txt", "a\n", "First").unwrap();
    let hash = git
        .commit_file("a.txt", "b\n", "Subject\n\nBody line one.\nBody line two.")
        .unwrap();
    let details = open(&git).commit(&hash[..8]).unwrap();
    assert_eq!(details.hash, hash);
    assert_eq!(
        details.message,
        "Subject\n\nBody line one.\nBody line two.\n"
    );
    assert_eq!(details.parents, [first]);
    assert_eq!(details.author.name, repo_gen::FIXTURE_NAME);
    assert_eq!(details.committer.email, repo_gen::FIXTURE_EMAIL);
    assert!(details.committer.time.seconds >= details.author.time.seconds);
    assert!(matches!(
        open(&git).commit("nope"),
        Err(DiffError::UnknownCommit { .. })
    ));
}

#[test]
fn ui_14_changed_files_of_fixture_commits() {
    let fixture = Fixture::new().unwrap();
    let git = fixture.repo("app").unwrap();
    git.write("keep.txt", "keep\n").unwrap();
    git.write("old-name.txt", body("moved")).unwrap();
    git.write("gone.txt", "bye\n").unwrap();
    git.write("run.sh", "echo hi\n").unwrap();
    let root = git.commit_all("Root").unwrap();
    let repo = open(&git);

    // Root commit: everything added, against the empty tree.
    assert_eq!(
        summary(&repo.changed_files(&root, 0).unwrap()),
        [
            (FileStatus::Added, "gone.txt", None, Some(1), Some(0)),
            (FileStatus::Added, "keep.txt", None, Some(1), Some(0)),
            (FileStatus::Added, "old-name.txt", None, Some(12), Some(0)),
            (FileStatus::Added, "run.sh", None, Some(1), Some(0)),
        ]
    );
    assert!(matches!(
        repo.changed_files(&root, 1),
        Err(DiffError::NoParent { index: 1, .. })
    ));

    // Add, modify, delete and rename in one commit.
    git.write("new.txt", "new\n").unwrap();
    git.write("keep.txt", "keep\nmore\n").unwrap();
    git.remove("gone.txt").unwrap();
    git.remove("old-name.txt").unwrap();
    git.write(
        "dir/new-name.txt",
        body("moved").replace("line 12", "line twelve"),
    )
    .unwrap();
    let changes = git.commit_all("Changes").unwrap();
    let files = repo.changed_files(&changes, 0).unwrap();
    assert_eq!(
        summary(&files),
        [
            (
                FileStatus::Renamed,
                "dir/new-name.txt",
                Some("old-name.txt"),
                Some(1),
                Some(1)
            ),
            (FileStatus::Deleted, "gone.txt", None, Some(0), Some(1)),
            (FileStatus::Modified, "keep.txt", None, Some(1), Some(0)),
            (FileStatus::Added, "new.txt", None, Some(1), Some(0)),
        ]
    );
    let ids: Vec<usize> = files.iter().map(|f| f.id).collect();
    assert_eq!(ids, [0, 1, 2, 3]);

    // Rename detection off: a deletion and an addition.
    git.git(["config", "diff.renames", "false"]).unwrap();
    let repo = open(&git);
    let files = repo.changed_files(&changes, 0).unwrap();
    let statuses: Vec<(FileStatus, &str)> =
        files.iter().map(|f| (f.status, f.path.as_str())).collect();
    assert_eq!(
        statuses,
        [
            (FileStatus::Added, "dir/new-name.txt"),
            (FileStatus::Deleted, "gone.txt"),
            (FileStatus::Modified, "keep.txt"),
            (FileStatus::Added, "new.txt"),
            (FileStatus::Deleted, "old-name.txt"),
        ]
    );

    // Mode change only.
    git.git(["update-index", "--chmod=+x", "run.sh"]).unwrap();
    let chmod = commit_index(&git, "Make it executable");
    let files = repo.changed_files(&chmod, 0).unwrap();
    assert_eq!(
        summary(&files),
        [(FileStatus::Modified, "run.sh", None, Some(0), Some(0))]
    );
    let mode = files[0].mode_change.as_ref().unwrap();
    assert_eq!((mode.old.as_str(), mode.new.as_str()), ("100644", "100755"));
    let diff = text(diff_of(&repo, &chmod, 0, "run.sh", DiffOptions::default()));
    assert!(diff.hunks.is_empty());

    // A file becomes a symbolic link.
    stage(&git, "120000", "keep.txt", b"run.sh");
    let typechange = commit_index(&git, "Link");
    let files = repo.changed_files(&typechange, 0).unwrap();
    assert_eq!(files[0].status, FileStatus::TypeChanged);
    assert_eq!(files[0].path, "keep.txt");
}

#[test]
fn ui_16_merge_commit_against_each_parent() {
    let fixture = Fixture::new().unwrap();
    let git = fixture.repo("app").unwrap();
    git.commit_file("base.txt", "base\n", "Base").unwrap();
    git.branch("side").unwrap();
    git.commit_file("main.txt", "main\n", "On main").unwrap();
    git.switch("side").unwrap();
    git.commit_file("side.txt", "side\n", "On side").unwrap();
    git.switch("main").unwrap();
    let merge = git.merge("side", "Merge side").unwrap();
    let repo = open(&git);

    let paths = |parent| -> Vec<String> {
        repo.changed_files(&merge, parent)
            .unwrap()
            .into_iter()
            .map(|f| f.path)
            .collect()
    };
    assert_eq!(paths(0), ["side.txt"], "against the first parent (main)");
    assert_eq!(paths(1), ["main.txt"], "against the second parent (side)");
    assert!(matches!(
        repo.changed_files(&merge, 2),
        Err(DiffError::NoParent { index: 2, .. })
    ));

    // A file listed for one parent is refused for the other.
    let files = repo.changed_files(&merge, 0).unwrap();
    let other = DiffSource::Commit {
        hash: merge.clone(),
        parent: 1,
    };
    assert!(matches!(
        repo.file_diff(&other, &files[0], DiffOptions::default()),
        Err(DiffError::FileMismatch)
    ));
}

#[test]
fn arch_8_diff_is_one_file_at_a_time() {
    let fixture = Fixture::new().unwrap();
    let git = fixture.repo("app").unwrap();
    git.write("a.txt", "alpha-content\n").unwrap();
    git.write("b.txt", "beta-content\n").unwrap();
    let hash = git.commit_all("Two files").unwrap();
    let repo = open(&git);

    // The list holds no content, only what the UI shows.
    let files = repo.changed_files(&hash, 0).unwrap();
    let list = serde_json::to_value(&files).unwrap();
    let json = list.to_string();
    assert!(!json.contains("alpha-content") && !json.contains("beta-content"));
    let keys: Vec<&String> = list[0].as_object().unwrap().keys().collect();
    assert_eq!(
        keys,
        [
            "added",
            "id",
            "mode_change",
            "old_path",
            "path",
            "removed",
            "status"
        ]
    );

    // A diff holds its own file only.
    let source = DiffSource::Commit { hash, parent: 0 };
    let diff = repo
        .file_diff(&source, &files[0], DiffOptions::default())
        .unwrap();
    let json = serde_json::to_string(&diff).unwrap();
    assert!(json.contains("alpha-content") && !json.contains("beta-content"));
}

#[test]
fn ui_17_ignore_whitespace_and_context_options() {
    let fixture = Fixture::new().unwrap();
    let git = fixture.repo("app").unwrap();
    let lines: String = (1..=30).map(|n| format!("line {n}\n")).collect();
    git.commit_file("f.txt", &lines, "First").unwrap();
    let changed = lines
        .replace("line 2\n", "line  2\n")
        .replace("line 15\n", "line fifteen\n");
    let hash = git.commit_file("f.txt", &changed, "Second").unwrap();
    let repo = open(&git);

    let options = |ignore_whitespace, context| DiffOptions {
        ignore_whitespace,
        context,
        ..DiffOptions::default()
    };
    let three = text(diff_of(
        &repo,
        &hash,
        0,
        "f.txt",
        options(false, Context::Three),
    ));
    assert_eq!((three.added, three.removed, three.hunks.len()), (2, 2, 2));
    assert_eq!(
        three.hunks[0].lines.len(),
        6,
        "one line before, three after"
    );
    assert_eq!(three.old_text, None);

    let ten = text(diff_of(
        &repo,
        &hash,
        0,
        "f.txt",
        options(false, Context::Ten),
    ));
    assert_eq!(ten.hunks.len(), 1);
    assert_eq!((ten.hunks[0].old_start, ten.hunks[0].old_lines), (1, 25));

    let whole = text(diff_of(
        &repo,
        &hash,
        0,
        "f.txt",
        options(false, Context::WholeFile),
    ));
    assert_eq!(whole.hunks.len(), 1);
    assert_eq!(whole.hunks[0].old_lines, 30);
    assert_eq!(whole.old_text.as_deref(), Some(lines.as_str()));
    assert_eq!(whole.new_text.as_deref(), Some(changed.as_str()));

    let ignored = text(diff_of(
        &repo,
        &hash,
        0,
        "f.txt",
        options(true, Context::Three),
    ));
    assert_eq!(
        (ignored.added, ignored.removed, ignored.hunks.len()),
        (1, 1, 1)
    );
    let changed_lines: Vec<(LineKind, &str)> = ignored.hunks[0]
        .lines
        .iter()
        .filter(|line| line.kind != LineKind::Context)
        .map(|line| (line.kind, line.text.as_str()))
        .collect();
    assert_eq!(
        changed_lines,
        [
            (LineKind::Removed, "line 15"),
            (LineKind::Added, "line fifteen")
        ]
    );
}

#[test]
fn ui_18_binary_by_nul_byte_and_by_attribute() {
    let fixture = Fixture::new().unwrap();
    let git = fixture.repo("app").unwrap();
    git.write(
        ".gitattributes",
        "*.dat binary\n*.nodiff -diff\n*.svg text\n",
    )
    .unwrap();
    git.commit_all("Attributes").unwrap();
    // A NUL byte past the first 8,000 bytes does not make a file binary.
    let mut late_nul = vec![b'a'; 8_000];
    late_nul.extend_from_slice(b"\0\n");
    git.write("late.txt", &late_nul).unwrap();
    git.write("zero.bin", b"head\0tail\n").unwrap();
    git.write("text.dat", "plain text\n").unwrap();
    git.write("text.nodiff", "plain text\n").unwrap();
    git.write("logo.svg", "<svg xmlns=\"http://www.w3.org/2000/svg\"/>\n")
        .unwrap();
    let hash = git.commit_all("Binary files").unwrap();
    let repo = open(&git);

    let binary = FileDiff::Binary {
        old_size: None,
        new_size: Some(10),
    };
    assert_eq!(
        diff_of(&repo, &hash, 0, "zero.bin", DiffOptions::default()),
        binary
    );
    let binary = FileDiff::Binary {
        old_size: None,
        new_size: Some(11),
    };
    for path in ["text.dat", "text.nodiff"] {
        assert_eq!(
            diff_of(&repo, &hash, 0, path, DiffOptions::default()),
            binary
        );
    }
    for path in ["late.txt", "logo.svg"] {
        text(diff_of(&repo, &hash, 0, path, DiffOptions::default()));
    }
    let files = repo.changed_files(&hash, 0).unwrap();
    let zero = files.iter().find(|f| f.path == "zero.bin").unwrap();
    assert_eq!(
        (zero.added, zero.removed),
        (None, None),
        "no counts for binary files"
    );

    // Like git, the working tree's attributes apply, staged or not.
    git.write(".gitattributes", "*.svg -diff\n").unwrap();
    let repo = open(&git);
    assert_eq!(
        diff_of(&repo, &hash, 0, "logo.svg", DiffOptions::default()),
        FileDiff::Binary {
            old_size: None,
            new_size: Some(42)
        }
    );
}

#[test]
fn ui_18_images_before_and_after() {
    let fixture = Fixture::new().unwrap();
    let git = fixture.repo("app").unwrap();
    let png = |n: u8| [b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR".as_slice(), &[n; 20]].concat();
    git.commit_file("logo.png", png(1), "Logo").unwrap();
    git.write("logo.png", png(2)).unwrap();
    git.write("photo.jpg", b"\xff\xd8\xff\xe0\0\x10JFIF\0")
        .unwrap();
    // Too big to show: a binary file with sizes.
    let mut big = png(3);
    big.resize(10 * 1024 * 1024 + 1, 0);
    git.write("big.png", &big).unwrap();
    let hash = git.commit_all("Images").unwrap();
    let repo = open(&git);

    match diff_of(&repo, &hash, 0, "logo.png", DiffOptions::default()) {
        FileDiff::Image {
            old: Some(old),
            new: Some(new),
        } => {
            assert_eq!(
                (old.format, new.format),
                (ImageFormat::Png, ImageFormat::Png)
            );
            assert_eq!((old.bytes, new.bytes), (png(1), png(2)));
            assert_eq!(new.size, 36);
        }
        other => panic!("not an image diff: {other:?}"),
    }
    assert!(matches!(
        diff_of(&repo, &hash, 0, "photo.jpg", DiffOptions::default()),
        FileDiff::Image { old: None, new: Some(new) } if new.format == ImageFormat::Jpeg
    ));
    assert_eq!(
        diff_of(&repo, &hash, 0, "big.png", DiffOptions::default()),
        FileDiff::Binary {
            old_size: None,
            new_size: Some(10 * 1024 * 1024 + 1)
        }
    );
}

#[test]
fn ui_18_too_large_until_loaded_anyway() {
    let fixture = Fixture::new().unwrap();
    let git = fixture.repo("app").unwrap();
    let many: String = (0..=MAX_DIFF_LINES).map(|n| format!("{n}\n")).collect();
    git.write("small.txt", "1\n").unwrap();
    git.write("many.txt", &many).unwrap();
    let hash = git.commit_all("Many lines").unwrap();
    let repo = open(&git);

    assert_eq!(
        diff_of(&repo, &hash, 0, "many.txt", DiffOptions::default()),
        FileDiff::TooLarge {
            added: MAX_DIFF_LINES + 1,
            removed: 0
        }
    );
    let loaded = DiffOptions {
        load_anyway: true,
        ..DiffOptions::default()
    };
    let diff = text(diff_of(&repo, &hash, 0, "many.txt", loaded));
    assert_eq!(diff.hunks[0].lines.len(), MAX_DIFF_LINES as usize + 1);
    text(diff_of(
        &repo,
        &hash,
        0,
        "small.txt",
        DiffOptions::default(),
    ));
}

#[test]
fn ui_18_submodule_commits() {
    let fixture = Fixture::new().unwrap();
    let git = fixture.repo("app").unwrap();
    git.commit_file("a.txt", "a\n", "First").unwrap();
    let first = "1111111111111111111111111111111111111111";
    let second = "2222222222222222222222222222222222222222";
    git.git([
        "update-index",
        "--add",
        "--cacheinfo",
        "160000",
        first,
        "lib",
    ])
    .unwrap();
    let added = commit_index(&git, "Add submodule");
    git.git(["update-index", "--cacheinfo", "160000", second, "lib"])
        .unwrap();
    let moved = commit_index(&git, "Move submodule");
    let repo = open(&git);

    assert_eq!(
        diff_of(&repo, &added, 0, "lib", DiffOptions::default()),
        FileDiff::Submodule {
            old: None,
            new: Some(first.into())
        }
    );
    assert_eq!(
        diff_of(&repo, &moved, 0, "lib", DiffOptions::default()),
        FileDiff::Submodule {
            old: Some(first.into()),
            new: Some(second.into())
        }
    );
    let files = repo.changed_files(&moved, 0).unwrap();
    assert_eq!(
        summary(&files),
        [(FileStatus::Modified, "lib", None, None, None)]
    );
}

#[test]
fn plat_2_file_names_round_trip_through_ids() {
    let fixture = Fixture::new().unwrap();
    let git = fixture.repo("app").unwrap();
    git.commit_file("a.txt", "a\n", "First").unwrap();
    // Quotes are not allowed in Windows file names, and git on Windows
    // refuses them in the index unless core.protectNTFS is off. Repos made
    // elsewhere can still hold them, so the index takes them here.
    git.git(["config", "core.protectNTFS", "false"]).unwrap();
    let name = "dir with space/it's \"quoted\" – ü 日本.txt";
    stage(&git, "100644", name, b"unicode name\n");
    let mut names = vec![name.to_owned()];
    // Two names that are not UTF-8 and display the same.
    #[cfg(unix)]
    for byte in [0xfe_u8, 0xff] {
        use std::os::unix::ffi::OsStringExt;
        let raw = OsString::from_vec([b"raw-".as_slice(), &[byte], b".txt"].concat());
        stage(&git, "100644", raw, format!("byte {byte:x}\n").as_bytes());
        names.push("raw-\u{fffd}.txt".to_owned());
    }
    let hash = commit_index(&git, "Odd names");
    let repo = open(&git);

    let files = repo.changed_files(&hash, 0).unwrap();
    let mut shown: Vec<String> = files.iter().map(|f| f.path.clone()).collect();
    shown.sort();
    names.sort();
    assert_eq!(shown, names);

    // Each id finds its own file, even where the display strings are equal.
    let source = DiffSource::Commit {
        hash: hash.clone(),
        parent: 0,
    };
    let mut contents = Vec::new();
    // The id the UI sends back is the index into the list the core kept.
    for id in files.iter().map(|f| f.id) {
        let diff = text(
            repo.file_diff(&source, &files[id], DiffOptions::default())
                .unwrap(),
        );
        contents.push(diff.hunks[0].lines[0].text.clone());
    }
    let expected: Vec<String> = files
        .iter()
        .map(|f| match f.git_path() {
            b"raw-\xfe.txt" => "byte fe".to_owned(),
            b"raw-\xff.txt" => "byte ff".to_owned(),
            _ => "unicode name".to_owned(),
        })
        .collect();
    assert_eq!(contents, expected);
}

#[test]
fn sec_3_content_that_is_not_utf8_is_decoded_lossily() {
    let fixture = Fixture::new().unwrap();
    let git = fixture.repo("app").unwrap();
    git.commit_file("latin1.txt", b"caf\xe9\n", "Latin-1")
        .unwrap();
    let hash = git
        .commit_file("latin1.txt", b"caf\xe9!\n", "Change")
        .unwrap();
    let diff = text(diff_of(
        &open(&git),
        &hash,
        0,
        "latin1.txt",
        DiffOptions::default(),
    ));
    let texts: Vec<&str> = diff.hunks[0]
        .lines
        .iter()
        .map(|l| l.text.as_str())
        .collect();
    assert_eq!(texts, ["caf\u{fffd}", "caf\u{fffd}!"]);
}
