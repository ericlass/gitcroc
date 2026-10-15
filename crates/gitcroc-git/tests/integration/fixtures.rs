//! Fixture repos built in code (QA-2), read back by the product's git runner
//! and by the read API.

use std::ffi::OsString;
use std::path::Path;

use gitcroc_git::{GitRepo, GitRunner, SearchEnv, detect};
use repo_gen::Fixture;

async fn read(repo: &Path, args: &[&str]) -> String {
    let git = detect(None, &SearchEnv::from_process())
        .await
        .expect("git 2.40 or newer on PATH");
    let args: Vec<OsString> = args.iter().map(OsString::from).collect();
    let out = GitRunner::new(git.path)
        .run(&args, Some(repo))
        .await
        .unwrap();
    String::from_utf8(out.stdout).unwrap()
}

#[tokio::test]
async fn qa_2_fixture_helpers_build_repos_in_code() {
    let fixture = Fixture::new().unwrap();
    let repo = fixture.repo("app").unwrap();
    let first = repo
        .commit_file("README.md", "hello\n", "Initial commit")
        .unwrap();
    repo.annotated_tag("v1.0.0", "First release").unwrap();
    repo.branch("feature").unwrap();
    repo.switch("feature").unwrap();
    let feature = repo
        .commit_file("src/lib.rs", "pub fn f() {}\n", "Add f")
        .unwrap();
    repo.switch("main").unwrap();
    repo.commit_file("CHANGELOG.md", "- f\n", "Changelog")
        .unwrap();
    let merge = repo.merge("feature", "Merge branch 'feature'").unwrap();
    repo.write("README.md", "hello, edited\n").unwrap();
    repo.stash("work in progress").unwrap();

    let path = repo.path();
    assert_eq!(
        read(path, &["for-each-ref", "--format=%(refname)", "refs/heads"]).await,
        "refs/heads/feature\nrefs/heads/main\n"
    );
    assert_eq!(
        read(path, &["rev-parse", "v1.0.0^{commit}"]).await.trim(),
        first
    );
    assert_eq!(
        read(path, &["cat-file", "-t", "v1.0.0"]).await.trim(),
        "tag"
    );
    let parents = read(path, &["rev-list", "--parents", "-n", "1", "HEAD"]).await;
    let parents: Vec<&str> = parents.split_whitespace().collect();
    assert_eq!(parents.len(), 3, "a merge has two parents: {parents:?}");
    assert_eq!((parents[0], parents[2]), (merge.as_str(), feature.as_str()));
    assert!(
        read(path, &["stash", "list"])
            .await
            .contains("work in progress")
    );
    assert!(read(path, &["status", "--porcelain"]).await.is_empty());

    // The read API opens it too.
    let opened = GitRepo::open(path).unwrap();
    let work_dir = opened.work_dir().unwrap();
    assert_eq!(
        work_dir.canonicalize().unwrap(),
        path.canonicalize().unwrap()
    );
}

#[tokio::test]
async fn qa_2_fixture_remotes_and_conflicts() {
    let fixture = Fixture::new().unwrap();
    let upstream = fixture.repo("upstream").unwrap();
    upstream.commit_file("a.txt", "one\n", "One").unwrap();
    let clone = upstream.clone_to(fixture.path().join("clone")).unwrap();
    assert_eq!(
        read(clone.path(), &["rev-parse", "origin/main"])
            .await
            .trim(),
        upstream.head().unwrap()
    );

    let other = fixture.repo("other").unwrap();
    other.commit_file("b.txt", "two\n", "Two").unwrap();
    clone.add_remote("other", &other).unwrap();
    assert_eq!(
        read(clone.path(), &["rev-parse", "other/main"])
            .await
            .trim(),
        other.head().unwrap()
    );

    clone.branch("theirs").unwrap();
    clone.commit_file("a.txt", "ours\n", "Ours").unwrap();
    clone.switch("theirs").unwrap();
    clone.commit_file("a.txt", "theirs\n", "Theirs").unwrap();
    clone.switch("main").unwrap();
    clone.merge_with_conflicts("theirs").unwrap();
    assert!(clone.path().join(".git/MERGE_HEAD").is_file());
    assert_eq!(
        read(clone.path(), &["diff", "--name-only", "--diff-filter=U"]).await,
        "a.txt\n"
    );
}

#[test]
fn fixture_commit_ids_are_reproducible() {
    let build = || {
        let fixture = Fixture::new().unwrap();
        let repo = fixture.repo("app").unwrap();
        repo.commit_file("a.txt", "a\n", "A").unwrap();
        repo.commit_file("b.txt", "b\n", "B").unwrap()
    };
    assert_eq!(build(), build());
}
