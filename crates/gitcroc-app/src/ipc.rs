//! The IPC surface: commands, events and the typed error, all exported to
//! `ui/src/ipc/bindings.ts` by tauri-specta. Commands only forward to the core.
//!
//! Every command is `async` and returns `Result<_, IpcError>` (ARCH-7);
//! `arch_7_commands_are_async` checks the first rule for everything listed in
//! [`with_commands!`].

use std::path::{Path, PathBuf};
use std::sync::Arc;

use base64::Engine as _;
use gitcroc_core::{
    AddReport, AppInfo, AppLog, Appearance, BranchChoices, BranchError, BranchRequest, BranchStart,
    CancelResult, ChangeRequest, ChangeStart, Changes, ChangesError, ChangesSettings,
    ChangesTarget, ChangesView, ChooseRequest, CloneError, CloneReport, CloneRequest,
    CommitGraphError, CommitRequest, ConflictDetails, ConflictRead, ConflictStart, DiffRequest,
    FileDiff, FileRef, FsmonitorError, GitRunner, GitSetup, GitStatus, GroupId, GroupSummary,
    Histories, HistoryTarget, HistoryView, HistoryViewError, ImageSide, IntegrateError,
    IntegrateRequest, IntegrateStart, JobError, JobEvents, JobId, JobProgress, JobView, Jobs,
    Outcome, PaneLayout, PushChoices, Refusal, RemoteError, RemoteRequest, RemoteStart,
    RemoteUrlsView, RepoChange, RepoId, ResetError, ResetRequest, ResetStart, ResolveRequest,
    SettingsError, SettingsStore, SettingsView, StashError, StashRequest, StashStart, StatusError,
    StatusReadError, Summaries, SummaryView, SyncError, SyncRequest, SyncStart, TabSwitch,
    TagError, TagRequest, TagStart, TargetSet, Theme, TrackedJob, TreeError, TreeNode, Trees,
    WatchWarning, Watchers, WorkspaceError, WorkspaceId, WorkspaceListItem, WorkspaceTabs,
    WorkspaceView, Workspaces, add_path, group_summaries, refresh_summaries, refresh_workspace,
    tree_refs,
};
use serde::{Deserialize, Serialize};
use specta::Type;
use specta::function::collect_functions;
use specta_typescript::Typescript;
use tauri::{AppHandle, Manager, State};

use crate::askpass::{AskpassPrompt, WindowAskpassUi};
use crate::platform::{FileManager, LaunchError, PlatformFileManager, PlatformTerminal, Terminal};
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
    /// The request was refused as it stands, such as a workspace name that
    /// is taken; the message says why and the user can change it.
    Refused,
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
            WorkspaceError::UnknownWorkspace(_)
            | WorkspaceError::UnknownRepo(_)
            | WorkspaceError::UnknownGroup(_)
            | WorkspaceError::NoSuchWorkspace(_) => IpcErrorCode::NotFound,
            WorkspaceError::BlankName
            | WorkspaceError::NameTooLong
            | WorkspaceError::NameTaken(_)
            | WorkspaceError::IsOpen
            | WorkspaceError::NotConfirmed
            | WorkspaceError::BadOrder
            | WorkspaceError::BlankGroupName
            | WorkspaceError::GroupNameTooLong
            | WorkspaceError::GroupNameTaken(_)
            | WorkspaceError::GroupNameReserved
            | WorkspaceError::BadGroupOrder => IpcErrorCode::Refused,
            WorkspaceError::Write { .. }
            | WorkspaceError::Read { .. }
            | WorkspaceError::Delete { .. }
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

impl From<TreeError> for IpcError {
    fn from(error: TreeError) -> Self {
        match error {
            TreeError::Workspace(error) => error.into(),
            TreeError::UnknownGroup(group) => WorkspaceError::UnknownGroup(group).into(),
            TreeError::Open(_) | TreeError::Refs(_) | TreeError::Stopped(_) => Self {
                code: IpcErrorCode::Internal,
                message: error.to_string(),
                git_stderr: None,
            },
        }
    }
}

impl From<CommitGraphError> for IpcError {
    fn from(error: CommitGraphError) -> Self {
        match error {
            CommitGraphError::Workspace(error)
            | CommitGraphError::Job(JobError::Workspace(error)) => error.into(),
            CommitGraphError::Job(JobError::NoRepo) => Self {
                code: IpcErrorCode::Internal,
                message: error.to_string(),
                git_stderr: None,
            },
        }
    }
}

impl From<HistoryViewError> for IpcError {
    fn from(error: HistoryViewError) -> Self {
        match error {
            HistoryViewError::Workspace(error) => error.into(),
            HistoryViewError::Open(_) | HistoryViewError::Page(_) | HistoryViewError::NoPage => {
                Self {
                    code: IpcErrorCode::Internal,
                    message: error.to_string(),
                    git_stderr: None,
                }
            }
        }
    }
}

impl From<BranchError> for IpcError {
    fn from(error: BranchError) -> Self {
        match error {
            BranchError::Workspace(error) | BranchError::Job(JobError::Workspace(error)) => {
                error.into()
            }
            // git refused the repo; its message is for the user (SEC-6).
            BranchError::Status(StatusReadError::Read(StatusError::Refused { message })) => Self {
                code: IpcErrorCode::Internal,
                message: "git refused to read the repo".to_owned(),
                git_stderr: Some(message),
            },
            other => Self {
                code: IpcErrorCode::Internal,
                message: other.to_string(),
                git_stderr: None,
            },
        }
    }
}

impl From<SyncError> for IpcError {
    fn from(error: SyncError) -> Self {
        match error {
            SyncError::Workspace(error) | SyncError::Job(JobError::Workspace(error)) => {
                error.into()
            }
            // git refused the repo; its message is for the user (SEC-6).
            SyncError::Status(StatusReadError::Read(StatusError::Refused { message })) => Self {
                code: IpcErrorCode::Internal,
                message: "git refused to read the repo".to_owned(),
                git_stderr: Some(message),
            },
            other => Self {
                code: IpcErrorCode::Internal,
                message: other.to_string(),
                git_stderr: None,
            },
        }
    }
}

impl From<IntegrateError> for IpcError {
    fn from(error: IntegrateError) -> Self {
        match error {
            IntegrateError::Workspace(error) | IntegrateError::Job(JobError::Workspace(error)) => {
                error.into()
            }
            // git refused the repo; its message is for the user (SEC-6).
            IntegrateError::Status(StatusReadError::Read(StatusError::Refused { message })) => {
                Self {
                    code: IpcErrorCode::Internal,
                    message: "git refused to read the repo".to_owned(),
                    git_stderr: Some(message),
                }
            }
            other => Self {
                code: IpcErrorCode::Internal,
                message: other.to_string(),
                git_stderr: None,
            },
        }
    }
}

impl From<TagError> for IpcError {
    fn from(error: TagError) -> Self {
        match error {
            TagError::Workspace(error) | TagError::Job(JobError::Workspace(error)) => error.into(),
            // git refused the repo; its message is for the user (SEC-6).
            TagError::Status(StatusReadError::Read(StatusError::Refused { message })) => Self {
                code: IpcErrorCode::Internal,
                message: "git refused to read the repo".to_owned(),
                git_stderr: Some(message),
            },
            other => Self {
                code: IpcErrorCode::Internal,
                message: other.to_string(),
                git_stderr: None,
            },
        }
    }
}

impl From<ResetError> for IpcError {
    fn from(error: ResetError) -> Self {
        match error {
            ResetError::Workspace(error) | ResetError::Job(JobError::Workspace(error)) => {
                error.into()
            }
            // git refused the repo; its message is for the user (SEC-6).
            ResetError::Status(StatusReadError::Read(StatusError::Refused { message })) => Self {
                code: IpcErrorCode::Internal,
                message: "git refused to read the repo".to_owned(),
                git_stderr: Some(message),
            },
            other => Self {
                code: IpcErrorCode::Internal,
                message: other.to_string(),
                git_stderr: None,
            },
        }
    }
}

impl From<RemoteError> for IpcError {
    fn from(error: RemoteError) -> Self {
        match error {
            RemoteError::Workspace(error) | RemoteError::Job(JobError::Workspace(error)) => {
                error.into()
            }
            // git refused the repo; its message is for the user (SEC-6).
            RemoteError::Status(StatusReadError::Read(StatusError::Refused { message })) => Self {
                code: IpcErrorCode::Internal,
                message: "git refused to read the repo".to_owned(),
                git_stderr: Some(message),
            },
            other => Self {
                code: IpcErrorCode::Internal,
                message: other.to_string(),
                git_stderr: None,
            },
        }
    }
}

impl From<FsmonitorError> for IpcError {
    fn from(error: FsmonitorError) -> Self {
        match error {
            FsmonitorError::Workspace(error) | FsmonitorError::Job(JobError::Workspace(error)) => {
                error.into()
            }
            // git refused the repo; its message is for the user (SEC-6).
            FsmonitorError::Status(StatusReadError::Read(StatusError::Refused { message })) => {
                Self {
                    code: IpcErrorCode::Internal,
                    message: "git refused to read the repo".to_owned(),
                    git_stderr: Some(message),
                }
            }
            other => Self {
                code: IpcErrorCode::Internal,
                message: other.to_string(),
                git_stderr: None,
            },
        }
    }
}

impl From<CloneError> for IpcError {
    fn from(error: CloneError) -> Self {
        match error {
            CloneError::Workspace(error) | CloneError::Job(JobError::Workspace(error)) => {
                error.into()
            }
            other => Self {
                code: IpcErrorCode::Internal,
                message: other.to_string(),
                git_stderr: None,
            },
        }
    }
}

impl From<LaunchError> for IpcError {
    fn from(error: LaunchError) -> Self {
        Self {
            code: IpcErrorCode::Internal,
            message: error.to_string(),
            git_stderr: None,
        }
    }
}

impl From<StashError> for IpcError {
    fn from(error: StashError) -> Self {
        match error {
            StashError::Workspace(error) | StashError::Job(JobError::Workspace(error)) => {
                error.into()
            }
            // git refused the repo; its message is for the user (SEC-6).
            StashError::Status(StatusReadError::Read(StatusError::Refused { message })) => Self {
                code: IpcErrorCode::Internal,
                message: "git refused to read the repo".to_owned(),
                git_stderr: Some(message),
            },
            other => Self {
                code: IpcErrorCode::Internal,
                message: other.to_string(),
                git_stderr: None,
            },
        }
    }
}

