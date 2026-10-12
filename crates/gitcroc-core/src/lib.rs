//! Workspace, group and repo model, job engine, preflight, result reports, config and cache.

mod app_info;
mod diff;
mod git;
mod history;
mod job;
mod settings;

pub use app_info::AppInfo;
pub use diff::{DiffReadError, changed_files, commit_details, file_diff};
pub use git::{GitSetup, GitStatus};
pub use gitcroc_git::{
    ChangedFile, CommitDetails, CommitRow, Context, Cursor, DiffError, DiffOptions, DiffRead,
    DiffSource, Edge, FileDiff, FileStatus, Filter, GitRepo, GraphRow, HistoryError, HistoryStart,
    Hunk, Image, ImageFormat, Line, LineKind, MAX_COUNTED_FILES, MAX_DIFF_LINES, MAX_IMAGE_BYTES,
    ModeChange, OpenError, PAGE_SIZE, Page, Person, RefKind, RefLabel, RepoRead, SearchEnv,
    TextDiff, Time,
};
pub use history::{HistoryPageError, history_page};
pub use job::{JobId, JobProgress, WorkspaceId};
pub use settings::{
    ConfigDir, PlatformConfigDir, SETTINGS_FILE, SETTINGS_SCHEMA, Settings, SettingsError,
    SettingsStore, SettingsView, SettingsWarning, Theme,
};

/// Errors returned by `gitcroc-core`.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
}
