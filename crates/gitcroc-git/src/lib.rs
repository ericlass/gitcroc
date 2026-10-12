//! Git access: read API (gix), git CLI runner, output parsers and conflict state detection.

mod detect;
mod diff;
mod history;
mod probe;
mod read;
mod refs;
mod runner;
mod status;

pub use detect::{
    DetectError, GitInstall, GitVersion, MIN_GIT, SearchEnv, candidates, detect, parse_version,
};
pub use diff::{
    ChangedFile, CommitDetails, Context, DiffError, DiffOptions, DiffRead, DiffSource, FileDiff,
    FileStatus, Hunk, Image, ImageFormat, Line, LineKind, MAX_COUNTED_FILES, MAX_DIFF_LINES,
    MAX_IMAGE_BYTES, ModeChange, Person, TextDiff,
};
pub use history::{
    CommitRow, Cursor, Edge, Filter, GraphRow, HistoryError, HistoryStart, PAGE_SIZE, Page,
    RefKind, RefLabel, RepoRead, Time,
};
pub use probe::{InspectError, Inspected, Layout, inspect, layout};
pub use read::{GitRepo, OpenError};
pub use refs::{
    AheadBehind, BranchUpstream, LocalBranch, Refs, RefsError, RefsRead, RemoteBranches,
    StashEntry, Tag,
};
pub use runner::{GIT_ENV, GitError, GitRunner, Output};
pub use status::{
    Conflict, Engine, Fallback, Head, Operation, RepoState, RepoSummary, StatusError, StatusRead,
    StatusReader, UnmergedFile, Upstream, WorkingCopy,
};

/// Errors returned by `gitcroc-git`.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
}
