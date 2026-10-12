//! The IPC surface: commands, events and the typed error, all exported to
//! `ui/src/ipc/bindings.ts` by tauri-specta. Commands only forward to the core.
//!
//! Every command is `async` and returns `Result<_, IpcError>` (ARCH-7);
//! `arch_7_commands_are_async` checks the first rule for everything listed in
//! [`with_commands!`].

use std::path::Path;

use gitcroc_core::{AppInfo, JobProgress};
use serde::{Deserialize, Serialize};
use specta::Type;
use specta::function::collect_functions;
use specta_typescript::Typescript;
use tauri_specta::{Builder, Event, collect_commands, collect_events};

/// Where `just ipc` writes the bindings, relative to the workspace root.
pub const BINDINGS_PATH: &str = "ui/src/ipc/bindings.ts";

/// Goes above tauri-specta's own "generated, do not edit" line.
const BINDINGS_HEADER: &str =
    "// Source: crates/gitcroc-app/src/ipc.rs. Regenerate with `just ipc`.";

/// The error every command returns.
#[derive(Debug, Clone, Serialize, Deserialize, Type, thiserror::Error)]
#[error("{message}")]
pub struct IpcError {
    pub code: IpcErrorCode,
    pub message: String,
    /// What git printed on stderr, when the error came from a git command.
    pub git_stderr: Option<String>,
}

/// What kind of failure an [`IpcError`] reports.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum IpcErrorCode {
    /// A bug or an environment problem the user cannot fix from the UI.
    Internal,
}

impl From<tauri::Error> for IpcError {
    fn from(error: tauri::Error) -> Self {
        Self {
            code: IpcErrorCode::Internal,
            message: error.to_string(),
            git_stderr: None,
        }
    }
}

/// Progress of a running job, sent as event `job_progress` (ARCH-7).
#[derive(Debug, Clone, Serialize, Deserialize, Type, Event)]
#[tauri_specta(event_name = "job_progress")]
pub struct JobProgressEvent(pub JobProgress);

/// The name, version, OS and architecture of the running build.
#[tauri::command]
#[specta::specta]
pub async fn app_info() -> Result<AppInfo, IpcError> {
    Ok(AppInfo::current())
}

/// Debug builds only: commands to exercise the IPC plumbing by hand.
#[cfg(debug_assertions)]
pub mod debug {
    use std::sync::atomic::{AtomicU32, Ordering};
    use std::time::Duration;

    use gitcroc_core::{JobId, JobProgress, WorkspaceId};
    use tauri::AppHandle;
    use tauri_specta::Event;

    use super::{IpcError, JobProgressEvent};

    static NEXT_JOB: AtomicU32 = AtomicU32::new(1);

    /// Starts a fake job that reports three steps 100 ms apart, and returns
    /// its id without waiting for it.
    #[tauri::command]
    #[specta::specta]
    pub async fn debug_emit_progress(
        app: AppHandle,
        workspace_id: WorkspaceId,
    ) -> Result<JobId, IpcError> {
        let job_id = JobId(NEXT_JOB.fetch_add(1, Ordering::Relaxed));
        tauri::async_runtime::spawn(async move {
            const TOTAL: u32 = 3;
            for done in 1..=TOTAL {
                tokio::time::sleep(Duration::from_millis(100)).await;
                let progress = JobProgress {
                    job_id,
                    workspace_id: workspace_id.clone(),
                    label: "Debug job".to_owned(),
                    done,
                    total: TOTAL,
                };
                if JobProgressEvent(progress).emit(&app).is_err() {
                    break;
                }
            }
        });
        Ok(job_id)
    }
}

/// Expands `$collect![...]` with every registered command, so the invoke
/// handler and the checks on the command list cannot drift apart.
macro_rules! with_commands {
    ($collect:ident) => {{
        #[cfg(debug_assertions)]
        {
            $collect![app_info, debug::debug_emit_progress]
        }
        #[cfg(not(debug_assertions))]
        {
            $collect![app_info]
        }
    }};
}

/// The commands and events the app registers.
pub fn builder() -> Builder<tauri::Wry> {
    Builder::new()
        .commands(with_commands!(collect_commands))
        .events(collect_events![JobProgressEvent])
}

/// Specta's description of every registered command.
pub fn command_signatures() -> Vec<specta::datatype::Function> {
    with_commands!(collect_functions)(&mut specta::Types::default())
}

/// Writes the TypeScript bindings to `path`.
pub fn export_bindings(path: &Path) -> Result<(), specta_typescript::Error> {
    builder().export(Typescript::default().header(BINDINGS_HEADER), path)
}
