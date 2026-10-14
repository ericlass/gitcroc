//! Workspace, group and repo model, job engine, preflight, result reports, config and cache.

mod app_info;
mod changes;
mod commit;
mod config_file;
mod diff;
mod git;
mod history;
mod job;
mod log;
mod settings;
mod stage;
mod status;
mod summary;
mod tree;
mod watch;
mod workspace;

pub use app_info::AppInfo;
pub use changes::{
    Changes, ChangesContent, ChangesError, ChangesTarget, ChangesView, DiffRequest, FileList,
    FileRef, ImageSide, changes_get, diff_get, diff_image,
};
pub use commit::{CommitMessage, CommitRequest, changes_commit};
pub use config_file::ConfigWarning;
pub use diff::{DiffReadError, changed_files, commit_details, file_diff};
pub use git::{GitSetup, GitStatus};
pub use gitcroc_git::{
    AheadBehind, BranchUpstream, ChangedFile, CommitDetails, CommitRow, Conflict, ContentKey,
    Context, Cursor, DiffError, DiffOptions, DiffRead, DiffSource, Edge, Engine, Fallback,
    FileDiff, FileStatus, Filter, GitError, GitRepo, GitRunner, GraphRow, Head, HistoryError,
    HistoryStart, Hunk, HunkPatch, Image, ImageFormat, Line, LineKind, LocalBranch,
    MAX_COUNTED_FILES, MAX_DIFF_LINES, MAX_IMAGE_BYTES, ModeChange, OpenError, Operation,
    PAGE_SIZE, Page, Person, RefKind, RefLabel, Refs, RefsError, RefsRead, RemoteBranches,
    RepoRead, RepoState, RepoSummary, SearchEnv, StashEntry, StatusError, StatusRead, StatusReader,
    Tag, TextDiff, Time, UnmergedFile, Upstream, WorkingCopy,
};
pub use gitcroc_git::{
    Askpass, CommandRecord, Exit, Kill, Output, RefSnapshot, log_commands_to, ops,
};
pub use history::{
    COMMIT_GRAPH_THRESHOLD, CommitGraphError, CommitGraphWrite, Histories, HistoryPageError,
    HistoryTarget, HistoryView, HistoryViewError, commit_graph_write, history_get, history_more,
    history_page,
};
pub use job::{
    AskpassGrant, AskpassIssuer, CancelResult, JobError, JobEvents, JobHandle, JobId, JobIds,
    JobProgress, JobRun, JobState, JobView, Jobs, MAX_ENDED_JOBS, MAX_OUTPUT, NothingToDo,
    OnCancel, Outcome, POOL_SIZE, Pool, Priority, SkipReason, TrackedJob, Worker, WorkspaceId,
    WriteJob,
};
pub use log::{AppLog, LOG_FILE, LOG_FILE_SIZE, LOG_FILES, LogDir, PlatformLogDir};
pub use settings::{
    Appearance, ChangesSettings, CodeTheme, ConfigDir, DEFAULT_FONT_SIZE, DiffMode, FileListMode,
    MAX_FONT_SIZE, MIN_FONT_SIZE, PlatformConfigDir, SETTINGS_FILE, SETTINGS_SCHEMA, Settings,
    SettingsError, SettingsStore, SettingsView, Theme,
};
pub use stage::{ChangeAction, ChangeRequest, ChangeStart, ChangeTarget, changes_apply};
pub use status::{StatusReadError, repo_state, repo_summary, working_copy};
pub use summary::{
    CACHE_FILE, CACHE_SCHEMA, CacheDir, PlatformCacheDir, REFRESH_READS, Summaries, SummaryCache,
    SummaryView, refresh_summaries, refresh_workspace,
};
pub use tree::{TreeError, TreeItem, TreeNode, Trees, UNGROUPED_ID, tree_filter, tree_refs};
pub use watch::{DEBOUNCE, MAX_DELAY, RepoChange, WatchWarning, WatchedPath, Watchers};
pub use workspace::{
    AddReport, DEFAULT_WORKSPACE_NAME, Dock, PaneLayout, Refusal, RepoId, RepoView, SCAN_DEPTH,
    WORKSPACE_SCHEMA, WORKSPACES_DIR, WorkspaceError, WorkspaceView, Workspaces, add_path,
};

/// Errors returned by `gitcroc-core`.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
}
