//! Operations on one repo (section 6): branches, sync, integrate, stash,
//! tags, reset, remotes, fsmonitor, clone, and the repo's folder. Then the
//! group operations (section 5).

use std::path::PathBuf;

use gitcroc_core::{
    BranchChoices, BranchRequest, BranchStart, CloneRequest, GroupOp, IntegrateRequest,
    IntegrateStart, JobId, PreflightId, PushChoices, RemoteRequest, RemoteStart, RemoteUrlsView,
    RepoId, ResetRequest, ResetStart, StashRequest, StashStart, SyncRequest, SyncStart, TagRequest,
    TagStart, TargetSet, WorkspaceId,
};

use super::{App, AppError, AppEvent};
use crate::platform::{FileManager, PlatformFileManager, PlatformTerminal, Terminal};

impl App {
    /// Runs a branch operation on one repo as a job (section 6 "Branches").
    /// Refused for a name git would not take (SEC-4) and for a repo that is
    /// busy, missing or in conflict state (GRP-8). A force delete or a
    /// delete on a remote sent without `confirmed` answers `confirm` with
    /// what to name in the dialog, and runs nothing (GRP-9).
    pub async fn op_branch(
        &self,
        workspace_id: WorkspaceId,
        request: BranchRequest,
    ) -> Result<BranchStart, AppError> {
        let git = self.usable_git().await?;
        Ok(gitcroc_core::op_branch(
            self.workspaces().clone(),
            self.summaries().clone(),
            self.inner.jobs.clone(),
            git,
            workspace_id,
            request,
        )
        .await?)
    }

    /// What the branch dialogs offer for a repo: start points, its default
    /// branch and the remote branches to track.
    pub async fn branch_choices(
        &self,
        workspace_id: WorkspaceId,
        repo_id: RepoId,
    ) -> Result<BranchChoices, AppError> {
        let workspaces = self.workspaces().clone();
        Ok(gitcroc_core::branch_choices(workspaces, workspace_id, repo_id).await?)
    }

    /// Why `name` is not a valid branch name, or `None` when it is (SEC-4).
    /// Checked as the user types.
    pub fn branch_name_check(&self, name: &str) -> Option<String> {
        gitcroc_core::branch_name_check(name)
    }

    /// Fetches, pulls or pushes one repo as a job (section 6 "Sync"), with
    /// the transfer's progress as the job's steps. Refused for a repo that
    /// is busy, missing or, except for fetch, in conflict state (GRP-8), and
    /// for a pull or push on a detached `HEAD`. A force push sent without
    /// `confirmed` answers `confirm` and runs nothing (GRP-9); a push that
    /// needs a remote picked answers `choose_remote`.
    pub async fn op_sync(
        &self,
        workspace_id: WorkspaceId,
        request: SyncRequest,
    ) -> Result<SyncStart, AppError> {
        let git = self.usable_git().await?;
        Ok(gitcroc_core::op_sync(
            self.workspaces().clone(),
            self.summaries().clone(),
            self.inner.jobs.clone(),
            git,
            workspace_id,
            request,
        )
        .await?)
    }

    /// What the push options offer for a repo: its branch, upstream and
    /// remotes.
    pub async fn push_choices(
        &self,
        workspace_id: WorkspaceId,
        repo_id: RepoId,
    ) -> Result<PushChoices, AppError> {
        let workspaces = self.workspaces().clone();
        Ok(gitcroc_core::push_choices(workspaces, workspace_id, repo_id).await?)
    }

    /// Merges, rebases, cherry-picks or reverts in a repo, or aborts or
    /// continues the operation it is in, as a job (section 6 "Integrate",
    /// OPS-2). Its end arrives as `JobEnded`; a revert of a merge commit
    /// answers `choose_mainline`, and the abort of a stash apply sent
    /// without `confirmed` answers `confirm` and runs nothing.
    pub async fn op_integrate(
        &self,
        workspace_id: WorkspaceId,
        request: IntegrateRequest,
    ) -> Result<IntegrateStart, AppError> {
        let git = self.usable_git().await?;
        Ok(gitcroc_core::op_integrate(
            self.workspaces().clone(),
            self.summaries().clone(),
            self.inner.jobs.clone(),
            git,
            workspace_id,
            request,
        )
        .await?)
    }

