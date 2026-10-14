//! Stash, apply, pop and drop in one repo (section 6 "Stash", M2-11). An
//! entry is named `stash@{n}`; the caller checked that it is still the one
//! the user picked. A stash applied with conflicts leaves unmerged paths,
//! the conflict state of a stash apply (OPS-1); its abort is
//! [`super::resume`].

use std::ffi::OsString;
use std::path::Path;

use crate::{GitError, GitRunner, Output, Write};

async fn stash_write(
    git: &GitRunner,
    repo: &Path,
    args: &[&str],
    write: Write<'_>,
) -> Result<Output, GitError> {
    let args: Vec<OsString> = args.iter().map(OsString::from).collect();
    git.run_write(&args, repo, write).await
}

/// `stash@{index}`.
fn entry(index: u32) -> String {
    format!("stash@{{{index}}}")
}

/// Stashes the working copy's changes (`git stash push`), with `message`
/// when given and the untracked files too when `untracked`. A working copy
/// with nothing to stash changes no ref.
pub async fn stash_push(
    git: &GitRunner,
    repo: &Path,
    message: Option<&str>,
    untracked: bool,
    write: Write<'_>,
) -> Result<Output, GitError> {
    let mut args = vec!["stash", "push"];
    if let Some(message) = message {
        args.extend(["-m", message]);
    }
    if untracked {
        args.push("--include-untracked");
    }
    stash_write(git, repo, &args, write).await
}

/// Applies `stash@{index}` to the working copy (`git stash apply`), or
/// with `pop` also drops it, which git does only when it applied without
/// conflicts. Local changes the stash would overwrite make git refuse.
pub async fn stash_apply(
    git: &GitRunner,
    repo: &Path,
    index: u32,
    pop: bool,
    write: Write<'_>,
) -> Result<Output, GitError> {
    let entry = entry(index);
    let verb = if pop { "pop" } else { "apply" };
    stash_write(git, repo, &["stash", verb, &entry], write).await
}

/// Drops `stash@{index}` (`git stash drop`). Cannot be undone from the
/// app: the caller has the user's confirmation.
pub async fn stash_drop(
    git: &GitRunner,
    repo: &Path,
    index: u32,
    write: Write<'_>,
) -> Result<Output, GitError> {
    let entry = entry(index);
    stash_write(git, repo, &["stash", "drop", &entry], write).await
}
