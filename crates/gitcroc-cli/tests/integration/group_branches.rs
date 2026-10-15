//! Group checkout, create and delete of branch X (GRP-14, section 5
//! "Checkout branch X", "Create branch X" and "Delete branch X" rows) and
//! the mixed-branch scenario of section 5, through `gitcroc-cli group run`,
//! asserted on the preflight, the report and what the tree shows (GRP-4).

use std::ffi::OsString;
use std::path::PathBuf;

use repo_gen::Repo;

use crate::group_world::{World, classes, file_url, json, line, refused, report, text};

/// The branch `repo` is on.
fn current(repo: &Repo) -> String {
    repo.git(["branch", "--show-current"])
        .unwrap()
        .trim()
        .to_owned()
}

/// What local branch `branch` of `repo` tracks, such as `origin/x`.
fn upstream(repo: &Repo, branch: &str) -> String {
    let upstream = format!("{branch}@{{upstream}}");
    repo.git(["rev-parse", "--abbrev-ref", upstream.as_str()])
        .unwrap()
        .trim()
        .to_owned()
}

/// A bare copy of the world's remote, as it is now, named `name`.
fn bare_copy(world: &World, name: &str) -> PathBuf {
    let path = world.fixture.path().join(format!("{name}.git"));
    let mut args: Vec<OsString> = vec!["clone".into(), "--bare".into(), "--quiet".into()];
    args.extend(["--".into(), world.bare.clone().into(), path.clone().into()]);
    world.other.git(args).unwrap();
    path
}

/// `repo` gets remote `name` at `bare`, fetched.
fn add_remote(repo: &Repo, name: &str, bare: &std::path::Path) {
    repo.git(["remote", "add", name, file_url(bare).as_str()])
        .unwrap();
    repo.git(["fetch", "--quiet", name]).unwrap();
}

/// Branch `branch` exists in the bare repo at `bare`.
fn bare_has(world: &World, bare: &std::path::Path, branch: &str) -> bool {
    let mut args: Vec<OsString> = vec!["--git-dir".into(), bare.into()];
    args.extend(["show-ref".into(), "--verify".into(), "--quiet".into()]);
    args.push(format!("refs/heads/{branch}").into());
    world.other.git(args).is_ok()
}

/// Someone else pushes a new commit as branch `branch` of the remote.
fn other_pushes_branch(world: &World, branch: &str) {
    world
        .other
        .git(["switch", "--quiet", "-c", branch])
        .unwrap();
    world
        .other
        .commit_file("x.txt", format!("{branch}\n"), &format!("On {branch}"))
        .unwrap();
    let refspec = format!("HEAD:refs/heads/{branch}");
    world
        .other
        .git(["push", "--quiet", "origin", refspec.as_str()])
        .unwrap();
    world.other.git(["switch", "--quiet", "main"]).unwrap();
}

#[test]
fn grp_14_x_resolves_separately_in_each_repo() {
    let world = World::new(&["a", "b"]);
    for name in ["a", "b"] {
        let repo = world.repo(name);
        repo.git(["switch", "--quiet", "-c", "x"]).unwrap();
        repo.commit_file(&format!("{name}.txt"), "x\n", "On x")
            .unwrap();
        repo.git(["switch", "--quiet", "main"]).unwrap();
    }
    let (a, b) = (world.repo("a"), world.repo("b"));
    assert_ne!(a.rev_parse("x").unwrap(), b.rev_parse("x").unwrap());

    // Not flagged: every repo will run, so it runs without asking.
    let result = json(&world.run(&["checkout", "x"]));
    assert_eq!(result["preflight"]["needs_confirmation"], false);
    assert_eq!(
        classes(&result),
        [line("a", "will_run", ""), line("b", "will_run", "")]
    );
    assert_eq!(
        report(&result),
        [line("a", "succeeded", ""), line("b", "succeeded", "")]
    );
    assert_eq!(current(&a), "x");
    assert_eq!(current(&b), "x");
    assert_ne!(a.head().unwrap(), b.head().unwrap());
}

