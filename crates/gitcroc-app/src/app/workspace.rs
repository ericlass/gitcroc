//! Workspaces: the tabs, their repos, adding repos and reading summaries.

use std::path::PathBuf;

use gitcroc_core::{
    AddReport, GitRunner, JobId, JobProgress, MissingRepo, PaneLayout, Refusal, RepoId,
    SummaryView, TrackedJob, WatchWarning, WorkspaceError, WorkspaceId, WorkspaceListItem,
    WorkspaceTabs, WorkspaceView, Workspaces, add_path, refresh_workspace,
};

use super::{App, AppError, AppEvent};

impl App {
    /// The open workspaces in tab order, the active one, and the saved tabs
    /// whose file was gone at start (UI-27, UI-33).
    pub fn workspace_get(&self) -> WorkspaceTabs {
        self.workspaces().tabs()
    }

    /// Every workspace file, open or not, sorted by name (UI-28).
    pub async fn workspace_list(&self) -> Result<Vec<WorkspaceListItem>, AppError> {
        let workspaces = self.workspaces().clone();
        Ok(self.blocking(move || workspaces.list()).await??)
    }

    /// Creates a workspace, opened as the last tab and made active.
    pub async fn workspace_create(&self, name: String) -> Result<WorkspaceTabs, AppError> {
        self.change_tabs(move |workspaces| Ok(Some(workspaces.create(&name)?)))
            .await
    }

    /// Opens a workspace as the last tab and makes it active; one already
    /// open is made active instead (UI-27).
    pub async fn workspace_open(
        &self,
        workspace_id: WorkspaceId,
    ) -> Result<WorkspaceTabs, AppError> {
        self.change_tabs(move |workspaces| {
            Ok(workspaces.open(&workspace_id)?.then_some(workspace_id))
        })
        .await
    }

    /// Closes a workspace's tab. Its file and repos stay, and its jobs run
    /// on (UI-28, UI-21).
    pub async fn workspace_close(
        &self,
        workspace_id: WorkspaceId,
    ) -> Result<WorkspaceTabs, AppError> {
        let closed = workspace_id.clone();
        let tabs = self
            .change_tabs(move |workspaces| {
                workspaces.close(&workspace_id)?;
                Ok(None)
            })
            .await?;
        // Its state goes with its tab (ARCH-9).
        self.inner.watchers.stop(&closed);
        self.trees().forget(&closed);
        self.inner.palettes.forget(&closed);
        self.inner.histories.forget(&closed);
        self.inner.changes.forget(&closed);
        self.summaries().forget(&closed);
        Ok(tabs)
    }

    /// Renames a workspace, open or not.
    pub async fn workspace_rename(
        &self,
        workspace_id: WorkspaceId,
        name: String,
    ) -> Result<WorkspaceTabs, AppError> {
        let tabs = self
            .change_tabs(move |workspaces| {
                workspaces.rename(&workspace_id, &name)?;
                Ok(None)
            })
            .await?;
        for view in &tabs.open {
            self.send(AppEvent::WorkspaceChanged(view.clone()));
        }
        Ok(tabs)
    }

    /// Deletes the file of a workspace that is not open, and nothing else
    /// (GRP-3). Needs `confirmed`.
    pub async fn workspace_delete(
        &self,
        workspace_id: WorkspaceId,
        confirmed: bool,
    ) -> Result<(), AppError> {
        let workspaces = self.workspaces().clone();
        self.blocking(move || workspaces.delete(&workspace_id, confirmed))
            .await??;
        Ok(())
    }

    /// Puts the tabs in the order of `workspace_ids`, which names each open
    /// workspace once.
    pub async fn workspace_reorder(
        &self,
        workspace_ids: Vec<WorkspaceId>,
    ) -> Result<WorkspaceTabs, AppError> {
        self.change_tabs(move |workspaces| {
            workspaces.reorder(&workspace_ids)?;
            Ok(None)
        })
        .await
    }

