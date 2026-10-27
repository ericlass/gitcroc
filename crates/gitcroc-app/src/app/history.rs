//! The history pane and the commit-graph offer.

use gitcroc_core::{HistoryTarget, HistoryView, JobId, Outcome, RepoId, WorkspaceId};

use super::{App, AppError, AppEvent};

impl App {
    /// The first page of `target`'s history (section 8 "History pane"): of
    /// repo `repo_id`, or merged from a target set's repos with `repo_id`
    /// `None` (PERF-5), filtered by `text` when it is not empty (UI-12). It
    /// starts a new walk of the workspace; the previous one ends. `None`
    /// when a newer `history_get` of the workspace superseded this one.
    pub async fn history_get(
        &self,
        workspace_id: WorkspaceId,
        repo_id: Option<RepoId>,
        target: HistoryTarget,
        text: String,
    ) -> Result<Option<HistoryView>, AppError> {
        let (histories, workspaces) = (self.inner.histories.clone(), self.workspaces().clone());
        Ok(
            gitcroc_core::history_get(histories, workspaces, workspace_id, repo_id, target, text)
                .await?,
        )
    }

    /// The next page of the walk `handle` that `history_get` returned
    /// (ARCH-8). `None` when a newer walk of the workspace replaced it.
    pub async fn history_more(
        &self,
        workspace_id: WorkspaceId,
        handle: u32,
    ) -> Result<Option<HistoryView>, AppError> {
        let histories = self.inner.histories.clone();
        Ok(gitcroc_core::history_more(histories, workspace_id, handle).await?)
    }

    /// Starts writing a repo's commit-graph file as a job and returns its id
    /// at once; a write already running for the repo returns its id.
    /// Progress arrives as `JobProgress`, the end as `JobEnded` and
    /// `CommitGraphWritten`.
    pub async fn commit_graph_write(
        &self,
        workspace_id: WorkspaceId,
        repo_id: RepoId,
    ) -> Result<JobId, AppError> {
        self.workspaces().repo_path(&workspace_id, &repo_id)?;
        let git = self.usable_git().await?;
        let write = gitcroc_core::commit_graph_write(
            &self.inner.histories,
            &self.inner.jobs,
            self.workspaces(),
            &workspace_id,
            &repo_id,
            git,
        )?;
        if let Some(ended) = write.ended {
            let app = self.clone();
            self.inner.runtime.spawn(async move {
                let Ok(view) = ended.await else {
                    return;
                };
                let failed = view.outcome != Some(Outcome::Succeeded);
                app.send(AppEvent::CommitGraphWritten {
                    workspace_id,
                    repo_id,
                    job_id: view.job_id,
                    error: failed.then(|| {
                        view.message
                            .clone()
                            .unwrap_or_else(|| "the job did not finish".to_owned())
                    }),
                    git_stderr: (failed && !view.output.is_empty()).then(|| view.output.clone()),
                });
            });
        }
        Ok(write.job_id)
    }

    /// Keeps "don't ask again" for a repo's commit-graph offer (PERF-2).
    pub async fn commit_graph_decline(
        &self,
        workspace_id: WorkspaceId,
        repo_id: RepoId,
    ) -> Result<(), AppError> {
        let workspaces = self.workspaces().clone();
        self.blocking(move || workspaces.decline_commit_graph(&workspace_id, &repo_id))
            .await??;
        Ok(())
    }
}
