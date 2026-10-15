//! Group merge of X, rebase onto X and create tag X (GRP-13, section 5
//! "Merge X into current", "Rebase current onto X" and "Create tag X"
//! rows), through `gitcroc-cli group run`, asserted on the preflight and
//! the report.

use std::ffi::OsString;

use repo_gen::Repo;

use crate::group_world::{World, classes, json, line, refused, report};

/// `repo` gets branch `topic` from `main` with `file` set to `contents`,
/// and stays on `main`.
fn topic(repo: &Repo, file: &str, contents: &str) {
    repo.git(["switch", "--quiet", "-c", "topic"]).unwrap();
    repo.commit_file(file, contents, "On topic").unwrap();
    repo.git(["switch", "--quiet", "main"]).unwrap();
}

/// What a preflight must leave as it was: every ref, `HEAD`, and the
/// status of the index and the working tree.
fn untouched(repo: &Repo) -> (String, String, String) {
    (
        repo.git(["for-each-ref"]).unwrap(),
        repo.head().unwrap(),
        repo.git(["status", "--porcelain=v2", "--branch"]).unwrap(),
    )
}

/// Whether `repo` is stopped in a merge or a rebase (OPS-1).
fn merging(repo: &Repo) -> bool {
    repo.path().join(".git").join("MERGE_HEAD").exists()
}

fn rebasing(repo: &Repo) -> bool {
    repo.path().join(".git").join("rebase-merge").exists()
}

/// The world's three repos of the conflict prediction: `clean` and
/// `conflict` diverged from `topic`, `ff` behind it.
fn three(world: &World) {
    let clean = world.repo("clean");
    topic(&clean, "c.txt", "topic\n");
    clean.commit_file("b.txt", "main\n", "On main").unwrap();
    let conflict = world.repo("conflict");
    topic(&conflict, "a.txt", "topic\n");
    conflict.commit_file("a.txt", "main\n", "On main").unwrap();
    topic(&world.repo("ff"), "c.txt", "topic\n");
}

#[test]
fn grp_13_merge_tree_predicts_conflicts_as_needs_attention() {
    let world = World::new(&["clean", "conflict", "ff"]);
    three(&world);
    let before: Vec<_> = ["clean", "conflict", "ff"]
        .map(|name| untouched(&world.repo(name)))
        .into();

    let result = refused(&world.run(&["merge", "topic"]));
    let predicted = [
        line("clean", "will_run", ""),
        line(
            "conflict",
            "needs_attention",
            "conflicts predicted in 1 file: a.txt",
        ),
        line("ff", "will_run", ""),
    ];
    assert_eq!(classes(&result), predicted);
    let after: Vec<_> = ["clean", "conflict", "ff"]
        .map(|name| untouched(&world.repo(name)))
        .into();
    assert_eq!(before, after, "the preflight wrote a ref or changed a file");

    let result = json(&world.run(&["merge", "topic", "--confirm"]));
    assert_eq!(classes(&result), predicted);
    assert_eq!(
        report(&result),
        [
            line("clean", "succeeded", ""),
            line("conflict", "conflict", ""),
            line("ff", "succeeded", ""),
        ]
    );
    let conflict = world.repo("conflict");
    assert!(merging(&conflict));
    let message = &result["job"]["report"][1]["message"];
    assert_eq!(message, "in conflict state; resolve it in this repo");
    // A fast-forward made no merge commit.
    let ff = world.repo("ff");
    assert_eq!(ff.head().unwrap(), ff.rev_parse("topic").unwrap());
}

#[test]
fn grp_13_the_preflight_writes_no_ref_and_changes_no_file() {
    let world = World::new(&["clean", "conflict", "ff"]);
    three(&world);
    // Uncommitted work and an untracked file are left as they are too.
    let clean = world.repo("clean");
    clean.write("a.txt", "edited\n").unwrap();
    clean.write("new.txt", "new\n").unwrap();
    let names = ["clean", "conflict", "ff"];
    let before: Vec<_> = names.map(|name| untouched(&world.repo(name))).into();
    for op in [["merge", "topic"], ["rebase", "topic"]] {
        refused(&world.run(&op));
        let after: Vec<_> = names.map(|name| untouched(&world.repo(name))).into();
        assert_eq!(
            before, after,
            "{op:?}: the preflight wrote a ref or changed a file"
        );
    }
    assert_eq!(
        std::fs::read_to_string(clean.path().join("a.txt")).unwrap(),
        "edited\n"
    );
}

