//! Workspace, group and repo model, job engine, preflight, result reports, config and cache.

mod app_info;
mod git;
mod history;
mod job;
mod settings;

pub use app_info::AppInfo;
pub use git::{GitSetup, GitStatus};
pub use gitcroc_git::{
    CommitRow, Cursor, Edge, Filter, GitRepo, GraphRow, HistoryError, HistoryStart, OpenError,
    PAGE_SIZE, Page, RefKind, RefLabel, RepoRead, SearchEnv, Time,
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