#[test]
fn sec_4_a_group_branch_name_is_checked_once_before_anything_runs() {
    let world = World::new(&["a"]);
    let out = world.run(&["create-branch", "bad..name"]);
    assert_eq!(out.status.code(), Some(2));
    assert!(out.stdout.is_empty(), "no preflight ran");
    assert!(
        String::from_utf8_lossy(&out.stderr).contains(".."),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let out = world.run(&["checkout", "-x"]);
    assert_eq!(out.status.code(), Some(2));
    assert!(out.stdout.is_empty());
}

#[test]
fn grp_14_checkout_missing_branch_policies() {
    let world = World::new(&["has", "lacks", "nodefault"]);
    let has = world.repo("has");
    has.git(["branch", "x"]).unwrap();
    world
        .repo("lacks")
        .git(["switch", "--quiet", "-c", "other"])
        .unwrap();
    // Neither `origin/HEAD` nor `main` nor `master`: no default branch.
    let nodefault = world.repo("nodefault");
    nodefault
        .git(["remote", "set-head", "origin", "--delete"])
        .unwrap();
    nodefault.git(["branch", "-m", "main", "trunk"]).unwrap();

    // Skip, the default.
    let preflight = refused(&world.run(&["checkout", "x"]));
    assert_eq!(
        classes(&preflight),
        [
            line("has", "will_run", ""),
            line("lacks", "will_skip", "no branch x"),
            line("nodefault", "will_skip", "no branch x")
        ]
    );
    let result = json(&world.run(&["checkout", "x", "--missing", "skip", "--confirm"]));
    assert_eq!(
        report(&result),
        [
            line("has", "succeeded", ""),
            line("lacks", "skipped", "no branch x"),
            line("nodefault", "skipped", "no branch x")
        ]
    );
    assert_eq!(current(&world.repo("lacks")), "other");

    // Switch to the default branch: `origin/HEAD`'s, checked out locally.
    let args = ["checkout", "x", "--missing", "default-branch"];
    let preflight = refused(&world.run(&args));
    assert_eq!(
        classes(&preflight),
        [
            line("has", "will_run", ""),
            line("lacks", "needs_attention", "no branch x; switches to main"),
            line("nodefault", "will_skip", "no default branch")
        ]
    );
    let result = json(&world.run(&[&args[..], &["--confirm"]].concat()));
    assert_eq!(
        report(&result),
        [
            line("has", "nothing_to_do", ""),
            line("lacks", "succeeded", ""),
            line("nodefault", "skipped", "no default branch")
        ]
    );
    assert_eq!(current(&world.repo("lacks")), "main");
    assert_eq!(current(&nodefault), "trunk");

    // Create X from `HEAD`.
    let args = ["checkout", "x", "--missing", "create"];
    let preflight = refused(&world.run(&args));
    assert_eq!(
        classes(&preflight),
        [
            line("has", "will_run", ""),
            line("lacks", "needs_attention", "creates x"),
            line("nodefault", "needs_attention", "creates x")
        ]
    );
    let trunk = nodefault.head().unwrap();
    let result = json(&world.run(&[&args[..], &["--confirm"]].concat()));
    assert_eq!(
        report(&result),
        [
            line("has", "nothing_to_do", ""),
            line("lacks", "succeeded", ""),
            line("nodefault", "succeeded", "")
        ]
    );
    assert_eq!(current(&nodefault), "x");
    assert_eq!(nodefault.head().unwrap(), trunk);
}

#[test]
fn grp_14_checkout_tracks_the_one_remote_that_has_x() {
    let world = World::new(&["one", "two"]);
    other_pushes_branch(&world, "x");
    let second = bare_copy(&world, "second");
    let (one, two) = (world.repo("one"), world.repo("two"));
    one.git(["fetch", "--quiet", "origin"]).unwrap();
    two.git(["fetch", "--quiet", "origin"]).unwrap();
    add_remote(&two, "second", &second);

    let preflight = refused(&world.run(&["checkout", "x"]));
    assert_eq!(
        classes(&preflight),
        [
            line("one", "will_run", ""),
            line("two", "will_skip", "x is on several remotes")
        ]
    );
    let result = json(&world.run(&["checkout", "x", "--confirm"]));
    assert_eq!(
        report(&result),
        [
            line("one", "succeeded", ""),
            line("two", "skipped", "x is on several remotes")
        ]
    );
    assert_eq!(current(&one), "x");
    assert_eq!(upstream(&one, "x"), "origin/x");
    assert_eq!(one.head().unwrap(), world.remote("x"));
    assert_eq!(current(&two), "main");
}

#[test]
fn grp_14_create_skips_an_existing_branch_and_never_resets_it() {
    let world = World::new(&["new", "exists"]);
    let exists = world.repo("exists");
    exists.git(["switch", "--quiet", "-c", "x"]).unwrap();
    let kept = exists.commit_file("b.txt", "b\n", "On x").unwrap();
    exists.git(["switch", "--quiet", "main"]).unwrap();
    let new = world.repo("new");
    new.tag("v1").unwrap();

    let preflight = refused(&world.run(&["create-branch", "x"]));
    assert_eq!(
        classes(&preflight),
        [
            line("new", "will_run", ""),
            line("exists", "will_skip", "x already exists")
        ]
    );
    let result = json(&world.run(&["create-branch", "x", "--confirm"]));
    assert_eq!(
        report(&result),
        [
            line("new", "succeeded", ""),
            line("exists", "skipped", "x already exists")
        ]
    );
    assert_eq!(current(&new), "x");
    assert_eq!(exists.rev_parse("x").unwrap(), kept, "x was not reset");
    assert_eq!(current(&exists), "main");

    // A start point each repo resolves on its own; without switching.
    let args = ["create-branch", "y", "--start", "v1", "--no-switch"];
    let preflight = refused(&world.run(&args));
    assert_eq!(
        classes(&preflight),
        [
            line("new", "will_run", ""),
            line("exists", "will_skip", "v1 not found")
        ]
    );
    let result = json(&world.run(&[&args[..], &["--confirm"]].concat()));
    assert_eq!(report(&result)[0], line("new", "succeeded", ""));
    assert_eq!(new.rev_parse("y").unwrap(), new.rev_parse("v1").unwrap());
    assert_eq!(current(&new), "x");

    // From the default branch: `origin/HEAD`'s branch.
    let result = json(&world.run(&["create-branch", "z", "--start-default"]));
    assert_eq!(
        report(&result),
        [
            line("new", "succeeded", ""),
            line("exists", "succeeded", "")
        ]
    );
    assert_eq!(new.rev_parse("z").unwrap(), world.remote("main"));
    assert_eq!(current(&exists), "z");
}

#[test]
fn grp_9_group_delete_skips_and_asks_before_force_and_remote_delete() {
    let world = World::new(&["missing", "current", "merged"]);
    world
        .repo("current")
        .git(["switch", "--quiet", "-c", "x"])
        .unwrap();
    let merged = world.repo("merged");
    merged.git(["branch", "x"]).unwrap();

    let preflight = refused(&world.run(&["delete-branch", "x"]));
    assert_eq!(preflight["preflight"]["destructive"], serde_json::json!([]));
    assert_eq!(
        classes(&preflight),
        [
            line("missing", "will_skip", "no branch x"),
            line("current", "will_skip", "x is the current branch"),
            line("merged", "will_run", "")
        ]
    );
    let result = json(&world.run(&["delete-branch", "x", "--confirm"]));
    assert_eq!(
        report(&result),
        [
            line("missing", "skipped", "no branch x"),
            line("current", "skipped", "x is the current branch"),
            line("merged", "succeeded", "")
        ]
    );
    assert!(merged.rev_parse("refs/heads/x").is_err());

    // Not merged: `-d` fails with git's message, `--force` asks first.
    merged.git(["switch", "--quiet", "-c", "y"]).unwrap();
    merged.commit_file("y.txt", "y\n", "On y").unwrap();
    merged.git(["switch", "--quiet", "main"]).unwrap();
    let group = ["merged"];
    world.group("Merged", &group);
    let result = json(&world.run_on("Merged", &["delete-branch", "y"]));
    assert_eq!(report(&result), [line("merged", "failed", "")]);
    assert!(text(&result["job"]["report"][0]["message"]).contains("not fully merged"));

    let preflight = refused(&world.run_on("Merged", &["delete-branch", "y", "--force"]));
    assert_eq!(preflight["preflight"]["action"], "Delete branch y");
    assert_eq!(
        preflight["preflight"]["destructive"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    assert!(merged.rev_parse("refs/heads/y").is_ok(), "nothing ran");
    let args = ["delete-branch", "y", "--force", "--confirm"];
    let result = json(&world.run_on("Merged", &args));
    assert_eq!(report(&result), [line("merged", "succeeded", "")]);
    assert!(merged.rev_parse("refs/heads/y").is_err());
}

#[test]
fn grp_9_group_delete_on_the_remote_of_the_upstream_else_origin() {
    let world = World::new(&["tracked", "origin_only", "no_remote"]);
    other_pushes_branch(&world, "x");
    let mirror = bare_copy(&world, "mirror");
    // `x` tracks `mirror/x`.
    let tracked = world.repo("tracked");
    add_remote(&tracked, "mirror", &mirror);
    tracked.git(["branch", "--quiet", "x", "mirror/x"]).unwrap();
    assert_eq!(upstream(&tracked, "x"), "mirror/x");
    // `x` has no upstream: the remote delete goes to `origin`.
    world.repo("origin_only").git(["branch", "x"]).unwrap();
    // Neither an upstream nor `origin`: deleted here only.
    let no_remote = world.repo("no_remote");
    no_remote.git(["remote", "remove", "origin"]).unwrap();
    no_remote.git(["branch", "x"]).unwrap();

    let preflight = refused(&world.run(&["delete-branch", "x", "--on-remote"]));
    assert_eq!(
        classes(&preflight),
        [
            line("tracked", "will_run", ""),
            line("origin_only", "will_run", ""),
            line("no_remote", "will_run", "")
        ]
    );
    assert_eq!(
        preflight["preflight"]["destructive"],
        serde_json::json!(["Delete branch x on the remote"])
    );
    assert!(bare_has(&world, &mirror, "x") && bare_has(&world, &world.bare, "x"));

    let result = json(&world.run(&["delete-branch", "x", "--on-remote", "--confirm"]));
    assert_eq!(
        report(&result),
        [
            line("tracked", "succeeded", ""),
            line("origin_only", "succeeded", ""),
            line("no_remote", "succeeded", "")
        ]
    );
    assert!(!bare_has(&world, &mirror, "x"));
    assert!(!bare_has(&world, &world.bare, "x"));
    for name in ["tracked", "origin_only", "no_remote"] {
        assert!(
            world.repo(name).rev_parse("refs/heads/x").is_err(),
            "{name}"
        );
    }
    let output = text(&result["job"]["report"][2]["output"]);
    assert!(output.contains("deleted here only"), "{output}");
}

#[test]
fn grp_12_group_checkout_never_stashes() {
    let world = World::new(&["clean", "dirty", "already"]);
    for name in ["clean", "dirty", "already"] {
        let repo = world.repo(name);
        repo.git(["switch", "--quiet", "-c", "x"]).unwrap();
        repo.commit_file("a.txt", "x\n", "On x").unwrap();
        if name != "already" {
            repo.git(["switch", "--quiet", "main"]).unwrap();
        }
    }
    let dirty = world.repo("dirty");
    dirty.write("a.txt", "dirty\n").unwrap();

    let result = json(&world.run(&["checkout", "x"]));
    assert_eq!(
        report(&result),
        [
            line("clean", "succeeded", ""),
            line("dirty", "failed", ""),
            line("already", "nothing_to_do", "")
        ]
    );
    let message = text(&result["job"]["report"][1]["message"]);
    assert!(message.contains("would be overwritten"), "{message}");
    assert_eq!(current(&dirty), "main");
    assert_eq!(
        std::fs::read_to_string(dirty.path().join("a.txt")).unwrap(),
        "dirty\n"
    );
    assert_eq!(dirty.git(["stash", "list"]).unwrap(), "");
}

/// Section 5 "The mixed-branch scenario, step by step", on 5 repos.
#[test]
fn grp_mixed_branch_scenario() {
    let names = ["r1", "r2", "r3", "r4", "r5"];
    let world = World::new(&names);
    other_pushes_branch(&world, "feature/x");
    world
        .repo("r2")
        .git(["switch", "--quiet", "-c", "topic"])
        .unwrap();
    json(&world.run(&["fetch"]));

    // 1. Group checkout `main`: every repo is on it; the group shows it.
    let result = json(&world.run(&["checkout", "main"]));
    let mut expected: Vec<_> = names
        .iter()
        .map(|name| line(name, "nothing_to_do", ""))
        .collect();
    expected[1] = line("r2", "succeeded", "");
    assert_eq!(report(&result), expected);
    world.summaries();
    assert_eq!(
        world.group_branch("All"),
        serde_json::json!({ "kind": "branch", "name": "main" })
    );

    // 2. One repo checks out `feature/x`: only it switches.
    world.group("Just r3", &["r3"]);
    let result = json(&world.run_on("Just r3", &["checkout", "feature/x"]));
    assert_eq!(report(&result), [line("r3", "succeeded", "")]);
    for name in names {
        let branch = if name == "r3" { "feature/x" } else { "main" };
        assert_eq!(current(&world.repo(name)), branch, "{name}");
    }

    // 3. The group shows "mixed (2)"; each repo's summary has its own
    // branch.
    let summaries = world.summaries();
    let heads: Vec<String> = summaries
        .as_array()
        .unwrap()
        .iter()
        .map(|view| text(&view["summary"]["head"]["name"]))
        .collect();
    assert_eq!(heads, ["main", "main", "feature/x", "main", "main"]);
    assert_eq!(
        world.group_branch("All"),
        serde_json::json!({ "kind": "mixed", "count": 2 })
    );

    // 4. Group pull: each repo pulls its own branch. r3's `feature/x` is
    // up to date, but its fetch brought in the new `origin/main`.
    world.other_pushes("b.txt", "b\n");
    let result = json(&world.run(&["pull"]));
    let expected: Vec<_> = names
        .iter()
        .map(|name| line(name, "succeeded", ""))
        .collect();
    assert_eq!(report(&result), expected);
    for name in names {
        let branch = if name == "r3" { "feature/x" } else { "main" };
        assert_eq!(
            world.repo(name).head().unwrap(),
            world.remote(branch),
            "{name}"
        );
    }

    // 5. Group checkout `main` brings everything back in line.
    let result = json(&world.run(&["checkout", "main"]));
    let mut expected: Vec<_> = names
        .iter()
        .map(|name| line(name, "nothing_to_do", ""))
        .collect();
    expected[2] = line("r3", "succeeded", "");
    assert_eq!(report(&result), expected);
    world.summaries();
    assert_eq!(
        world.group_branch("All"),
        serde_json::json!({ "kind": "branch", "name": "main" })
    );
}
