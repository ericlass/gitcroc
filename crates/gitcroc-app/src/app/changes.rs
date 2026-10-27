//! The changes pane: its lists, diffs, staging and commit.

use gitcroc_core::{
    ChangeRequest, ChangeStart, ChangesTarget, ChangesView, CommitRequest, DiffRequest, FileDiff,
    FileRef, ImageSide, JobId, RepoId, TargetSet, WorkspaceId,
};

use super::{App, AppError};

impl App {
    /// What the changes pane shows for `target` in a repo (UI-14, UI-15,
    /// UI-16), as a new view of the workspace; the previous one ends.
    /// `None` when a newer `changes_get` of the workspace superseded this
    /// one.
    pub async fn changes_get(
        &self,
        workspace_id: WorkspaceId,
        repo_id: RepoId,
        target: ChangesTarget,
    ) -> Result<Option<ChangesView>, AppError> {
        let git = self.usable_git().await?;
        Ok(gitcroc_core::changes_get(
            self.inner.changes.clone(),
            self.workspaces().clone(),
            git,
            workspace_id,
            repo_id,
            target,
        )
        .await?)
    }

    /// What the changes pane shows for "Uncommitted changes" of a group or
    /// several repos (UI-34): the lists of every dirty repo of `target`,
    /// each with its own handle, as a new view of the workspace; the
    /// previous one ends. `None` when a newer view of the workspace
    /// superseded this one.
    pub async fn changes_get_merged(
        &self,
        workspace_id: WorkspaceId,
        target: TargetSet,
    ) -> Result<Option<ChangesView>, AppError> {
        let git = self.usable_git().await?;
        Ok(gitcroc_core::changes_get_merged(
            self.inner.changes.clone(),
            self.workspaces().clone(),
            git,
            workspace_id,
            target,
        )
        .await?)
    }

    /// The diff of one file of view `handle` (ARCH-8, UI-17, UI-18). `None`
    /// when a newer view of the workspace replaced it.
    pub async fn diff_get(
        &self,
        workspace_id: WorkspaceId,
        handle: u32,
        file: FileRef,
        request: DiffRequest,
    ) -> Result<Option<FileDiff>, AppError> {
        let changes = self.inner.changes.clone();
        Ok(gitcroc_core::diff_get(changes, workspace_id, handle, file, request).await?)
    }

    /// One side of the image diff `diff_get` last read for `file`, as the
    /// image file's bytes (UI-18). `None` when that side has no image, or
    /// the view or the file is no longer the one read.
    pub async fn diff_image(
        &self,
        workspace_id: WorkspaceId,
        handle: u32,
        file: FileRef,
        side: ImageSide,
    ) -> Result<Option<Vec<u8>>, AppError> {
        let changes = self.inner.changes.clone();
        // Copying up to 10 MB takes a few milliseconds; keep it off the
        // caller's thread all the same (ARCH-7).
        self.blocking(move || gitcroc_core::diff_image(&changes, &workspace_id, handle, file, side))
            .await
    }

    /// Stages, unstages or discards files or one hunk of view `handle` as a
    /// job (UI-15). A discard sent without `confirmed` answers `confirm`
    /// with the repo and files to name in the dialog, and runs nothing
    /// (GRP-9). A hunk whose file changed since its diff was read answers
    /// `file_changed`. `None` when a newer view of the workspace replaced
    /// it.
    pub async fn changes_apply(
        &self,
        workspace_id: WorkspaceId,
        request: ChangeRequest,
    ) -> Result<Option<ChangeStart>, AppError> {
        let git = self.usable_git().await?;
        Ok(gitcroc_core::changes_apply(
            self.inner.changes.clone(),
            self.inner.jobs.clone(),
            self.workspaces().clone(),
            git,
            workspace_id,
            request,
        )
        .await?)
    }

    /// Commits the staged changes of view `handle`, or amends the last
    /// commit with them, as a job (UI-15). Refused without a subject, in
    /// conflict state, for a commit with nothing staged and for an amend
    /// with no commit. `None` when a newer view of the workspace replaced
    /// it.
    pub async fn changes_commit(
        &self,
        workspace_id: WorkspaceId,
        request: CommitRequest,
    ) -> Result<Option<JobId>, AppError> {
        let git = self.usable_git().await?;
        Ok(gitcroc_core::changes_commit(
            self.inner.changes.clone(),
            self.inner.jobs.clone(),
            git,
            workspace_id,
            request,
        )
        .await?)
    }
}
