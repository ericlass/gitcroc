//! Group commit (GRP-15, section 5 "Commit" row), through `gitcroc-cli
//! group run`, asserted on the preflight, the report and the repos.

use repo_gen::Repo;

use crate::group_world::{World, classes, json, line, refused, report, text};

/// Stages a change to `a.txt` in `repo`.
fn stage(repo: &Repo, contents: &str) {
    repo.write("a.txt", contents).unwrap();
    repo.git(["add", "a.txt"]).unwrap();
}

/// The full message of `repo`'s `HEAD`.
fn message(repo: &Repo) -> String {
    repo.git(["show", "-s", "--format=%B", "HEAD"])
        .unwrap()
        .trim_end()
        .to_owned()
}

/// The parents of `repo`'s `HEAD`.
fn parents(repo: &Repo) -> Vec<String> {
    let parents = repo.git(["rev-parse", "HEAD^@"]).unwrap();
    parents.lines().map(str::to_owned).collect()
}

/// A `pre-commit` hook in `repo` that prints `said` and exits 1.
fn rejecting_hook(repo: &Repo, said: &str) {
    let path = repo.path().join(".git").join("hooks").join("pre-commit");
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(&path, format!("#!/bin/sh\necho '{said}'\nexit 1\n")).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
    }
}

/// Each report line's message.
fn messages(result: &serde_json::Value) -> Vec<String> {
    let lines = result["job"]["report"].as_array().unwrap();
    lines.iter().map(|line| text(&line["message"])).collect()
}

#[test]
fn grp_15_one_commit_per_repo_with_the_same_message() {
    let world = World::new(&["a", "b", "c", "d"]);
    // Each repo has a history of its own, so no two commits share a parent.
    for name in ["a", "b", "c"] {
        let repo = world.repo(name);
        repo.commit_file(&format!("{name}.txt"), "own\n", &format!("Own {name}"))
            .unwrap();
        stage(&repo, &format!("{name} edit\n"));
    }
    let d = world.repo("d");
    d.write("a.txt", "not staged\n").unwrap();
    let before: Vec<String> = ["a", "b", "c", "d"]
        .map(|name| world.repo(name).head().unwrap())
        .into();

    let result = json(&world.run(&["commit", "Shared subject", "--body", "Why.", "--confirm"]));
    assert_eq!(
        report(&result),
        [
            line("a", "succeeded", ""),
            line("b", "succeeded", ""),
            line("c", "succeeded", ""),
            line("d", "skipped", "nothing staged"),
        ]
    );
    let mut heads = Vec::new();
    for (name, old) in ["a", "b", "c"].iter().zip(&before) {
        let repo = world.repo(name);
        assert_eq!(message(&repo), "Shared subject\n\nWhy.", "{name}");
        // One parent, the repo's own previous HEAD: nothing links the commits.
        assert_eq!(parents(&repo), std::slice::from_ref(old), "{name}");
        heads.push(repo.head().unwrap());
    }
    heads.dedup();
    assert_eq!(heads.len(), 3);
    assert_eq!(d.head().unwrap(), before[3]);
    assert!(
        d.git(["status", "--porcelain"])
            .unwrap()
            .contains(" M a.txt")
    );
}

#[test]
fn grp_15_a_hook_rejection_fails_one_repo_only() {
    let world = World::new(&["a", "b", "c"]);
    for name in ["a", "b", "c"] {
        stage(&world.repo(name), &format!("{name} edit\n"));
    }
    let b = world.repo("b");
    rejecting_hook(&b, "pre-commit says no");
    let before = b.head().unwrap();

    let result = json(&world.run(&["commit", "Shared", "--confirm"]));
    assert_eq!(
        report(&result),
        [
            line("a", "succeeded", ""),
            line("b", "failed", ""),
            line("c", "succeeded", ""),
        ]
    );
    let output = text(&result["job"]["report"][1]["output"]);
    assert!(output.contains("pre-commit says no"), "{output}");
    assert_eq!(b.head().unwrap(), before);
    assert_eq!(message(&world.repo("a")), "Shared");
    assert_eq!(message(&world.repo("c")), "Shared");
}

