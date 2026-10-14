//! Branch writes (section 6 "Branches", M2-06). The core checks every name
//! against git's ref format rules first (SEC-4); `--end-of-options` keeps
//! a name that starts with `-` from being read as an option all the same.
//! Local changes git refuses to carry make a checkout fail with git's
//! message; nothing is stashed (GRP-12).

use std::ffi::OsString;
use std::path::Path;

use crate::{GitError, GitRunner, Output, Write};

async fn branch_write(
    git: &GitRunner,
    repo: &Path,
    args: &[&str],
    write: Write<'_>,
) -> Result<Output, GitError> {
    let args: Vec<OsString> = args.iter().map(OsString::from).collect();
    git.run_write(&args, repo, write).await
}

/// Checks out local branch `branch` (`git switch`). A missing branch fails;
/// git does not guess one from a remote.
pub async fn switch(
    git: &GitRunner,
    repo: &Path,
    branch: &str,
    write: Write<'_>,
) -> Result<Output, GitError> {
    branch_write(
        git,
        repo,
        &["switch", "--no-guess", "--end-of-options", branch],
        write,
    )
    .await
}

/// Creates a local branch that tracks `remote_branch`, such as
/// `origin/feature/x`, under the name without the remote, and checks it
/// out (`git switch --track`).
pub async fn switch_track(
    git: &GitRunner,
    repo: &Path,
    remote_branch: &str,
    write: Write<'_>,
) -> Result<Output, GitError> {
    branch_write(
        git,
        repo,
        &["switch", "--track", "--end-of-options", remote_branch],
        write,
    )
    .await
}

/// Checks out commit `commit` with a detached `HEAD` (`git switch
/// --detach`).
pub async fn switch_detach(
    git: &GitRunner,
    repo: &Path,
    commit: &str,
    write: Write<'_>,
) -> Result<Output, GitError> {
    branch_write(
        git,
        repo,
        &["switch", "--detach", "--end-of-options", commit],
        write,
    )
    .await
}

/// Creates branch `name` at `start`, or at `HEAD` when `None`, and checks
/// it out when `switch` says so (`git switch -c`, else `git branch`). An
/// existing branch fails; it is never reset.
pub async fn branch_create(
    git: &GitRunner,
    repo: &Path,
    name: &str,
    start: Option<&str>,
    switch: bool,
    write: Write<'_>,
) -> Result<Output, GitError> {
    let mut args = if switch {
        vec!["switch", "--create", name, "--end-of-options"]
    } else {
        vec!["branch", "--end-of-options", name]
    };
    args.extend(start);
    branch_write(git, repo, &args, write).await
}

/// Renames branch `old` to `new` (`git branch -m`); an existing `new`
/// fails.
pub async fn branch_rename(
    git: &GitRunner,
    repo: &Path,
    old: &str,
    new: &str,
    write: Write<'_>,
) -> Result<Output, GitError> {
    branch_write(
        git,
        repo,
        &["branch", "-m", "--end-of-options", old, new],
        write,
    )
    .await
}

/// Deletes branch `name`: `git branch -d`, which refuses a branch not
/// merged, or `-D` with `force`. Force is destructive (GRP-9): the caller
/// has the user's confirmation.
pub async fn branch_delete(
    git: &GitRunner,
    repo: &Path,
    name: &str,
    force: bool,
    write: Write<'_>,
) -> Result<Output, GitError> {
    let flag = if force { "-D" } else { "-d" };
    branch_write(
        git,
        repo,
        &["branch", flag, "--end-of-options", name],
        write,
    )
    .await
}

/// Deletes branch `branch` on remote `remote` (`git push --delete`), a
/// network write. Destructive (GRP-9): the caller has the user's
/// confirmation.
pub async fn remote_branch_delete(
    git: &GitRunner,
    repo: &Path,
    remote: &str,
    branch: &str,
    write: Write<'_>,
) -> Result<Output, GitError> {
    branch_write(
        git,
        repo,
        &["push", "--delete", "--end-of-options", remote, branch],
        write,
    )
    .await
}

/// Makes branch `branch` track `upstream`, such as `origin/main`, or
/// track nothing when `None` (`git branch --set-upstream-to`,
/// `--unset-upstream`).
pub async fn branch_set_upstream(
    git: &GitRunner,
    repo: &Path,
    branch: &str,
    upstream: Option<&str>,
    write: Write<'_>,
) -> Result<Output, GitError> {
    let set;
    let mut args = vec!["branch"];
    match upstream {
        Some(upstream) => {
            set = format!("--set-upstream-to={upstream}");
            args.push(&set);
        }
        None => args.push("--unset-upstream"),
    }
    args.extend(["--end-of-options", branch]);
    branch_write(git, repo, &args, write).await
}
