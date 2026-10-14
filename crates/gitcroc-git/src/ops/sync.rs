//! Fetch, pull and push (section 6 "Sync", M2-07): network writes, each
//! with `--progress` so its stderr lines report the transfer. Nothing is
//! stashed to make one succeed (GRP-12): `--no-autostash` wins over
//! `rebase.autoStash` and `merge.autoStash`.

use std::ffi::OsString;
use std::path::Path;

use crate::{GitError, GitRunner, Output, Write};

async fn sync_write(
    git: &GitRunner,
    repo: &Path,
    args: &[&str],
    write: Write<'_>,
) -> Result<Output, GitError> {
    let args: Vec<OsString> = args.iter().map(OsString::from).collect();
    git.run_write(&args, repo, write).await
}

/// Fetches every remote (`git fetch --all`). `prune` removes remote-tracking
/// branches whose branch is gone (`--prune`) or keeps them (`--no-prune`);
/// `None` follows `fetch.prune` and `remote.<name>.prune`.
pub async fn fetch_all(
    git: &GitRunner,
    repo: &Path,
    prune: Option<bool>,
    write: Write<'_>,
) -> Result<Output, GitError> {
    let mut args = vec!["fetch", "--all", "--progress"];
    match prune {
        Some(true) => args.push("--prune"),
        Some(false) => args.push("--no-prune"),
        None => {}
    }
    sync_write(git, repo, &args, write).await
}

/// Fetches remote `remote` as its config says (`git fetch <remote>`).
pub async fn fetch_remote(
    git: &GitRunner,
    repo: &Path,
    remote: &str,
    write: Write<'_>,
) -> Result<Output, GitError> {
    sync_write(
        git,
        repo,
        &["fetch", "--progress", "--end-of-options", remote],
        write,
    )
    .await
}

/// How a pull brings in its upstream.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PullMode {
    /// As `branch.<name>.rebase`, `pull.rebase` and `pull.ff` say; with
    /// none set, git fast-forwards only.
    Config,
    /// Fast-forward, or fail.
    FfOnly,
    /// Fast-forward when possible, else a merge commit.
    Merge,
    Rebase,
}

/// Updates the current branch from `tracking`, a remote-tracking branch
/// such as `refs/remotes/origin/main` fetched before (`git pull . <ref>`),
/// so nothing here contacts a remote. Local changes git would overwrite
/// make it fail; a merge or rebase that stops on conflicts leaves the repo
/// in conflict state.
pub async fn pull_fetched(
    git: &GitRunner,
    repo: &Path,
    tracking: &str,
    mode: PullMode,
    write: Write<'_>,
) -> Result<Output, GitError> {
    let mut args = vec!["pull", "--no-autostash"];
    match mode {
        PullMode::Config => {}
        PullMode::FfOnly => args.push("--ff-only"),
        PullMode::Merge => args.extend(["--no-rebase", "--ff"]),
        PullMode::Rebase => args.push("--rebase"),
    }
    args.extend(["--end-of-options", ".", tracking]);
    sync_write(git, repo, &args, write).await
}

/// What a push sends where.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PushTo<'a> {
    pub remote: &'a str,
    /// The local branch, without `refs/heads/`.
    pub branch: &'a str,
    /// Its name on the remote, without `refs/heads/`.
    pub remote_branch: &'a str,
    /// Makes `remote_branch` the branch's upstream (`-u`).
    pub set_upstream: bool,
    /// Replaces the remote branch even when the push is not a fast-forward,
    /// as long as it is where its remote-tracking branch says
    /// (`--force-with-lease`). Destructive (GRP-9): the caller has the
    /// user's confirmation.
    pub force_with_lease: bool,
}

/// Pushes a branch (`git push <remote> refs/heads/<branch>:refs/heads/<remote
/// branch>`). A push that is not a fast-forward fails unless forced.
pub async fn push(
    git: &GitRunner,
    repo: &Path,
    to: PushTo<'_>,
    write: Write<'_>,
) -> Result<Output, GitError> {
    let mut args = vec!["push", "--progress"];
    if to.set_upstream {
        args.push("--set-upstream");
    }
    if to.force_with_lease {
        args.push("--force-with-lease");
    }
    let refspec = format!("refs/heads/{}:refs/heads/{}", to.branch, to.remote_branch);
    args.extend(["--end-of-options", to.remote, &refspec]);
    sync_write(git, repo, &args, write).await
}