#[test]
fn grp_15_stage_all_tracked_changes_first() {
    let world = World::new(&["a", "b"]);
    let a = world.repo("a");
    a.write("a.txt", "edited\n").unwrap();
    a.write("new.txt", "untracked\n").unwrap();
    world.repo("b").write("new.txt", "untracked\n").unwrap();

    // Without the option nothing is staged anywhere.
    let result = refused(&world.run(&["commit", "Tracked"]));
    assert_eq!(
        classes(&result),
        [
            line("a", "will_skip", "nothing staged"),
            line("b", "will_skip", "nothing staged"),
        ]
    );

    let result = json(&world.run(&["commit", "Tracked", "--stage-all", "--confirm"]));
    assert_eq!(
        classes(&result),
        [
            line("a", "will_run", ""),
            line("b", "will_skip", "nothing staged"),
        ]
    );
    assert_eq!(result["preflight"]["repos"][0]["staged_files"], 1);
    assert_eq!(
        report(&result),
        [
            line("a", "succeeded", ""),
            line("b", "skipped", "nothing staged"),
        ]
    );
    let committed = a.git(["show", "--name-only", "--format=", "HEAD"]).unwrap();
    assert_eq!(committed.trim(), "a.txt");
    // Untracked files stay untracked.
    assert_eq!(a.git(["status", "--porcelain"]).unwrap(), "?? new.txt\n");
}

#[test]
fn grp_15_stage_all_on_a_locked_index_fails_that_repo_only() {
    let world = World::new(&["locked", "free"]);
    for name in ["locked", "free"] {
        world.repo(name).write("a.txt", "edited\n").unwrap();
    }
    let locked = world.repo("locked");
    std::fs::write(locked.path().join(".git").join("index.lock"), "").unwrap();

    let result = json(&world.run(&["commit", "Tracked", "--stage-all", "--confirm"]));
    assert_eq!(
        report(&result),
        [line("locked", "failed", ""), line("free", "succeeded", "")]
    );
    let said = messages(&result);
    assert!(said[0].contains("index.lock"), "{}", said[0]);
    assert_eq!(message(&world.repo("free")), "Tracked");
    assert!(
        locked
            .git(["status", "--porcelain"])
            .unwrap()
            .contains(" M a.txt")
    );
}

#[test]
fn grp_15_push_after_commit_with_one_rejected_push() {
    let world = World::new(&["pushed", "rejected", "local"]);
    let pushed = world.repo("pushed");
    pushed.git(["switch", "--quiet", "-c", "feature"]).unwrap();
    pushed
        .git(["push", "--quiet", "-u", "origin", "feature"])
        .unwrap();
    stage(&pushed, "pushed\n");
    stage(&world.repo("rejected"), "rejected\n");
    // The remote's main moves on, so a push of the old one is rejected.
    world.other_pushes("other.txt", "other\n");
    let local = world.repo("local");
    local.git(["switch", "--quiet", "-c", "solo"]).unwrap();
    stage(&local, "local\n");

    let expected = [
        line("pushed", "will_run", ""),
        line("rejected", "will_run", ""),
        line(
            "local",
            "needs_attention",
            "no upstream; the commit will not be pushed",
        ),
    ];
    let result = refused(&world.run(&["commit", "Ship", "--push"]));
    assert_eq!(classes(&result), expected);

    let result = json(&world.run(&["commit", "Ship", "--push", "--confirm"]));
    assert_eq!(classes(&result), expected);
    assert_eq!(
        report(&result),
        [
            line("pushed", "succeeded", ""),
            line("rejected", "failed", ""),
            line("local", "failed", ""),
        ]
    );
    let said = messages(&result);
    assert_eq!(said[0], "");
    assert!(
        said[1].starts_with("the commit was made; the push failed: "),
        "{}",
        said[1]
    );
    assert_eq!(said[2], "the commit was made; the push failed: no upstream");
    assert_eq!(world.remote("feature"), pushed.head().unwrap());
    for name in ["pushed", "rejected", "local"] {
        assert_eq!(message(&world.repo(name)), "Ship", "{name}");
    }
    assert_ne!(world.remote("main"), world.repo("rejected").head().unwrap());
}
