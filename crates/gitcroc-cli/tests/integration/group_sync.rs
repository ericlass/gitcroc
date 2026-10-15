//! Group fetch, pull and push (GRP-12, section 5 "Fetch", "Pull" and
//! "Push" rows) through `gitcroc-cli group run`, asserted on the preflight
//! and the report (section 12 "Group scenarios").

use crate::group_world::{World, classes, json, line, refused, report, text};

#[test]
fn grp_12_group_fetch_skips_a_repo_without_remote() {
    let world = World::new(&["a", "b", "c"]);
    world.repo("c").git(["remote", "remove", "origin"]).unwrap();
    world.other_pushes("b.txt", "b\n");

    let preflight = refused(&world.run(&["fetch"]));
    assert_eq!(
        classes(&preflight),
        [
            line("a", "will_run", ""),
            line("b", "will_run", ""),
            line("c", "will_skip", "no remote")
        ]
    );
    let result = json(&world.run(&["fetch", "--confirm"]));
    assert_eq!(result["job"]["label"], "Fetch · All (3 repos)");
    assert_eq!(
        report(&result),
        [
            line("a", "succeeded", ""),
            line("b", "succeeded", ""),
            line("c", "skipped", "no remote")
        ]
    );
    assert_eq!(
        world.repo("a").rev_parse("origin/main").unwrap(),
        world.remote("main")
    );

    let again = json(&world.run(&["fetch", "--confirm"]));
    assert_eq!(
        report(&again),
        [
            line("a", "nothing_to_do", ""),
            line("b", "nothing_to_do", ""),
            line("c", "skipped", "no remote")
        ]
    );
}

#[test]
fn grp_12_group_pull_never_stashes() {
    let names = ["detached", "local", "ff", "conflict", "dirty"];
    let world = World::new(&names);
    world.repo("detached").git(["switch", "--detach"]).unwrap();
    world.repo("local").git(["switch", "-c", "local"]).unwrap();
    world
        .repo("conflict")
        .commit_file("a.txt", "ours\n", "Ours")
        .unwrap();
    world.repo("dirty").write("a.txt", "dirty\n").unwrap();
    world.other_pushes("a.txt", "theirs\n");

    // Each repo pulls its own branch: a merge where it cannot
    // fast-forward, as the strategy says.
    let preflight = refused(&world.run(&["pull", "--strategy", "merge"]));
    assert_eq!(
        classes(&preflight),
        [
            line("detached", "will_skip", "detached HEAD"),
            line("local", "will_skip", "no upstream"),
            line("ff", "will_run", ""),
            line("conflict", "will_run", ""),
            line("dirty", "will_run", "")
        ]
    );
    let result = json(&world.run(&["pull", "--strategy", "merge", "--confirm"]));
    assert_eq!(
        report(&result),
        [
            line("detached", "skipped", "detached HEAD"),
            line("local", "skipped", "no upstream"),
            line("ff", "succeeded", ""),
            line("conflict", "conflict", ""),
            line("dirty", "failed", "")
        ]
    );
    assert_eq!(world.repo("ff").head().unwrap(), world.remote("main"));
    // OPS-1: the merge stopped in conflict state.
    assert!(
        world
            .repo("conflict")
            .rev_parse("MERGE_HEAD")
            .is_ok_and(|merge| merge == world.remote("main"))
    );
    // GRP-12: git refused, with its message; nothing was stashed.
    let dirty = &result["job"]["report"][4];
    assert!(
        text(&dirty["message"]).contains("would be overwritten"),
        "{dirty:#?}"
    );
    let dirty_repo = world.repo("dirty");
    assert_eq!(
        std::fs::read_to_string(dirty_repo.path().join("a.txt")).unwrap(),
        "dirty\n"
    );
    assert_eq!(dirty_repo.git(["stash", "list"]).unwrap(), "");
    assert_eq!(result["job"]["outcome"]["kind"], "failed");
}

#[test]
fn grp_12_group_push_sets_the_upstream_of_a_new_branch() {
    let world = World::new(&["new", "rejected", "current", "elsewhere"]);
    let new = world.repo("new");
    new.git(["switch", "-c", "feature"]).unwrap();
    new.commit_file("b.txt", "b\n", "New").unwrap();
    world
        .repo("rejected")
        .commit_file("c.txt", "c\n", "Ours")
        .unwrap();
    let elsewhere = world.repo("elsewhere");
    elsewhere
        .git(["remote", "rename", "origin", "upstream"])
        .unwrap();
    elsewhere.git(["switch", "-c", "topic"]).unwrap();
    world.other_pushes("d.txt", "d\n");
    world
        .repo("current")
        .git(["pull", "--quiet", "--ff-only"])
        .unwrap();

    // Without "set upstream", a branch without one is skipped.
    let preflight = refused(&world.run(&["push", "--no-set-upstream"]));
    assert_eq!(
        classes(&preflight)[0],
        line("new", "will_skip", "no upstream")
    );

    let preflight = refused(&world.run(&["push"]));
    assert_eq!(
        classes(&preflight),
        [
            line(
                "new",
                "needs_attention",
                "no upstream; will set origin/feature"
            ),
            line("rejected", "will_run", ""),
            line("current", "will_run", ""),
            line("elsewhere", "will_skip", "no upstream and no origin")
        ]
    );
    let result = json(&world.run(&["push", "--confirm"]));
    assert_eq!(
        report(&result),
        [
            line("new", "succeeded", ""),
            line("rejected", "failed", ""),
            line("current", "nothing_to_do", ""),
            line("elsewhere", "skipped", "no upstream and no origin")
        ]
    );
    assert_eq!(world.remote("feature"), new.head().unwrap());
    assert_eq!(
        new.git(["rev-parse", "--abbrev-ref", "feature@{upstream}"])
            .unwrap()
            .trim(),
        "origin/feature"
    );
}

#[test]
fn grp_9_group_force_with_lease_needs_confirmation() {
    let world = World::new(&["diverged"]);
    let diverged = world.repo("diverged");
    diverged.commit_file("c.txt", "c\n", "Ours").unwrap();
    world.other_pushes("d.txt", "d\n");
    json(&world.run(&["fetch"]));
    let theirs = world.remote("main");

    let preflight = refused(&world.run(&["push", "--force-with-lease"]));
    assert_eq!(preflight["preflight"]["action"], "Force push");
    assert_eq!(
        preflight["preflight"]["destructive"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    assert_eq!(classes(&preflight), [line("diverged", "will_run", "")]);
    assert_eq!(world.remote("main"), theirs, "nothing was pushed");

    let result = json(&world.run(&["push", "--force-with-lease", "--confirm"]));
    assert_eq!(report(&result), [line("diverged", "succeeded", "")]);
    assert_eq!(world.remote("main"), diverged.head().unwrap());
}
