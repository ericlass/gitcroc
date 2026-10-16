//! A conflicted file as the conflict editor reads it (OPS-4, OPS-8): hunks
//! from the index stages, both sides labelled, and a file edited outside
//! the app told apart. Conflicts are made with `git` as a terminal user
//! would.

use std::path::PathBuf;

use gitcroc_git::{
    Conflict, ConflictChunk, ConflictContent, Engine, GitRepo, GitRunner, Operation, SideMessages,
    SidesRead, SidesReader, chunks_from_texts,
};
use repo_gen::{Fixture, Repo};

const BASE: &str = "one\ntwo\nthree\nfour\nfive\nsix\nseven\n";
/// Changes "two" and "six" and adds a line at the end.
const MAIN: &str = "one\nmain two\nshared\nthree\nfour\nfive\nmain six\nseven\nend\n";
/// Changes "two" and "six" differently; "shared" makes the first hunk
/// smaller in merge and zdiff3 style than in diff3.
const FEATURE: &str = "one\nfeature two\nshared\nthree\nfour\nfive\nfeature six\nseven\n";

/// `main` and `feature` changed `a.txt` differently since "Base"; `main`
/// checked out.
fn diverged(fixture: &Fixture, name: &str) -> Repo {
    let repo = fixture.repo(name).unwrap();
    repo.commit_file("a.txt", BASE, "Base").unwrap();
    repo.branch("feature").unwrap();
    repo.commit_file("a.txt", MAIN, "Main").unwrap();
    repo.switch("feature").unwrap();
    repo.commit_file("a.txt", FEATURE, "Feature").unwrap();
    repo.switch("main").unwrap();
    repo
}

