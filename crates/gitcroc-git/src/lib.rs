//! Git access: read API (gix), git CLI runner, output parsers and conflict state detection.

mod detect;
mod history;
mod read;
mod runner;

pub use detect::{
    DetectError, GitInstall, GitVersion, MIN_GIT, SearchEnv, candidates, detect, parse_version,
};
pub use history::{
    CommitRow, Cursor, Edge, Filter, GraphRow, HistoryError, HistoryStart, PAGE_SIZE, Page,
    RefKind, RefLabel, RepoRead, Time,
};
pub use read::{GitRepo, OpenError};
pub use runner::{GIT_ENV, GitError, GitRunner, Output};

/// Errors returned by `gitcroc-git`.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
}
