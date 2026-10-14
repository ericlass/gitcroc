//! Every repo write (ARCH-1). One function per operation builds its
//! argument array (ARCH-3, paths after `--` or as a pathspec file on
//! stdin), runs it through the runner and
//! returns git's output, or a [`GitError`] that carries git's stderr. No
//! other module starts a git process that changes a repo or contacts a
//! remote; `arch_1_writes_go_through_the_runner` checks it. Running one
//! write per repo at a time (ARCH-4) is the job engine's part.

use std::ffi::OsString;
use std::path::{Path, PathBuf};

use crate::{GitError, GitRunner, Output, Write};

mod branch;
pub mod conflict;
mod integrate;
mod stash;
mod sync;

pub use branch::{
    branch_create, branch_delete, branch_rename, branch_set_upstream, remote_branch_delete, switch,
    switch_detach, switch_track,
};
pub use integrate::{FastForward, Resume, cherry_pick, merge, rebase, resume, revert};
pub use stash::{stash_apply, stash_drop, stash_push};
pub use sync::{PullMode, PushTo, fetch_all, fetch_remote, pull_fetched, push};

/// Writes the repo's commit-graph file from every ref (PERF-2, M1-08). Only
/// the cache file in the git directory changes: no ref, object, index,
/// working file or config.
pub async fn commit_graph_write(
    git: &GitRunner,
    repo: &Path,
    write: Write<'_>,
) -> Result<Output, GitError> {
    let args = ["commit-graph", "write", "--reachable"].map(OsString::from);
    git.run_write(&args, repo, write).await
}

/// What a patch does to the working copy (UI-15).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Apply {
    /// A hunk of the working tree's diff goes into the index.
    Stage,
    /// A hunk of the index's diff is taken out of the index.
    Unstage,
    /// A hunk of the working tree's diff is undone in the working tree.
    Discard,
}

/// Paths for `--pathspec-from-file=-`: each as git stores it, ended by NUL.
fn pathspec_input(paths: &[&[u8]]) -> Vec<u8> {
    let mut input = Vec::new();
    for path in paths {
        input.extend_from_slice(path);
        input.push(0);
    }
    input
}

/// Runs `args`, then git reads `paths` as literal pathspecs from stdin, so
/// no name is taken for a pattern and no list is too long for a command
/// line.
async fn with_paths(
    git: &GitRunner,
    repo: &Path,
    args: &[&str],
    paths: &[&[u8]],
    write: Write<'_>,
) -> Result<Output, GitError> {
    let mut all = vec![OsString::from("--literal-pathspecs")];
    all.extend(args.iter().map(OsString::from));
    all.extend(["--pathspec-from-file=-", "--pathspec-file-nul"].map(OsString::from));
    let input = pathspec_input(paths);
    let write = Write {
        stdin: Some(&input),
        ..write
    };
    git.run_write(&all, repo, write).await
}

/// Stages `paths` as they are in the working tree, deletions included
/// (`git add -A`).
pub async fn stage(
    git: &GitRunner,
    repo: &Path,
    paths: &[&[u8]],
    write: Write<'_>,
) -> Result<Output, GitError> {
    with_paths(git, repo, &["add", "-A"], paths, write).await
}

/// Takes `paths` out of the index, back to `HEAD`; on a branch with no
/// commit yet (`unborn`), out of the index altogether.
pub async fn unstage(
    git: &GitRunner,
    repo: &Path,
    paths: &[&[u8]],
    unborn: bool,
    write: Write<'_>,
) -> Result<Output, GitError> {
    let args: &[&str] = if unborn {
        &["rm", "--cached", "--quiet"]
    } else {
        &["restore", "--staged"]
    };
    with_paths(git, repo, args, paths, write).await
}

/// Puts tracked `paths` in the working tree back to what the index holds.
/// Destructive (GRP-9): the caller has the user's confirmation.
pub async fn discard_tracked(
    git: &GitRunner,
    repo: &Path,
    paths: &[&[u8]],
    write: Write<'_>,
) -> Result<Output, GitError> {
    with_paths(git, repo, &["restore", "--worktree"], paths, write).await
}

/// The most bytes of paths one `git clean` gets, well under the shortest
/// command line limit (Windows: 32,767 characters).
const CLEAN_ARGS_BYTES: usize = 16 * 1024;

/// Deletes untracked `paths`, files and folders (`git clean -f -d`), in
/// several runs when the list is long; `git clean` reads no pathspec file.
/// Destructive (GRP-9): the caller has the user's confirmation.
pub async fn discard_untracked(
    git: &GitRunner,
    repo: &Path,
    paths: &[PathBuf],
    write: Write<'_>,
) -> Result<Output, GitError> {
    let mut all = Output {
        stdout: Vec::new(),
        stderr: Vec::new(),
    };
    let mut rest = paths;
    while !rest.is_empty() {
        let mut bytes = 0;
        let take = rest
            .iter()
            .take_while(|path| {
                bytes += path.as_os_str().len() + 1;
                bytes <= CLEAN_ARGS_BYTES
            })
            .count()
            .max(1);
        let (now, later) = rest.split_at(take);
        let mut args = ["--literal-pathspecs", "clean", "-f", "-d", "--"]
            .map(OsString::from)
            .to_vec();
        args.extend(now.iter().map(|path| path.as_os_str().to_owned()));
        let output = git.run_write(&args, repo, write).await?;
        all.stdout.extend(output.stdout);
        all.stderr.extend(output.stderr);
        rest = later;
    }
    Ok(all)
}

/// Applies `patch`, one hunk the core built from its own diff, as `apply`
/// says, with the line counts recounted.
pub async fn apply_patch(
    git: &GitRunner,
    repo: &Path,
    patch: &[u8],
    apply: Apply,
    write: Write<'_>,
) -> Result<Output, GitError> {
    let mut args = vec!["apply", "--recount", "--whitespace=nowarn"];
    match apply {
        Apply::Stage => args.push("--cached"),
        Apply::Unstage => args.extend(["--cached", "-R"]),
        Apply::Discard => args.push("-R"),
    }
    args.push("-");
    let args: Vec<OsString> = args.into_iter().map(OsString::from).collect();
    let write = Write {
        stdin: Some(patch),
        ..write
    };
    git.run_write(&args, repo, write).await
}

/// How a commit is made (UI-15).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CommitMode {
    /// Replace the last commit (`--amend`), its staged changes added.
    pub amend: bool,
    /// Add a `Signed-off-by` trailer (`--signoff`), as `format.signOff`
    /// asks.
    pub sign_off: bool,
}

/// Commits the staged changes with `message`, read from stdin (`git commit
/// -F -`). Hooks, `commit.gpgSign`, the message template and
/// `commit.cleanup` apply as on the command line (SEC-6): nothing is
/// skipped or forced.
pub async fn commit(
    git: &GitRunner,
    repo: &Path,
    message: &[u8],
    mode: CommitMode,
    write: Write<'_>,
) -> Result<Output, GitError> {
    let mut args = vec!["commit"];
    if mode.amend {
        args.push("--amend");
    }
    if mode.sign_off {
        args.push("--signoff");
    }
    args.extend(["-F", "-"]);
    let args: Vec<OsString> = args.into_iter().map(OsString::from).collect();
    let write = Write {
        stdin: Some(message),
        ..write
    };
    git.run_write(&args, repo, write).await
}
