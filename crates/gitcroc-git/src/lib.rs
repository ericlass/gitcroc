//! Git access: read API (gix), git CLI runner, output parsers and conflict state detection.

mod branches;
mod detect;
mod diff;
mod history;
pub mod ops;
mod probe;
mod progress;
mod read;
mod refs;
mod runner;
mod status;

pub use branches::TrackedBranch;
pub use detect::{
    DetectError, GitInstall, GitVersion, MIN_GIT, SearchEnv, candidates, detect, parse_version,
};
pub use diff::{
    ChangedFile, CommitDetails, ContentKey, Context, DiffError, DiffOptions, DiffRead, DiffSource,
    FileDiff, FileStatus, Hunk, HunkPatch, Image, ImageFormat, Line, LineKind, MAX_COUNTED_FILES,
    MAX_DIFF_LINES, MAX_IMAGE_BYTES, ModeChange, Person, TextDiff,
};
pub use history::{
    CommitRow, Cursor, Edge, Filter, GraphRow, HistoryError, HistoryStart, PAGE_SIZE, Page,
    RefKind, RefLabel, RepoRead, Time,
};
pub use probe::{InspectError, Inspected, Layout, inspect, layout};
pub use progress::{Progress, parse_progress};
pub use read::{GitRepo, OpenError};
pub use refs::{
    AheadBehind, BranchUpstream, LocalBranch, RefSnapshot, Refs, RefsError, RefsRead,
    RemoteBranches, StashEntry, Tag,
};
pub use runner::{
    ASKPASS_VARS, Askpass, CommandRecord, Exit, GIT_ENV, GitError, GitRunner, Kill, Output,
    WRITE_ENV, Write, log_commands_to,
};
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