#[test]
fn grp_13_prediction_is_advisory() {
    let world = World::new(&["driven"]);
    let driven = world.repo("driven");
    topic(&driven, "a.txt", "topic\n");
    driven.commit_file("a.txt", "main\n", "On main").unwrap();
    // A merge driver that fails the first merge of a.txt, which the
    // preflight's prediction makes, and keeps our side in every later one,
    // which the run makes: git, not the prediction, decides the outcome.
    driven
        .git([
            "config",
            "merge.once.driver",
            "test -f .git/predicted || { touch .git/predicted; exit 1; }",
        ])
        .unwrap();
    let attributes = driven.path().join(".git").join("info").join("attributes");
    std::fs::write(attributes, "a.txt merge=once\n").unwrap();

    let result = json(&world.run(&["merge", "topic", "--confirm"]));
    assert_eq!(
        classes(&result),
        [line(
            "driven",
            "needs_attention",
            "conflicts predicted in 1 file: a.txt"
        )]
    );
    assert_eq!(report(&result), [line("driven", "succeeded", "")]);
    assert!(!merging(&driven));
    let parents = driven
        .git(["rev-list", "--parents", "-n", "1", "HEAD"])
        .unwrap();
    assert_eq!(parents.split_whitespace().count(), 3, "a merge commit");
    assert_eq!(
        std::fs::read_to_string(driven.path().join("a.txt")).unwrap(),
        "main\n"
    );
}

#[test]
fn grp_7_group_merge_outcomes_and_fast_forward_options() {
    let world = World::new(&["diverged", "uptodate", "conflict", "detached", "missing"]);
    let diverged = world.repo("diverged");
    topic(&diverged, "c.txt", "topic\n");
    diverged.commit_file("b.txt", "main\n", "On main").unwrap();
    world.repo("uptodate").git(["branch", "topic"]).unwrap();
    let conflict = world.repo("conflict");
    topic(&conflict, "a.txt", "topic\n");
    conflict.commit_file("a.txt", "main\n", "On main").unwrap();
    let detached = world.repo("detached");
    detached.git(["branch", "topic"]).unwrap();
    detached.git(["switch", "--quiet", "--detach"]).unwrap();

    let result = json(&world.run(&["merge", "topic", "--ff", "ff-only", "--confirm"]));
    assert_eq!(
        classes(&result),
        [
            line("diverged", "will_run", ""),
            line("uptodate", "will_run", ""),
            line(
                "conflict",
                "needs_attention",
                "conflicts predicted in 1 file: a.txt"
            ),
            line("detached", "will_skip", "detached HEAD"),
            line("missing", "will_skip", "topic not found"),
        ]
    );
    // ff-only cannot merge diverged history, the conflicting repo either.
    assert_eq!(
        report(&result),
        [
            line("diverged", "failed", ""),
            line("uptodate", "nothing_to_do", ""),
            line("conflict", "failed", ""),
            line("detached", "skipped", "detached HEAD"),
            line("missing", "skipped", "topic not found"),
        ]
    );
    assert!(!merging(&conflict));

    let result = json(&world.run(&["merge", "topic", "--ff", "no-ff", "--confirm"]));
    assert_eq!(
        report(&result)[..3],
        [
            line("diverged", "succeeded", ""),
            line("uptodate", "nothing_to_do", ""),
            line("conflict", "conflict", ""),
        ]
    );
    assert!(merging(&conflict));
    let parents = diverged
        .git(["rev-list", "--parents", "-n", "1", "HEAD"])
        .unwrap();
    assert_eq!(parents.split_whitespace().count(), 3, "a merge commit");

    // The repo in conflict state is blocked from now on (GRP-8).
    let result = refused(&world.run(&["merge", "topic"]));
    assert_eq!(classes(&result)[2].1, "blocked");
}

