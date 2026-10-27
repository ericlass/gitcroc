//! The tree, its filter, the command palette and the groups.

use gitcroc_core::{
    GroupId, PaletteResult, RepoId, TargetSet, TreeNode, WorkspaceError, WorkspaceId,
    WorkspaceView, Workspaces, tree_refs,
};

use super::{App, AppError, AppEvent};

impl App {
    /// The workspace's tree (UI-1): each group with its Branches node and
    /// its members, then Ungrouped with the repos in no group. Each repo has
    /// its Local, Remotes, Tags and Stashes nodes not loaded; `refs_get`
    /// loads them, and `group_branches` a group's Branches node (ARCH-8).
    pub fn tree_get(&self, workspace_id: &WorkspaceId) -> Result<Vec<TreeNode>, AppError> {
        Ok(self
            .trees()
            .tree(self.workspaces(), self.summaries(), workspace_id)?)
    }

    /// One repo's Local, Remotes, Tags and Stashes nodes where it appears
    /// under group `group_id`, or under Ungrouped for `None`, loaded, with
    /// ahead and behind for each local branch.
    pub async fn refs_get(
        &self,
        workspace_id: WorkspaceId,
        repo_id: RepoId,
        group_id: Option<GroupId>,
    ) -> Result<Vec<TreeNode>, AppError> {
        let (trees, workspaces) = (self.trees().clone(), self.workspaces().clone());
        Ok(tree_refs(trees, workspaces, workspace_id, repo_id, group_id).await?)
    }

    /// The children of a group's Branches node (UI-5): every local branch
    /// name of at least one member, with how many members have it.
    pub async fn group_branches(
        &self,
        workspace_id: WorkspaceId,
        group_id: GroupId,
    ) -> Result<Vec<TreeNode>, AppError> {
        let (trees, workspaces) = (self.trees().clone(), self.workspaces().clone());
        Ok(gitcroc_core::group_branches(trees, workspaces, workspace_id, group_id).await?)
    }

    /// The tree filtered by `text` (UI-8); empty text gives the whole tree.
    /// `None` when a newer `tree_filter` of the workspace superseded this
    /// one.
    pub async fn tree_filter(
        &self,
        workspace_id: WorkspaceId,
        text: String,
    ) -> Result<Option<Vec<TreeNode>>, AppError> {
        let (trees, workspaces) = (self.trees().clone(), self.workspaces().clone());
        let summaries = self.summaries().clone();
        Ok(gitcroc_core::tree_filter(trees, workspaces, summaries, workspace_id, text).await?)
    }

    /// The command palette's items for `text` (UI-22): the operations, and
    /// the repos, groups and branches that match, ranked, at most 100 with
    /// the total. `None` when a newer `palette_query` of the workspace
    /// superseded this one.
    pub async fn palette_query(
        &self,
        workspace_id: WorkspaceId,
        text: String,
    ) -> Result<Option<PaletteResult>, AppError> {
        let (palettes, trees) = (self.inner.palettes.clone(), self.trees().clone());
        let (workspaces, settings) = (self.workspaces().clone(), self.inner.settings.clone());
        Ok(
            gitcroc_core::palette_query(palettes, trees, workspaces, settings, workspace_id, text)
                .await?,
        )
    }

    /// The repos of `target` as the workspace is now (GRP-5): a group's
    /// members in group order, or a multi-selection in tree order, each repo
    /// once. A group or repo that no longer exists is refused.
    pub fn target_repos(
        &self,
        workspace_id: &WorkspaceId,
        target: &TargetSet,
    ) -> Result<Vec<RepoId>, AppError> {
        Ok(gitcroc_core::target_repos(
            self.workspaces(),
            workspace_id,
            target,
        )?)
    }