impl From<ChangesError> for IpcError {
    fn from(error: ChangesError) -> Self {
        match error {
            ChangesError::Workspace(error) | ChangesError::Job(JobError::Workspace(error)) => {
                error.into()
            }
            // git refused the repo; its message is for the user (SEC-6).
            ChangesError::Status(StatusReadError::Read(StatusError::Refused { message })) => Self {
                code: IpcErrorCode::Internal,
                message: "git refused to read the working copy".to_owned(),
                git_stderr: Some(message),
            },
            other => Self {
                code: IpcErrorCode::Internal,
                message: other.to_string(),
                git_stderr: None,
            },
        }
    }
}

/// A job was queued, began to run, or made progress, sent as event
/// `job_progress` (ARCH-7, UI-21).
#[derive(Debug, Clone, Serialize, Deserialize, Type, Event)]
#[tauri_specta(event_name = "job_progress")]
pub struct JobProgressEvent(pub JobView);

/// Git or ssh asks for something it cannot get silently, sent as event
/// `askpass_prompt` (D10). Answered with `askpass_answer`.
#[derive(Debug, Clone, Serialize, Deserialize, Type, Event)]
#[tauri_specta(event_name = "askpass_prompt")]
pub struct AskpassPromptEvent(pub AskpassPrompt);

/// A prompt was withdrawn before it was answered, as when its job ended,
/// sent as event `askpass_closed`.
#[derive(Debug, Clone, Serialize, Deserialize, Type, Event)]
#[tauri_specta(event_name = "askpass_closed")]
pub struct AskpassClosedEvent {
    pub prompt_id: u32,
}

/// A job ended, sent as event `job_ended` with its outcome and git's
/// output, after its repos' fresh summaries.
#[derive(Debug, Clone, Serialize, Deserialize, Type, Event)]
#[tauri_specta(event_name = "job_ended")]
pub struct JobEndedEvent(pub JobView);

/// The result of git detection, sent as event `git_status` whenever it
/// changes: `checking` when detection starts, then the result (PLAT-3).
#[derive(Debug, Clone, Serialize, Deserialize, Type, Event)]
#[tauri_specta(event_name = "git_status")]
pub struct GitStatusEvent(pub GitStatus);

/// A group's row after a member's summary changed, sent as event
/// `group_summary` (GRP-4, UI-4).
#[derive(Debug, Clone, Serialize, Deserialize, Type, Event)]
#[tauri_specta(event_name = "group_summary")]
pub struct GroupSummaryEvent(pub GroupSummary);

/// A workspace's fresh state, sent as event `workspace_changed` after it
/// changes (ARCH-6).
#[derive(Debug, Clone, Serialize, Deserialize, Type, Event)]
#[tauri_specta(event_name = "workspace_changed")]
pub struct WorkspaceChangedEvent(pub WorkspaceView);

/// The open workspaces and the active one changed, sent as event
/// `workspaces_changed` after every open, close, create, rename, reorder
/// and activate (UI-27).
#[derive(Debug, Clone, Serialize, Deserialize, Type, Event)]
#[tauri_specta(event_name = "workspaces_changed")]
pub struct WorkspacesChangedEvent(pub WorkspaceTabs);

/// What an add job did, sent as event `workspace_add_report` when it ends,
/// after its `workspace_changed`.
#[derive(Debug, Clone, Serialize, Deserialize, Type, Event)]
#[tauri_specta(event_name = "workspace_add_report")]
pub struct WorkspaceAddReportEvent(pub AddReport);

/// The parent folder the user picked for a clone, sent as event
/// `clone_parent_picked`; nothing is sent when they cancel.
#[derive(Debug, Clone, Serialize, Deserialize, Type, Event)]
#[tauri_specta(event_name = "clone_parent_picked")]
pub struct CloneParentPickedEvent {
    pub workspace_id: WorkspaceId,
    pub path: PathBuf,
}

/// How a clone ended, sent as event `clone_report` after its
/// `workspace_changed`: the cloned repo, to select, when it was added.
#[derive(Debug, Clone, Serialize, Deserialize, Type, Event)]
#[tauri_specta(event_name = "clone_report")]
pub struct CloneReportEvent(pub CloneReport);

/// One repo's fresh summary, sent as event `repo_summary` each time a repo
/// has been read (PERF-3).
#[derive(Debug, Clone, Serialize, Deserialize, Type, Event)]
#[tauri_specta(event_name = "repo_summary")]
pub struct RepoSummaryEvent(pub SummaryView);

/// A repo of a workspace.
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct RepoRef {
    pub workspace_id: WorkspaceId,
    pub repo_id: RepoId,
}

/// A repo's loaded refs were dropped because its summary was read again,
/// sent as event `repo_refs_changed`; `refs_get` reads them anew.
#[derive(Debug, Clone, Serialize, Deserialize, Type, Event)]
#[tauri_specta(event_name = "repo_refs_changed")]
pub struct RepoRefsChangedEvent(pub RepoRef);

/// Watching a workspace's files failed, sent once as event `watch_warning`;
/// changes made outside the app then show when the window regains focus.
#[derive(Debug, Clone, Serialize, Deserialize, Type, Event)]
#[tauri_specta(event_name = "watch_warning")]
pub struct WatchWarningEvent(pub WatchWarning);

/// A commit-graph write job ended, sent as event `commit_graph_written`.
#[derive(Debug, Clone, Serialize, Deserialize, Type, Event)]
#[tauri_specta(event_name = "commit_graph_written")]
pub struct CommitGraphWrittenEvent {
    pub workspace_id: WorkspaceId,
    pub repo_id: RepoId,
    pub job_id: JobId,
    /// Why the write failed; `None` when the file was written.
    pub error: Option<String>,
    /// What git printed on stderr, when git ran and failed.
    pub git_stderr: Option<String>,
}

/// The desktop's UI and monospace font families, read once at startup
/// (UI-26) by [`crate::platform::DesktopFontSource`]; `None` for what the
/// desktop does not tell.
#[derive(Debug, Clone, Default)]
pub struct DesktopFonts {
    pub ui: Option<String>,
    pub code: Option<String>,
}

