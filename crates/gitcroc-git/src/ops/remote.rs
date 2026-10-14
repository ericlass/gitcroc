//! Remote config writes (section 6 "Remotes", M2-13). The core checks a new
//! name against git's ref format rules and refuses a URL that starts with
//! `-` first (SEC-4); `--end-of-options` keeps either from being read as an
//! option all the same.

use std::ffi::OsString;
use std::path::Path;

use crate::{GitError, GitRunner, Output, Write};

async fn remote_write(
    git: &GitRunner,
    repo: &Path,
    args: &[&str],
    write: Write<'_>,
) -> Result<Output, GitError> {
    let args: Vec<OsString> = args.iter().map(OsString::from).collect();
    git.run_write(&args, repo, write).await
}

/// Adds remote `name` at `url` (`git remote add`), without fetching it. An
/// existing name fails.
pub async fn remote_add(
    git: &GitRunner,
    repo: &Path,
    name: &str,
    url: &str,
    write: Write<'_>,
) -> Result<Output, GitError> {
    let args = ["remote", "add", "--end-of-options", name, url];
    remote_write(git, repo, &args, write).await
}

/// Renames remote `name` to `new_name` (`git remote rename`); git moves its
/// remote-tracking branches and the upstreams that name it.
pub async fn remote_rename(
    git: &GitRunner,
    repo: &Path,
    name: &str,
    new_name: &str,
    write: Write<'_>,
) -> Result<Output, GitError> {
    let args = ["remote", "rename", "--end-of-options", name, new_name];
    remote_write(git, repo, &args, write).await
}

/// Removes remote `name` (`git remote remove`) with its remote-tracking
/// branches; the upstreams that name it are unset. The caller has the
/// user's confirmation.
pub async fn remote_remove(
    git: &GitRunner,
    repo: &Path,
    name: &str,
    write: Write<'_>,
) -> Result<Output, GitError> {
    let args = ["remote", "remove", "--end-of-options", name];
    remote_write(git, repo, &args, write).await
}

/// Sets the first URL of remote `name` (`git remote set-url`): the fetch
/// URL, or with `push` the push URL.
pub async fn remote_set_url(
    git: &GitRunner,
    repo: &Path,
    name: &str,
    url: &str,
    push: bool,
    write: Write<'_>,
) -> Result<Output, GitError> {
    let mut args = vec!["remote", "set-url"];
    if push {
        args.push("--push");
    }
    args.extend(["--end-of-options", name, url]);
    remote_write(git, repo, &args, write).await
}
