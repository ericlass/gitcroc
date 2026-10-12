//! The shape of a job's progress (ARCH-7). The job engine arrives in M2.

use std::sync::atomic::{AtomicU32, Ordering};

use serde::{Deserialize, Serialize};

/// Identifies a workspace. All session state is keyed by it (ARCH-9).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, specta::Type)]
pub struct WorkspaceId(pub String);

/// Identifies a job, unique while the app runs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, specta::Type)]
pub struct JobId(pub u32);

/// Hands out job ids for the whole app, starting at 1.
#[derive(Debug, Default)]
pub struct JobIds(AtomicU32);

impl JobIds {
    pub fn next(&self) -> JobId {
        JobId(self.0.fetch_add(1, Ordering::Relaxed).wrapping_add(1))
    }
}

/// How far a job has come: `done` of `total` steps. A `total` of 0 means
/// the number of steps is not known yet.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
pub struct JobProgress {
    pub job_id: JobId,
    pub workspace_id: WorkspaceId,
    pub label: String,
    pub done: u32,
    pub total: u32,
}