/// The name, version, OS and architecture of the running build, and the
/// desktop's fonts.
#[tauri::command]
#[specta::specta]
pub async fn app_info(fonts: State<'_, DesktopFonts>) -> Result<AppInfo, IpcError> {
    Ok(AppInfo::current()
        .with_ui_font(fonts.ui.clone())
        .with_code_font(fonts.code.clone()))
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
pub async fn settings_get(
    settings: State<'_, Arc<SettingsStore>>,
) -> Result<SettingsView, IpcError> {
    Ok(settings.view())
}

/// Changes the app theme (UI-23) and saves the settings.
#[tauri::command]
#[specta::specta]
pub async fn settings_set_theme(app: AppHandle, theme: Theme) -> Result<SettingsView, IpcError> {
    // Saving touches the disk; keep it off the async workers (ARCH-7).
    let saved = tauri::async_runtime::spawn_blocking(move || {
        app.state::<Arc<SettingsStore>>().set_theme(theme)
    })
    .await?;
    Ok(saved?)
}

/// The open workspaces in tab order, the active one, and the saved tabs
/// whose file was gone at start (UI-27, UI-33).
#[tauri::command]
#[specta::specta]
pub async fn workspace_get(
    workspaces: State<'_, Arc<Workspaces>>,
) -> Result<WorkspaceTabs, IpcError> {
    Ok(workspaces.tabs())
}

/// Every workspace file, open or not, sorted by name (UI-28).
#[tauri::command]
#[specta::specta]
pub async fn workspace_list(app: AppHandle) -> Result<Vec<WorkspaceListItem>, IpcError> {
    // Reads every workspace file; keep it off the async workers (ARCH-7).
    let workspaces = app.state::<Arc<Workspaces>>().inner().clone();
    Ok(tauri::async_runtime::spawn_blocking(move || workspaces.list()).await??)
}

/// Creates a workspace, opened as the last tab and made active.
#[tauri::command]
#[specta::specta]
pub async fn workspace_create(app: AppHandle, name: String) -> Result<WorkspaceTabs, IpcError> {
    change_tabs(app, move |workspaces| {
        let id = workspaces.create(&name)?;
        Ok(Some(id))
    })
    .await
}

/// Opens a workspace as the last tab and makes it active; one already open
/// is made active instead (UI-27).
#[tauri::command]
#[specta::specta]
pub async fn workspace_open(
    app: AppHandle,
    workspace_id: WorkspaceId,
) -> Result<WorkspaceTabs, IpcError> {
    change_tabs(app, move |workspaces| {
        Ok(workspaces.open(&workspace_id)?.then_some(workspace_id))
    })
    .await
}

/// Closes a workspace's tab. Its file and repos stay, and its jobs run on
/// (UI-28, UI-21).
#[tauri::command]
#[specta::specta]
pub async fn workspace_close(
    app: AppHandle,
    workspace_id: WorkspaceId,
) -> Result<WorkspaceTabs, IpcError> {
    let closed = workspace_id.clone();
    let tabs = change_tabs(app.clone(), move |workspaces| {
        workspaces.close(&workspace_id)?;
        Ok(None)
    })
    .await?;
    // Its state goes with its tab (ARCH-9).
    app.state::<Arc<Watchers>>().stop(&closed);
    app.state::<Arc<Trees>>().forget(&closed);
    app.state::<Arc<Histories>>().forget(&closed);
    app.state::<Arc<Changes>>().forget(&closed);
    app.state::<Arc<Summaries>>().forget(&closed);
    Ok(tabs)
}

/// Renames a workspace, open or not.
#[tauri::command]
#[specta::specta]
pub async fn workspace_rename(
    app: AppHandle,
    workspace_id: WorkspaceId,
    name: String,
) -> Result<WorkspaceTabs, IpcError> {
    let tabs = change_tabs(app.clone(), move |workspaces| {
        workspaces.rename(&workspace_id, &name)?;
        Ok(None)
    })
    .await?;
    for view in &tabs.open {
        let _ = WorkspaceChangedEvent(view.clone()).emit(&app);
    }
    Ok(tabs)
}

/// Deletes the file of a workspace that is not open, and nothing else
/// (GRP-3). Needs `confirmed`.
#[tauri::command]
#[specta::specta]
pub async fn workspace_delete(
    app: AppHandle,
    workspace_id: WorkspaceId,
    confirmed: bool,
) -> Result<(), IpcError> {
    let workspaces = app.state::<Arc<Workspaces>>().inner().clone();
    tauri::async_runtime::spawn_blocking(move || workspaces.delete(&workspace_id, confirmed))
        .await??;
    Ok(())
}

/// Puts the tabs in the order of `workspace_ids`, which names each open
/// workspace once.
#[tauri::command]
#[specta::specta]
pub async fn workspace_reorder(
    app: AppHandle,
    workspace_ids: Vec<WorkspaceId>,
) -> Result<WorkspaceTabs, IpcError> {
    change_tabs(app, move |workspaces| {
        workspaces.reorder(&workspace_ids)?;
        Ok(None)
    })
    .await
}

/// Makes an open workspace the active tab. It is then watched, and its
/// summaries are read again as when the window regains focus (UI-30).
#[tauri::command]
#[specta::specta]
pub async fn workspace_activate(
    app: AppHandle,
    workspace_id: WorkspaceId,
) -> Result<WorkspaceTabs, IpcError> {
    change_tabs(app, move |workspaces| {
        workspaces.activate(&workspace_id)?;
        Ok(None)
    })
    .await
}

/// Runs `change` off the async workers (ARCH-7), as it saves files, then
/// sends `workspaces_changed`. `change` returns the workspace it opened,
/// whose cached summaries are then taken. When the active tab changed, the
/// old one goes to the background and the new one is watched and read
/// again (UI-30, UI-31).
async fn change_tabs(
    app: AppHandle,
    change: impl FnOnce(&Workspaces) -> Result<Option<WorkspaceId>, WorkspaceError> + Send + 'static,
) -> Result<WorkspaceTabs, IpcError> {
    let workspaces = app.state::<Arc<Workspaces>>().inner().clone();
    let summaries = app.state::<Arc<Summaries>>().inner().clone();
    let (before, tabs) = tauri::async_runtime::spawn_blocking(move || {
        let before = workspaces.active();
        if let Some(opened) = change(&workspaces)? {
            summaries.open(&workspaces, &opened);
        }
        Ok::<_, WorkspaceError>((before, workspaces.tabs()))
    })
    .await??;
    let _ = WorkspacesChangedEvent(tabs.clone()).emit(&app);
    if tabs.active != before {
        let switch = tab_switch(&app);
        if let Some(before) = &before {
            let trees = app.state::<Arc<Trees>>().inner().clone();
            for view in switch.background(before) {
                send_summary(&app, &trees, view, false);
            }
        }
        if let Some(active) = tabs.active.clone() {
            let git = app
                .state::<GitSetup>()
                .git()
                .await
                .map(|git| GitRunner::new(git.path));
            let emit = summary_sink(app.clone());
            tauri::async_runtime::spawn(async move {
                // A workspace closed since has nothing to read.
                let _ = switch.foreground(active, git, emit).await;
            });
        }
    }
    Ok(tabs)
}

/// The per-workspace state a tab switch acts on.
fn tab_switch(app: &AppHandle) -> TabSwitch {
    TabSwitch {
        workspaces: app.state::<Arc<Workspaces>>().inner().clone(),
        summaries: app.state::<Arc<Summaries>>().inner().clone(),
        watchers: app.state::<Arc<Watchers>>().inner().clone(),
        histories: app.state::<Arc<Histories>>().inner().clone(),
        changes: app.state::<Arc<Changes>>().inner().clone(),
    }
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
    // The end-to-end smoke test's answer to the dialog, which WebDriver
    // cannot drive (M2-15); only in builds with the `e2e` feature.
    #[cfg(feature = "e2e")]
    if let Some(path) = std::env::var_os("GITCROC_E2E_PICK_FOLDER") {
        spawn_add(app, workspace_id, PathBuf::from(path), git);
        return Ok(());
    }
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
                    let job_id = picked.state::<Arc<Jobs>>().next_id();
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
/// first when its summary waits for a refresh (PERF-4) and its working tree
/// is the one watched (PERF-6). `None` clears it. Returns at once; the
/// watch moves in the background.
#[tauri::command]
#[specta::specta]
pub async fn repo_select(
    app: AppHandle,
    workspace_id: WorkspaceId,
    repo_id: Option<RepoId>,
) -> Result<(), IpcError> {
    app.state::<Arc<Summaries>>()
        .select(&workspace_id, repo_id.clone());
    let watchers = app.state::<Arc<Watchers>>().inner().clone();
    // Recorded here, so selections keep their order; the watch follows in
    // the background, as a large working tree takes a watch per folder
    // (ARCH-7).
    watchers.select(&workspace_id, repo_id);
    tauri::async_runtime::spawn_blocking(move || watchers.follow_selection(&workspace_id));
    Ok(())
}

/// Reads every repo of the workspace again, the selected one first, as when
/// the window regains focus (OPS-7). Returns at once; each summary arrives
/// as `repo_summary`.
#[tauri::command]
#[specta::specta]
pub async fn workspace_refresh(app: AppHandle, workspace_id: WorkspaceId) -> Result<(), IpcError> {
    app.state::<Arc<Workspaces>>().view(&workspace_id)?;
    let git = usable_git(&app).await?;
    let workspaces = app.state::<Arc<Workspaces>>().inner().clone();
    let summaries = app.state::<Arc<Summaries>>().inner().clone();
    tauri::async_runtime::spawn(async move {
        let emit = summary_sink(app);
        // The workspace was checked above; one closed since has nothing to read.
        let _ = refresh_workspace(workspaces, summaries, workspace_id, git, emit).await;
    });
    Ok(())
}

/// Why watching the workspace's files failed, if it did; the same warning
/// is also sent once as `watch_warning`.
#[tauri::command]
#[specta::specta]
pub async fn watch_warning(
    watchers: State<'_, Arc<Watchers>>,
    workspace_id: WorkspaceId,
) -> Result<Option<WatchWarning>, IpcError> {
    Ok(watchers.warning(&workspace_id))
}

/// The workspace's tree (UI-1): each group with its Branches node and its
/// members, then Ungrouped with the repos in no group. Each repo has its
/// Local, Remotes, Tags and Stashes nodes not loaded; `refs_get` loads
/// them, and `group_branches` a group's Branches node (ARCH-8).
#[tauri::command]
#[specta::specta]
pub async fn tree_get(
    workspaces: State<'_, Arc<Workspaces>>,
    summaries: State<'_, Arc<Summaries>>,
    trees: State<'_, Arc<Trees>>,
    workspace_id: WorkspaceId,
) -> Result<Vec<TreeNode>, IpcError> {
    Ok(trees.tree(&workspaces, &summaries, &workspace_id)?)
}

/// One repo's Local, Remotes, Tags and Stashes nodes where it appears under
/// group `group_id`, or under Ungrouped for `null`, loaded, with ahead and
/// behind for each local branch.
#[tauri::command]
#[specta::specta]
pub async fn refs_get(
    workspaces: State<'_, Arc<Workspaces>>,
    trees: State<'_, Arc<Trees>>,
    workspace_id: WorkspaceId,
    repo_id: RepoId,
    group_id: Option<GroupId>,
) -> Result<Vec<TreeNode>, IpcError> {
    let (trees, workspaces) = (trees.inner().clone(), workspaces.inner().clone());
    Ok(tree_refs(trees, workspaces, workspace_id, repo_id, group_id).await?)
}

/// The children of a group's Branches node (UI-5): every local branch name
/// of at least one member, with how many members have it.
#[tauri::command]
#[specta::specta]
pub async fn group_branches(
    workspaces: State<'_, Arc<Workspaces>>,
    trees: State<'_, Arc<Trees>>,
    workspace_id: WorkspaceId,
    group_id: GroupId,
) -> Result<Vec<TreeNode>, IpcError> {
    let (trees, workspaces) = (trees.inner().clone(), workspaces.inner().clone());
    Ok(gitcroc_core::group_branches(trees, workspaces, workspace_id, group_id).await?)
}

/// The tree filtered by `text` (UI-8); empty text gives the whole tree.
/// `null` when a newer `tree_filter` of the workspace superseded this one.
#[tauri::command]
#[specta::specta]
pub async fn tree_filter(
    workspaces: State<'_, Arc<Workspaces>>,
    summaries: State<'_, Arc<Summaries>>,
    trees: State<'_, Arc<Trees>>,
    workspace_id: WorkspaceId,
    text: String,
) -> Result<Option<Vec<TreeNode>>, IpcError> {
    let (trees, workspaces) = (trees.inner().clone(), workspaces.inner().clone());
    let summaries = summaries.inner().clone();
    Ok(gitcroc_core::tree_filter(trees, workspaces, summaries, workspace_id, text).await?)
}

/// The first page of `target`'s history in a repo (section 8 "History
/// pane"), filtered by `text` when it is not empty (UI-12). It starts a new
/// walk of the workspace; the previous one ends. `null` when a newer
/// `history_get` of the workspace superseded this one.
#[tauri::command]
#[specta::specta]
pub async fn history_get(
    workspaces: State<'_, Arc<Workspaces>>,
    histories: State<'_, Arc<Histories>>,
    workspace_id: WorkspaceId,
    repo_id: RepoId,
    target: HistoryTarget,
    text: String,
) -> Result<Option<HistoryView>, IpcError> {
    let (histories, workspaces) = (histories.inner().clone(), workspaces.inner().clone());
    Ok(
        gitcroc_core::history_get(histories, workspaces, workspace_id, repo_id, target, text)
            .await?,
    )
}

/// The next page of the walk `handle` returned by `history_get` (ARCH-8).
/// `null` when a newer walk of the workspace replaced it.
#[tauri::command]
#[specta::specta]
pub async fn history_more(
    histories: State<'_, Arc<Histories>>,
    workspace_id: WorkspaceId,
    handle: u32,
) -> Result<Option<HistoryView>, IpcError> {
    let histories = histories.inner().clone();
    Ok(gitcroc_core::history_more(histories, workspace_id, handle).await?)
}

/// Starts writing a repo's commit-graph file as a job and returns its id
/// at once; a write already running for the repo returns its id. Progress
/// arrives as `job_progress`, the end as `job_ended` and
/// `commit_graph_written`.
#[tauri::command]
#[specta::specta]
pub async fn commit_graph_write(
    app: AppHandle,
    workspace_id: WorkspaceId,
    repo_id: RepoId,
) -> Result<JobId, IpcError> {
    let workspaces = app.state::<Arc<Workspaces>>().inner().clone();
    workspaces.repo_path(&workspace_id, &repo_id)?;
    let git = usable_git(&app).await?;
    let histories = app.state::<Arc<Histories>>().inner().clone();
    let jobs = app.state::<Arc<Jobs>>().inner().clone();
    let write = gitcroc_core::commit_graph_write(
        &histories,
        &jobs,
        &workspaces,
        &workspace_id,
        &repo_id,
        git,
    )?;
    if let Some(ended) = write.ended {
        tauri::async_runtime::spawn(async move {
            let Ok(view) = ended.await else {
                return;
            };
            let failed = view.outcome != Some(Outcome::Succeeded);
            // An emit fails only while the app shuts down.
            let _ = CommitGraphWrittenEvent {
                workspace_id,
                repo_id,
                job_id: view.job_id,
                error: failed.then(|| {
                    view.message
                        .clone()
                        .unwrap_or_else(|| "the job did not finish".to_owned())
                }),
                git_stderr: (failed && !view.output.is_empty()).then(|| view.output.clone()),
            }
            .emit(&app);
        });
    }
    Ok(write.job_id)
}

/// This run's jobs of every workspace, newest first, with their outcome
/// and git's output, for the job list (UI-21).
#[tauri::command]
#[specta::specta]
pub async fn jobs_get(jobs: State<'_, Arc<Jobs>>) -> Result<Vec<JobView>, IpcError> {
    Ok(jobs.all_views())
}

/// Cancels a job: one that has not started never runs, a running fetch or
/// clone is killed, any other running job finishes (GRP-10). The end
/// arrives as `job_ended`.
#[tauri::command]
#[specta::specta]
pub async fn job_cancel(
    jobs: State<'_, Arc<Jobs>>,
    job_id: JobId,
) -> Result<CancelResult, IpcError> {
    Ok(jobs.cancel(job_id))
}

/// The user's answer to prompt `prompt_id`, which goes to git and nowhere
/// else (D10); `None` is a cancel, and git gets no answer. A prompt no
/// longer open is ignored.
#[tauri::command]
#[specta::specta]
pub async fn askpass_answer(
    ui: State<'_, Arc<WindowAskpassUi>>,
    prompt_id: u32,
    answer: Option<String>,
) -> Result<(), IpcError> {
    ui.answer(prompt_id, answer);
    Ok(())
}

/// The askpass dialog of the window (D10): prompts go out as
/// `askpass_prompt`, withdrawn ones as `askpass_closed`.
pub fn askpass_ui(app: &AppHandle) -> Arc<WindowAskpassUi> {
    let (shown, closed) = (app.clone(), app.clone());
    Arc::new(WindowAskpassUi::new(
        move |prompt| {
            // An emit fails only while the app shuts down.
            let _ = AskpassPromptEvent(prompt.clone()).emit(&shown);
        },
        move |prompt_id| {
            let _ = AskpassClosedEvent { prompt_id }.emit(&closed);
        },
    ))
}

/// Keeps "don't ask again" for a repo's commit-graph offer (PERF-2).
#[tauri::command]
#[specta::specta]
pub async fn commit_graph_decline(
    app: AppHandle,
    workspace_id: WorkspaceId,
    repo_id: RepoId,
) -> Result<(), IpcError> {
    // Saving touches the disk; keep it off the async workers (ARCH-7).
    let workspaces = app.state::<Arc<Workspaces>>().inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        workspaces.decline_commit_graph(&workspace_id, &repo_id)
    })
    .await??;
    Ok(())
}

