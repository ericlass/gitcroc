//! The IPC surface: commands, events and the typed error, all exported to
//! `ui/src/ipc/bindings.ts` by tauri-specta. Commands only forward to the core.
//!
//! Every command is `async` and returns `Result<_, IpcError>` (ARCH-7);
//! `arch_7_commands_are_async` checks the first rule for everything listed in
//! [`with_commands!`].

use std::path::{Path, PathBuf};
use std::sync::Arc;

use gitcroc_core::{
    AddReport, AppInfo, GitRunner, GitSetup, GitStatus, JobId, JobIds, JobProgress, Refusal,
    RepoId, SettingsError, SettingsStore, SettingsView, Summaries, SummaryView, Theme,
    WorkspaceError, WorkspaceId, WorkspaceView, Workspaces, add_path, refresh_summaries,
};
use serde::{Deserialize, Serialize};
use specta::Type;
use specta::function::collect_functions;
use specta_typescript::Typescript;
use tauri::{AppHandle, Manager, State};
use tauri_plugin_dialog::DialogExt;
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
    /// The settings are read-only (DATA-1); `settings_get` says why.
    SettingsReadOnly,
    /// The workspace is read-only (DATA-1); its view says why.
    WorkspaceReadOnly,
    /// No open workspace has the id, or the workspace has no such repo.
    NotFound,
    /// No usable git was found (PLAT-3); `git_status` says why.
    GitUnavailable,
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

impl From<SettingsError> for IpcError {
    fn from(error: SettingsError) -> Self {
        let code = match error {
            SettingsError::ReadOnly => IpcErrorCode::SettingsReadOnly,
            SettingsError::Write { .. } | SettingsError::Encode(_) => IpcErrorCode::Internal,
        };
        Self {
            code,
            message: error.to_string(),
            git_stderr: None,
        }
    }
}

impl From<WorkspaceError> for IpcError {
    fn from(error: WorkspaceError) -> Self {
        let code = match error {
            WorkspaceError::ReadOnly => IpcErrorCode::WorkspaceReadOnly,
            WorkspaceError::UnknownWorkspace(_) | WorkspaceError::UnknownRepo(_) => {
                IpcErrorCode::NotFound
            }
            WorkspaceError::Write { .. }
            | WorkspaceError::Encode(_)
            | WorkspaceError::Stopped(_) => IpcErrorCode::Internal,
        };
        Self {
            code,
            message: error.to_string(),
            git_stderr: None,
        }
    }
}

/// Progress of a running job, sent as event `job_progress` (ARCH-7).
#[derive(Debug, Clone, Serialize, Deserialize, Type, Event)]
#[tauri_specta(event_name = "job_progress")]
pub struct JobProgressEvent(pub JobProgress);

/// The result of git detection, sent as event `git_status` whenever it
/// changes: `checking` when detection starts, then the result (PLAT-3).
#[derive(Debug, Clone, Serialize, Deserialize, Type, Event)]
#[tauri_specta(event_name = "git_status")]
pub struct GitStatusEvent(pub GitStatus);

/// A workspace's fresh state, sent as event `workspace_changed` after it
/// changes (ARCH-6).
#[derive(Debug, Clone, Serialize, Deserialize, Type, Event)]
#[tauri_specta(event_name = "workspace_changed")]
pub struct WorkspaceChangedEvent(pub WorkspaceView);

/// What an add job did, sent as event `workspace_add_report` when it ends,
/// after its `workspace_changed`.
#[derive(Debug, Clone, Serialize, Deserialize, Type, Event)]
#[tauri_specta(event_name = "workspace_add_report")]
pub struct WorkspaceAddReportEvent(pub AddReport);

/// One repo's fresh summary, sent as event `repo_summary` each time a repo
/// has been read (PERF-3).
#[derive(Debug, Clone, Serialize, Deserialize, Type, Event)]
#[tauri_specta(event_name = "repo_summary")]
pub struct RepoSummaryEvent(pub SummaryView);

/// The name, version, OS and architecture of the running build.
#[tauri::command]
#[specta::specta]
pub async fn app_info() -> Result<AppInfo, IpcError> {
    Ok(AppInfo::current())
}

/// The latest git detection result; `checking` until the first one is done.
#[tauri::command]
#[specta::specta]
pub async fn git_status(git: State<'_, GitSetup>) -> Result<GitStatus, IpcError> {
    Ok(git.git_status().await)
}

/// Starts detecting git again and returns at once; the result arrives as a
/// `git_status` event.
#[tauri::command]
#[specta::specta]
pub async fn git_recheck(app: AppHandle) -> Result<(), IpcError> {
    spawn_git_detection(app);
    Ok(())
}

/// The settings, and why they are read-only if they are (DATA-1).
#[tauri::command]
#[specta::specta]
pub async fn settings_get(settings: State<'_, SettingsStore>) -> Result<SettingsView, IpcError> {
    Ok(settings.view())
}

