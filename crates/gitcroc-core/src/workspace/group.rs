//! Groups (section 3 "Group", D7): a named, ordered list of a workspace's
//! repos, kept in `[[group]]` of the workspace file. A repo is at most once
//! in one group and may be in several (D7). Every change rewrites only the
//! workspace file, never anything on disk (GRP-3).

use serde::{Deserialize, Serialize};

use super::{Group, RepoEntry, RepoId, Workspace, WorkspaceError, Workspaces};
use crate::WorkspaceId;

/// The longest group name, in characters (GRP-2).
pub const MAX_GROUP_NAME: usize = 64;

/// The built-in node of the repos in no group. No group may take its name,
/// ignoring case, so the two are never confused.
pub const UNGROUPED_NAME: &str = "Ungrouped";

/// Identifies a group inside its workspace for the session: `g1`, `g2` and
/// so on, given in file order when the workspace loads. Not kept in the
/// file, so a rename keeps it.
#[derive(
    Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, specta::Type,
)]
pub struct GroupId(pub String);

impl std::fmt::Display for GroupId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

/// A group as the UI sees it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
pub struct GroupView {
    pub id: GroupId,
    pub name: String,
    /// In the user's order (GRP-2).
    pub repos: Vec<RepoId>,
}

impl Workspace {
    pub(super) fn new_group_id(&mut self) -> GroupId {
        let id = GroupId(format!("g{}", self.next_group));
        self.next_group = self.next_group.saturating_add(1);
        id
    }

    pub(super) fn group_views(&self) -> Vec<GroupView> {
        self.group_ids
            .iter()
            .zip(&self.file.groups)
            .map(|(id, group)| GroupView {
                id: id.clone(),
                name: group.name.clone(),
                repos: group.repos.clone(),
            })
            .collect()
    }

    /// Applies `change` to copies of the groups and their ids, saves the
    /// file, then keeps both.
    fn update_groups<T>(
        &mut self,
        change: impl FnOnce(&mut Groups<'_>) -> Result<T, WorkspaceError>,
    ) -> Result<T, WorkspaceError> {
        let mut ids = self.group_ids.clone();
        let result = self.update(|file| {
            let mut groups = Groups {
                repos: &file.repos,
                groups: &mut file.groups,
                ids: &mut ids,
            };
            change(&mut groups)
        })?;
        self.group_ids = ids;
        Ok(result)
    }
}

/// The groups being changed, with what the checks need.
struct Groups<'a> {
    repos: &'a [RepoEntry],
    groups: &'a mut Vec<Group>,
    ids: &'a mut Vec<GroupId>,
}

impl Groups<'_> {
    fn index(&self, group: &GroupId) -> Result<usize, WorkspaceError> {
        self.ids
            .iter()
            .position(|id| id == group)
            .ok_or_else(|| WorkspaceError::UnknownGroup(group.clone()))
    }

    /// `name` trimmed, when it is a valid name no other group than
    /// `except` has (GRP-2).
    fn check_name(&self, name: &str, except: Option<usize>) -> Result<String, WorkspaceError> {
        let name = name.trim();
        if name.is_empty() {
            return Err(WorkspaceError::BlankGroupName);
        }
        if name.chars().count() > MAX_GROUP_NAME {
            return Err(WorkspaceError::GroupNameTooLong);
        }
        let lower = name.to_lowercase();
        if lower == UNGROUPED_NAME.to_lowercase() {
            return Err(WorkspaceError::GroupNameReserved);
        }
        let taken =
            self.groups.iter().enumerate().find(|(index, group)| {
                Some(*index) != except && group.name.to_lowercase() == lower
            });
        if let Some((_, group)) = taken {
            return Err(WorkspaceError::GroupNameTaken(group.name.clone()));
        }
        Ok(name.to_owned())
    }

    /// Refuses a repo that is not in the workspace.
    fn check_repos(&self, repos: &[RepoId]) -> Result<(), WorkspaceError> {
        match repos
            .iter()
            .find(|repo| !self.repos.iter().any(|entry| entry.id == **repo))
        {
            Some(unknown) => Err(WorkspaceError::UnknownRepo(unknown.clone())),
            None => Ok(()),
        }
    }

    fn members(&mut self, index: usize) -> &mut Vec<RepoId> {
        // `index` came from `Self::index`, which found it in `ids`, kept
        // the same length as `groups`.
        &mut self.groups[index].repos
    }
}