    /// The refs the merge dialog offers for a repo, filtered by `filter`
    /// (UI-20).
    pub async fn merge_targets(
        &self,
        workspace_id: WorkspaceId,
        repo_id: RepoId,
        filter: String,
    ) -> Result<Vec<String>, AppError> {
        let workspaces = self.workspaces().clone();
        Ok(gitcroc_core::merge_targets(workspaces, workspace_id, repo_id, filter).await?)
    }

    /// Stashes, applies, pops or drops in one repo as a job (section 6
    /// "Stash"). Refused for a repo that is busy, missing or in conflict
    /// state (GRP-8) and for an entry the stash list no longer has at its
    /// index. A drop sent without `confirmed` answers `confirm` and runs
    /// nothing.
    pub async fn op_stash(
        &self,
        workspace_id: WorkspaceId,
        request: StashRequest,
    ) -> Result<StashStart, AppError> {
        let git = self.usable_git().await?;
        Ok(gitcroc_core::op_stash(
            self.workspaces().clone(),
            self.summaries().clone(),
            self.inner.jobs.clone(),
            git,
            workspace_id,
            request,
        )
        .await?)
    }

    /// Creates, deletes or pushes a tag in one repo as a job (section 6
    /// "Tags"). Refused for an invalid name (SEC-4), a repo that is busy,
    /// missing or in conflict state (GRP-8), an existing name to create and
    /// a missing tag. A delete on the remote sent without `confirmed`
    /// answers `confirm` and runs nothing; a push or delete on the remote
    /// that needs a remote picked answers `choose_remote`.
    pub async fn op_tag(
        &self,
        workspace_id: WorkspaceId,
        request: TagRequest,
    ) -> Result<TagStart, AppError> {
        let git = self.usable_git().await?;
        Ok(gitcroc_core::op_tag(
            self.workspaces().clone(),
            self.summaries().clone(),
            self.inner.jobs.clone(),
            git,
            workspace_id,
            request,
        )
        .await?)
    }

    /// Why `name` is not a valid tag name, or `None` when it is (SEC-4).
    /// Checked as the user types.
    pub fn tag_name_check(&self, name: &str) -> Option<String> {
        gitcroc_core::tag_name_check(name)
    }

    /// Resets the current branch of one repo, or a detached `HEAD`, to a
    /// commit as a job (section 6 "Reset", UI-13). Refused for a repo that
    /// is busy, missing or in conflict state (GRP-8). A hard reset sent
    /// without `confirmed` answers `confirm`, naming what it loses, and runs
    /// nothing (GRP-9).
    pub async fn op_reset(
        &self,
        workspace_id: WorkspaceId,
        request: ResetRequest,
    ) -> Result<ResetStart, AppError> {
        let git = self.usable_git().await?;
        Ok(gitcroc_core::op_reset(
            self.workspaces().clone(),
            self.summaries().clone(),
            self.inner.jobs.clone(),
            git,
            workspace_id,
            request,
        )
        .await?)
    }

    /// Adds, renames or removes a remote of one repo or sets its URLs, as a
    /// job (section 6 "Remotes"). Refused for an invalid or taken name
    /// (SEC-4), a URL that starts with `-`, a missing remote and a repo that
    /// is busy, missing or in conflict state (GRP-8). A remove sent without
    /// `confirmed` answers `confirm`, naming the upstreams it unsets, and
    /// runs nothing.
    pub async fn op_remote(
        &self,
        workspace_id: WorkspaceId,
        request: RemoteRequest,
    ) -> Result<RemoteStart, AppError> {
        let git = self.usable_git().await?;
        Ok(gitcroc_core::op_remote(
            self.workspaces().clone(),
            self.summaries().clone(),
            self.inner.jobs.clone(),
            git,
            workspace_id,
            request,
        )
        .await?)
    }

    /// Why `name` is not a valid name for a new remote, or `None` when it
    /// is (SEC-4). Checked as the user types.
    pub fn remote_name_check(&self, name: &str) -> Option<String> {
        gitcroc_core::remote_name_check(name)
    }

