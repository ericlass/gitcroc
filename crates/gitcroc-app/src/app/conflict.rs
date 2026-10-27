//! Conflicts: the banner's details and the conflict editor.

use gitcroc_core::{
    ChooseRequest, ConflictDetails, ConflictRead, ConflictStart, RepoId, ResolveRequest,
    WorkspaceId,
};

use super::{App, AppError};

impl App {
    /// What the conflict banner names for a repo in conflict state (OPS-2).
    pub async fn conflict_details(
        &self,
        workspace_id: WorkspaceId,
        repo_id: RepoId,
    ) -> Result<ConflictDetails, AppError> {
        let git = self.usable_git().await?;
        Ok(
            gitcroc_core::conflict_details(self.workspaces().clone(), git, workspace_id, repo_id)
                .await?,
        )
    }

    /// Unmerged file `file` of changes view `handle` as the conflict editor
    /// shows it (OPS-4, OPS-6, OPS-8): text hunks from the index stages, or
    /// a choice between the two sides, each side labelled with its branch
    /// or commit. `already_resolved` when it is no longer in conflict: read
    /// the lists again. `None` when a newer view of the workspace replaced
    /// it.
    pub async fn conflict_get(
        &self,
        workspace_id: WorkspaceId,
        handle: u32,
        file: u32,
    ) -> Result<Option<ConflictRead>, AppError> {
        let changes = self.inner.changes.clone();
        Ok(gitcroc_core::conflict_get(changes, workspace_id, handle, file).await?)
    }

    /// Mark resolved (OPS-5): writes the editor's text to the file and
    /// stages it, as a job. `markers_remain` names the first conflict marker
    /// line unless the request overrides it; `already_resolved` when the
    /// file is no longer in conflict. `None` when a newer view of the
    /// workspace replaced it.
    pub async fn conflict_resolve(
        &self,
        workspace_id: WorkspaceId,
        request: ResolveRequest,
    ) -> Result<Option<ConflictStart>, AppError> {
        let git = self.usable_git().await?;
        Ok(gitcroc_core::conflict_resolve(
            self.inner.changes.clone(),
            self.inner.jobs.clone(),
            git,
            workspace_id,
            request,
        )
        .await?)
    }

    /// Resolves a file with one side (OPS-6), as a job. `already_resolved`
    /// when it is no longer in conflict. `None` when a newer view of the
    /// workspace replaced it.
    pub async fn conflict_choose(
        &self,
        workspace_id: WorkspaceId,
        request: ChooseRequest,
    ) -> Result<Option<ConflictStart>, AppError> {
        let git = self.usable_git().await?;
        Ok(gitcroc_core::conflict_choose(
            self.inner.changes.clone(),
            self.inner.jobs.clone(),
            git,
            workspace_id,
            request,
        )
        .await?)
    }
}