    /// Runs `change` on the workspace's groups off the caller's thread, as
    /// it saves the file, then sends `WorkspaceChanged`, which also makes
    /// the UI read the tree again. Nothing on disk but the workspace file
    /// changes (GRP-3).
    async fn change_groups<T: Send + 'static>(
        &self,
        workspace_id: WorkspaceId,
        change: impl FnOnce(&Workspaces, &WorkspaceId) -> Result<T, WorkspaceError> + Send + 'static,
    ) -> Result<(T, WorkspaceView), AppError> {
        let workspaces = self.workspaces().clone();
        let (result, view) = self
            .blocking(move || {
                let result = change(&workspaces, &workspace_id)?;
                Ok::<_, WorkspaceError>((result, workspaces.view(&workspace_id)?))
            })
            .await??;
        self.send(AppEvent::WorkspaceChanged(view.clone()));
        Ok((result, view))
    }

    /// Creates a group as the last one, with `repo_ids` as its members in
    /// that order (GRP-2), and returns its id.
    pub async fn group_create(
        &self,
        workspace_id: WorkspaceId,
        name: String,
        repo_ids: Vec<RepoId>,
    ) -> Result<GroupId, AppError> {
        let (group, _) = self
            .change_groups(workspace_id, move |workspaces, id| {
                workspaces.create_group(id, &name, &repo_ids)
            })
            .await?;
        Ok(group)
    }

    /// Renames a group (GRP-2); its id stays.
    pub async fn group_rename(
        &self,
        workspace_id: WorkspaceId,
        group_id: GroupId,
        name: String,
    ) -> Result<WorkspaceView, AppError> {
        let (_, view) = self
            .change_groups(workspace_id, move |workspaces, id| {
                workspaces.rename_group(id, &group_id, &name)
            })
            .await?;
        Ok(view)
    }

    /// Deletes a group; its repos stay in the workspace (GRP-3).
    pub async fn group_delete(
        &self,
        workspace_id: WorkspaceId,
        group_id: GroupId,
    ) -> Result<WorkspaceView, AppError> {
        let (_, view) = self
            .change_groups(workspace_id, move |workspaces, id| {
                workspaces.delete_group(id, &group_id)
            })
            .await?;
        Ok(view)
    }

    /// Adds repos to a group at position `at`, or at the end; a member
    /// moves there.
    pub async fn group_add_repos(
        &self,
        workspace_id: WorkspaceId,
        group_id: GroupId,
        repo_ids: Vec<RepoId>,
        at: Option<u32>,
    ) -> Result<WorkspaceView, AppError> {
        let (_, view) = self
            .change_groups(workspace_id, move |workspaces, id| {
                workspaces.add_to_group(id, &group_id, &repo_ids, at)
            })
            .await?;
        Ok(view)
    }

    /// Removes repos from a group; they stay in the workspace (GRP-3).
    pub async fn group_remove_repos(
        &self,
        workspace_id: WorkspaceId,
        group_id: GroupId,
        repo_ids: Vec<RepoId>,
    ) -> Result<WorkspaceView, AppError> {
        let (_, view) = self
            .change_groups(workspace_id, move |workspaces, id| {
                workspaces.remove_from_group(id, &group_id, &repo_ids)
            })
            .await?;
        Ok(view)
    }

    /// Moves members of a group to position `at` among the others.
    pub async fn group_move_repos(
        &self,
        workspace_id: WorkspaceId,
        group_id: GroupId,
        repo_ids: Vec<RepoId>,
        at: u32,
    ) -> Result<WorkspaceView, AppError> {
        let (_, view) = self
            .change_groups(workspace_id, move |workspaces, id| {
                workspaces.move_in_group(id, &group_id, &repo_ids, at)
            })
            .await?;
        Ok(view)
    }

    /// Moves a repo from one group to another at position `at`, or at the
    /// end, in one save.
    pub async fn group_move_repo_between(
        &self,
        workspace_id: WorkspaceId,
        repo_id: RepoId,
        from: GroupId,
        to: GroupId,
        at: Option<u32>,
    ) -> Result<WorkspaceView, AppError> {
        let (_, view) = self
            .change_groups(workspace_id, move |workspaces, id| {
                workspaces.move_between_groups(id, &repo_id, &from, &to, at)
            })
            .await?;
        Ok(view)
    }

    /// Puts the groups in the order of `group_ids`, which names each once.
    pub async fn group_reorder(
        &self,
        workspace_id: WorkspaceId,
        group_ids: Vec<GroupId>,
    ) -> Result<WorkspaceView, AppError> {
        let (_, view) = self
            .change_groups(workspace_id, move |workspaces, id| {
                workspaces.reorder_groups(id, &group_ids)
            })
            .await?;
        Ok(view)
    }
}
