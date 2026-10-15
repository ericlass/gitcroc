//! Tag writes (section 6 "Tags", M2-12). The core checks every name
//! against git's ref format rules first (SEC-4); `--end-of-options` keeps
//! a name that starts with `-` from being read as an option all the same.
//! A tag is pushed and deleted on a remote by its full ref name, so a
//! branch of the same name is never meant.

use std::path::Path;

use super::run_args;
use crate::{GitError, GitRunner, Output, Write};

/// Creates tag `name` at `commit` (`git tag`): annotated with `message`,
/// read from stdin, or lightweight when `None`. An existing tag fails; it
/// is never replaced. Lines of the message that start with `#` are kept
/// (`--cleanup=whitespace`); `tag.gpgSign` applies as on the command line.
pub async fn tag_create(
    git: &GitRunner,
    repo: &Path,
    name: &str,
    commit: &str,
    message: Option<&[u8]>,
    write: Write<'_>,
) -> Result<Output, GitError> {
    let mut args = vec!["tag"];
    if message.is_some() {
        args.extend(["--annotate", "--cleanup=whitespace", "--file=-"]);
    }
    args.extend(["--end-of-options", name, commit]);
    let write = Write {
        stdin: message,
        ..write
    };
    run_args(git, repo, &args, write).await
}

/// Deletes local tag `name` (`git tag -d`).
pub async fn tag_delete(
    git: &GitRunner,
    repo: &Path,
    name: &str,
    write: Write<'_>,
) -> Result<Output, GitError> {
    run_args(git, repo, &["tag", "-d", "--end-of-options", name], write).await
}

/// Pushes tag `name` to remote `remote` under the same name (`git push
/// <remote> refs/tags/<name>`). A remote tag of that name that points
/// elsewhere makes git reject it; it is never replaced.
pub async fn tag_push(
    git: &GitRunner,
    repo: &Path,
    remote: &str,
    name: &str,
    write: Write<'_>,
) -> Result<Output, GitError> {
    let refspec = format!("refs/tags/{name}");
    run_args(
        git,
        repo,
        &["push", "--progress", "--end-of-options", remote, &refspec],
        write,
    )
    .await
}

/// Deletes tag `name` on remote `remote` (`git push <remote> --delete
/// refs/tags/<name>`), a network write. The caller has the user's
/// confirmation.
pub async fn remote_tag_delete(
    git: &GitRunner,
    repo: &Path,
    remote: &str,
    name: &str,
    write: Write<'_>,
) -> Result<Output, GitError> {
    let refspec = format!("refs/tags/{name}");
    run_args(
        git,
        repo,
        &[
            "push",
            "--progress",
            "--delete",
            "--end-of-options",
            remote,
            &refspec,
        ],
        write,
    )
    .await
}