/// Saves the changes pane's view: diff mode and options and the file list
/// toggle, the same for every workspace (UI-14, UI-17).
#[tauri::command]
#[specta::specta]
pub async fn settings_set_changes(
    app: AppHandle,
    changes: ChangesSettings,
) -> Result<SettingsView, IpcError> {
    // Saving touches the disk; keep it off the async workers (ARCH-7).
    tauri::async_runtime::spawn_blocking(move || {
        app.state::<Arc<SettingsStore>>().set_changes(changes)
    })
    .await?
    .map_err(IpcError::from)
}

/// Changes the code themes and fonts (UI-24, UI-26) and saves the settings.
/// Font sizes outside 8 to 32 are moved to the nearest limit.
#[tauri::command]
#[specta::specta]
pub async fn settings_set_appearance(
    app: AppHandle,
    appearance: Appearance,
) -> Result<SettingsView, IpcError> {
    // Saving touches the disk; keep it off the async workers (ARCH-7).
    tauri::async_runtime::spawn_blocking(move || {
        app.state::<Arc<SettingsStore>>().set_appearance(appearance)
    })
    .await?
    .map_err(IpcError::from)
}

/// Saves the workspace's pane layout: the dock position and pane sizes
/// (UI-19).
#[tauri::command]
#[specta::specta]
pub async fn workspace_set_layout(
    app: AppHandle,
    workspace_id: WorkspaceId,
    layout: PaneLayout,
) -> Result<(), IpcError> {
    // Saving touches the disk; keep it off the async workers (ARCH-7).
    let workspaces = app.state::<Arc<Workspaces>>().inner().clone();
    tauri::async_runtime::spawn_blocking(move || workspaces.set_layout(&workspace_id, layout))
        .await??;
    Ok(())
}

/// What the changes pane shows for `target` in a repo (UI-14, UI-15,
/// UI-16), as a new view of the workspace; the previous one ends. `null`
/// when a newer `changes_get` of the workspace superseded this one.
#[tauri::command]
#[specta::specta]
pub async fn changes_get(
    app: AppHandle,
    workspace_id: WorkspaceId,
    repo_id: RepoId,
    target: ChangesTarget,
) -> Result<Option<ChangesView>, IpcError> {
    let workspaces = app.state::<Arc<Workspaces>>().inner().clone();
    let changes = app.state::<Arc<Changes>>().inner().clone();
    let git = usable_git(&app).await?;
    Ok(gitcroc_core::changes_get(changes, workspaces, git, workspace_id, repo_id, target).await?)
}

/// The diff of one file of view `handle` (ARCH-8, UI-17, UI-18). `null`
/// when a newer view of the workspace replaced it.
#[tauri::command]
#[specta::specta]
pub async fn diff_get(
    changes: State<'_, Arc<Changes>>,
    workspace_id: WorkspaceId,
    handle: u32,
    file: FileRef,
    request: DiffRequest,
) -> Result<Option<FileDiff>, IpcError> {
    let changes = changes.inner().clone();
    Ok(gitcroc_core::diff_get(changes, workspace_id, handle, file, request).await?)
}

/// One side of the image diff `diff_get` last read for `file`, as base64
/// (UI-18). `null` when that side has no image, or the view or the file is
/// no longer the one read.
#[tauri::command]
#[specta::specta]
pub async fn diff_image(
    changes: State<'_, Arc<Changes>>,
    workspace_id: WorkspaceId,
    handle: u32,
    file: FileRef,
    side: ImageSide,
) -> Result<Option<String>, IpcError> {
    let changes = changes.inner().clone();
    // Encoding up to 10 MB takes a few milliseconds; keep it off the async
    // workers all the same (ARCH-7).
    let encoded = tauri::async_runtime::spawn_blocking(move || {
        gitcroc_core::diff_image(&changes, &workspace_id, handle, file, side)
            .map(|bytes| base64::engine::general_purpose::STANDARD.encode(bytes))
    })
    .await?;
    Ok(encoded)
}

/// Stages, unstages or discards files or one hunk of view `handle` as a
/// job (UI-15). A discard sent without `confirmed` answers `confirm` with
/// the repo and files to name in the dialog, and runs nothing (GRP-9). A
/// hunk whose file changed since its diff was read answers
/// `file_changed`. `null` when a newer view of the workspace replaced it.
#[tauri::command]
#[specta::specta]
pub async fn changes_apply(
    app: AppHandle,
    workspace_id: WorkspaceId,
    request: ChangeRequest,
) -> Result<Option<ChangeStart>, IpcError> {
    let workspaces = app.state::<Arc<Workspaces>>().inner().clone();
    let changes = app.state::<Arc<Changes>>().inner().clone();
    let jobs = app.state::<Arc<Jobs>>().inner().clone();
    let git = usable_git(&app).await?;
    Ok(gitcroc_core::changes_apply(changes, jobs, workspaces, git, workspace_id, request).await?)
}

/// Unmerged file `file` of changes view `handle` as the conflict editor
/// shows it (OPS-4, OPS-6, OPS-8): text hunks from the index stages, or a
/// choice between the two sides, each side labelled with its branch or
/// commit. `already_resolved` when it is no longer in conflict: read the
/// lists again. `null` when a newer view of the workspace replaced it.
#[tauri::command]
#[specta::specta]
pub async fn conflict_get(
    app: AppHandle,
    workspace_id: WorkspaceId,
    handle: u32,
    file: u32,
) -> Result<Option<ConflictRead>, IpcError> {
    let changes = app.state::<Arc<Changes>>().inner().clone();
    Ok(gitcroc_core::conflict_get(changes, workspace_id, handle, file).await?)
}

/// Mark resolved (OPS-5): writes the editor's text to the file and stages
/// it, as a job. `markers_remain` names the first conflict marker line
/// unless the request overrides it; `already_resolved` when the file is no
/// longer in conflict. `null` when a newer view of the workspace replaced
/// it.
#[tauri::command]
#[specta::specta]
pub async fn conflict_resolve(
    app: AppHandle,
    workspace_id: WorkspaceId,
    request: ResolveRequest,
) -> Result<Option<ConflictStart>, IpcError> {
    let changes = app.state::<Arc<Changes>>().inner().clone();
    let jobs = app.state::<Arc<Jobs>>().inner().clone();
    let git = usable_git(&app).await?;
    Ok(gitcroc_core::conflict_resolve(changes, jobs, git, workspace_id, request).await?)
}

/// Resolves a file with one side (OPS-6), as a job. `already_resolved` when
/// it is no longer in conflict. `null` when a newer view of the workspace
/// replaced it.
#[tauri::command]
#[specta::specta]
pub async fn conflict_choose(
    app: AppHandle,
    workspace_id: WorkspaceId,
    request: ChooseRequest,
) -> Result<Option<ConflictStart>, IpcError> {
    let changes = app.state::<Arc<Changes>>().inner().clone();
    let jobs = app.state::<Arc<Jobs>>().inner().clone();
    let git = usable_git(&app).await?;
    Ok(gitcroc_core::conflict_choose(changes, jobs, git, workspace_id, request).await?)
}

