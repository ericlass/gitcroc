//! Git access: read API (gix), git CLI runner, output parsers and conflict state detection.

mod branches;
mod commits;
mod conflict;
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

pub use branches::{RemoteUrls, TrackedBranch};
pub use conflict::{
    ChoiceKind, Conflict, ConflictChunk, ConflictContent, ConflictError, MARKER_SIZE,
    MAX_CONFLICT_TEXT, SideHolds, StageEntry, first_marker_line,
};
pub use detect::{
    DetectError, GitInstall, GitVersion, MIN_GIT, SearchEnv, candidates, detect, parse_version,
};
pub use diff::{
    ChangedFile, CommitDetails, ContentKey, Context, DiffError, DiffOptions, DiffRead, DiffSource,
    FileDiff, FileStatus, Hunk, HunkPatch, Image, ImageFormat, Line, LineKind, MAX_COUNTED_FILES,
    MAX_DIFF_LINES, MAX_IMAGE_BYTES, ModeChange, Person, TextDiff,
};
pub use history::{
    CommitRow, Cursor, Edge, Filter, GraphRow, HistoryError, HistoryStart, MergedCursor,
    MergedPage, MergedRow, PAGE_SIZE, Page, RefKind, RefLabel, RepoRead, Time, merged_history,
};
pub use probe::{InspectError, Inspected, Layout, inspect, layout};
pub use progress::{Progress, parse_progress};
pub use read::{GitRepo, OpenError};
pub use refs::{
    AheadBehind, BranchUpstream, LocalBranch, RefSnapshot, Refs, RefsError, RemoteBranches,
    StashEntry, Tag,
};
pub use runner::{
    ASKPASS_VARS, Askpass, CommandRecord, Exit, GIT_ENV, GitError, GitRunner, Kill, Output,
    WRITE_ENV, Write, log_commands_to,
};
pub use status::{
    ConflictDetails, ConflictLabels, Engine, Fallback, FsmonitorState, Head, Operation, RepoState,
    RepoSummary, StatusError, StatusRead, StatusReader, Step, UnmergedFile, UnmergedKind, Upstream,
    WorkingCopy,
};
