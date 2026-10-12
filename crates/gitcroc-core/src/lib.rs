//! Workspace, group and repo model, job engine, preflight, result reports, config and cache.

mod app_info;
mod git;
mod job;

pub use app_info::AppInfo;
pub use git::{GitSetup, GitStatus};
pub use gitcroc_git::SearchEnv;
pub use job::{JobId, JobProgress, WorkspaceId};

/// Errors returned by `gitcroc-core`.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
}