/// Runs git expecting it to stop with a conflict.
fn stops(repo: &Repo, args: &[&str]) {
    let output = repo.git_output(args).unwrap();
    assert!(!output.status.success(), "git {args:?} did not stop");
    assert!(
        !repo.git(["ls-files", "--unmerged"]).unwrap().is_empty(),
        "git {args:?} left no conflict: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

fn conflict(repo: &Repo, path: &str) -> Conflict {
    GitRepo::open(repo.path())
        .unwrap()
        .conflict(path.as_bytes())
        .unwrap()
        .expect("in conflict")
}

fn chunks(conflict: &Conflict) -> &[ConflictChunk] {
    match &conflict.content {
        ConflictContent::Text { chunks } => chunks,
        other => panic!("not a text conflict: {other:?}"),
    }
}

/// `chunks` written with diff3 markers and the labels `git merge-file -L`
/// gets below.
fn with_markers(chunks: &[ConflictChunk]) -> String {
    chunks
        .iter()
        .map(|chunk| match chunk {
            ConflictChunk::Resolved { text } => text.clone(),
            ConflictChunk::Hunk { ours, base, theirs } => {
                format!("<<<<<<< ours\n{ours}||||||| base\n{base}=======\n{theirs}>>>>>>> theirs\n")
            }
        })
        .collect()
}

/// `git merge-file --diff3 -p` of the stages of `path`, written out of
/// the repo first.
fn merge_file(fixture: &Fixture, repo: &Repo, path: &str) -> String {
    let dir = fixture.path().join("stages");
    std::fs::create_dir_all(&dir).unwrap();
    let mut files = Vec::new();
    for (stage, name) in [(2, "ours"), (1, "base"), (3, "theirs")] {
        let blob = repo
            .git_output(["show", &format!(":{stage}:{path}")])
            .unwrap()
            .stdout;
        let file = dir.join(name);
        std::fs::write(&file, blob).unwrap();
        files.push(file);
    }
    let mut args: Vec<std::ffi::OsString> = [
        "merge-file",
        "--diff3",
        "-p",
        "-L",
        "ours",
        "-L",
        "base",
        "-L",
        "theirs",
    ]
    .map(Into::into)
    .to_vec();
    args.extend(files.into_iter().map(Into::into));
    // merge-file exits with the number of conflicts.
    String::from_utf8(repo.git_output(args).unwrap().stdout).unwrap()
}

fn hunk(ours: &str, base: &str, theirs: &str) -> ConflictChunk {
    ConflictChunk::Hunk {
        ours: ours.into(),
        base: base.into(),
        theirs: theirs.into(),
    }
}

fn resolved(text: &str) -> ConflictChunk {
    ConflictChunk::Resolved { text: text.into() }
}

#[test]
fn ops_8_hunks_come_from_the_index_stages() {
    let fixture = Fixture::new().unwrap();
    let repo = diverged(&fixture, "app");
    repo.switch("feature").unwrap();
    repo.commit_file("new.txt", "feature\n", "Feature adds")
        .unwrap();
    repo.switch("main").unwrap();
    repo.commit_file("new.txt", "main\n", "Main adds").unwrap();
    stops(&repo, &["merge", "--no-edit", "feature"]);

    let expected = vec![
        resolved("one\n"),
        hunk("main two\nshared\n", "two\n", "feature two\nshared\n"),
        resolved("three\nfour\nfive\n"),
        hunk("main six\n", "six\n", "feature six\n"),
        resolved("seven\nend\n"),
    ];
    let read = conflict(&repo, "a.txt");
    assert_eq!(chunks(&read), expected);
    assert_eq!(
        with_markers(chunks(&read)),
        merge_file(&fixture, &repo, "a.txt")
    );
    // Both added: no base, merged against nothing.
    assert_eq!(
        chunks(&conflict(&repo, "new.txt")),
        [hunk("main\n", "", "feature\n")]
    );

    // The file on disk is not where the hunks come from.
    repo.write("a.txt", "overwritten\n").unwrap();
    let again = conflict(&repo, "a.txt");
    assert_eq!(chunks(&again), expected);
    assert!(again.edited_outside);

    // Resolved in a terminal: no conflict any more.
    repo.git(["add", "a.txt"]).unwrap();
    assert_eq!(
        GitRepo::open(repo.path())
            .unwrap()
            .conflict(b"a.txt")
            .unwrap(),
        None
    );
}

#[test]
fn ops_8_detects_a_file_edited_outside() {
    let fixture = Fixture::new().unwrap();
    let operations: [(&str, &str, &[&str]); 3] = [
        ("merge", "main", &["merge", "--no-edit", "feature"]),
        ("rebase", "feature", &["rebase", "main"]),
        ("cherry-pick", "main", &["cherry-pick", "feature"]),
    ];
    for style in ["merge", "diff3", "zdiff3"] {
        for (name, branch, args) in operations {
            let case = format!("{name} with {style}");
            let repo = diverged(&fixture, &format!("{name}-{style}"));
            repo.git(["config", "merge.conflictStyle", style]).unwrap();
            repo.switch(branch).unwrap();
            stops(&repo, args);
            let read = conflict(&repo, "a.txt");
            assert!(!read.edited_outside, "{case}");
            assert_eq!(read.on_disk, None, "{case}");

            let on_disk = std::fs::read_to_string(repo.path().join("a.txt")).unwrap();
            let edited = on_disk.replacen("one\n", "one!\n", 1);
            repo.write("a.txt", &edited).unwrap();
            let read = conflict(&repo, "a.txt");
            assert!(read.edited_outside, "{case}");
            // What keeping the file as it is opens.
            assert_eq!(read.on_disk, Some(edited), "{case}");
        }
    }
}

#[test]
fn ops_4_labels_name_the_real_sides() {
    let fixture = Fixture::new().unwrap();
    let labels = |repo: &Repo| {
        let read = conflict(repo, "a.txt");
        (read.labels.ours, read.labels.theirs)
    };
    let pair = |ours: &str, theirs: &str| (ours.to_owned(), theirs.to_owned());
    let commit_text = |repo: &Repo, revision: &str| {
        repo.git(["log", "-1", "--format=%h %s", revision])
            .unwrap()
            .trim()
            .to_owned()
    };

    let merge = diverged(&fixture, "merge");
    stops(&merge, &["merge", "--no-edit", "feature"]);
    assert_eq!(labels(&merge), pair("main", "feature"));

    // A rebase swaps the sides: ours is what it rebases onto.
    let rebase = diverged(&fixture, "rebase");
    rebase.switch("feature").unwrap();
    let replayed = commit_text(&rebase, "feature");
    stops(&rebase, &["rebase", "main"]);
    assert_eq!(
        labels(&rebase),
        pair("main", &format!("{replayed} (feature)"))
    );

    let pick = diverged(&fixture, "pick");
    stops(&pick, &["cherry-pick", "feature"]);
    assert_eq!(labels(&pick), pair("main", "feature"));

    let revert = fixture.repo("revert").unwrap();
    revert.commit_file("a.txt", "a\n", "Base").unwrap();
    revert.commit_file("a.txt", "b\n", "Change").unwrap();
    revert.commit_file("a.txt", "c\n", "Again").unwrap();
    let reverted = commit_text(&revert, "HEAD~1");
    stops(&revert, &["revert", "--no-edit", "HEAD~1"]);
    assert_eq!(
        labels(&revert),
        pair("main", &format!("revert of {reverted}"))
    );

    let stash = fixture.repo("stash").unwrap();
    stash.commit_file("a.txt", "a\n", "Base").unwrap();
    stash.write("a.txt", "stashed\n").unwrap();
    stash.stash("Work").unwrap();
    stash.commit_file("a.txt", "committed\n", "Commit").unwrap();
    stops(&stash, &["stash", "apply"]);
    assert_eq!(labels(&stash), pair("main", "stash"));
}

/// The side messages of `path` read by `engine`, at most `limit` per side.
fn side_messages(repo: &Repo, path: &str, engine: Engine, limit: usize) -> SideMessages {
    SidesReader::new(
        GitRepo::open(repo.path()).unwrap(),
        GitRunner::new(PathBuf::from("git")),
    )
    .with_engine(engine)
    .side_messages(path.as_bytes(), limit)
    .unwrap()
}

#[test]
fn arch_2_side_messages_are_the_same_from_gix_and_git() {
    let fixture = Fixture::new().unwrap();
    let repo = fixture.repo("app").unwrap();
    // A path git would read as a glob without --literal-pathspecs.
    let path = "dir/[a] b.txt";
    repo.commit_file(path, "base\n", "Base").unwrap();
    repo.commit_file("dir/a b.txt", "base\n", "Base of the look-alike")
        .unwrap();
    repo.branch("feature").unwrap();
    repo.branch("side").unwrap();
    for index in 1..=4 {
        repo.commit_file(path, format!("main {index}\n"), &format!("Main {index}"))
            .unwrap();
    }
    repo.commit_file("dir/a b.txt", "main\n", "Only the look-alike")
        .unwrap();
    // A merge that takes the file from its other side: left out by both.
    repo.switch("side").unwrap();
    repo.commit_file("side.txt", "side\n", "Side").unwrap();
    repo.switch("main").unwrap();
    repo.merge("side", "Merge side").unwrap();
    repo.switch("feature").unwrap();
    repo.commit_file(path, "feature\n", "Feature\n\nWith a body.\n")
        .unwrap();
    repo.switch("main").unwrap();
    repo.merge_with_conflicts("feature").unwrap();

    for limit in [0, 3, 10] {
        let by_gix = side_messages(&repo, path, Engine::Gix, limit);
        assert_eq!(
            by_gix,
            side_messages(&repo, path, Engine::Git, limit),
            "limit {limit}"
        );
        assert_eq!(
            by_gix,
            side_messages(&repo, path, Engine::Auto, limit),
            "limit {limit}"
        );
    }
    assert_eq!(
        side_messages(&repo, path, Engine::Gix, 10),
        SideMessages {
            operation: Some(Operation::Merge),
            ours: ["Main 4", "Main 3", "Main 2", "Main 1"]
                .map(str::to_owned)
                .to_vec(),
            theirs: vec!["Feature\n\nWith a body.".to_owned()],
        }
    );
}

#[test]
fn ai_15_chunks_from_texts_is_the_editor_merge() {
    let fixture = Fixture::new().unwrap();
    let repo = diverged(&fixture, "app");
    stops(&repo, &["merge", "--no-edit", "feature"]);
    let read = conflict(&repo, "a.txt");
    assert_eq!(
        chunks_from_texts(BASE, MAIN, FEATURE).as_deref(),
        Some(chunks(&read))
    );
}