/// Changes the app theme (UI-23) and saves the settings.
#[tauri::command]
#[specta::specta]
pub async fn settings_set_theme(app: AppHandle, theme: Theme) -> Result<SettingsView, IpcError> {
    // Saving touches the disk; keep it off the async workers (ARCH-7).
    let saved =
        tauri::async_runtime::spawn_blocking(move || app.state::<SettingsStore>().set_theme(theme))
            .await?;
    Ok(saved?)
}

/// The open workspaces, in tab order. One until workspace tabs exist (M3).
#[tauri::command]
#[specta::specta]
pub async fn workspace_get(
    workspaces: State<'_, Arc<Workspaces>>,
) -> Result<Vec<WorkspaceView>, IpcError> {
    Ok(workspaces.views())
}

/// Starts a job that adds the repo at `path`, or every repo found below it,
/// and returns its id at once. Progress arrives as `job_progress` events,
/// then `workspace_changed` and `workspace_add_report` (ARCH-7).
#[tauri::command]
#[specta::specta]
pub async fn workspace_add_path(
    app: AppHandle,
    workspace_id: WorkspaceId,
    path: PathBuf,
) -> Result<JobId, IpcError> {
    let git = prepare_add(&app, &workspace_id).await?;
    Ok(spawn_add(app, workspace_id, path, git))
}

/// Opens the native folder dialog and returns at once. When the user picks
/// a folder, an add job starts as for `workspace_add_path`; when they
/// cancel, nothing happens. The dialog is opened from Rust: the front end
/// has no dialog permission (SEC-2).
#[tauri::command]
#[specta::specta]
pub async fn workspace_pick_and_add(
    app: AppHandle,
    workspace_id: WorkspaceId,
) -> Result<(), IpcError> {
    let git = prepare_add(&app, &workspace_id).await?;
    let picked = app.clone();
    app.dialog()
        .file()
        .set_title("Add repositories")
        .pick_folder(move |folder| {
            let Some(folder) = folder else {
                return;
            };
            match folder.into_path() {
                Ok(path) => {
                    spawn_add(picked, workspace_id, path, git);
                }
                Err(error) => {
                    let job_id = picked.state::<JobIds>().next();
                    let report = AddReport {
                        workspace_id,
                        job_id,
                        added: Vec::new(),
                        already_present: Vec::new(),
                        refused: vec![Refusal {
                            path: String::new(),
                            reason: format!("the dialog returned no usable folder: {error}"),
                        }],
                    };
                    let _ = WorkspaceAddReportEvent(report).emit(&picked);
                }
            }
        });
    Ok(())
}

/// The current summary of every repo of the workspace, in workspace order:
/// at start the cached ones, marked stale; then each fresh one as it is
/// read, which also arrives as a `repo_summary` event (PERF-3).
#[tauri::command]
#[specta::specta]
pub async fn workspace_summaries(
    workspaces: State<'_, Arc<Workspaces>>,
    summaries: State<'_, Arc<Summaries>>,
    workspace_id: WorkspaceId,
) -> Result<Vec<SummaryView>, IpcError> {
    Ok(summaries.views(&workspaces, &workspace_id)?)
}

/// Tells the core which repo of the workspace is selected, so it is read
/// first when its summary waits for a refresh (PERF-4). `None` clears it.
#[tauri::command]
#[specta::specta]
pub async fn repo_select(
    summaries: State<'_, Arc<Summaries>>,
    workspace_id: WorkspaceId,
    repo_id: Option<RepoId>,
) -> Result<(), IpcError> {
    summaries.select(&workspace_id, repo_id);
    Ok(())
}

/// Removes a repo from the workspace; nothing on disk changes (GRP-3).
#[tauri::command]
#[specta::specta]
pub async fn workspace_remove_repo(
    app: AppHandle,
    workspace_id: WorkspaceId,
    repo_id: RepoId,
) -> Result<WorkspaceView, IpcError> {
    // Saving touches the disk; keep it off the async workers (ARCH-7).
    let workspaces = app.state::<Arc<Workspaces>>().inner().clone();
    let view = tauri::async_runtime::spawn_blocking(move || {
        workspaces.remove_repo(&workspace_id, &repo_id)
    })
    .await??;
    let _ = WorkspaceChangedEvent(view.clone()).emit(&app);
    Ok(view)
}

/// Checks an add can start: the workspace takes changes and git is usable.
async fn prepare_add(app: &AppHandle, workspace_id: &WorkspaceId) -> Result<GitRunner, IpcError> {
    let view = app.state::<Arc<Workspaces>>().view(workspace_id)?;
    if view.read_only {
        return Err(WorkspaceError::ReadOnly.into());
    }
    let setup = app.state::<GitSetup>();
    match setup.git().await {
        Some(git) => Ok(GitRunner::new(git.path)),
        None => Err(IpcError {
            code: IpcErrorCode::GitUnavailable,
            message: "no usable git was found".to_owned(),
            git_stderr: None,
        }),
    }
}

