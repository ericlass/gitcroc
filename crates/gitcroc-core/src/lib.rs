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
mod group;
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
mod tabs;
mod tag;
mod target;
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
    FileRef, ImageSide, RepoChanges, changes_get, changes_get_merged, diff_get, diff_image,
};
pub use clone::{
    CloneError, CloneGroup, CloneHandle, CloneReport, CloneRequest, clone_default_name,
    clone_start, clone_target_check,
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
    Askpass, CommandRecord, Exit, Kill, Output, RefSnapshot, log_commands_to, ops,
};
pub use gitcroc_git::{
    ChangedFile, ChoiceKind, CommitDetails, CommitRow, Conflict, ConflictContent, ConflictDetails,
    ConflictError, ContentKey, Context, Cursor, DiffError, DiffOptions, DiffRead, DiffSource,
    Engine, Fallback, FileDiff, FileStatus, Filter, GitError, GitRepo, GitRunner, Head,
    HistoryError, HistoryStart, Hunk, HunkPatch, Image, Line, LocalBranch, MergedCursor, MergedRow,
    OpenError, Operation, PAGE_SIZE, Page, Refs, RefsError, RemoteBranches, RepoRead, RepoState,
    RepoSummary, SearchEnv, SideHolds, StashEntry, StatusError, StatusRead, StatusReader, Step,
    Tag, TextDiff, UnmergedFile, WorkingCopy, merged_history,
};
pub use group::{
    Class, GroupBranch, GroupError, GroupOp, GroupOperation, GroupOps, GroupSummary, MissingBranch,
    PreflightId, PreflightReady, RepoClass, RunFuture, group_summaries,
};
pub use history::{
    CommitGraphError, CommitGraphWrite, Histories, HistoryPageError, HistoryRow, HistoryTarget,
    HistoryView, HistoryViewError, commit_graph_write, history_get, history_more, history_page,
};
pub use integrate::{
    IntegrateError, IntegrateOp, IntegrateRequest, IntegrateStart, MergeFastForward,
    conflict_details, merge_targets, op_integrate,
};
pub use job::{
    AskpassGrant, AskpassIssuer, CancelResult, GroupRun, JobError, JobEvents, JobHandle, JobId,
    JobIds, JobProgress, JobRun, JobState, JobView, Jobs, MAX_ENDED_JOBS, MAX_OUTPUT, NothingToDo,
    OnCancel, Outcome, POOL_SIZE, Pool, Priority, RepoResult, SkipReason, SubJob, SubJobError,
    SubJobSpec, TrackedJob, Worker, WorkspaceId, WriteJob,
};
pub use log::{AppLog, LOG_FILE, LOG_FILE_SIZE, LOG_FILES, LogDir, PlatformLogDir};
pub use ref_name::RefNameError;
pub use remote::{
    RemoteError, RemoteOp, RemoteRequest, RemoteStart, RemoteUrlsView, op_remote,
    remote_name_check, remote_urls,
};
pub use reset::{ResetError, ResetMode, ResetRequest, ResetStart, op_reset};
pub use settings::{
    Appearance, ChangesSettings, CodeTheme, ConfigDir, DEFAULT_FONT_SIZE, DiffMode, FileListMode,
    GroupSettings, MAX_FONT_SIZE, MAX_GROUP_PARALLELISM, MIN_FONT_SIZE, MIN_GROUP_PARALLELISM,
    PlatformConfigDir, SETTINGS_FILE, SETTINGS_SCHEMA, Session, Settings, SettingsError,
    SettingsStore, SettingsView, Theme,
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
pub use tabs::TabSwitch;
pub use tag::{TagError, TagOp, TagRequest, TagStart, op_tag, tag_name_check};
pub use target::{TargetSet, target_repos};
pub use tree::{
    TreeError, TreeItem, TreeNode, Trees, UNGROUPED_ID, group_branches, tree_filter, tree_refs,
};
pub use watch::{DEBOUNCE, MAX_DELAY, RepoChange, WatchWarning, WatchedPath, Watchers};
pub use workspace::{
    AddReport, DEFAULT_WORKSPACE_NAME, Dock, ExportReport, GroupId, GroupView, ImportReport,
    MAX_GROUP_NAME, MAX_WORKSPACE_NAME, MissingRepo, PaneLayout, Refusal, RepoId, RepoView,
    SCAN_DEPTH, UNGROUPED_NAME, WORKSPACE_SCHEMA, WORKSPACES_DIR, WorkspaceError,
    WorkspaceListItem, WorkspaceTabs, WorkspaceView, Workspaces, add_path,
};

/// Locks `mutex`, also when a thread panicked while holding it. Every
/// caller changes its state so that a panic leaves none half-changed.
pub(crate) fn lock<T>(mutex: &std::sync::Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}