    /// A remote's URLs, for the edit dialog.
    pub async fn remote_urls(
        &self,
        workspace_id: WorkspaceId,
        repo_id: RepoId,
        name: String,
    ) -> Result<RemoteUrlsView, AppError> {
        let workspaces = self.workspaces().clone();
        Ok(gitcroc_core::remote_urls(workspaces, workspace_id, repo_id, name).await?)
    }

    /// The user accepted the offer to speed up a large repo's status
    /// (PERF-7): turns on the offered keys in its local config as a job.
    /// Refused for a repo that is busy, missing or in conflict state
    /// (GRP-8).
    pub async fn fsmonitor_enable(
        &self,
        workspace_id: WorkspaceId,
        repo_id: RepoId,
    ) -> Result<JobId, AppError> {
        let git = self.usable_git().await?;
        Ok(gitcroc_core::fsmonitor_enable(
            self.workspaces().clone(),
            self.summaries().clone(),
            self.inner.changes.clone(),
            self.inner.jobs.clone(),
            git,
            workspace_id,
            repo_id,
        )
        .await?)
    }

    /// Keeps "don't ask again" for a repo's fsmonitor offer (PERF-7).
    pub async fn fsmonitor_decline(
        &self,
        workspace_id: WorkspaceId,
        repo_id: RepoId,
    ) -> Result<(), AppError> {
        let workspaces = self.workspaces().clone();
        self.blocking(move || workspaces.decline_fsmonitor(&workspace_id, &repo_id))
            .await??;
        Ok(())
    }

    /// Opens the folder dialog for a clone's parent folder and returns at
    /// once; the folder picked arrives as `CloneParentPicked`, with `key`.
    /// Nothing is sent when the user cancels.
    pub fn clone_pick_parent(
        &self,
        workspace_id: WorkspaceId,
        key: Option<String>,
    ) -> Result<(), AppError> {
        self.workspaces().view(&workspace_id)?;
        let picked = self.inner.dialogs.pick_folder("Clone into folder");
        let app = self.clone();
        self.inner.runtime.spawn(async move {
            if let Some(path) = picked.await {
                app.send(AppEvent::CloneParentPicked {
                    workspace_id,
                    path,
                    key,
                });
            }
        });
        Ok(())
    }

    /// The folder a clone of `url` gets by default: the last segment of its
    /// path without `.git`; `None` when there is none.
    pub fn clone_default_name(&self, url: &str) -> Option<String> {
        gitcroc_core::clone_default_name(url)
    }

    /// Why a clone into folder `name` of `parent` would be refused, such as
    /// a folder that exists and is not empty, or `None`. Checked as the user
    /// types. With `creates_parent`, as for a missing repo, a `parent` that
    /// does not exist yet is not refused: the clone creates it.
    pub async fn clone_target_check(
        &self,
        parent: PathBuf,
        name: String,
        creates_parent: bool,
    ) -> Option<String> {
        gitcroc_core::clone_target_check(parent, name, creates_parent).await
    }

    /// Starts cloning `request` and returns its job's id at once. Progress
    /// arrives as `JobProgress`; at its end, a cloned repo is in the
    /// workspace, announced by `WorkspaceChanged`, then `CloneReport` names
    /// it. Refused before git runs for a read-only workspace, an unusable
    /// URL or folder name, or a target that exists and is not empty.
    pub async fn clone_start(
        &self,
        workspace_id: WorkspaceId,
        request: CloneRequest,
    ) -> Result<JobId, AppError> {
        let git = self.prepare_add(&workspace_id).await?;
        let handle = gitcroc_core::clone_start(
            self.workspaces().clone(),
            self.inner.jobs.clone(),
            git.clone(),
            workspace_id,
            request,
        )
        .await?;
        let app = self.clone();
        self.inner.runtime.spawn(async move {
            let Ok(report) = handle.ended.await else {
                return;
            };
            let workspace_id = report.workspace_id.clone();
            let added: Vec<RepoId> = report.repo.iter().map(|repo| repo.id.clone()).collect();
            if !added.is_empty()
                && let Ok(view) = app.workspaces().view(&workspace_id)
            {
                app.send(AppEvent::WorkspaceChanged(view));
            }
            app.send(AppEvent::CloneReport(report));
            if !added.is_empty() {
                app.spawn_watch_sync(workspace_id.clone());
            }
            app.inner.hub.refresh(workspace_id, added, git).await;
        });
        Ok(handle.job_id)
    }

