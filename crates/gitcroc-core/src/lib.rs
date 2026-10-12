//! Workspace, group and repo model, job engine, preflight, result reports, config and cache.

mod app_info;
mod config_file;
mod diff;
mod git;
mod history;
mod job;
mod settings;
mod status;
mod summary;
mod tree;
mod workspace;

pub use app_info::AppInfo;
pub use config_file::ConfigWarning;
pub use diff::{DiffReadError, changed_files, commit_details, file_diff};
pub use git::{GitSetup, GitStatus};
pub use gitcroc_git::{
    AheadBehind, BranchUpstream, ChangedFile, CommitDetails, CommitRow, Conflict, Context, Cursor,
    DiffError, DiffOptions, DiffRead, DiffSource, Edge, Engine, Fallback, FileDiff, FileStatus,
    Filter, GitError, GitRepo, GitRunner, GraphRow, Head, HistoryError, HistoryStart, Hunk, Image,
    ImageFormat, Line, LineKind, LocalBranch, MAX_COUNTED_FILES, MAX_DIFF_LINES, MAX_IMAGE_BYTES,
    ModeChange, OpenError, Operation, PAGE_SIZE, Page, Person, RefKind, RefLabel, Refs, RefsError,
    RefsRead, RemoteBranches, RepoRead, RepoState, RepoSummary, SearchEnv, StashEntry, StatusError,
    StatusRead, StatusReader, Tag, TextDiff, Time, UnmergedFile, Upstream, WorkingCopy,
};
pub use history::{
    COMMIT_GRAPH_THRESHOLD, CommitGraphError, Histories, HistoryPageError, HistoryTarget,
    HistoryView, HistoryViewError, commit_graph_begin, commit_graph_write, history_get,
    history_more, history_page,
};
pub use job::{JobId, JobIds, JobProgress, WorkspaceId};
pub use settings::{
    ConfigDir, PlatformConfigDir, SETTINGS_FILE, SETTINGS_SCHEMA, Settings, SettingsError,
    SettingsStore, SettingsView, Theme,
};
pub use status::{StatusReadError, repo_state, repo_summary, working_copy};
pub use summary::{
    CACHE_FILE, CACHE_SCHEMA, CacheDir, MAX_REFRESHES, PlatformCacheDir, Summaries, SummaryCache,
    SummaryView, refresh_summaries,
};
pub use tree::{TreeError, TreeItem, TreeNode, Trees, UNGROUPED_ID, tree_filter, tree_refs};
pub use workspace::{
    AddReport, DEFAULT_WORKSPACE_NAME, Refusal, RepoId, RepoView, SCAN_DEPTH, WORKSPACE_SCHEMA,
    WORKSPACES_DIR, WorkspaceError, WorkspaceView, Workspaces, add_path,
};

/// Errors returned by `gitcroc-core`.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
}
