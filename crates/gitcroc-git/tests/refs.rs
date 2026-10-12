//! The refs the tree shows (UI-1): local branches with and without an
//! upstream, remote branches by remote, both kinds of tag, and stashes.

// Integration test crate: helpers outside #[test] fns are test code too (QA-4).
#![allow(clippy::unwrap_used, clippy::expect_used)]

use gitcroc_git::{
    AheadBehind, BranchUpstream, GitRepo, LocalBranch, RefsRead, RemoteBranches, Tag,
};
use repo_gen::Fixture;

#[test]
fn ui_1_refs_lists_branches_remotes_tags_and_stashes() {
    let fixture = Fixture::new().unwrap();
    let upstream = fixture.repo("upstream").unwrap();
    upstream.commit_file("a.txt", "one\n", "One").unwrap();
    upstream.branch("release/1.x").unwrap();
    let clone = upstream.clone_to(fixture.path().join("clone")).unwrap();
    let other = fixture.repo("other").unwrap();
    other.commit_file("b.txt", "two\n", "Two").unwrap();
    other.branch("topic").unwrap();
    clone.add_remote("other", &other).unwrap();

    // main is 1 ahead and 2 behind origin/main.
    upstream.commit_file("a.txt", "two\n", "Two").unwrap();
    upstream.commit_file("a.txt", "three\n", "Three").unwrap();
    clone.git(["fetch", "--quiet", "origin"]).unwrap();
    clone.commit_file("c.txt", "mine\n", "Mine").unwrap();
    clone.branch("feature/a/b").unwrap();
    clone.tag("v1.0").unwrap();
    clone.annotated_tag("v2.0", "Second").unwrap();
    clone.write("c.txt", "first stash\n").unwrap();
    clone.stash("first").unwrap();
    clone.write("c.txt", "second stash\n").unwrap();
    clone.stash("second").unwrap();

    let repo = GitRepo::open(clone.path()).unwrap();
    let refs = repo.refs(true).unwrap();
    assert_eq!(
        refs.local,
        vec![
            LocalBranch {
                name: "feature/a/b".into(),
                current: false,
                upstream: None,
            },
            LocalBranch {
                name: "main".into(),
                current: true,
                upstream: Some(BranchUpstream {
                    name: "origin/main".into(),
                    gone: false,
                    counts: Some(AheadBehind {
                        ahead: 1,
                        behind: 2
                    }),
                }),
            },
        ]
    );
    assert_eq!(
        refs.remotes,
        vec![
            RemoteBranches {
                remote: "origin".into(),
                branches: vec!["main".into(), "release/1.x".into()],
            },
            RemoteBranches {
                remote: "other".into(),
                branches: vec!["main".into(), "topic".into()],
            },
        ]
    );
    assert_eq!(
        refs.tags,
        vec![
            Tag {
                name: "v1.0".into(),
                annotated: false,
            },
            Tag {
                name: "v2.0".into(),
                annotated: true,
            },
        ]
    );
    let messages: Vec<_> = refs
        .stashes
        .iter()
        .map(|stash| (stash.index, stash.message.as_str()))
        .collect();
    assert_eq!(messages, [(0, "On main: second"), (1, "On main: first")]);
    let times = clone
        .git(["log", "--walk-reflogs", "--format=%ct", "refs/stash"])
        .unwrap();
    let times: Vec<i64> = times.lines().map(|line| line.parse().unwrap()).collect();
    let seconds: Vec<i64> = refs
        .stashes
        .iter()
        .map(|stash| stash.time.seconds)
        .collect();
    assert_eq!(seconds, times);

    // Without counts no history is walked.
    let names_only = repo.refs(false).unwrap();
    let main = &names_only.local[1];
    assert_eq!(main.upstream.as_ref().map(|u| u.counts), Some(None));
    assert_eq!(names_only.remotes, refs.remotes);
}

#[test]
fn ui_1_refs_of_a_new_repo_are_empty() {
    let fixture = Fixture::new().unwrap();
    let repo = fixture.repo("empty").unwrap();
    let refs = GitRepo::open(repo.path()).unwrap().refs(true).unwrap();
    assert!(refs.local.is_empty() && refs.remotes.is_empty());
    assert!(refs.tags.is_empty() && refs.stashes.is_empty());
}
