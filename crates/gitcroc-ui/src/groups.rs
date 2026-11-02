//! The group changes the tree offers (UI-7), from its menus and by
//! dragging: which core request a gesture sends, a port of rc.1's
//! `ui/src/tree/groups.ts` and of the group entries of `tree/menus.ts`.
//! The groups and their members are read off the core's tree; the core
//! checks and applies every change (GRP-2, GRP-3) and the tree follows its
//! `WorkspaceChanged` (ARCH-6).

use gitcroc_app::app::{App, AppError};
use gitcroc_core::{GroupId, RepoId, TreeItem, TreeNode, WorkspaceId};

use crate::group_ops::SyncKind;
use crate::menu::MenuItem;
use crate::row_context::RowChoice;
use crate::row_menus::TreeAction;
use crate::strings::tree_menu as text;

/// A group as the tree shows it: its members in the user's order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GroupInfo {
    pub id: GroupId,
    pub name: String,
    pub repos: Vec<RepoId>,
}

/// The groups of the unfiltered tree `roots`, in the user's order.
pub fn groups_of(roots: &[TreeNode]) -> Vec<GroupInfo> {
    roots
        .iter()
        .filter_map(|node| match &node.item {
            TreeItem::Group { group_id, .. } => Some(GroupInfo {
                id: group_id.clone(),
                name: node.label.clone(),
                repos: node
                    .children
                    .iter()
                    .flatten()
                    .filter_map(|child| match &child.item {
                        TreeItem::Repo { repo_id, .. } => Some(repo_id.clone()),
                        _ => None,
                    })
                    .collect(),
            }),
            _ => None,
        })
        .collect()
}

/// A change of the groups, as one core request.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GroupOp {
    /// Adds `repos` at `at`, or at the end; a member moves there.
    Add {
        group: GroupId,
        repos: Vec<RepoId>,
        at: Option<u32>,
    },
    Remove {
        group: GroupId,
        repo: RepoId,
    },
    /// Moves a member to `at` among the other members.
    MoveIn {
        group: GroupId,
        repo: RepoId,
        at: u32,
    },
    MoveBetween {
        repo: RepoId,
        from: GroupId,
        to: GroupId,
        at: Option<u32>,
    },
    Reorder {
        groups: Vec<GroupId>,
    },
}

/// Sends `op` to the core.
pub async fn send(app: App, workspace: WorkspaceId, op: GroupOp) -> Result<(), AppError> {
    match op {
        GroupOp::Add { group, repos, at } => app.group_add_repos(workspace, group, repos, at).await,
        GroupOp::Remove { group, repo } => {
            app.group_remove_repos(workspace, group, vec![repo]).await
        }
        GroupOp::MoveIn { group, repo, at } => {
            app.group_move_repos(workspace, group, vec![repo], at).await
        }
        GroupOp::MoveBetween { repo, from, to, at } => {
            app.group_move_repo_between(workspace, repo, from, to, at)
                .await
        }
        GroupOp::Reorder { groups } => app.group_reorder(workspace, groups).await,
    }
    .map(|_| ())
}

/// A position as the core takes it.
fn position(at: usize) -> u32 {
    u32::try_from(at).unwrap_or(u32::MAX)
}

/// `from` moved one step by `step`, when it stays within `len`.
fn stepped(len: usize, from: usize, step: isize) -> Option<usize> {
    let to = from.checked_add_signed(step)?;
    (to < len).then_some(to)
}

/// Moves group `group` one place up (`-1`) or down (`1`); `None` when it
/// is first or last.
pub fn move_group(groups: &[GroupInfo], group: &GroupId, step: isize) -> Option<GroupOp> {
    let mut order: Vec<GroupId> = groups.iter().map(|known| known.id.clone()).collect();
    let from = order.iter().position(|id| id == group)?;
    let to = stepped(order.len(), from, step)?;
    order.swap(from, to);
    Some(GroupOp::Reorder { groups: order })
}

