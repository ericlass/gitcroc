//! The IPC surface: commands, events and the typed error, all exported to
//! `ui/src/ipc/bindings.ts` by tauri-specta. Commands only forward to the core.
//!
//! Every command is `async` and returns `Result<_, IpcError>` (ARCH-7);
//! `arch_7_commands_are_async` checks the first rule for everything listed in
//! [`with_commands!`].

use std::path::{Path, PathBuf};
use std::sync::Arc;

use gitcroc_core::{
    AddReport, AppInfo, CommitGraphError, GitError, GitRunner, GitSetup, GitStatus, Histories,
    HistoryTarget, HistoryView, HistoryViewError, JobId, JobIds, JobProgress, Refusal, RepoId,
    SettingsError, SettingsStore, SettingsView, Summaries, SummaryView, Theme, TreeError, TreeNode,
    Trees, WorkspaceError, WorkspaceId, WorkspaceView, Workspaces, add_path, commit_graph_begin,
    refresh_summaries, tree_refs,
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

impl From<TreeError> for IpcError {
    fn from(error: TreeError) -> Self {
        match error {
            TreeError::Workspace(error) => error.into(),
            TreeError::Open(_) | TreeError::Refs(_) | TreeError::Stopped(_) => Self {
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

/// The workspace's tree (UI-1): Ungrouped with every repo, each with its
/// Local, Remotes, Tags and Stashes nodes not loaded; `refs_get` loads
/// them (ARCH-8).
#[tauri::command]
#[specta::specta]
pub async fn tree_get(
    workspaces: State<'_, Arc<Workspaces>>,
    trees: State<'_, Arc<Trees>>,
    workspace_id: WorkspaceId,
) -> Result<Vec<TreeNode>, IpcError> {
    Ok(trees.tree(&workspaces, &workspace_id)?)
}

/// One repo's Local, Remotes, Tags and Stashes nodes, loaded, with ahead
/// and behind for each local branch.
#[tauri::command]
#[specta::specta]
pub async fn refs_get(
    workspaces: State<'_, Arc<Workspaces>>,
    trees: State<'_, Arc<Trees>>,
    workspace_id: WorkspaceId,
    repo_id: RepoId,
) -> Result<Vec<TreeNode>, IpcError> {
    let (trees, workspaces) = (trees.inner().clone(), workspaces.inner().clone());
    Ok(tree_refs(trees, workspaces, workspace_id, repo_id).await?)
}

/// The tree filtered by `text` (UI-8); empty text gives the whole tree.
/// `null` when a newer `tree_filter` of the workspace superseded this one.
#[tauri::command]
#[specta::specta]
pub async fn tree_filter(
    workspaces: State<'_, Arc<Workspaces>>,
    trees: State<'_, Arc<Trees>>,
    workspace_id: WorkspaceId,
    text: String,
) -> Result<Option<Vec<TreeNode>>, IpcError> {
    let (trees, workspaces) = (trees.inner().clone(), workspaces.inner().clone());
    Ok(gitcroc_core::tree_filter(trees, workspaces, workspace_id, text).await?)
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

/// Starts writing a repo's commit-graph file and returns the job id at
/// once; a write already running for the repo returns its id. Progress
/// arrives as `job_progress`, the result as `commit_graph_written`.
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
    let next = app.state::<JobIds>().next();
    let (job_id, new) = commit_graph_begin(&histories, &workspace_id, &repo_id, next);
    if new {
        tauri::async_runtime::spawn(async move {
            let progress = {
                let app = app.clone();
                // An emit fails only while the app shuts down.
                move |progress| {
                    let _ = JobProgressEvent(progress).emit(&app);
                }
            };
            let result = gitcroc_core::commit_graph_write(
                histories,
                workspaces,
                workspace_id.clone(),
                repo_id.clone(),
                git,
                job_id,
                progress,
            )
            .await;
            let git_stderr = match &result {
                Err(CommitGraphError::Git(GitError::Failed { stderr, .. })) => {
                    Some(String::from_utf8_lossy(stderr).trim_end().to_owned())
                }
                _ => None,
            };
            let _ = CommitGraphWrittenEvent {
                workspace_id,
                repo_id,
                job_id,
                error: result.err().map(|error| error.to_string()),
                git_stderr,
            }
            .emit(&app);
        });
    }
    Ok(job_id)
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
/// A repo read again also drops its loaded refs, announced as
/// `repo_refs_changed`.
async fn refresh(app: AppHandle, workspace_id: WorkspaceId, repos: Vec<RepoId>, git: GitRunner) {
    if repos.is_empty() {
        return;
    }
    let workspaces = app.state::<Arc<Workspaces>>().inner().clone();
    let summaries = app.state::<Arc<Summaries>>().inner().clone();
    let trees = app.state::<Arc<Trees>>().inner().clone();
    refresh_summaries(
        workspaces,
        summaries,
        workspace_id,
        repos,
        git,
        move |view| {
            let read = !view.refreshing;
            let changed = RepoRef {
                workspace_id: view.workspace_id.clone(),
                repo_id: view.repo_id.clone(),
            };
            // An emit fails only while the app shuts down.
            let _ = RepoSummaryEvent(view).emit(&app);
            if read && trees.invalidate(&changed.workspace_id, &changed.repo_id) {
                let _ = RepoRefsChangedEvent(changed).emit(&app);
            }
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
                tree_get,
                refs_get,
                tree_filter,
                history_get,
                history_more,
                commit_graph_write,
                commit_graph_decline,
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
                repo_select,
                tree_get,
                refs_get,
                tree_filter,
                history_get,
                history_more,
                commit_graph_write,
                commit_graph_decline
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
            RepoSummaryEvent,
            RepoRefsChangedEvent,
            CommitGraphWrittenEvent
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