/// `repos` without repeats, in their first order.
fn distinct(repos: &[RepoId]) -> Vec<RepoId> {
    let mut seen = Vec::with_capacity(repos.len());
    for repo in repos {
        if !seen.contains(repo) {
            seen.push(repo.clone());
        }
    }
    seen
}

/// Inserts `repos` into `members` at `at`, clamped to the end; those
/// already members move there.
fn insert(members: &mut Vec<RepoId>, repos: &[RepoId], at: Option<u32>) {
    let repos = distinct(repos);
    members.retain(|member| !repos.contains(member));
    let at = at.map_or(members.len(), |at| {
        usize::try_from(at).map_or(members.len(), |at| at.min(members.len()))
    });
    members.splice(at..at, repos);
}

impl Workspaces {
    /// The groups of workspace `id`, in the user's order.
    pub fn groups(&self, id: &WorkspaceId) -> Result<Vec<GroupView>, WorkspaceError> {
        self.with(id, |workspace| Ok(workspace.group_views()))
    }

    /// Creates a group named `name`, trimmed, as the last group, with
    /// `repos` as its members in that order. The name is unique in the
    /// workspace, ignoring case, and is not "Ungrouped" (GRP-2).
    pub fn create_group(
        &self,
        id: &WorkspaceId,
        name: &str,
        repos: &[RepoId],
    ) -> Result<GroupId, WorkspaceError> {
        self.with(id, |workspace| {
            let group = workspace.new_group_id();
            let created = group.clone();
            workspace.update_groups(move |groups| {
                let name = groups.check_name(name, None)?;
                groups.check_repos(repos)?;
                groups.groups.push(Group {
                    name,
                    repos: distinct(repos),
                });
                groups.ids.push(created);
                Ok(())
            })?;
            Ok(group)
        })
    }

    /// Renames group `group` with the rules of [`Self::create_group`]. A
    /// change of case only is allowed. The id stays.
    pub fn rename_group(
        &self,
        id: &WorkspaceId,
        group: &GroupId,
        name: &str,
    ) -> Result<(), WorkspaceError> {
        self.with(id, |workspace| {
            workspace.update_groups(|groups| {
                let index = groups.index(group)?;
                let name = groups.check_name(name, Some(index))?;
                groups.groups[index].name = name;
                Ok(())
            })
        })
    }

    /// Deletes group `group`. Its repos stay in the workspace (GRP-3).
    pub fn delete_group(&self, id: &WorkspaceId, group: &GroupId) -> Result<(), WorkspaceError> {
        self.with(id, |workspace| {
            workspace.update_groups(|groups| {
                let index = groups.index(group)?;
                groups.groups.remove(index);
                groups.ids.remove(index);
                Ok(())
            })
        })
    }

    /// Adds `repos` to group `group` at position `at`, or at the end. A
    /// repo already in the group moves there: it is in a group once.
    pub fn add_to_group(
        &self,
        id: &WorkspaceId,
        group: &GroupId,
        repos: &[RepoId],
        at: Option<u32>,
    ) -> Result<(), WorkspaceError> {
        self.with(id, |workspace| {
            workspace.update_groups(|groups| {
                let index = groups.index(group)?;
                groups.check_repos(repos)?;
                insert(groups.members(index), repos, at);
                Ok(())
            })
        })
    }