/// Moves member `repo` of `group` one place up or down; `None` when it is
/// first or last.
pub fn move_member(
    groups: &[GroupInfo],
    group: &GroupId,
    repo: &RepoId,
    step: isize,
) -> Option<GroupOp> {
    let members = &groups.iter().find(|known| &known.id == group)?.repos;
    let from = members.iter().position(|member| member == repo)?;
    // The position counts among the other members.
    let to = stepped(members.len(), from, step)?;
    Some(GroupOp::MoveIn {
        group: group.clone(),
        repo: repo.clone(),
        at: position(to),
    })
}

/// What is being dragged: a repo from the place it was dragged from, or a
/// group.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Dragged {
    Repo { repo: RepoId, from: Option<GroupId> },
    Group { group: GroupId },
}

/// What dragging `item`'s row drags, if it can be dragged.
pub fn dragged_of(item: &TreeItem) -> Option<Dragged> {
    match item {
        TreeItem::Repo { repo_id, group_id } => Some(Dragged::Repo {
            repo: repo_id.clone(),
            from: group_id.clone(),
        }),
        TreeItem::Group { group_id, .. } => Some(Dragged::Group {
            group: group_id.clone(),
        }),
        _ => None,
    }
}

/// Where on a row a drop lands: on it, or above or below it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DropPlace {
    Into,
    Before,
    After,
}

impl DropPlace {
    /// Its name in element ids.
    pub fn id(self) -> &'static str {
        match self {
            Self::Into => "into",
            Self::Before => "before",
            Self::After => "after",
        }
    }
}

/// Where on `item`'s row a drop at `fraction` of its height lands, for
/// `dragged`; `None` when it cannot.
pub fn drop_place(dragged: &Dragged, item: &TreeItem, fraction: f32) -> Option<DropPlace> {
    let half = if fraction < 0.5 {
        DropPlace::Before
    } else {
        DropPlace::After
    };
    match (dragged, item) {
        (Dragged::Group { group }, TreeItem::Group { group_id, .. }) => {
            (group_id != group).then_some(half)
        }
        (Dragged::Group { .. }, _) => None,
        (Dragged::Repo { .. }, TreeItem::Group { .. } | TreeItem::Ungrouped) => {
            Some(DropPlace::Into)
        }
        // Under Ungrouped there is no order to drop into.
        (Dragged::Repo { .. }, TreeItem::Repo { group_id: None, .. }) => Some(DropPlace::Into),
        (
            Dragged::Repo { repo, from },
            TreeItem::Repo {
                repo_id,
                group_id: Some(group_id),
            },
        ) => (!(repo_id == repo && from.as_ref() == Some(group_id))).then_some(half),
        (Dragged::Repo { .. }, _) => None,
    }
}

/// What dropping `dragged` at `place` of `item`'s row does (UI-7). A repo
/// dropped on another group moves there from the group it was dragged
/// from, or is added there with `copy` (Ctrl, or Alt on macOS) or when it
/// came from Ungrouped. Dropped on Ungrouped, it leaves the group it came
/// from. Dropped between members, it goes there; within its own group that
/// reorders. A group dropped above or below another moves there.
pub fn drop_op(
    groups: &[GroupInfo],
    dragged: &Dragged,
    item: &TreeItem,
    place: DropPlace,
    copy: bool,
) -> Option<GroupOp> {
    let (repo, from) = match dragged {
        Dragged::Group { group } => {
            let TreeItem::Group { group_id, .. } = item else {
                return None;
            };
            if group_id == group || place == DropPlace::Into {
                return None;
            }
            let mut order: Vec<GroupId> = groups
                .iter()
                .map(|known| known.id.clone())
                .filter(|id| id != group)
                .collect();
            let target = order.iter().position(|id| id == group_id)?;
            let at = if place == DropPlace::Before {
                target
            } else {
                target + 1
            };
            order.insert(at, group.clone());
            let same = order.iter().eq(groups.iter().map(|known| &known.id));
            return (!same).then_some(GroupOp::Reorder { groups: order });
        }
        Dragged::Repo { repo, from } => (repo, from),
    };
    let to = match item {
        TreeItem::Ungrouped | TreeItem::Repo { group_id: None, .. } => {
            return from.clone().map(|group| GroupOp::Remove {
                group,
                repo: repo.clone(),
            });
        }
        TreeItem::Group { group_id, .. }
        | TreeItem::Repo {
            group_id: Some(group_id),
            ..
        } => group_id,
        _ => return None,
    };
    let members = groups
        .iter()
        .find(|known| &known.id == to)
        .map_or(&[][..], |known| known.repos.as_slice());
    let at = match item {
        TreeItem::Repo { repo_id, .. } if place != DropPlace::Into => {
            // Counted among the members other than the dragged repo.
            let others: Vec<&RepoId> = members.iter().filter(|member| *member != repo).collect();
            others
                .iter()
                .position(|member| *member == repo_id)
                .map(|target| match place {
                    DropPlace::Before => position(target),
                    _ => position(target + 1),
                })
        }
        _ => None,
    };
    if from.as_ref() == Some(to) {
        let at = at?;
        let now = members
            .iter()
            .position(|member| member == repo)
            .map(position);
        return (now != Some(at)).then(|| GroupOp::MoveIn {
            group: to.clone(),
            repo: repo.clone(),
            at,
        });
    }
    match from {
        Some(from) if !copy => Some(GroupOp::MoveBetween {
            repo: repo.clone(),
            from: from.clone(),
            to: to.clone(),
            at,
        }),
        _ => Some(GroupOp::Add {
            group: to.clone(),
            repos: vec![repo.clone()],
            at,
        }),
    }
}

