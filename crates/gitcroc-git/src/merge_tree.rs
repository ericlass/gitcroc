//! The conflict prediction of a group merge or rebase (GRP-13): git's
//! `merge-tree --write-tree` merges two commits in memory and names the
//! files that would conflict. It writes objects to the object store only,
//! never a ref, the index or a working file, so like the commit-graph file
//! of M1 it counts as a read and runs outside `ops` (ARCH-1). Its options
//! are those git 2.40 has (D13).

use std::ffi::OsString;
use std::path::Path;

use crate::{GitError, GitRunner};

/// What a merge of two commits would do.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MergePrediction {
    /// It merges without a conflict.
    Clean,
    /// It stops in conflicts in these files, each named once. Empty when
    /// git reports a conflict without a file, as some directory renames do.
    Conflicts(Vec<String>),
}

/// Merges commits `ours` and `theirs`, full hex ids, in memory in the repo
/// at `path`, and tells whether it would conflict. Runs git, so call it
/// from a thread that may block.
pub fn predict_merge(
    git: &GitRunner,
    path: &Path,
    ours: &str,
    theirs: &str,
) -> Result<MergePrediction, GitError> {
    let args: Vec<OsString> = [
        "merge-tree",
        "--write-tree",
        "--name-only",
        "-z",
        "--no-messages",
        ours,
        theirs,
    ]
    .into_iter()
    .map(OsString::from)
    .collect();
    match git.run_blocking(&args, Some(path)) {
        Ok(_) => Ok(MergePrediction::Clean),
        // Exit status 1 is a merge with conflicts; any other is an error.
        Err(GitError::Failed {
            code: Some(1),
            stdout,
            ..
        }) => Ok(MergePrediction::Conflicts(conflicted(&stdout))),
        Err(error) => Err(error),
    }
}

/// The conflicted files of `merge-tree`'s output with `-z` and
/// `--name-only`: the tree's id, then each file, every one ending in NUL.
fn conflicted(stdout: &[u8]) -> Vec<String> {
    stdout
        .split(|&byte| byte == 0)
        .skip(1)
        .take_while(|name| !name.is_empty())
        .map(|name| String::from_utf8_lossy(name).into_owned())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn grp_13_reads_the_conflicted_files() {
        let oid = "1e071d5b6cd376be9717b18883c366e455839299";
        assert_eq!(
            conflicted(format!("{oid}\0a.txt\0dir/b c.txt\0").as_bytes()),
            ["a.txt", "dir/b c.txt"]
        );
        assert!(conflicted(format!("{oid}\0").as_bytes()).is_empty());
        // Messages, were they on, start after an empty record.
        assert_eq!(
            conflicted(format!("{oid}\0a.txt\0\01\0a.txt\0CONFLICT\0").as_bytes()),
            ["a.txt"]
        );
    }
}
