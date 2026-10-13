//! Every repo write (ARCH-1). One function per operation builds its
//! argument array (ARCH-3, paths after `--`), runs it through the runner and
//! returns git's output, or a [`GitError`] that carries git's stderr. No
//! other module starts a git process that changes a repo or contacts a
//! remote; `arch_1_writes_go_through_the_runner` checks it. Running one
//! write per repo at a time (ARCH-4) is the job engine's part.

use std::ffi::OsString;
use std::path::Path;

use crate::{GitError, GitRunner, Output, Write};

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