/// Commits the staged changes of view `handle`, or amends the last commit
/// with them, as a job (UI-15). Refused without a subject, in conflict
/// state, for a commit with nothing staged and for an amend with no
/// commit. `null` when a newer view of the workspace replaced it.
#[tauri::command]
#[specta::specta]
pub async fn changes_commit(
    app: AppHandle,
    workspace_id: WorkspaceId,
    request: CommitRequest,
) -> Result<Option<JobId>, IpcError> {
    let changes = app.state::<Arc<Changes>>().inner().clone();
    let jobs = app.state::<Arc<Jobs>>().inner().clone();
    let git = usable_git(&app).await?;
    Ok(gitcroc_core::changes_commit(changes, jobs, git, workspace_id, request).await?)
}

/// Runs a branch operation on one repo as a job (section 6 "Branches").
/// Refused for a name git would not take (SEC-4) and for a repo that is
/// busy, missing or in conflict state (GRP-8). A force delete or a delete
/// on a remote sent without `confirmed` answers `confirm` with what to
/// name in the dialog, and runs nothing (GRP-9).
#[tauri::command]
#[specta::specta]
pub async fn op_branch(
    app: AppHandle,
    workspace_id: WorkspaceId,
    request: BranchRequest,
) -> Result<BranchStart, IpcError> {
    let workspaces = app.state::<Arc<Workspaces>>().inner().clone();
    let summaries = app.state::<Arc<Summaries>>().inner().clone();
    let jobs = app.state::<Arc<Jobs>>().inner().clone();
    let git = usable_git(&app).await?;
    Ok(gitcroc_core::op_branch(workspaces, summaries, jobs, git, workspace_id, request).await?)
}

/// What the branch dialogs offer for a repo: start points, its default
/// branch and the remote branches to track.
#[tauri::command]
#[specta::specta]
pub async fn branch_choices(
    app: AppHandle,
    workspace_id: WorkspaceId,
    repo_id: RepoId,
) -> Result<BranchChoices, IpcError> {
    let workspaces = app.state::<Arc<Workspaces>>().inner().clone();
    Ok(gitcroc_core::branch_choices(workspaces, workspace_id, repo_id).await?)
}

/// Why `name` is not a valid branch name, or `null` when it is (SEC-4).
/// Checked as the user types.
#[tauri::command]
#[specta::specta]
pub async fn branch_name_check(name: String) -> Result<Option<String>, IpcError> {
    Ok(gitcroc_core::branch_name_check(&name))
}

/// Fetches, pulls or pushes one repo as a job (section 6 "Sync"), with
/// the transfer's progress as the job's steps. Refused for a repo that is
/// busy, missing or, except for fetch, in conflict state (GRP-8), and for a
/// pull or push on a detached `HEAD`. A force push sent without `confirmed`
/// answers `confirm` and runs nothing (GRP-9); a push that needs a remote
/// picked answers `choose_remote`.
#[tauri::command]
#[specta::specta]
pub async fn op_sync(
    app: AppHandle,
    workspace_id: WorkspaceId,
    request: SyncRequest,
) -> Result<SyncStart, IpcError> {
    let workspaces = app.state::<Arc<Workspaces>>().inner().clone();
    let summaries = app.state::<Arc<Summaries>>().inner().clone();
    let jobs = app.state::<Arc<Jobs>>().inner().clone();
    let git = usable_git(&app).await?;
    Ok(gitcroc_core::op_sync(workspaces, summaries, jobs, git, workspace_id, request).await?)
}

/// What the push options offer for a repo: its branch, upstream and
/// remotes.
#[tauri::command]
#[specta::specta]
pub async fn push_choices(
    app: AppHandle,
    workspace_id: WorkspaceId,
    repo_id: RepoId,
) -> Result<PushChoices, IpcError> {
    let workspaces = app.state::<Arc<Workspaces>>().inner().clone();
    Ok(gitcroc_core::push_choices(workspaces, workspace_id, repo_id).await?)
}

/// Merges, rebases, cherry-picks or reverts in a repo, or aborts or
/// continues the operation it is in, as a job (section 6 "Integrate",
/// OPS-2). Its end arrives as `job_ended`; a revert of a merge commit
/// answers `choose_mainline`, and the abort of a stash apply sent without
/// `confirmed` answers `confirm` and runs nothing.
#[tauri::command]
#[specta::specta]
pub async fn op_integrate(
    app: AppHandle,
    workspace_id: WorkspaceId,
    request: IntegrateRequest,
) -> Result<IntegrateStart, IpcError> {
    let workspaces = app.state::<Arc<Workspaces>>().inner().clone();
    let summaries = app.state::<Arc<Summaries>>().inner().clone();
    let jobs = app.state::<Arc<Jobs>>().inner().clone();
    let git = usable_git(&app).await?;
    Ok(gitcroc_core::op_integrate(workspaces, summaries, jobs, git, workspace_id, request).await?)
}

/// Stashes, applies, pops or drops in one repo as a job (section 6
/// "Stash"). Refused for a repo that is busy, missing or in conflict state
/// (GRP-8) and for an entry the stash list no longer has at its index. A
/// drop sent without `confirmed` answers `confirm` and runs nothing.
#[tauri::command]
#[specta::specta]
pub async fn op_stash(
    app: AppHandle,
    workspace_id: WorkspaceId,
    request: StashRequest,
) -> Result<StashStart, IpcError> {
    let workspaces = app.state::<Arc<Workspaces>>().inner().clone();
    let summaries = app.state::<Arc<Summaries>>().inner().clone();
    let jobs = app.state::<Arc<Jobs>>().inner().clone();
    let git = usable_git(&app).await?;
    Ok(gitcroc_core::op_stash(workspaces, summaries, jobs, git, workspace_id, request).await?)
}

/// Creates, deletes or pushes a tag in one repo as a job (section 6
/// "Tags"). Refused for an invalid name (SEC-4), a repo that is busy,
/// missing or in conflict state (GRP-8), an existing name to create and a
/// missing tag. A delete on the remote sent without `confirmed` answers
/// `confirm` and runs nothing; a push or delete on the remote that needs a
/// remote picked answers `choose_remote`.
#[tauri::command]
#[specta::specta]
pub async fn op_tag(
    app: AppHandle,
    workspace_id: WorkspaceId,
    request: TagRequest,
) -> Result<TagStart, IpcError> {
    let workspaces = app.state::<Arc<Workspaces>>().inner().clone();
    let summaries = app.state::<Arc<Summaries>>().inner().clone();
    let jobs = app.state::<Arc<Jobs>>().inner().clone();
    let git = usable_git(&app).await?;
    Ok(gitcroc_core::op_tag(workspaces, summaries, jobs, git, workspace_id, request).await?)
}

/// Why `name` is not a valid tag name, or `null` when it is (SEC-4).
/// Checked as the user types.
#[tauri::command]
#[specta::specta]
pub async fn tag_name_check(name: String) -> Result<Option<String>, IpcError> {
    Ok(gitcroc_core::tag_name_check(&name))
}

/// Resets the current branch of one repo, or a detached `HEAD`, to a
/// commit as a job (section 6 "Reset", UI-13). Refused for a repo that is
/// busy, missing or in conflict state (GRP-8). A hard reset sent without
/// `confirmed` answers `confirm`, naming what it loses, and runs nothing
/// (GRP-9).
#[tauri::command]
#[specta::specta]
pub async fn op_reset(
    app: AppHandle,
    workspace_id: WorkspaceId,
    request: ResetRequest,
) -> Result<ResetStart, IpcError> {
    let workspaces = app.state::<Arc<Workspaces>>().inner().clone();
    let summaries = app.state::<Arc<Summaries>>().inner().clone();
    let jobs = app.state::<Arc<Jobs>>().inner().clone();
    let git = usable_git(&app).await?;
    Ok(gitcroc_core::op_reset(workspaces, summaries, jobs, git, workspace_id, request).await?)
}

/// Adds, renames or removes a remote of one repo or sets its URLs, as a
/// job (section 6 "Remotes"). Refused for an invalid or taken name (SEC-4),
/// a URL that starts with `-`, a missing remote and a repo that is busy,
/// missing or in conflict state (GRP-8). A remove sent without `confirmed`
/// answers `confirm`, naming the upstreams it unsets, and runs nothing.
#[tauri::command]
#[specta::specta]
pub async fn op_remote(
    app: AppHandle,
    workspace_id: WorkspaceId,
    request: RemoteRequest,
) -> Result<RemoteStart, IpcError> {
    let workspaces = app.state::<Arc<Workspaces>>().inner().clone();
    let summaries = app.state::<Arc<Summaries>>().inner().clone();
    let jobs = app.state::<Arc<Jobs>>().inner().clone();
    let git = usable_git(&app).await?;
    Ok(gitcroc_core::op_remote(workspaces, summaries, jobs, git, workspace_id, request).await?)
}

/// Why `name` is not a valid name for a new remote, or `null` when it is
/// (SEC-4). Checked as the user types.
#[tauri::command]
#[specta::specta]
pub async fn remote_name_check(name: String) -> Result<Option<String>, IpcError> {
    Ok(gitcroc_core::remote_name_check(&name))
}

/// A remote's URLs, for the edit dialog.
#[tauri::command]
#[specta::specta]
pub async fn remote_urls(
    app: AppHandle,
    workspace_id: WorkspaceId,
    repo_id: RepoId,
    name: String,
) -> Result<RemoteUrlsView, IpcError> {
    let workspaces = app.state::<Arc<Workspaces>>().inner().clone();
    Ok(gitcroc_core::remote_urls(workspaces, workspace_id, repo_id, name).await?)
}

/// The user accepted the offer to speed up a large repo's status (PERF-7):
/// turns on the offered keys in its local config as a job. Refused for a
/// repo that is busy, missing or in conflict state (GRP-8).
#[tauri::command]
#[specta::specta]
pub async fn fsmonitor_enable(
    app: AppHandle,
    workspace_id: WorkspaceId,
    repo_id: RepoId,
) -> Result<JobId, IpcError> {
    let workspaces = app.state::<Arc<Workspaces>>().inner().clone();
    let summaries = app.state::<Arc<Summaries>>().inner().clone();
    let changes = app.state::<Arc<Changes>>().inner().clone();
    let jobs = app.state::<Arc<Jobs>>().inner().clone();
    let git = usable_git(&app).await?;
    Ok(gitcroc_core::fsmonitor_enable(
        workspaces,
        summaries,
        changes,
        jobs,
        git,
        workspace_id,
        repo_id,
    )
    .await?)
}

