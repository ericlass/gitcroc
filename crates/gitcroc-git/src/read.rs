//! The read API (ARCH-2): repos opened with `gix`. M1 adds history, diffs and
//! status on top of [`GitRepo`].

use std::path::Path;

/// A repo opened for reading.
#[derive(Debug)]
pub struct GitRepo {
    repo: gix::ThreadSafeRepository,
}

/// Why a repo could not be opened.
#[derive(Debug, thiserror::Error)]
#[error("cannot open the repo at {path}: {source}")]
pub struct OpenError {
    path: String,
    #[source]
    source: gix::Error,
}

impl GitRepo {
    /// Opens the repo whose working tree or git directory is `path`, with the
    /// repo's and the user's git config in effect, as git would.
    pub fn open(path: &Path) -> Result<Self, OpenError> {
        gix::ThreadSafeRepository::open(path)
            .map(|repo| Self { repo })
            .map_err(|source| OpenError {
                path: path.display().to_string(),
                source,
            })
    }

    /// The git directory.
    pub fn git_dir(&self) -> &Path {
        self.repo.git_dir()
    }

    /// The working tree, or `None` for a bare repo.
    pub fn work_dir(&self) -> Option<&Path> {
        self.repo.work_tree.as_deref()
    }
}