    /// Makes an open workspace the active tab. It is then watched, and its
    /// summaries are read again as when the window regains focus (UI-30).
    pub async fn workspace_activate(
        &self,
        workspace_id: WorkspaceId,
    ) -> Result<WorkspaceTabs, AppError> {
        self.change_tabs(move |workspaces| {
            workspaces.activate(&workspace_id)?;
            Ok(None)
        })
        .await
    }

    /// Runs `change` off the caller's thread, as it saves files, then sends
    /// `WorkspacesChanged`. `change` returns the workspace it opened, whose
    /// cached summaries are then taken. When the active tab changed, the old
    /// one goes to the background and the new one is watched and read again
    /// (UI-30, UI-31).
    pub(super) async fn change_tabs(
        &self,
        change: impl FnOnce(&Workspaces) -> Result<Option<WorkspaceId>, WorkspaceError> + Send + 'static,
    ) -> Result<WorkspaceTabs, AppError> {
        let workspaces = self.workspaces().clone();
        let summaries = self.summaries().clone();
        let (before, tabs) = self
            .blocking(move || {
                let before = workspaces.active();
                if let Some(opened) = change(&workspaces)? {
                    summaries.open(&workspaces, &opened);
                }
                Ok::<_, WorkspaceError>((before, workspaces.tabs()))
            })
            .await??;
        self.send(AppEvent::WorkspacesChanged(tabs.clone()));
        if tabs.active != before {
            let switch = self.tab_switch();
            if let Some(before) = &before {
                for view in switch.background(before) {
                    self.inner.hub.summary(view, false);
                }
            }
            if let Some(active) = tabs.active.clone() {
                let git = self.usable_git().await.ok();
                let emit = self.inner.hub.summary_sink();
                self.inner.runtime.spawn(async move {
                    // A workspace closed since has nothing to read.
                    let _ = switch.foreground(active, git, emit).await;
                });
            }
        }
        Ok(tabs)
    }

    /// Starts a job that adds the repo at `path`, or every repo found below
    /// it, and returns its id at once. Progress arrives as `JobProgress`
    /// events, then `WorkspaceChanged` and `WorkspaceAddReport` (ARCH-7).
    pub async fn workspace_add_path(
        &self,
        workspace_id: WorkspaceId,
        path: PathBuf,
    ) -> Result<JobId, AppError> {
        let git = self.prepare_add(&workspace_id).await?;
        Ok(self.spawn_add(workspace_id, path, git))
    }

    /// Checks an add can start: the workspace takes changes and git is
    /// usable.
    pub(super) async fn prepare_add(
        &self,
        workspace_id: &WorkspaceId,
    ) -> Result<GitRunner, AppError> {
        if self.workspaces().view(workspace_id)?.read_only {
            return Err(WorkspaceError::ReadOnly.into());
        }
        self.usable_git().await
    }

