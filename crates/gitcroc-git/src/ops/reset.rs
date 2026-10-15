//! Reset of the current branch to a commit (section 6 "Reset", M2-12).

use std::path::Path;

use super::run_args;
use crate::{GitError, GitRunner, Output, Write};

/// What a reset puts at the commit besides the branch.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResetMode {
    /// The branch only; the index and the working tree stay.
    Soft,
    /// The branch and the index; the working tree stays.
    Mixed,
    /// The branch, the index and the working tree: uncommitted changes to
    /// tracked files are lost. Destructive (GRP-9): the caller has the
    /// user's confirmation.
    Hard,
}

/// Points the current branch, or a detached `HEAD`, at `commit`, a full
/// hash (`git reset --soft|--mixed|--hard`). A hash never starts with `-`,
/// and `--` keeps it from being read as a path; git 2.43's reset refuses
/// `--end-of-options` before it.
pub async fn reset(
    git: &GitRunner,
    repo: &Path,
    commit: &str,
    mode: ResetMode,
    write: Write<'_>,
) -> Result<Output, GitError> {
    let flag = match mode {
        ResetMode::Soft => "--soft",
        ResetMode::Mixed => "--mixed",
        ResetMode::Hard => "--hard",
    };
    run_args(git, repo, &["reset", "--quiet", flag, commit, "--"], write).await
}
