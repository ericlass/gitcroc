//! The shape of a job's progress (ARCH-7). The job engine arrives in M2.

use serde::{Deserialize, Serialize};

/// Identifies a workspace. All session state is keyed by it (ARCH-9).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, specta::Type)]
pub struct WorkspaceId(pub String);

/// Identifies a job, unique while the app runs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, specta::Type)]
pub struct JobId(pub u32);

/// How far a job has come: `done` of `total` steps.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
pub struct JobProgress {
    pub job_id: JobId,
    pub workspace_id: WorkspaceId,
    pub label: String,
    pub done: u32,
    pub total: u32,
}