/// Runs an add job in the background and returns its id.
fn spawn_add(app: AppHandle, workspace_id: WorkspaceId, path: PathBuf, git: GitRunner) -> JobId {
    let job_id = app.state::<JobIds>().next();
    tauri::async_runtime::spawn(async move {
        let workspaces = app.state::<Arc<Workspaces>>().inner().clone();
        let progress = {
            let app = app.clone();
            // An emit fails only while the app shuts down.
            move |progress| {
                let _ = JobProgressEvent(progress).emit(&app);
            }
        };
        let shown = path.display().to_string();
        let result = add_path(
            workspaces.clone(),
            workspace_id.clone(),
            path,
            git.clone(),
            job_id,
            progress,
        )
        .await;
        let report = result.unwrap_or_else(|error| AddReport {
            workspace_id: workspace_id.clone(),
            job_id,
            added: Vec::new(),
            already_present: Vec::new(),
            refused: vec![Refusal {
                path: shown,
                reason: error.to_string(),
            }],
        });
        if let Ok(view) = workspaces.view(&workspace_id) {
            let _ = WorkspaceChangedEvent(view).emit(&app);
        }
        let added = report.added.iter().map(|repo| repo.id.clone()).collect();
        let _ = WorkspaceAddReportEvent(report).emit(&app);
        refresh(app, workspace_id, added, git).await;
    });
    job_id
}

/// Reads the summaries of `repos` again, sending each as `repo_summary`.
async fn refresh(app: AppHandle, workspace_id: WorkspaceId, repos: Vec<RepoId>, git: GitRunner) {
    if repos.is_empty() {
        return;
    }
    let workspaces = app.state::<Arc<Workspaces>>().inner().clone();
    let summaries = app.state::<Arc<Summaries>>().inner().clone();
    refresh_summaries(
        workspaces,
        summaries,
        workspace_id,
        repos,
        git,
        move |view| {
            // An emit fails only while the app shuts down.
            let _ = RepoSummaryEvent(view).emit(&app);
        },
    )
    .await;
}

/// Detects git in the background (ARCH-7), sending `git_status` events when it
/// starts and when it is done. Window creation never waits for it. With a
/// usable git, every repo's summary is then read again (PERF-3); until
/// then the cached summaries stand.
pub fn spawn_git_detection(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        let setup = app.state::<GitSetup>();
        // An emit fails only while the app shuts down; the state is current
        // either way and `git_status` reads it.
        let _ = GitStatusEvent(GitStatus::Checking).emit(&app);
        let status = setup.detect().await;
        let _ = GitStatusEvent(status).emit(&app);
        let Some(git) = setup.git().await.map(|git| GitRunner::new(git.path)) else {
            return;
        };
        for workspace in app.state::<Arc<Workspaces>>().views() {
            let repos = workspace.repos.into_iter().map(|repo| repo.id).collect();
            tauri::async_runtime::spawn(refresh(app.clone(), workspace.id, repos, git.clone()));
        }
    });
}

/// Debug builds only: commands to exercise the IPC plumbing by hand.
#[cfg(debug_assertions)]
pub mod debug {
    use std::time::Duration;

    use gitcroc_core::{JobId, JobIds, JobProgress, WorkspaceId};
    use tauri::{AppHandle, Manager};
    use tauri_specta::Event;

    use super::{IpcError, JobProgressEvent};

    /// Starts a fake job that reports three steps 100 ms apart, and returns
    /// its id without waiting for it.
    #[tauri::command]
    #[specta::specta]
    pub async fn debug_emit_progress(
        app: AppHandle,
        workspace_id: WorkspaceId,
    ) -> Result<JobId, IpcError> {
        let job_id = app.state::<JobIds>().next();
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
            $collect![
                app_info,
                git_status,
                git_recheck,
                settings_get,
                settings_set_theme,
                workspace_get,
                workspace_add_path,
                workspace_pick_and_add,
                workspace_remove_repo,
                workspace_summaries,
                repo_select,
                debug::debug_emit_progress
            ]
        }
        #[cfg(not(debug_assertions))]
        {
            $collect![
                app_info,
                git_status,
                git_recheck,
                settings_get,
                settings_set_theme,
                workspace_get,
                workspace_add_path,
                workspace_pick_and_add,
                workspace_remove_repo,
                workspace_summaries,
                repo_select
            ]
        }
    }};
}

/// The commands and events the app registers.
pub fn builder() -> Builder<tauri::Wry> {
    Builder::new()
        .commands(with_commands!(collect_commands))
        .events(collect_events![
            JobProgressEvent,
            GitStatusEvent,
            WorkspaceChangedEvent,
            WorkspaceAddReportEvent,
            RepoSummaryEvent
        ])
}

/// Specta's description of every registered command.
pub fn command_signatures() -> Vec<specta::datatype::Function> {
    with_commands!(collect_functions)(&mut specta::Types::default())
}

/// Writes the TypeScript bindings to `path`.
pub fn export_bindings(path: &Path) -> Result<(), specta_typescript::Error> {
    builder().export(Typescript::default().header(BINDINGS_HEADER), path)
}
