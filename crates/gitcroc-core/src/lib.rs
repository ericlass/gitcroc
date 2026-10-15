//! Workspace, group and repo model, job engine, preflight, result reports, config and cache.

mod app_info;
mod branch;
mod changes;
mod clone;
mod commit;
mod config_file;
mod conflict;
mod diff;
mod fsmonitor;
mod git;
mod history;
mod integrate;
mod job;
mod log;
mod ref_name;
mod remote;
mod reset;
mod settings;
mod stage;
mod stash;
mod status;
mod summary;
mod sync;
mod tag;
mod tree;
mod unblocked;
mod watch;
mod workspace;

pub use app_info::AppInfo;
pub use branch::{
    BranchChoices, BranchError, BranchOp, BranchRequest, BranchStart, RemoteBranchRef, StartPoint,
    branch_choices, branch_name_check, op_branch,
};
pub use changes::{
    Changes, ChangesContent, ChangesError, ChangesTarget, ChangesView, DiffRequest, FileList,
    FileRef, ImageSide, changes_get, diff_get, diff_image,
};
pub use clone::{
    CloneError, CloneHandle, CloneReport, CloneRequest, clone_default_name, clone_start,
    clone_target_check,
};
pub use commit::{CommitMessage, CommitRequest, changes_commit};
pub use config_file::ConfigWarning;
pub use conflict::{
    ChooseRequest, ConflictChoice, ConflictRead, ConflictStart, ResolveRequest, conflict_choose,
    conflict_get, conflict_resolve,
};
pub use diff::{DiffReadError, changed_files, commit_details, file_diff};
pub use fsmonitor::{FSMONITOR_THRESHOLD, FsmonitorError, fsmonitor_enable};
pub use git::{GitSetup, GitStatus};
pub use gitcroc_git::{
    AheadBehind, BranchUpstream, ChangedFile, ChoiceKind, CommitDetails, CommitRow, Conflict,
    ConflictChunk, ConflictContent, ConflictDetails, ConflictError, ConflictLabels, ContentKey,
    Context, Cursor, DiffError, DiffOptions, DiffRead, DiffSource, Edge, Engine, Fallback,
    FileDiff, FileStatus, Filter, GitError, GitRepo, GitRunner, GraphRow, Head, HistoryError,
    HistoryStart, Hunk, HunkPatch, Image, ImageFormat, Line, LineKind, LocalBranch,
    MAX_CONFLICT_TEXT, MAX_COUNTED_FILES, MAX_DIFF_LINES, MAX_IMAGE_BYTES, ModeChange, OpenError,
    Operation, PAGE_SIZE, Page, Person, RefKind, RefLabel, Refs, RefsError, RefsRead,
    RemoteBranches, RepoRead, RepoState, RepoSummary, SearchEnv, SideHolds, StashEntry,
    StatusError, StatusRead, StatusReader, Step, Tag, TextDiff, Time, UnmergedFile, UnmergedKind,
    Upstream, WorkingCopy,
};
pub use gitcroc_git::{
    Askpass, CommandRecord, Exit, Kill, Output, RefSnapshot, log_commands_to, ops,
};
pub use history::{
    COMMIT_GRAPH_THRESHOLD, CommitGraphError, CommitGraphWrite, Histories, HistoryPageError,
    HistoryTarget, HistoryView, HistoryViewError, commit_graph_write, history_get, history_more,
    history_page,
};
pub use integrate::{
    IntegrateError, IntegrateOp, IntegrateRequest, IntegrateStart, MAX_MERGE_TARGETS,
    MergeFastForward, conflict_details, merge_targets, op_integrate,
};
pub use job::{
    AskpassGrant, AskpassIssuer, CancelResult, JobError, JobEvents, JobHandle, JobId, JobIds,
    JobProgress, JobRun, JobState, JobView, Jobs, MAX_ENDED_JOBS, MAX_OUTPUT, NothingToDo,
    OnCancel, Outcome, POOL_SIZE, Pool, Priority, SkipReason, TrackedJob, Worker, WorkspaceId,
    WriteJob,
};
pub use log::{AppLog, LOG_FILE, LOG_FILE_SIZE, LOG_FILES, LogDir, PlatformLogDir};
pub use ref_name::{RefNameError, check_branch_name, check_remote_name, check_tag_name};
pub use remote::{
    RemoteError, RemoteOp, RemoteRequest, RemoteStart, RemoteUrlsView, op_remote,
    remote_name_check, remote_urls,
};
pub use reset::{ResetError, ResetMode, ResetRequest, ResetStart, op_reset};
pub use settings::{
    Appearance, ChangesSettings, CodeTheme, ConfigDir, DEFAULT_FONT_SIZE, DiffMode, FileListMode,
    MAX_FONT_SIZE, MIN_FONT_SIZE, PlatformConfigDir, SETTINGS_FILE, SETTINGS_SCHEMA, Session,
    Settings, SettingsError, SettingsStore, SettingsView, Theme,
};
pub use stage::{ChangeAction, ChangeRequest, ChangeStart, ChangeTarget, changes_apply};
pub use stash::{StashError, StashOp, StashRef, StashRequest, StashStart, op_stash};
pub use status::{StatusReadError, repo_state, repo_summary, working_copy};
pub use summary::{
    CACHE_FILE, CACHE_SCHEMA, CacheDir, PlatformCacheDir, REFRESH_READS, Summaries, SummaryCache,
    SummaryView, refresh_summaries, refresh_workspace,
};
pub use sync::{
    Prune, PullStrategy, PushChoices, SyncError, SyncOp, SyncRequest, SyncStart, op_sync,
    push_choices,
};
pub use tag::{TagError, TagOp, TagRequest, TagStart, op_tag, tag_name_check};
pub use tree::{TreeError, TreeItem, TreeNode, Trees, UNGROUPED_ID, tree_filter, tree_refs};
pub use watch::{DEBOUNCE, MAX_DELAY, RepoChange, WatchWarning, WatchedPath, Watchers};
pub use workspace::{
    AddReport, DEFAULT_WORKSPACE_NAME, Dock, MAX_WORKSPACE_NAME, PaneLayout, Refusal, RepoId,
    RepoView, SCAN_DEPTH, WORKSPACE_SCHEMA, WORKSPACES_DIR, WorkspaceError, WorkspaceListItem,
    WorkspaceTabs, WorkspaceView, Workspaces, add_path,
};

/// Errors returned by `gitcroc-core`.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
}