    /// Shows the repo's folder in the system file manager.
    pub async fn repo_open_file_manager(
        &self,
        workspace_id: WorkspaceId,
        repo_id: RepoId,
    ) -> Result<(), AppError> {
        self.repo_open_file_manager_using(workspace_id, repo_id, PlatformFileManager::show)
            .await
    }

    /// Platform adapter, injectable by native UI tests.
    pub async fn repo_open_file_manager_using(
        &self,
        workspace_id: WorkspaceId,
        repo_id: RepoId,
        show: fn(&std::path::Path) -> crate::platform::Launch,
    ) -> Result<(), AppError> {
        let path = self.workspaces().repo_path(&workspace_id, &repo_id)?;
        let launch = show(&path);
        self.blocking(move || crate::platform::start(&[launch]))
            .await??;
        Ok(())
    }

    /// Opens a terminal in the repo's folder; fails naming the terminals
    /// tried when none is installed.
    pub async fn repo_open_terminal(
        &self,
        workspace_id: WorkspaceId,
        repo_id: RepoId,
    ) -> Result<(), AppError> {
        self.repo_open_terminal_using(workspace_id, repo_id, PlatformTerminal::open_in)
            .await
    }

    /// Platform adapter, injectable by native UI tests.
    pub async fn repo_open_terminal_using(
        &self,
        workspace_id: WorkspaceId,
        repo_id: RepoId,
        open: fn(&std::path::Path) -> Vec<crate::platform::Launch>,
    ) -> Result<(), AppError> {
        let path = self.workspaces().repo_path(&workspace_id, &repo_id)?;
        let candidates = open(&path);
        self.blocking(move || crate::platform::start(&candidates))
            .await??;
        Ok(())
    }
}

/// Group operations (section 5).
impl App {
    /// Starts the preflight of group operation `op` on `target` (section 5,
    /// step 1) and returns its id at once (ARCH-7); the result arrives as
    /// `PreflightReady`, with whether the user must confirm. Refused for an
    /// operation that is not available yet.
    pub async fn group_preflight(
        &self,
        workspace_id: WorkspaceId,
        target: TargetSet,
        op: GroupOp,
    ) -> Result<PreflightId, AppError> {
        let operation = op.operation()?;
        let git = self.usable_git().await?;
        Ok(self
            .inner
            .groups
            .preflight(git, &workspace_id, &target, operation)?)
    }

    /// Starts the job of preflight `preflight_id` (section 5, step 3) and
    /// returns its id at once. Refused without `confirmed` when the
    /// preflight said the user must confirm, for a preflight used already,
    /// and when the workspace's repos or groups changed since it ran.
    /// Sub-jobs arrive in the job's `JobProgress` events, and its report
    /// with `JobEnded`.
    pub async fn group_start(
        &self,
        preflight_id: PreflightId,
        confirmed: bool,
    ) -> Result<JobId, AppError> {
        let git = self.usable_git().await?;
        Ok(self.inner.groups.start(git, preflight_id, confirmed)?)
    }

    /// Drops preflight `preflight_id`, as the user cancelled its dialog.
    pub fn group_drop(&self, preflight_id: PreflightId) {
        self.inner.groups.drop_preflight(preflight_id);
    }

    /// Retry failed (GRP-11): starts a new preflight of ended group job
    /// `job_id`'s operation, with the same options, on the repos its report
    /// has as *failed*, and returns its id at once; the result arrives as
    /// `PreflightReady`. The core builds the target set from the report
    /// (GRP-5).
    pub async fn group_retry_failed(&self, job_id: JobId) -> Result<PreflightId, AppError> {
        let git = self.usable_git().await?;
        Ok(self.inner.groups.retry_failed(git, job_id)?)
    }
}
