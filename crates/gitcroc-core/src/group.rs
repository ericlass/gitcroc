//! What a group shows, derived from its members' summaries on every read
//! and never stored: its branch (GRP-4) and its totals (UI-4).

use std::collections::{BTreeSet, HashMap};

use gitcroc_git::Head;
use serde::{Deserialize, Serialize};

use crate::{GroupId, RepoId, Summaries, SummaryView, WorkspaceError, WorkspaceId, Workspaces};

/// A group's branch as the tree shows it (GRP-4).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum GroupBranch {
    /// No member counts yet: none has a summary, or all are missing.
    None,
    /// Every counted member is on this branch.
    Branch { name: String },
    /// Every counted member has a detached `HEAD`.
    Detached,
    /// The counted members are on `count` different branches; a detached
    /// `HEAD` counts as one value, however many members have one.
    Mixed { count: u32 },
}

/// What a group's row shows (UI-4), sent with its tree node and as event
/// `group_summary` when a member's summary changes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
pub struct GroupSummary {
    pub workspace_id: WorkspaceId,
    pub group_id: GroupId,
    pub branch: GroupBranch,
    pub members: u32,
    /// Members whose working copy is dirty.
    pub dirty: u32,
    /// Members in conflict state.
    pub conflicts: u32,
}

/// One value a member adds to the group's branch.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
enum Value {
    Branch(String),
    Detached,
}

/// The branch of a group whose members have `views`, in any order. A
/// member with a missing path or no summary does not count.
pub fn group_branch<'a>(views: impl IntoIterator<Item = &'a SummaryView>) -> GroupBranch {
    let values: BTreeSet<Value> = views
        .into_iter()
        .filter(|view| !view.missing)
        .filter_map(|view| view.summary.as_ref())
        .map(|summary| match &summary.head {
            Head::Branch { name, .. } => Value::Branch(name.clone()),
            Head::Detached { .. } => Value::Detached,
        })
        .collect();
    let mut distinct = values.into_iter();
    match (distinct.next(), distinct.len()) {
        (None, _) => GroupBranch::None,
        (Some(Value::Branch(name)), 0) => GroupBranch::Branch { name },
        (Some(Value::Detached), 0) => GroupBranch::Detached,
        (Some(_), others) => GroupBranch::Mixed {
            count: u32::try_from(others.saturating_add(1)).unwrap_or(u32::MAX),
        },
    }
}

fn count<'a>(views: &[&'a SummaryView], test: impl Fn(&'a SummaryView) -> bool) -> u32 {
    u32::try_from(views.iter().filter(|view| test(view)).count()).unwrap_or(u32::MAX)
}

/// The summary of each group of workspace `id`, in the user's order.
pub fn group_summaries(
    workspaces: &Workspaces,
    summaries: &Summaries,
    id: &WorkspaceId,
) -> Result<Vec<GroupSummary>, WorkspaceError> {
    let views = summaries.views(workspaces, id)?;
    let by_repo: HashMap<&RepoId, &SummaryView> =
        views.iter().map(|view| (&view.repo_id, view)).collect();
    Ok(workspaces
        .groups(id)?
        .into_iter()
        .map(|group| {
            let members: Vec<&SummaryView> = group
                .repos
                .iter()
                .filter_map(|repo| by_repo.get(repo).copied())
                .collect();
            GroupSummary {
                workspace_id: id.clone(),
                group_id: group.id,
                branch: group_branch(members.iter().copied()),
                members: u32::try_from(group.repos.len()).unwrap_or(u32::MAX),
                dirty: count(&members, |view| {
                    !view.missing && view.summary.as_ref().is_some_and(|summary| summary.dirty)
                }),
                conflicts: count(&members, |view| view.conflict),
            }
        })
        .collect())
}
