//! Merge, rebase, cherry-pick and revert, and the abort and continue of an
//! operation stopped in conflict state (section 6 "Integrate", M2-08).
//! Nothing is stashed to make one succeed (GRP-12): `--no-autostash` wins
//! over `merge.autoStash` and `rebase.autoStash`. Messages are git's own:
//! `--no-edit`, and the runner's `GIT_EDITOR=true` for a continue.

use std::ffi::OsString;
use std::path::Path;

use crate::{GitError, GitRunner, Operation, Output, Write};

async fn integrate_write(
    git: &GitRunner,
    repo: &Path,
    args: &[&str],
    write: Write<'_>,
) -> Result<Output, GitError> {
    let args: Vec<OsString> = args.iter().map(OsString::from).collect();
    git.run_write(&args, repo, write).await
}

/// Whether a merge may fast-forward (section 5 "Merge X into current").
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FastForward {
    /// As `merge.ff` and `branch.<name>.mergeOptions` say; by default a
    /// fast-forward when possible, else a merge commit.
    Config,
    /// Fast-forward, or fail.
    Only,
    /// Always a merge commit.
    Never,
}

/// Merges `revision`, a ref or a hash, into the current branch with git's
/// prepared message (`git merge --no-edit`). Conflicts leave the repo in
/// conflict state.
pub async fn merge(
    git: &GitRunner,
    repo: &Path,
    revision: &str,
    ff: FastForward,
    write: Write<'_>,
) -> Result<Output, GitError> {
    let mut args = vec!["merge", "--no-edit", "--no-autostash"];
    match ff {
        FastForward::Config => {}
        FastForward::Only => args.push("--ff-only"),
        FastForward::Never => args.push("--no-ff"),
    }
    args.extend(["--end-of-options", revision]);
    integrate_write(git, repo, &args, write).await
}

/// Rebases the current branch onto `onto`, a ref or a hash, without
/// stopping for edits (`git rebase <onto>`). A conflict leaves the repo in
/// conflict state at the commit that stopped.
pub async fn rebase(
    git: &GitRunner,
    repo: &Path,
    onto: &str,
    write: Write<'_>,
) -> Result<Output, GitError> {
    let args = ["rebase", "--no-autostash", "--end-of-options", onto];
    integrate_write(git, repo, &args, write).await
}

/// Applies `commits`, full hashes, in the order given (`git cherry-pick`);
/// the caller puts them oldest first. A conflict stops the series there.
pub async fn cherry_pick(
    git: &GitRunner,
    repo: &Path,
    commits: &[String],
    write: Write<'_>,
) -> Result<Output, GitError> {
    let mut args = vec!["cherry-pick", "--end-of-options"];
    args.extend(commits.iter().map(String::as_str));
    integrate_write(git, repo, &args, write).await
}

/// Commits the reverse of `commit`, a full hash, with git's prepared
/// message (`git revert --no-edit`). A merge commit needs `mainline`, the
/// number of the parent whose side is kept (`-m`), counted from 1.
pub async fn revert(
    git: &GitRunner,
    repo: &Path,
    commit: &str,
    mainline: Option<u32>,
    write: Write<'_>,
) -> Result<Output, GitError> {
    let mainline = mainline.map(|parent| parent.to_string());
    let mut args = vec!["revert", "--no-edit"];
    if let Some(parent) = &mainline {
        args.extend(["-m", parent]);
    }
    args.extend(["--end-of-options", commit]);
    integrate_write(git, repo, &args, write).await
}

/// What to do with an operation stopped in conflict state (OPS-2).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Resume {
    /// Back to where the operation started (`--abort`).
    Abort,
    /// On with the resolved, staged result (`--continue`).
    Continue,
}

/// Aborts or continues `operation`, the one the repo's state shows (OPS-1):
/// `git merge`, `rebase`, `cherry-pick`, `revert` or `am` with `--abort`
/// or `--continue`. A continue that stops on the next conflict leaves the
/// repo in conflict state again.
pub async fn resume(
    git: &GitRunner,
    repo: &Path,
    operation: Operation,
    resume: Resume,
    write: Write<'_>,
) -> Result<Output, GitError> {
    let command = match operation {
        Operation::Merge => "merge",
        Operation::Rebase => "rebase",
        Operation::ApplyMailbox => "am",
        Operation::CherryPick => "cherry-pick",
        Operation::Revert => "revert",
    };
    let flag = match resume {
        Resume::Abort => "--abort",
        Resume::Continue => "--continue",
    };
    integrate_write(git, repo, &[command, flag], write).await
}
