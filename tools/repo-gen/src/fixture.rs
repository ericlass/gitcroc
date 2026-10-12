//! Fixture repos for tests, built in code (QA-2). Every repo write runs the
//! `git` CLI with an argument array (ARCH-1, ARCH-3).
//!
//! ```no_run
//! # fn main() -> Result<(), repo_gen::Error> {
//! let fixture = repo_gen::Fixture::new()?;
//! let repo = fixture.repo("app")?;
//! repo.commit_file("README.md", "hello\n", "Initial commit")?;
//! repo.branch("feature")?;
//! # Ok(())
//! # }
//! ```

use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use std::process::Output;
use std::sync::atomic::{AtomicU64, Ordering};

use tempfile::TempDir;

use crate::{Error, git};

/// Author and committer of every fixture commit.
pub const FIXTURE_NAME: &str = "Fixture Author";
pub const FIXTURE_EMAIL: &str = "author@example.com";

/// Fixture commits are dated from here (2024-01-01 00:00 UTC), one minute
/// apart, so commit ids are the same on every run.
const FIXTURE_EPOCH: u64 = 1_704_067_200;

/// A temporary directory holding fixture repos. Everything is deleted when it
/// is dropped.
pub struct Fixture {
    dir: TempDir,
}

impl Fixture {
    pub fn new() -> Result<Self, Error> {
        Ok(Self {
            dir: tempfile::Builder::new()
                .prefix("gitcroc-fixture-")
                .tempdir()?,
        })
    }

    pub fn path(&self) -> &Path {
        self.dir.path()
    }

    /// A new empty repo with `main` as its initial branch, in a subdirectory.
    pub fn repo(&self, name: &str) -> Result<Repo, Error> {
        Repo::init(self.dir.path().join(name))
    }
}

/// A repo with a working tree, written through `git`.
#[derive(Debug)]
pub struct Repo {
    path: PathBuf,
    clock: AtomicU64,
}

impl Repo {
    /// Runs `git init` with `main` as the initial branch.
    pub fn init(path: impl Into<PathBuf>) -> Result<Self, Error> {
        let path = path.into();
        std::fs::create_dir_all(&path)?;
        let repo = Self::open(path);
        repo.git(["init", "--quiet", "--initial-branch=main"])?;
        Ok(repo)
    }

