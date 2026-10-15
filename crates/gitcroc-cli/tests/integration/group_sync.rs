//! Group fetch, pull and push (GRP-12, section 5 "Fetch", "Pull" and
//! "Push" rows) through `gitcroc-cli group run`, asserted on the preflight
//! and the report (section 12 "Group scenarios"). Every repo is a clone of
//! one bare remote on disk reached over `file://`; another clone of it
//! stands in for someone else pushing. git runs without the machine's
//! config, so no setting of it takes part.

use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use repo_gen::{Fixture, Repo};

const BIN: &str = env!("CARGO_BIN_EXE_gitcroc-cli");

fn file_url(path: &Path) -> String {
    let path = path.to_string_lossy().replace('\\', "/");
    if path.starts_with('/') {
        format!("file://{path}")
    } else {
        format!("file:///{path}")
    }
}

/// A workspace whose group `All` holds clones of one bare remote.
struct World {
    fixture: Fixture,
    config: tempfile::TempDir,
    bare: PathBuf,
    other: Repo,
}

impl World {
    /// Clones `names` into the workspace, in that order in group `All`,
    /// each with `a.txt` on `main`.
    fn new(names: &[&str]) -> Self {
        let fixture = Fixture::new().unwrap();
        let seed = fixture.repo("seed").unwrap();
        seed.commit_file("a.txt", "a\n", "First").unwrap();
        let bare = fixture.path().join("origin.git");
        let mut args: Vec<std::ffi::OsString> = vec!["clone".into(), "--bare".into()];
        args.extend(["--quiet".into(), "--".into()]);
        args.extend([seed.path().into(), bare.clone().into()]);
        seed.git(args).unwrap();
        let clone = |path: PathBuf| {
            let mut args: Vec<std::ffi::OsString> =
                vec!["clone".into(), "--quiet".into(), "--".into()];
            args.extend([file_url(&bare).into(), path.clone().into()]);
            seed.git(args).unwrap();
            Repo::open(path)
        };
        for name in names {
            clone(fixture.path().join("repos").join(name));
        }
        let other = clone(fixture.path().join("other"));
        let world = Self {
            fixture,
            config: tempfile::tempdir().unwrap(),
            bare,
            other,
        };
        let repos = world.fixture.path().join("repos");
        json(&world.cli(&["workspace".as_ref(), "add".as_ref(), repos.as_os_str()]));
        let shown = json(&world.cli(&["workspace".as_ref(), "show".as_ref()]));
        let ids: Vec<String> = names
            .iter()
            .map(|name| {
                let repos = shown["repos"].as_array().unwrap();
                let repo = repos.iter().find(|repo| repo["name"] == *name).unwrap();
                repo["id"].as_str().unwrap().to_owned()
            })
            .collect();
        let mut create: Vec<&OsStr> = vec!["group".as_ref(), "create".as_ref(), "All".as_ref()];
        create.extend(ids.iter().map(OsStr::new));
        json(&world.cli(&create));
        world
    }

    /// Runs the CLI on the world's config with git kept from the machine's
    /// config, and an identity for the merges it may make.
    fn cli(&self, args: &[&OsStr]) -> Output {
        let (command, rest) = args.split_first().unwrap();
        let no_config = self.config.path().join("no-gitconfig");
        Command::new(BIN)
            .arg(command)
            .arg("--config-dir")
            .arg(self.config.path())
            .args(rest)
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env("GIT_CONFIG_GLOBAL", no_config)
            .env("GIT_AUTHOR_NAME", "Test")
            .env("GIT_AUTHOR_EMAIL", "test@example.com")
            .env("GIT_COMMITTER_NAME", "Test")
            .env("GIT_COMMITTER_EMAIL", "test@example.com")
            .output()
            .unwrap()
    }

    /// `gitcroc-cli group run All <args>`.
    fn run(&self, args: &[&str]) -> Output {
        let mut all: Vec<&OsStr> = vec!["group".as_ref(), "run".as_ref(), "All".as_ref()];
        all.extend(args.iter().map(OsStr::new));
        self.cli(&all)
    }

    fn repo(&self, name: &str) -> Repo {
        Repo::open(self.fixture.path().join("repos").join(name))
    }

    /// Someone else commits `file` and pushes it to `main`.
    fn other_pushes(&self, file: &str, contents: &str) {
        self.other.git(["pull", "--quiet", "--ff-only"]).unwrap();
        self.other
            .commit_file(file, contents, &format!("Other {file}"))
            .unwrap();
        self.other
            .git(["push", "--quiet", "origin", "main"])
            .unwrap();
    }

    /// The commit branch `branch` of the remote points to.
    fn remote(&self, branch: &str) -> String {
        let mut args: Vec<std::ffi::OsString> = vec!["--git-dir".into(), self.bare.clone().into()];
        args.extend(["rev-parse".into(), format!("refs/heads/{branch}").into()]);
        self.other.git(args).unwrap().trim().to_owned()
    }
}

fn json(out: &Output) -> serde_json::Value {
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    serde_json::from_slice(&out.stdout).unwrap()
}

/// The preflight a run printed when it needed a confirmation it did not
/// have: it exits 2 and starts nothing.
fn refused(out: &Output) -> serde_json::Value {
    assert_eq!(
        out.status.code(),
        Some(2),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let result: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert!(result["job"].is_null());
    assert_eq!(result["preflight"]["needs_confirmation"], true);
    result
}

fn text(value: &serde_json::Value) -> String {
    value.as_str().unwrap_or_default().to_owned()
}

/// Each preflight line as (repo name, class, reason).
fn classes(result: &serde_json::Value) -> Vec<(String, String, String)> {
    let repos = result["preflight"]["repos"].as_array().unwrap();
    repos
        .iter()
        .map(|repo| {
            let class = &repo["class"];
            (
                text(&repo["repo_name"]),
                text(&class["kind"]),
                text(&class["reason"]),
            )
        })
        .collect()
}

/// Each report line as (repo name, outcome, skip reason).
fn report(result: &serde_json::Value) -> Vec<(String, String, String)> {
    let lines = result["job"]["report"].as_array().unwrap();
    lines
        .iter()
        .map(|line| {
            let outcome = &line["outcome"];
            let reason = text(&outcome["reason"]["reason"]);
            (text(&line["repo_name"]), text(&outcome["kind"]), reason)
        })
        .collect()
}

fn line(name: &str, kind: &str, reason: &str) -> (String, String, String) {
    (name.to_owned(), kind.to_owned(), reason.to_owned())
}

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