    /// Removes `repos` from group `group`. They stay in the workspace and
    /// in other groups (GRP-3).
    pub fn remove_from_group(
        &self,
        id: &WorkspaceId,
        group: &GroupId,
        repos: &[RepoId],
    ) -> Result<(), WorkspaceError> {
        self.with(id, |workspace| {
            workspace.update_groups(|groups| {
                let index = groups.index(group)?;
                groups
                    .members(index)
                    .retain(|member| !repos.contains(member));
                Ok(())
            })
        })
    }

    /// Moves members `repos` of group `group` to position `at` among the
    /// other members, keeping their order. A repo that is not a member is
    /// refused.
    pub fn move_in_group(
        &self,
        id: &WorkspaceId,
        group: &GroupId,
        repos: &[RepoId],
        at: u32,
    ) -> Result<(), WorkspaceError> {
        self.with(id, |workspace| {
            workspace.update_groups(|groups| {
                let index = groups.index(group)?;
                let members = groups.members(index);
                if let Some(stranger) = repos.iter().find(|repo| !members.contains(repo)) {
                    return Err(WorkspaceError::UnknownRepo(stranger.clone()));
                }
                insert(members, repos, Some(at));
                Ok(())
            })
        })
    }

    /// Moves `repo` from group `from` to group `to` at position `at`, or at
    /// the end, in one save.
    pub fn move_between_groups(
        &self,
        id: &WorkspaceId,
        repo: &RepoId,
        from: &GroupId,
        to: &GroupId,
        at: Option<u32>,
    ) -> Result<(), WorkspaceError> {
        self.with(id, |workspace| {
            workspace.update_groups(|groups| {
                let (source, target) = (groups.index(from)?, groups.index(to)?);
                if !groups.members(source).contains(repo) {
                    return Err(WorkspaceError::UnknownRepo(repo.clone()));
                }
                groups.members(source).retain(|member| member != repo);
                insert(groups.members(target), std::slice::from_ref(repo), at);
                Ok(())
            })
        })
    }

    /// Puts the groups in the order of `order`, which names each group
    /// once.
    pub fn reorder_groups(
        &self,
        id: &WorkspaceId,
        order: &[GroupId],
    ) -> Result<(), WorkspaceError> {
        self.with(id, |workspace| {
            workspace.update_groups(|groups| {
                let mut sorted = order.to_vec();
                sorted.sort();
                sorted.dedup();
                let mut known = groups.ids.clone();
                known.sort();
                if sorted != known || order.len() != known.len() {
                    return Err(WorkspaceError::BadGroupOrder);
                }
                let mut moved = Vec::with_capacity(order.len());
                for group in order {
                    let index = groups.index(group)?;
                    moved.push((groups.ids[index].clone(), groups.groups[index].clone()));
                }
                let (ids, list): (Vec<_>, Vec<_>) = moved.into_iter().unzip();
                *groups.ids = ids;
                *groups.groups = list;
                Ok(())
            })
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ids(names: &[&str]) -> Vec<RepoId> {
        names
            .iter()
            .map(|name| RepoId((*name).to_owned()))
            .collect()
    }

    #[test]
    fn grp_2_insert_places_repos_once_at_the_position() {
        let mut members = ids(&["r1", "r2", "r3"]);
        insert(&mut members, &ids(&["r4", "r4"]), Some(1));
        assert_eq!(members, ids(&["r1", "r4", "r2", "r3"]));
        // A member moves; the position counts among the others.
        insert(&mut members, &ids(&["r3", "r1"]), Some(0));
        assert_eq!(members, ids(&["r3", "r1", "r4", "r2"]));
        insert(&mut members, &ids(&["r3"]), Some(99));
        assert_eq!(members, ids(&["r1", "r4", "r2", "r3"]));
        insert(&mut members, &ids(&["r5"]), None);
        assert_eq!(members, ids(&["r1", "r4", "r2", "r3", "r5"]));
    }
}
