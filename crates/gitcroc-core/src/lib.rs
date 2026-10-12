//! Workspace, group and repo model, job engine, preflight, result reports, config and cache.

mod app_info;
mod git;
mod job;
mod settings;

pub use app_info::AppInfo;
pub use git::{GitSetup, GitStatus};
pub use gitcroc_git::SearchEnv;
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