/// Keeps "don't ask again" for a repo's fsmonitor offer (PERF-7).
#[tauri::command]
#[specta::specta]
pub async fn fsmonitor_decline(
    app: AppHandle,
    workspace_id: WorkspaceId,
    repo_id: RepoId,
) -> Result<(), IpcError> {
    // Saving touches the disk; keep it off the async workers (ARCH-7).
    let workspaces = app.state::<Arc<Workspaces>>().inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        workspaces.decline_fsmonitor(&workspace_id, &repo_id)
    })
    .await??;
    Ok(())
}

/// Opens the native folder dialog for a clone's parent folder and returns
/// at once; the folder picked arrives as `clone_parent_picked`. Opened from
/// Rust, as for `workspace_pick_and_add` (SEC-2).
#[tauri::command]
#[specta::specta]
pub async fn clone_pick_parent(app: AppHandle, workspace_id: WorkspaceId) -> Result<(), IpcError> {
    app.state::<Arc<Workspaces>>().view(&workspace_id)?;
    let picked = app.clone();
    app.dialog()
        .file()
        .set_title("Clone into folder")
        .pick_folder(move |folder| {
            if let Some(Ok(path)) = folder.map(|folder| folder.into_path()) {
                let _ = CloneParentPickedEvent { workspace_id, path }.emit(&picked);
            }
        });
    Ok(())
}

/// The folder a clone of `url` gets by default: the last segment of its
/// path without `.git`; `null` when there is none.
#[tauri::command]
#[specta::specta]
pub async fn clone_default_name(url: String) -> Result<Option<String>, IpcError> {
    Ok(gitcroc_core::clone_default_name(&url))
}

/// Why a clone into folder `name` of `parent` would be refused, such as a
/// folder that exists and is not empty, or `null`. Checked as the user
/// types.
#[tauri::command]
#[specta::specta]
pub async fn clone_target_check(parent: PathBuf, name: String) -> Result<Option<String>, IpcError> {
    Ok(gitcroc_core::clone_target_check(parent, name).await)
}

/// Starts cloning `request` and returns its job's id at once. Progress
/// arrives as `job_progress`; at its end, a cloned repo is in the
/// workspace, announced by `workspace_changed`, then `clone_report` names
/// it. Refused before git runs for a read-only workspace, an unusable URL
/// or folder name, or a target that exists and is not empty.
#[tauri::command]
#[specta::specta]
pub async fn clone_start(
    app: AppHandle,
    workspace_id: WorkspaceId,
    request: CloneRequest,
) -> Result<JobId, IpcError> {
    let git = prepare_add(&app, &workspace_id).await?;
    let workspaces = app.state::<Arc<Workspaces>>().inner().clone();
    let jobs = app.state::<Arc<Jobs>>().inner().clone();
    let handle =
        gitcroc_core::clone_start(workspaces.clone(), jobs, git.clone(), workspace_id, request)
            .await?;
    tauri::async_runtime::spawn(async move {
        let Ok(report) = handle.ended.await else {
            return;
        };
        let workspace_id = report.workspace_id.clone();
        let added: Vec<RepoId> = report.repo.iter().map(|repo| repo.id.clone()).collect();
        if !added.is_empty()
            && let Ok(view) = workspaces.view(&workspace_id)
        {
            let _ = WorkspaceChangedEvent(view).emit(&app);
        }
        let _ = CloneReportEvent(report).emit(&app);
        if !added.is_empty() {
            spawn_watch_sync(&app, workspace_id.clone());
        }
        refresh(app, workspace_id, added, git).await;
    });
    Ok(handle.job_id)
}

/// Shows the repo's folder in the system file manager.
#[tauri::command]
#[specta::specta]
pub async fn repo_open_file_manager(
    app: AppHandle,
    workspace_id: WorkspaceId,
    repo_id: RepoId,
) -> Result<(), IpcError> {
    let path = app
        .state::<Arc<Workspaces>>()
        .repo_path(&workspace_id, &repo_id)?;
    let launch = PlatformFileManager::show(&path);
    tauri::async_runtime::spawn_blocking(move || crate::platform::start(&[launch])).await??;
    Ok(())
}

/// Opens a terminal in the repo's folder; fails naming the terminals tried
/// when none is installed.
#[tauri::command]
#[specta::specta]
pub async fn repo_open_terminal(
    app: AppHandle,
    workspace_id: WorkspaceId,
    repo_id: RepoId,
) -> Result<(), IpcError> {
    let path = app
        .state::<Arc<Workspaces>>()
        .repo_path(&workspace_id, &repo_id)?;
    let candidates = PlatformTerminal::open_in(&path);
    tauri::async_runtime::spawn_blocking(move || crate::platform::start(&candidates)).await??;
    Ok(())
}

/// The refs the merge dialog offers for a repo, filtered by `filter`
/// (UI-20).
#[tauri::command]
#[specta::specta]
pub async fn merge_targets(
    app: AppHandle,
    workspace_id: WorkspaceId,
    repo_id: RepoId,
    filter: String,
) -> Result<Vec<String>, IpcError> {
    let workspaces = app.state::<Arc<Workspaces>>().inner().clone();
    Ok(gitcroc_core::merge_targets(workspaces, workspace_id, repo_id, filter).await?)
}

/// What the conflict banner names for a repo in conflict state (OPS-2).
#[tauri::command]
#[specta::specta]
pub async fn conflict_details(
    app: AppHandle,
    workspace_id: WorkspaceId,
    repo_id: RepoId,
) -> Result<ConflictDetails, IpcError> {
    let workspaces = app.state::<Arc<Workspaces>>().inner().clone();
    let git = usable_git(&app).await?;
    Ok(gitcroc_core::conflict_details(workspaces, git, workspace_id, repo_id).await?)
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
    spawn_watch_sync(&app, view.id.clone());
    Ok(view)
}

/// Runs `change` on the workspace's groups off the async workers, as it
/// saves the file (ARCH-7), then sends `workspace_changed`, which also
/// makes the UI read the tree again. Nothing on disk but the workspace
/// file changes (GRP-3).
async fn change_groups<T: Send + 'static>(
    app: &AppHandle,
    workspace_id: WorkspaceId,
    change: impl FnOnce(&Workspaces, &WorkspaceId) -> Result<T, WorkspaceError> + Send + 'static,
) -> Result<(T, WorkspaceView), IpcError> {
    let workspaces = app.state::<Arc<Workspaces>>().inner().clone();
    let (result, view) = tauri::async_runtime::spawn_blocking(move || {
        let result = change(&workspaces, &workspace_id)?;
        Ok::<_, WorkspaceError>((result, workspaces.view(&workspace_id)?))
    })
    .await??;
    let _ = WorkspaceChangedEvent(view.clone()).emit(app);
    Ok((result, view))
}

/// Creates a group as the last one, with `repo_ids` as its members in
/// that order (GRP-2), and returns its id.
#[tauri::command]
#[specta::specta]
pub async fn group_create(
    app: AppHandle,
    workspace_id: WorkspaceId,
    name: String,
    repo_ids: Vec<RepoId>,
) -> Result<GroupId, IpcError> {
    let (group, _) = change_groups(&app, workspace_id, move |workspaces, id| {
        workspaces.create_group(id, &name, &repo_ids)
    })
    .await?;
    Ok(group)
}

/// Renames a group (GRP-2); its id stays.
#[tauri::command]
#[specta::specta]
pub async fn group_rename(
    app: AppHandle,
    workspace_id: WorkspaceId,
    group_id: GroupId,
    name: String,
) -> Result<WorkspaceView, IpcError> {
    let (_, view) = change_groups(&app, workspace_id, move |workspaces, id| {
        workspaces.rename_group(id, &group_id, &name)
    })
    .await?;
    Ok(view)
}

/// Deletes a group; its repos stay in the workspace (GRP-3).
#[tauri::command]
#[specta::specta]
pub async fn group_delete(
    app: AppHandle,
    workspace_id: WorkspaceId,
    group_id: GroupId,
) -> Result<WorkspaceView, IpcError> {
    let (_, view) = change_groups(&app, workspace_id, move |workspaces, id| {
        workspaces.delete_group(id, &group_id)
    })
    .await?;
    Ok(view)
}

/// Adds repos to a group at position `at`, or at the end; a member moves
/// there.
#[tauri::command]
#[specta::specta]
pub async fn group_add_repos(
    app: AppHandle,
    workspace_id: WorkspaceId,
    group_id: GroupId,
    repo_ids: Vec<RepoId>,
    at: Option<u32>,
) -> Result<WorkspaceView, IpcError> {
    let (_, view) = change_groups(&app, workspace_id, move |workspaces, id| {
        workspaces.add_to_group(id, &group_id, &repo_ids, at)
    })
    .await?;
    Ok(view)
}

/// Removes repos from a group; they stay in the workspace (GRP-3).
#[tauri::command]
#[specta::specta]
pub async fn group_remove_repos(
    app: AppHandle,
    workspace_id: WorkspaceId,
    group_id: GroupId,
    repo_ids: Vec<RepoId>,
) -> Result<WorkspaceView, IpcError> {
    let (_, view) = change_groups(&app, workspace_id, move |workspaces, id| {
        workspaces.remove_from_group(id, &group_id, &repo_ids)
    })
    .await?;
    Ok(view)
}

/// Moves members of a group to position `at` among the others.
#[tauri::command]
#[specta::specta]
pub async fn group_move_repos(
    app: AppHandle,
    workspace_id: WorkspaceId,
    group_id: GroupId,
    repo_ids: Vec<RepoId>,
    at: u32,
) -> Result<WorkspaceView, IpcError> {
    let (_, view) = change_groups(&app, workspace_id, move |workspaces, id| {
        workspaces.move_in_group(id, &group_id, &repo_ids, at)
    })
    .await?;
    Ok(view)
}

/// Moves a repo from one group to another at position `at`, or at the
/// end, in one save.
#[tauri::command]
#[specta::specta]
pub async fn group_move_repo_between(
    app: AppHandle,
    workspace_id: WorkspaceId,
    repo_id: RepoId,
    from: GroupId,
    to: GroupId,
    at: Option<u32>,
) -> Result<WorkspaceView, IpcError> {
    let (_, view) = change_groups(&app, workspace_id, move |workspaces, id| {
        workspaces.move_between_groups(id, &repo_id, &from, &to, at)
    })
    .await?;
    Ok(view)
}

