//! The target set (GRP-5, section 3 "Target set"): the repos an operation
//! runs on. It is the selected repo, a group's members or a multi-selection
//! of repos, and nothing else; group operations take one and resolve it
//! here against the workspace as it is now.

use serde::{Deserialize, Serialize};

use crate::{GroupId, RepoId, WorkspaceError, WorkspaceId, WorkspaceView, Workspaces};

/// What the user selected to act on (GRP-5).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum TargetSet {
    /// One repo.
    Repo { repo_id: RepoId },
    /// The members of a group, in group order.
    Group { group_id: GroupId },
    /// A multi-selection of repo rows. It resolves in tree order, each repo
    /// once, however it was given.
    Repos { repo_ids: Vec<RepoId> },
}

/// The repos of `target` in workspace `id`, as the workspace is now. A
/// group that no longer exists, or a repo removed meanwhile, is an error.
pub fn target_repos(
    workspaces: &Workspaces,
    id: &WorkspaceId,
    target: &TargetSet,
) -> Result<Vec<RepoId>, WorkspaceError> {
    let view = workspaces.view(id)?;
    let known = |repo: &RepoId| {
        if view.repos.iter().any(|entry| entry.id == *repo) {
            Ok(repo.clone())
        } else {
            Err(WorkspaceError::UnknownRepo(repo.clone()))
        }
    };
    match target {
        TargetSet::Repo { repo_id } => Ok(vec![known(repo_id)?]),
        TargetSet::Group { group_id } => view
            .groups
            .iter()
            .find(|group| group.id == *group_id)
            .ok_or_else(|| WorkspaceError::UnknownGroup(group_id.clone()))
            .map(|group| group.repos.clone()),
        TargetSet::Repos { repo_ids } => {
            for repo in repo_ids {
                known(repo)?;
            }
            Ok(tree_order(&view)
                .into_iter()
                .filter(|repo| repo_ids.contains(repo))
                .collect())
        }
    }
}

/// Each repo of `view` once, where it first shows in the tree (UI-1): the
/// groups' members in the user's order, then the repos in no group.
fn tree_order(view: &WorkspaceView) -> Vec<RepoId> {
    let mut order: Vec<RepoId> = Vec::with_capacity(view.repos.len());
    let grouped = view.groups.iter().flat_map(|group| &group.repos);
    let all = view.repos.iter().map(|entry| &entry.id);
    for repo in grouped.chain(all) {
        if !order.contains(repo) {
            order.push(repo.clone());
        }
    }
    order
}