    /// Runs an add job in the background and returns its id. It is in the
    /// job list (UI-21) with the outcome of [`AddReport::ending`].
    pub(super) fn spawn_add(
        &self,
        workspace_id: WorkspaceId,
        path: PathBuf,
        git: GitRunner,
    ) -> JobId {
        let jobs = &self.inner.jobs;
        let shown = path.display().to_string();
        let tracked = jobs.track(&workspace_id, &format!("Add repos from {shown}"));
        let job_id = tracked
            .as_ref()
            .map_or_else(|_| jobs.next_id(), TrackedJob::job_id);
        let app = self.clone();
        self.inner.runtime.spawn(async move {
            let workspaces = app.workspaces().clone();
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
                app.send(AppEvent::WorkspaceChanged(view));
            }
            let added: Vec<RepoId> = report.added.iter().map(|repo| repo.id.clone()).collect();
            if let Some(tracked) = tracked {
                let (outcome, output, message) = report.ending();
                tracked.end(outcome, output, message);
            }
            app.send(AppEvent::WorkspaceAddReport(report));
            if !added.is_empty() {
                app.spawn_watch_sync(workspace_id.clone());
            }
            app.inner.hub.refresh(workspace_id, added, git).await;
        });
        job_id
    }

    /// The repos of the workspace whose folder is missing or is no repo,
    /// with their remote and default clone target (DATA-2).
    pub async fn workspace_missing_repos(
        &self,
        workspace_id: WorkspaceId,
    ) -> Result<Vec<MissingRepo>, AppError> {
        let workspaces = self.workspaces().clone();
        Ok(self
            .blocking(move || workspaces.missing_repos(&workspace_id))
            .await??)
    }

    /// The current summary of every repo of the workspace, in workspace
    /// order: at start the cached ones, marked stale; then each fresh one as
    /// it is read, which also arrives as `RepoSummary` (PERF-3).
    pub fn workspace_summaries(
        &self,
        workspace_id: &WorkspaceId,
    ) -> Result<Vec<SummaryView>, AppError> {
        Ok(self.summaries().views(self.workspaces(), workspace_id)?)
    }

    /// Tells the core which repo of the workspace is selected, so it is
    /// read first when its summary waits for a refresh (PERF-4) and its
    /// working tree is the one watched (PERF-6). `None` clears it. Returns
    /// at once; the watch moves in the background.
    pub fn repo_select(&self, workspace_id: WorkspaceId, repo_id: Option<RepoId>) {
        self.summaries().select(&workspace_id, repo_id.clone());
        let watchers = self.inner.watchers.clone();
        // Recorded here, so selections keep their order; the watch follows
        // in the background, as a large working tree takes a watch per
        // folder (ARCH-7).
        watchers.select(&workspace_id, repo_id);
        self.inner
            .runtime
            .spawn_blocking(move || watchers.follow_selection(&workspace_id));
    }

    /// Reads every repo of the workspace again, the selected one first, as
    /// when the window regains focus (OPS-7). Returns at once; each summary
    /// arrives as `RepoSummary`.
    pub async fn workspace_refresh(&self, workspace_id: WorkspaceId) -> Result<(), AppError> {
        self.workspaces().view(&workspace_id)?;
        let git = self.usable_git().await?;
        let (workspaces, summaries) = (self.workspaces().clone(), self.summaries().clone());
        let emit = self.inner.hub.summary_sink();
        self.inner.runtime.spawn(async move {
            // The workspace was checked above; one closed since has nothing
            // to read.
            let _ = refresh_workspace(workspaces, summaries, workspace_id, git, emit).await;
        });
        Ok(())
    }

    /// Why watching the workspace's files failed, if it did; the same
    /// warning is also sent once as `WatchWarning`.
    pub fn watch_warning(&self, workspace_id: &WorkspaceId) -> Option<WatchWarning> {
        self.inner.watchers.warning(workspace_id)
    }

    /// Saves the workspace's pane layout: the dock position and pane sizes
    /// (UI-19).
    pub async fn workspace_set_layout(
        &self,
        workspace_id: WorkspaceId,
        layout: PaneLayout,
    ) -> Result<(), AppError> {
        let workspaces = self.workspaces().clone();
        self.blocking(move || workspaces.set_layout(&workspace_id, layout))
            .await??;
        Ok(())
    }

    /// Turns AI on or off for the workspace (AI-6) and saves its file; the
    /// view also arrives as `WorkspaceChanged`.
    pub async fn workspace_set_ai(
        &self,
        workspace_id: WorkspaceId,
        enabled: bool,
    ) -> Result<WorkspaceView, AppError> {
        let workspaces = self.workspaces().clone();
        let view = self
            .blocking(move || workspaces.set_ai(&workspace_id, enabled))
            .await??;
        self.send(AppEvent::WorkspaceChanged(view.clone()));
        Ok(view)
    }

    /// Removes a repo from the workspace; nothing on disk changes (GRP-3).
    pub async fn workspace_remove_repo(
        &self,
        workspace_id: WorkspaceId,
        repo_id: RepoId,
    ) -> Result<WorkspaceView, AppError> {
        let workspaces = self.workspaces().clone();
        let view = self
            .blocking(move || workspaces.remove_repo(&workspace_id, &repo_id))
            .await??;
        self.send(AppEvent::WorkspaceChanged(view.clone()));
        self.spawn_watch_sync(view.id.clone());
        Ok(view)
    }
}