/// Puts the groups in the order of `group_ids`, which names each once.
#[tauri::command]
#[specta::specta]
pub async fn group_reorder(
    app: AppHandle,
    workspace_id: WorkspaceId,
    group_ids: Vec<GroupId>,
) -> Result<WorkspaceView, IpcError> {
    let (_, view) = change_groups(&app, workspace_id, move |workspaces, id| {
        workspaces.reorder_groups(id, &group_ids)
    })
    .await?;
    Ok(view)
}

/// The repos of `target` as the workspace is now (GRP-5): a group's members
/// in group order, or a multi-selection in tree order, each repo once. A
/// group or repo that no longer exists is refused.
#[tauri::command]
#[specta::specta]
pub async fn target_repos(
    workspaces: State<'_, Arc<Workspaces>>,
    workspace_id: WorkspaceId,
    target: TargetSet,
) -> Result<Vec<RepoId>, IpcError> {
    Ok(gitcroc_core::target_repos(
        &workspaces,
        &workspace_id,
        &target,
    )?)
}

/// Checks an add can start: the workspace takes changes and git is usable.
async fn prepare_add(app: &AppHandle, workspace_id: &WorkspaceId) -> Result<GitRunner, IpcError> {
    let view = app.state::<Arc<Workspaces>>().view(workspace_id)?;
    if view.read_only {
        return Err(WorkspaceError::ReadOnly.into());
    }
    usable_git(app).await
}

/// A runner for the detected git, or why there is none.
async fn usable_git(app: &AppHandle) -> Result<GitRunner, IpcError> {
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
/// It is in the job list (UI-21) with the outcome of [`AddReport::ending`].
fn spawn_add(app: AppHandle, workspace_id: WorkspaceId, path: PathBuf, git: GitRunner) -> JobId {
    let jobs = app.state::<Arc<Jobs>>().inner().clone();
    let shown = path.display().to_string();
    let tracked = jobs.track(&workspace_id, &format!("Add repos from {shown}"));
    let job_id = tracked
        .as_ref()
        .map_or_else(|_| jobs.next_id(), TrackedJob::job_id);
    tauri::async_runtime::spawn(async move {
        let workspaces = app.state::<Arc<Workspaces>>().inner().clone();
        let tracked = tracked.ok();
        let progress = {
            let report = tracked.as_ref().map(TrackedJob::reporter);
            move |progress: JobProgress| {
                if let Some(report) = &report {
                    report(progress.done, progress.total);
                }
            }
        };
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
        let added: Vec<RepoId> = report.added.iter().map(|repo| repo.id.clone()).collect();
        if let Some(tracked) = tracked {
            let (outcome, output, message) = report.ending();
            tracked.end(outcome, output, message);
        }
        let _ = WorkspaceAddReportEvent(report).emit(&app);
        if !added.is_empty() {
            spawn_watch_sync(&app, workspace_id.clone());
        }
        refresh(app, workspace_id, added, git).await;
    });
    job_id
}

/// Reads the summaries of `repos` again, sending each as `repo_summary`.
/// A repo read again also drops its loaded refs, announced as
/// `repo_refs_changed`.
async fn refresh(app: AppHandle, workspace_id: WorkspaceId, repos: Vec<RepoId>, git: GitRunner) {
    if repos.is_empty() {
        return;
    }
    let workspaces = app.state::<Arc<Workspaces>>().inner().clone();
    let summaries = app.state::<Arc<Summaries>>().inner().clone();
    let emit = summary_sink(app);
    refresh_summaries(workspaces, summaries, workspace_id, repos, git, emit).await;
    perf_mark("first refresh done");
}

/// Sends each summary read as `repo_summary`. A repo read again also drops
/// its loaded refs, announced as `repo_refs_changed`.
fn summary_sink(app: AppHandle) -> impl Fn(SummaryView) + Send + Sync + 'static {
    let trees = app.state::<Arc<Trees>>().inner().clone();
    move |view| {
        let read = !view.refreshing;
        send_summary(&app, &trees, view, read);
    }
}

/// Sends `view` as `repo_summary`; when the repo was `read` again, its
/// loaded refs are dropped too, announced as `repo_refs_changed`.
fn send_summary(app: &AppHandle, trees: &Trees, view: SummaryView, read: bool) {
    if read {
        perf_mark("first repo read");
    }
    let changed = RepoRef {
        workspace_id: view.workspace_id.clone(),
        repo_id: view.repo_id.clone(),
    };
    // An emit fails only while the app shuts down.
    let _ = RepoSummaryEvent(view).emit(app);
    send_group_summaries(app, &changed);
    if read && trees.invalidate(&changed.workspace_id, &changed.repo_id) {
        let _ = RepoRefsChangedEvent(changed).emit(app);
    }
}

/// Sends the summary of each group `changed` is a member of, as
/// `group_summary`: its row's branch and totals follow its members (GRP-4).
fn send_group_summaries(app: &AppHandle, changed: &RepoRef) {
    let workspaces = app.state::<Arc<Workspaces>>();
    let Ok(groups) = workspaces.groups(&changed.workspace_id) else {
        return;
    };
    let member: Vec<&GroupId> = groups
        .iter()
        .filter(|group| group.repos.contains(&changed.repo_id))
        .map(|group| &group.id)
        .collect();
    if member.is_empty() {
        return;
    }
    let summaries = app.state::<Arc<Summaries>>();
    let Ok(rows) = group_summaries(&workspaces, &summaries, &changed.workspace_id) else {
        return;
    };
    for row in rows {
        if member.contains(&&row.group_id) {
            let _ = GroupSummaryEvent(row).emit(app);
        }
    }
}

/// The job engine's events, sent to the window (ARCH-6, ARCH-7).
struct AppJobEvents {
    app: AppHandle,
    trees: Arc<Trees>,
    watchers: Arc<Watchers>,
}

impl JobEvents for AppJobEvents {
    fn progress(&self, job: &JobView) {
        // An emit fails only while the app shuts down.
        let _ = JobProgressEvent(job.clone()).emit(&self.app);
    }

    fn ended(&self, job: &JobView) {
        let _ = JobEndedEvent(job.clone()).emit(&self.app);
    }

    fn summary(&self, view: SummaryView, read: bool) {
        send_summary(&self.app, &self.trees, view, read);
    }

    fn hold_watch(&self, workspace: &WorkspaceId, repo: &RepoId, held: bool) {
        self.watchers.hold(workspace, repo, held);
    }
}

/// The job engine (M2-01), sending its events to the window. Needs the
/// watchers managed first.
pub fn jobs(app: &AppHandle) -> Arc<Jobs> {
    let events = AppJobEvents {
        app: app.clone(),
        trees: app.state::<Arc<Trees>>().inner().clone(),
        watchers: app.state::<Arc<Watchers>>().inner().clone(),
    };
    Jobs::new(
        app.state::<Arc<Workspaces>>().inner().clone(),
        app.state::<Arc<Summaries>>().inner().clone(),
        Arc::new(events),
    )
}

/// The file watchers (PERF-6). A repo that changed on disk is read again
/// (OPS-7); a ref or `HEAD` that changed also raises its `refs_version`, so
/// the history reads it again. A failure is logged and sent once as
/// `watch_warning`.
pub fn watchers(app: &AppHandle) -> Watchers {
    let changed = app.clone();
    let warned = app.clone();
    Watchers::new(
        move |change: RepoChange| {
            let app = changed.clone();
            tauri::async_runtime::spawn(async move {
                // Without a usable git there is nothing to read with yet;
                // the refresh after detection reads every repo.
                let Some(git) = app.state::<GitSetup>().git().await else {
                    return;
                };
                if change.refs {
                    app.state::<Arc<Summaries>>()
                        .refs_changed(&change.workspace_id, &change.repo_id);
                }
                let git = GitRunner::new(git.path);
                refresh(app, change.workspace_id, vec![change.repo_id], git).await;
            });
        },
        move |warning: WatchWarning| {
            warned
                .state::<Arc<AppLog>>()
                .warn(&format!("file watching: {}", warning.message));
            let _ = WatchWarningEvent(warning).emit(&warned);
        },
    )
}

/// Watches the active workspace's repos, in the background (ARCH-7). The
/// other tabs are not watched (UI-30).
pub fn spawn_watching(app: &AppHandle) {
    if let Some(active) = app.state::<Arc<Workspaces>>().active() {
        spawn_watch_sync(app, active);
    }
}

/// Brings the workspace's watches in line with its repos, in the
/// background, when it is the active one.
fn spawn_watch_sync(app: &AppHandle, workspace_id: WorkspaceId) {
    let watchers = app.state::<Arc<Watchers>>().inner().clone();
    let workspaces = app.state::<Arc<Workspaces>>().inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        if workspaces.active().as_ref() == Some(&workspace_id) {
            watchers.sync(&workspaces, &workspace_id);
        }
    });
}

/// Detects git in the background (ARCH-7), sending `git_status` events when it
/// starts and when it is done. Window creation never waits for it. With a
/// usable git, every repo's summary is then read again (PERF-3); until
/// then the cached summaries stand.
/// Records a step of the start for `just bench-ui --verbose` (M1-15); does
/// nothing in release builds.
pub fn perf_mark(_name: &str) {
    #[cfg(any(debug_assertions, feature = "perf-trace"))]
    debug::mark(_name);
}

pub fn spawn_git_detection(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        let setup = app.state::<GitSetup>();
        // An emit fails only while the app shuts down; the state is current
        // either way and `git_status` reads it.
        let _ = GitStatusEvent(GitStatus::Checking).emit(&app);
        let status = setup.detect().await;
        perf_mark("git detected");
        let _ = GitStatusEvent(status).emit(&app);
        let Some(git) = setup.git().await.map(|git| GitRunner::new(git.path)) else {
            return;
        };
        // The active tab only; the others are read when they become
        // active (UI-30).
        let workspaces = app.state::<Arc<Workspaces>>().inner().clone();
        let active = workspaces
            .active()
            .and_then(|active| workspaces.view(&active).ok());
        if let Some(workspace) = active {
            let repos = workspace.repos.into_iter().map(|repo| repo.id).collect();
            tauri::async_runtime::spawn(refresh(app.clone(), workspace.id, repos, git));
        }
    });
}