#[test]
fn grp_6_a_conflicting_rebase_leaves_the_others_to_succeed() {
    let world = World::new(&["clean", "conflict", "ff"]);
    three(&world);
    let result = json(&world.run(&["rebase", "topic", "--confirm"]));
    assert_eq!(
        classes(&result),
        [
            line("clean", "will_run", ""),
            line(
                "conflict",
                "needs_attention",
                "may conflict in 1 file: a.txt"
            ),
            line("ff", "will_run", ""),
        ]
    );
    assert_eq!(
        report(&result),
        [
            line("clean", "succeeded", ""),
            line("conflict", "conflict", ""),
            line("ff", "succeeded", ""),
        ]
    );
    let clean = world.repo("clean");
    let parent = clean.rev_parse("HEAD~1").unwrap();
    assert_eq!(
        parent,
        clean.rev_parse("topic").unwrap(),
        "main was replayed onto topic"
    );
    assert!(rebasing(&world.repo("conflict")));
    let ff = world.repo("ff");
    assert_eq!(ff.head().unwrap(), ff.rev_parse("topic").unwrap());
}

#[test]
fn grp_12_group_merge_never_stashes() {
    let world = World::new(&["dirty"]);
    let dirty = world.repo("dirty");
    topic(&dirty, "a.txt", "topic\n");
    dirty.write("a.txt", "mine\n").unwrap();
    let head = dirty.head().unwrap();

    let result = json(&world.run(&["merge", "topic"]));
    assert_eq!(classes(&result), [line("dirty", "will_run", "")]);
    assert_eq!(report(&result), [line("dirty", "failed", "")]);
    let message = result["job"]["report"][0]["message"].as_str().unwrap();
    assert!(message.contains("local changes"), "{message}");
    assert_eq!(dirty.head().unwrap(), head);
    assert_eq!(
        std::fs::read_to_string(dirty.path().join("a.txt")).unwrap(),
        "mine\n"
    );
    assert_eq!(dirty.git(["stash", "list"]).unwrap(), "");
}

#[test]
fn grp_14_group_create_tag() {
    let world = World::new(&["tagged", "a", "b"]);
    world.repo("tagged").tag("v1").unwrap();
    world.repo("b").commit_file("b.txt", "b\n", "On b").unwrap();

    let result = json(&world.run(&["create-tag", "v1", "--message", "One", "--confirm"]));
    assert_eq!(
        classes(&result),
        [
            line("tagged", "will_skip", "v1 already exists"),
            line("a", "will_run", ""),
            line("b", "will_run", ""),
        ]
    );
    assert_eq!(
        report(&result),
        [
            line("tagged", "skipped", "v1 already exists"),
            line("a", "succeeded", ""),
            line("b", "succeeded", ""),
        ]
    );
    for name in ["a", "b"] {
        let repo = world.repo(name);
        assert_eq!(repo.git(["cat-file", "-t", "v1"]).unwrap().trim(), "tag");
        assert_eq!(repo.rev_parse("v1^{commit}").unwrap(), repo.head().unwrap());
    }
    // Each at its own HEAD (GRP-14).
    assert_ne!(
        world.repo("a").rev_parse("v1^{commit}").unwrap(),
        world.repo("b").rev_parse("v1^{commit}").unwrap()
    );

    let result = json(&world.run(&["create-tag", "v2", "--lightweight"]));
    assert_eq!(
        report(&result),
        [
            line("tagged", "succeeded", ""),
            line("a", "succeeded", ""),
            line("b", "succeeded", ""),
        ]
    );
    assert_eq!(
        world
            .repo("a")
            .git(["cat-file", "-t", "v2"])
            .unwrap()
            .trim(),
        "commit"
    );

    // Pushed in the same sub-job, to the file remote. One repo only, as
    // repos with the tag at different commits would race for the name.
    world.group("Just a", &["a"]);
    let result = json(&world.run_on(
        "Just a",
        &["create-tag", "v3", "--message", "Three", "--push"],
    ));
    assert_eq!(report(&result), [line("a", "succeeded", "")]);
    let mut args: Vec<OsString> = vec!["--git-dir".into(), world.bare.clone().into()];
    args.extend([
        "show-ref".into(),
        "--verify".into(),
        "--quiet".into(),
        "refs/tags/v3".into(),
    ]);
    assert!(world.other.git(args).is_ok(), "v3 was pushed to the remote");

    // An annotated tag without a message is refused once, before anything runs.
    let out = world.run(&["create-tag", "v4"]);
    assert_eq!(out.status.code(), Some(2));
    assert!(out.stdout.is_empty());
    let out = world.run(&["create-tag", "bad..tag", "--lightweight"]);
    assert_eq!(out.status.code(), Some(2));
    assert!(out.stdout.is_empty());
}
