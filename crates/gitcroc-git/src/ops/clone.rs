//! Clone (section 6 "Repos", M2-14): a network write with `--progress`, so
//! its stderr lines report the transfer. The core checks the URL and the
//! target folder first; `--` keeps either from being read as an option all
//! the same.

use std::ffi::OsString;
use std::path::Path;

use crate::{GitError, GitRunner, Output, Write};

/// Clones `url` into `target` (`git clone`), run from `parent`, the folder
/// `target` is in. `target` must not exist, or be an empty folder.
pub async fn clone(
    git: &GitRunner,
    parent: &Path,
    url: &str,
    target: &Path,
    write: Write<'_>,
) -> Result<Output, GitError> {
    let mut args = ["clone", "--progress", "--"].map(OsString::from).to_vec();
    args.push(url.into());
    args.push(target.into());
    git.run_write(&args, parent, write).await
}