/// A group dialog to open.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GroupDialog {
    /// `repos` become the new group's members.
    New {
        repos: Vec<RepoId>,
    },
    Rename {
        group: GroupId,
        name: String,
    },
    Delete {
        group: GroupId,
        name: String,
    },
}

/// What a group entry of a row menu does: open a dialog, or send a
/// change.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GroupChoice {
    Dialog(GroupDialog),
    Op(GroupOp),
    /// Fetch, Pull or Push on the group (UI-20).
    Sync {
        group: GroupId,
        kind: SyncKind,
    },
}

/// What a group entry of a row's menu needs beyond its label.
pub struct GroupEntry {
    pub choice: Option<RowChoice>,
    /// The second menu's items, for Add to group.
    pub submenu: Vec<MenuItem>,
    /// It cannot apply, such as Move up on the first group.
    pub disabled: bool,
}

/// The group entry `action` of `item`'s menu, with the workspace's
/// `groups`, acting on `chosen`, the repos a repo row's entries act on
/// (GRP-5); `None` for another entry.
pub fn group_entry(
    action: TreeAction,
    item: &TreeItem,
    groups: &[GroupInfo],
    chosen: &[RepoId],
) -> Option<GroupEntry> {
    let entry = |choice: Option<GroupChoice>| GroupEntry {
        disabled: choice.is_none(),
        choice: choice.map(RowChoice::Group),
        submenu: Vec::new(),
    };
    let new_group = || {
        GroupChoice::Dialog(GroupDialog::New {
            repos: chosen.to_vec(),
        })
    };
    let named = |group: &GroupId| {
        groups
            .iter()
            .find(|known| &known.id == group)
            .map(|known| known.name.clone())
            .unwrap_or_default()
    };
    let step = if action == TreeAction::MoveUp { -1 } else { 1 };
    Some(match (action, item) {
        (TreeAction::NewGroup, _) => entry(Some(new_group())),
        (TreeAction::GroupFetch, TreeItem::Group { group_id, .. }) => {
            entry(Some(GroupChoice::Sync {
                group: group_id.clone(),
                kind: SyncKind::Fetch,
            }))
        }
        (TreeAction::GroupPull, TreeItem::Group { group_id, .. }) => {
            entry(Some(GroupChoice::Sync {
                group: group_id.clone(),
                kind: SyncKind::Pull,
            }))
        }
        (TreeAction::GroupPush, TreeItem::Group { group_id, .. }) => {
            entry(Some(GroupChoice::Sync {
                group: group_id.clone(),
                kind: SyncKind::Push,
            }))
        }
        (TreeAction::GroupRename, TreeItem::Group { group_id, .. }) => {
            entry(Some(GroupChoice::Dialog(GroupDialog::Rename {
                group: group_id.clone(),
                name: named(group_id),
            })))
        }
        (TreeAction::GroupDelete, TreeItem::Group { group_id, .. }) => {
            entry(Some(GroupChoice::Dialog(GroupDialog::Delete {
                group: group_id.clone(),
                name: named(group_id),
            })))
        }
        (TreeAction::MoveUp | TreeAction::MoveDown, TreeItem::Group { group_id, .. }) => {
            entry(move_group(groups, group_id, step).map(GroupChoice::Op))
        }
        (
            TreeAction::MoveUp | TreeAction::MoveDown,
            TreeItem::Repo {
                repo_id,
                group_id: Some(group_id),
            },
        ) => entry(move_member(groups, group_id, repo_id, step).map(GroupChoice::Op)),
        (
            TreeAction::RemoveFromGroup,
            TreeItem::Repo {
                repo_id,
                group_id: Some(group_id),
            },
        ) => entry(Some(GroupChoice::Op(GroupOp::Remove {
            group: group_id.clone(),
            repo: repo_id.clone(),
        }))),
        (TreeAction::AddToGroup, TreeItem::Repo { repo_id, .. }) => {
            // Each group the repo is not in, then New group. Members stay
            // where they are; only the others are added.
            let mut submenu = Vec::new();
            let mut choices = Vec::new();
            for group in groups.iter().filter(|group| !group.repos.contains(repo_id)) {
                let repos: Vec<RepoId> = chosen
                    .iter()
                    .filter(|repo| !group.repos.contains(repo))
                    .cloned()
                    .collect();
                submenu.push(MenuItem::new(group.name.clone()));
                choices.push(Some(RowChoice::Group(GroupChoice::Op(GroupOp::Add {
                    group: group.id.clone(),
                    repos,
                    at: None,
                }))));
            }
            submenu.push(MenuItem::new(text::NEW_GROUP));
            choices.push(Some(RowChoice::Group(new_group())));
            GroupEntry {
                choice: Some(RowChoice::Submenu(choices)),
                submenu,
                disabled: false,
            }
        }
        (
            TreeAction::AddToGroup
            | TreeAction::RemoveFromGroup
            | TreeAction::MoveUp
            | TreeAction::MoveDown
            | TreeAction::GroupRename
            | TreeAction::GroupDelete,
            _,
        ) => entry(None),
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn id(name: &str) -> RepoId {
        RepoId(name.into())
    }

    fn gid(name: &str) -> GroupId {
        GroupId(name.into())
    }

    /// Backend: r1 r2, Frontend: r2 r3; r4 in no group.
    fn groups() -> Vec<GroupInfo> {
        vec![
            GroupInfo {
                id: gid("b"),
                name: "Backend".into(),
                repos: vec![id("r1"), id("r2")],
            },
            GroupInfo {
                id: gid("f"),
                name: "Frontend".into(),
                repos: vec![id("r2"), id("r3")],
            },
        ]
    }

    fn repo(repo: &str, group: Option<&str>) -> TreeItem {
        TreeItem::Repo {
            repo_id: id(repo),
            group_id: group.map(gid),
        }
    }

    fn group(group: &str) -> TreeItem {
        TreeItem::Group {
            group_id: gid(group),
            summary: gitcroc_core::GroupSummary {
                workspace_id: WorkspaceId("w".into()),
                group_id: gid(group),
                branch: gitcroc_core::GroupBranch::None,
                members: 0,
                dirty: 0,
                conflicts: 0,
            },
        }
    }

    fn dragged(repo: &str, from: Option<&str>) -> Dragged {
        Dragged::Repo {
            repo: id(repo),
            from: from.map(gid),
        }
    }

    #[test]
    fn ui_7_a_repo_moves_to_another_group_or_is_added_with_copy() {
        let groups = groups();
        let r1 = dragged("r1", Some("b"));
        assert_eq!(
            drop_op(&groups, &r1, &group("f"), DropPlace::Into, false),
            Some(GroupOp::MoveBetween {
                repo: id("r1"),
                from: gid("b"),
                to: gid("f"),
                at: None
            })
        );
        assert_eq!(
            drop_op(&groups, &r1, &group("f"), DropPlace::Into, true),
            Some(GroupOp::Add {
                group: gid("f"),
                repos: vec![id("r1")],
                at: None
            })
        );
        // From Ungrouped it is always added; above a member it goes there.
        assert_eq!(
            drop_op(
                &groups,
                &dragged("r4", None),
                &repo("r3", Some("f")),
                DropPlace::Before,
                false
            ),
            Some(GroupOp::Add {
                group: gid("f"),
                repos: vec![id("r4")],
                at: Some(1)
            })
        );
    }

    #[test]
    fn ui_7_ungrouped_removes_from_the_group_it_came_from_only() {
        let groups = groups();
        assert_eq!(
            drop_op(
                &groups,
                &dragged("r2", Some("f")),
                &TreeItem::Ungrouped,
                DropPlace::Into,
                false
            ),
            Some(GroupOp::Remove {
                group: gid("f"),
                repo: id("r2")
            })
        );
        assert_eq!(
            drop_op(
                &groups,
                &dragged("r4", None),
                &repo("r4", None),
                DropPlace::Into,
                false
            ),
            None
        );
    }

    #[test]
    fn ui_7_within_a_group_dropping_between_members_reorders() {
        let groups = groups();
        let r1 = dragged("r1", Some("b"));
        assert_eq!(
            drop_place(&r1, &repo("r2", Some("b")), 0.9),
            Some(DropPlace::After)
        );
        assert_eq!(
            drop_op(
                &groups,
                &r1,
                &repo("r2", Some("b")),
                DropPlace::After,
                false
            ),
            Some(GroupOp::MoveIn {
                group: gid("b"),
                repo: id("r1"),
                at: 1
            })
        );
        // Where it is already, and on itself, nothing happens.
        assert_eq!(
            drop_op(
                &groups,
                &r1,
                &repo("r2", Some("b")),
                DropPlace::Before,
                false
            ),
            None
        );
        assert_eq!(drop_place(&r1, &repo("r1", Some("b")), 0.2), None);
        assert_eq!(
            drop_op(&groups, &r1, &group("b"), DropPlace::Into, false),
            None
        );
    }

    #[test]
    fn ui_7_a_group_drops_above_or_below_another_group_only() {
        let groups = groups();
        let frontend = Dragged::Group { group: gid("f") };
        assert_eq!(drop_place(&frontend, &repo("r1", Some("b")), 0.2), None);
        assert_eq!(drop_place(&frontend, &TreeItem::Ungrouped, 0.2), None);
        assert_eq!(drop_place(&frontend, &group("f"), 0.2), None);
        assert_eq!(
            drop_place(&frontend, &group("b"), 0.2),
            Some(DropPlace::Before)
        );
        assert_eq!(
            drop_op(&groups, &frontend, &group("b"), DropPlace::Before, false),
            Some(GroupOp::Reorder {
                groups: vec![gid("f"), gid("b")]
            })
        );
        assert_eq!(
            drop_op(&groups, &frontend, &group("b"), DropPlace::After, false),
            None
        );
    }

    #[test]
    fn ui_7_move_up_and_down_stop_at_the_ends() {
        let groups = groups();
        assert_eq!(move_group(&groups, &gid("b"), -1), None);
        assert_eq!(
            move_group(&groups, &gid("b"), 1),
            Some(GroupOp::Reorder {
                groups: vec![gid("f"), gid("b")]
            })
        );
        assert_eq!(move_member(&groups, &gid("b"), &id("r2"), 1), None);
        assert_eq!(
            move_member(&groups, &gid("b"), &id("r2"), -1),
            Some(GroupOp::MoveIn {
                group: gid("b"),
                repo: id("r2"),
                at: 0
            })
        );
    }

    #[test]
    fn ui_7_add_to_group_lists_the_other_groups_then_new_group() {
        let groups = groups();
        let chosen = [id("r1"), id("r2")];
        let entry = group_entry(
            TreeAction::AddToGroup,
            &repo("r1", Some("b")),
            &groups,
            &chosen,
        )
        .unwrap();
        let names: Vec<&str> = entry
            .submenu
            .iter()
            .map(|item| item.text.as_ref())
            .collect();
        assert_eq!(names, ["Frontend", text::NEW_GROUP]);
        let Some(RowChoice::Submenu(choices)) = entry.choice else {
            panic!("no second menu");
        };
        // r2 is in Frontend already: only r1 is added.
        assert!(matches!(
            &choices[0],
            Some(RowChoice::Group(GroupChoice::Op(GroupOp::Add { repos, .. }))) if repos == &[id("r1")]
        ));
    }
}