    /// Wraps an existing repo, for example one made by [`Repo::clone_to`].
    pub fn open(path: impl Into<PathBuf>) -> Self {
        Self {
            path: path.into(),
            clock: AtomicU64::new(0),
        }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Runs git in the repo and returns its standard output. Fails unless git
    /// exits with 0.
    pub fn git<I, S>(&self, args: I) -> Result<String, Error>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<OsStr>,
    {
        let output = git::run(self.command(args))?;
        Ok(String::from_utf8_lossy(&output.stdout).into_owned())
    }

    /// Runs git in the repo and returns whatever it did, without failing on a
    /// non-zero exit code.
    pub fn git_output<I, S>(&self, args: I) -> Result<Output, Error>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<OsStr>,
    {
        Ok(self.command(args).output()?)
    }

    /// Every invocation advances the fixture clock by a minute, so commits
    /// made one after the other have increasing, reproducible dates.
    fn command<I, S>(&self, args: I) -> std::process::Command
    where
        I: IntoIterator<Item = S>,
        S: AsRef<OsStr>,
    {
        let tick = self.clock.fetch_add(1, Ordering::Relaxed);
        let date = format!("{} +0000", FIXTURE_EPOCH + tick * 60);
        let mut command = git::command(&self.path, args);
        command
            .env("GIT_AUTHOR_NAME", FIXTURE_NAME)
            .env("GIT_AUTHOR_EMAIL", FIXTURE_EMAIL)
            .env("GIT_AUTHOR_DATE", &date)
            .env("GIT_COMMITTER_NAME", FIXTURE_NAME)
            .env("GIT_COMMITTER_EMAIL", FIXTURE_EMAIL)
            .env("GIT_COMMITTER_DATE", &date);
        command
    }

    /// Writes a file in the working tree, creating its directories. This is
    /// the test standing in for the user's editor, not a repo write.
    pub fn write(&self, path: &str, contents: impl AsRef<[u8]>) -> Result<(), Error> {
        let file = self.path.join(path);
        if let Some(parent) = file.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(file, contents)?;
        Ok(())
    }

    /// Deletes a file from the working tree.
    pub fn remove(&self, path: &str) -> Result<(), Error> {
        std::fs::remove_file(self.path.join(path))?;
        Ok(())
    }

    /// Stages everything and commits it. Returns the new commit id.
    pub fn commit_all(&self, message: &str) -> Result<String, Error> {
        self.git(["add", "--all"])?;
        self.git(["commit", "--quiet", "--no-verify", "-m", message])?;
        self.head()
    }

    /// Writes one file and commits everything. Returns the new commit id.
    pub fn commit_file(
        &self,
        path: &str,
        contents: impl AsRef<[u8]>,
        message: &str,
    ) -> Result<String, Error> {
        self.write(path, contents)?;
        self.commit_all(message)
    }

    /// The commit id of `HEAD`.
    pub fn head(&self) -> Result<String, Error> {
        self.rev_parse("HEAD")
    }

    /// The object id a revision names.
    pub fn rev_parse(&self, revision: &str) -> Result<String, Error> {
        let out = self.git(["rev-parse", "--verify", "--end-of-options", revision])?;
        Ok(out.trim_end().to_owned())
    }

    /// Creates a branch at `HEAD` without switching to it.
    pub fn branch(&self, name: &str) -> Result<(), Error> {
        self.git(["branch", "--end-of-options", name])?;
        Ok(())
    }

    /// Switches to a branch.
    pub fn switch(&self, name: &str) -> Result<(), Error> {
        self.git(["switch", "--quiet", name])?;
        Ok(())
    }

    /// Creates a lightweight tag at `HEAD`.
    pub fn tag(&self, name: &str) -> Result<(), Error> {
        self.git(["tag", "--end-of-options", name])?;
        Ok(())
    }

    /// Creates an annotated tag at `HEAD`.
    pub fn annotated_tag(&self, name: &str, message: &str) -> Result<(), Error> {
        self.git(["tag", "--annotate", "-m", message, "--end-of-options", name])?;
        Ok(())
    }

    /// Merges a branch into the current one with a merge commit. Returns the
    /// merge commit id.
    pub fn merge(&self, branch: &str, message: &str) -> Result<String, Error> {
        self.git([
            "merge",
            "--no-ff",
            "--no-verify",
            "--quiet",
            "-m",
            message,
            "--end-of-options",
            branch,
        ])?;
        self.head()
    }

    /// Starts a merge that is expected to stop with conflicts, leaving the
    /// repo in the merging state. Fails if the merge succeeds.
    pub fn merge_with_conflicts(&self, branch: &str) -> Result<(), Error> {
        let mut command = self.command(["merge", "--no-ff", "--no-edit", "--end-of-options"]);
        command.arg(branch);
        let output = command.output()?;
        let conflicted = self.git(["ls-files", "--unmerged"])?;
        if output.status.success() || conflicted.is_empty() {
            return Err(Error::Invalid(format!(
                "merging {branch} in {} gave no conflict",
                self.path.display()
            )));
        }
        Ok(())
    }

    /// Stashes the working copy changes, untracked files included.
    pub fn stash(&self, message: &str) -> Result<(), Error> {
        self.git([
            "stash",
            "push",
            "--quiet",
            "--include-untracked",
            "-m",
            message,
        ])?;
        Ok(())
    }

    /// Clones this repo to `path`. The clone's `origin` is this repo, so it
    /// has remote-tracking branches without any network.
    pub fn clone_to(&self, path: impl Into<PathBuf>) -> Result<Repo, Error> {
        let path = path.into();
        let mut args = vec![
            "clone".into(),
            "--quiet".into(),
            "--".into(),
            self.path.clone().into_os_string(),
        ];
        args.push(path.clone().into_os_string());
        self.git(args)?;
        Ok(Repo::open(path))
    }

    /// Adds another local repo as a remote and fetches it.
    pub fn add_remote(&self, name: &str, other: &Repo) -> Result<(), Error> {
        let mut args = vec!["remote".into(), "add".into(), "--".into(), name.into()];
        args.push(other.path.clone().into_os_string());
        self.git::<_, std::ffi::OsString>(args)?;
        self.git(["fetch", "--quiet", name])?;
        Ok(())
    }
}
