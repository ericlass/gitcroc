//! A workspace for group operations through `gitcroc-cli group run`
//! (section 12 "Group scenarios"): every repo is a clone of one bare remote
//! on disk reached over `file://`; another clone of it stands in for
//! someone else pushing. git runs without the machine's config, so no
//! setting of it takes part. Also what reads the preflight and the report.

use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use repo_gen::{Fixture, Repo};

pub(crate) const BIN: &str = env!("CARGO_BIN_EXE_gitcroc-cli");

pub(crate) fn file_url(path: &Path) -> String {
    let path = path.to_string_lossy().replace('\\', "/");
    if path.starts_with('/') {
        format!("file://{path}")
    } else {
        format!("file:///{path}")
    }
}

/// A workspace whose group `All` holds clones of one bare remote.
pub(crate) struct World {
    pub(crate) fixture: Fixture,
    pub(crate) config: tempfile::TempDir,
    pub(crate) bare: PathBuf,
    pub(crate) other: Repo,
    /// Each repo's name and id.
    ids: Vec<(String, String)>,
    cache: tempfile::TempDir,
}

impl World {
    /// Clones `names` into the workspace, in that order in group `All`,
    /// each with `a.txt` on `main`.
    pub(crate) fn new(names: &[&str]) -> Self {
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
        let mut world = Self {
            fixture,
            config: tempfile::tempdir().unwrap(),
            bare,
            other,
            ids: Vec::new(),
            cache: tempfile::tempdir().unwrap(),
        };
        let repos = world.fixture.path().join("repos");
        json(&world.cli(&["workspace".as_ref(), "add".as_ref(), repos.as_os_str()]));
        let shown = json(&world.cli(&["workspace".as_ref(), "show".as_ref()]));
        world.ids = names
            .iter()
            .map(|name| {
                let repos = shown["repos"].as_array().unwrap();
                let repo = repos.iter().find(|repo| repo["name"] == *name).unwrap();
                ((*name).to_owned(), repo["id"].as_str().unwrap().to_owned())
            })
            .collect();
        world.group("All", names);
        world
    }

    /// Creates group `name` of repos `members`.
    pub(crate) fn group(&self, name: &str, members: &[&str]) {
        let mut create: Vec<&OsStr> = vec!["group".as_ref(), "create".as_ref(), name.as_ref()];
        for member in members {
            let (_, id) = self.ids.iter().find(|(repo, _)| repo == member).unwrap();
            create.push(id.as_ref());
        }
        json(&self.cli(&create));
    }

    /// Runs the CLI on the world's config with git kept from the machine's
    /// config, and an identity for the merges it may make.
    pub(crate) fn cli(&self, args: &[&OsStr]) -> Output {
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
    pub(crate) fn run(&self, args: &[&str]) -> Output {
        self.run_on("All", args)
    }

    /// `gitcroc-cli group run <group> <args>`.
    pub(crate) fn run_on(&self, group: &str, args: &[&str]) -> Output {
        let mut all: Vec<&OsStr> = vec!["group".as_ref(), "run".as_ref(), group.as_ref()];
        all.extend(args.iter().map(OsStr::new));
        self.cli(&all)
    }

    /// The summaries of every repo read again, as `gitcroc-cli summaries`
    /// prints them; they are cached for [`Self::group_branch`].
    pub(crate) fn summaries(&self) -> serde_json::Value {
        let cache = self.cache.path().as_os_str();
        json(&self.cli(&["summaries".as_ref(), "--cache-dir".as_ref(), cache]))
    }

    /// The branch the tree shows for group `name` (GRP-4), from the
    /// summaries read last.
    pub(crate) fn group_branch(&self, name: &str) -> serde_json::Value {
        let cache = self.cache.path().as_os_str();
        let tree = json(&self.cli(&["tree".as_ref(), "--cache-dir".as_ref(), cache]));
        let nodes = tree.as_array().unwrap();
        let group = nodes
            .iter()
            .find(|node| node["item"]["kind"] == "group" && node["label"] == name)
            .unwrap();
        group["item"]["summary"]["branch"].clone()
    }

    pub(crate) fn repo(&self, name: &str) -> Repo {
        Repo::open(self.fixture.path().join("repos").join(name))
    }

    /// Someone else commits `file` and pushes it to `main`.
    pub(crate) fn other_pushes(&self, file: &str, contents: &str) {
        self.other.git(["pull", "--quiet", "--ff-only"]).unwrap();
        self.other
            .commit_file(file, contents, &format!("Other {file}"))
            .unwrap();
        self.other
            .git(["push", "--quiet", "origin", "main"])
            .unwrap();
    }

    /// The commit branch `branch` of the remote points to.
    pub(crate) fn remote(&self, branch: &str) -> String {
        let mut args: Vec<std::ffi::OsString> = vec!["--git-dir".into(), self.bare.clone().into()];
        args.extend(["rev-parse".into(), format!("refs/heads/{branch}").into()]);
        self.other.git(args).unwrap().trim().to_owned()
    }
}

pub(crate) fn json(out: &Output) -> serde_json::Value {
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    serde_json::from_slice(&out.stdout).unwrap()
}

/// The preflight a run printed when it needed a confirmation it did not
/// have: it exits 2 and starts nothing.
pub(crate) fn refused(out: &Output) -> serde_json::Value {
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

pub(crate) fn text(value: &serde_json::Value) -> String {
    value.as_str().unwrap_or_default().to_owned()
}

/// Each preflight line as (repo name, class, reason).
pub(crate) fn classes(result: &serde_json::Value) -> Vec<(String, String, String)> {
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
pub(crate) fn report(result: &serde_json::Value) -> Vec<(String, String, String)> {
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

pub(crate) fn line(name: &str, kind: &str, reason: &str) -> (String, String, String) {
    (name.to_owned(), kind.to_owned(), reason.to_owned())
}