/// Debug and `perf-trace` builds only: commands to exercise the IPC
/// plumbing by hand, and the UI-side measurements of `just bench-ui`
/// (PERF-8, M1-12).
#[cfg(any(debug_assertions, feature = "perf-trace"))]
pub mod debug {
    use std::path::PathBuf;
    use std::sync::{Mutex, OnceLock};
    use std::time::{Duration, SystemTime, UNIX_EPOCH};

    use serde::{Deserialize, Serialize};
    use specta::Type;
    use tauri::AppHandle;

    use super::{IpcError, IpcErrorCode};

    /// Where `perf_trace_write` writes; set by `just bench-ui`.
    pub const PERF_TRACE_ENV: &str = "GITCROC_PERF_TRACE";

    static PROCESS_START: OnceLock<SystemTime> = OnceLock::new();

    /// The steps of the start, each the first time it happened (M1-15).
    static MARKS: Mutex<Vec<(String, SystemTime)>> = Mutex::new(Vec::new());

    /// Records when the app started, for the cold start measurement. Called
    /// first thing in [`crate::run`].
    pub fn mark_process_start() {
        PROCESS_START.get_or_init(SystemTime::now);
    }

    /// Records when `name` first happened, for the phases of the cold start
    /// that `just bench-ui --verbose` prints. Later calls with the same name
    /// are ignored, and so is everything after the first 200 marks.
    pub fn mark(name: &str) {
        let now = SystemTime::now();
        let Ok(mut marks) = MARKS.lock() else {
            return;
        };
        if marks.len() < 200 && marks.iter().all(|(seen, _)| seen != name) {
            marks.push((name.to_owned(), now));
        }
    }

    /// Marks each time the app's main loop ran a task posted to it more than
    /// 15 ms late, during the first 3 s: the window, the webview's frames
    /// and the IPC answers all wait for that loop (M1-15).
    pub fn probe_main_loop(app: &AppHandle) {
        let app = app.clone();
        std::thread::spawn(move || {
            let begin = std::time::Instant::now();
            while begin.elapsed() < Duration::from_secs(3) {
                let posted = SystemTime::now();
                let (sender, receiver) = std::sync::mpsc::channel();
                if app
                    .run_on_main_thread(move || {
                        let _ = sender.send(SystemTime::now());
                    })
                    .is_err()
                {
                    return;
                }
                let Ok(ran) = receiver.recv() else {
                    return;
                };
                let late = ran.duration_since(posted).unwrap_or_default();
                if late > Duration::from_millis(15)
                    && let Ok(mut marks) = MARKS.lock()
                {
                    marks.push((format!("main loop stalled {} ms", late.as_millis()), posted));
                }
                std::thread::sleep(Duration::from_millis(5));
            }
        });
    }

    fn unix_ms(time: SystemTime) -> Option<f64> {
        time.duration_since(UNIX_EPOCH)
            .ok()
            .map(|since| since.as_secs_f64() * 1000.0)
    }

    /// When the kernel started this process, before the dynamic linker loaded
    /// the app's libraries; `None` off Linux. In clock ticks since boot, so
    /// to 10 ms.
    fn exec_unix_ms() -> Option<f64> {
        if !cfg!(target_os = "linux") {
            return None;
        }
        let stat = std::fs::read_to_string("/proc/self/stat").ok()?;
        let fields: Vec<&str> = stat.get(stat.rfind(')')? + 2..)?.split(' ').collect();
        let ticks: f64 = fields.get(19)?.parse().ok()?;
        let uptime: f64 = std::fs::read_to_string("/proc/uptime")
            .ok()?
            .split(' ')
            .next()?
            .parse()
            .ok()?;
        let now = unix_ms(SystemTime::now())?;
        Some(now - (uptime - ticks / 100.0) * 1000.0)
    }

    /// One measurement the UI took, in milliseconds.
    #[derive(Debug, Clone, Serialize, Deserialize, Type)]
    pub struct PerfSample {
        pub scenario: String,
        pub ms: f64,
    }

    /// What the UI measured in one run of the app.
    #[derive(Debug, Clone, Serialize, Deserialize, Type)]
    pub struct PerfTrace {
        /// When the tree first showed the cached repos, in milliseconds
        /// since the Unix epoch; `None` when it never did.
        pub tree_visible_unix_ms: Option<f64>,
        pub samples: Vec<PerfSample>,
        /// Steps of the start the page saw, as `performance.mark` names
        /// them, in milliseconds since the Unix epoch (M1-15).
        pub marks: Vec<PerfMark>,
    }

    /// One step of the start and when it happened, in milliseconds since the
    /// Unix epoch.
    #[derive(Debug, Clone, Serialize, Deserialize, Type)]
    pub struct PerfMark {
        pub name: String,
        pub unix_ms: f64,
    }

    fn internal(message: String) -> IpcError {
        IpcError {
            code: IpcErrorCode::Internal,
            message,
            git_stderr: None,
        }
    }

    /// Writes the UI's measurements to the JSON file `$GITCROC_PERF_TRACE`
    /// names, with the cold start (process start to tree visible) worked
    /// out. Fails in builds without the `perf-trace` feature.
    #[tauri::command]
    #[specta::specta]
    pub async fn perf_trace_write(trace: PerfTrace) -> Result<(), IpcError> {
        if !cfg!(feature = "perf-trace") {
            return Err(internal("this build has no perf-trace feature".into()));
        }
        let path = std::env::var_os(PERF_TRACE_ENV)
            .map(PathBuf::from)
            .ok_or_else(|| internal(format!("{PERF_TRACE_ENV} is not set")))?;
        let started = PROCESS_START.get().copied().and_then(unix_ms);
        let cold_start_ms = trace
            .tree_visible_unix_ms
            .zip(started)
            .map(|(visible, started)| visible - started);
        let json = serde_json::json!({
            "process_start_unix_ms": started,
            "cold_start_ms": cold_start_ms,
            "samples": trace.samples,
            "exec_unix_ms": exec_unix_ms(),
            "core_marks": core_marks(),
            "page_marks": trace.marks,
        });
        let text = serde_json::to_vec_pretty(&json).map_err(|error| internal(error.to_string()))?;
        tokio::fs::write(&path, text)
            .await
            .map_err(|error| internal(format!("cannot write {}: {error}", path.display())))
    }

    fn core_marks() -> Vec<PerfMark> {
        let Ok(marks) = MARKS.lock() else {
            return Vec::new();
        };
        marks
            .iter()
            .filter_map(|(name, time)| {
                Some(PerfMark {
                    name: name.clone(),
                    unix_ms: unix_ms(*time)?,
                })
            })
            .collect()
    }
}

/// Expands `$collect![...]` with every registered command, so the invoke
/// handler and the checks on the command list cannot drift apart.
macro_rules! with_commands {
    ($collect:ident) => {{
        #[cfg(any(debug_assertions, feature = "perf-trace"))]
        {
            $collect![
                app_info,
                git_status,
                git_recheck,
                settings_get,
                settings_set_theme,
                workspace_get,
                workspace_list,
                workspace_create,
                workspace_open,
                workspace_close,
                workspace_rename,
                workspace_delete,
                workspace_reorder,
                workspace_activate,
                workspace_add_path,
                workspace_pick_and_add,
                workspace_remove_repo,
                workspace_summaries,
                repo_select,
                tree_get,
                refs_get,
                tree_filter,
                group_branches,
                group_create,
                group_rename,
                group_delete,
                group_add_repos,
                group_remove_repos,
                group_move_repos,
                group_move_repo_between,
                group_reorder,
                target_repos,
                history_get,
                history_more,
                commit_graph_write,
                commit_graph_decline,
                jobs_get,
                job_cancel,
                settings_set_changes,
                workspace_set_layout,
                changes_get,
                diff_get,
                diff_image,
                changes_apply,
                changes_commit,
                op_branch,
                branch_choices,
                branch_name_check,
                op_sync,
                push_choices,
                op_integrate,
                merge_targets,
                op_stash,
                op_tag,
                tag_name_check,
                op_reset,
                op_remote,
                remote_name_check,
                remote_urls,
                fsmonitor_enable,
                fsmonitor_decline,
                clone_pick_parent,
                clone_default_name,
                clone_target_check,
                clone_start,
                repo_open_file_manager,
                repo_open_terminal,
                conflict_details,
                conflict_get,
                conflict_resolve,
                conflict_choose,
                settings_set_appearance,
                workspace_refresh,
                watch_warning,
                askpass_answer,
                debug::perf_trace_write
            ]
        }
        #[cfg(not(any(debug_assertions, feature = "perf-trace")))]
        {
            $collect![
                app_info,
                git_status,
                git_recheck,
                settings_get,
                settings_set_theme,
                workspace_get,
                workspace_list,
                workspace_create,
                workspace_open,
                workspace_close,
                workspace_rename,
                workspace_delete,
                workspace_reorder,
                workspace_activate,
                workspace_add_path,
                workspace_pick_and_add,
                workspace_remove_repo,
                workspace_summaries,
                repo_select,
                tree_get,
                refs_get,
                tree_filter,
                group_branches,
                group_create,
                group_rename,
                group_delete,
                group_add_repos,
                group_remove_repos,
                group_move_repos,
                group_move_repo_between,
                group_reorder,
                target_repos,
                history_get,
                history_more,
                commit_graph_write,
                commit_graph_decline,
                jobs_get,
                job_cancel,
                settings_set_changes,
                workspace_set_layout,
                changes_get,
                diff_get,
                diff_image,
                changes_apply,
                changes_commit,
                op_branch,
                branch_choices,
                branch_name_check,
                op_sync,
                push_choices,
                op_integrate,
                merge_targets,
                op_stash,
                op_tag,
                tag_name_check,
                op_reset,
                op_remote,
                remote_name_check,
                remote_urls,
                fsmonitor_enable,
                fsmonitor_decline,
                clone_pick_parent,
                clone_default_name,
                clone_target_check,
                clone_start,
                repo_open_file_manager,
                repo_open_terminal,
                conflict_details,
                conflict_get,
                conflict_resolve,
                conflict_choose,
                settings_set_appearance,
                workspace_refresh,
                watch_warning,
                askpass_answer
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
            GroupSummaryEvent,
            WorkspacesChangedEvent,
            WorkspaceAddReportEvent,
            CloneParentPickedEvent,
            CloneReportEvent,
            RepoSummaryEvent,
            RepoRefsChangedEvent,
            CommitGraphWrittenEvent,
            WatchWarningEvent,
            JobEndedEvent,
            AskpassPromptEvent,
            AskpassClosedEvent
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
