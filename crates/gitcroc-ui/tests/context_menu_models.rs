#![allow(clippy::unwrap_used, clippy::expect_used)]

use gitcroc_core::TreeItem;
use gitcroc_ui::row_menus::{TreeAction, commit_menu, tree_menu};

#[test]
fn ui_7_tree_row_menus_list_the_valid_operations() {
    let row = TreeItem::Branch {
        repo_id: gitcroc_core::RepoId("repo".into()),
        name: "main".into(),
        current: true,
        upstream: None,
        gone: false,
        ahead: None,
        behind: None,
    };
    let entries = tree_menu(&row, None, false);
    assert_eq!(entries.len(), 6);
    assert_eq!(entries[0].action, TreeAction::Checkout);
    assert!(entries[0].item.disabled);
    assert!(!entries[1].item.disabled);
    assert!(entries[3].item.disabled);
    assert!(entries[5].item.disabled);
    assert!(
        tree_menu(&row, Some("conflict"), false)
            .iter()
            .all(|entry| entry.item.disabled)
    );
    let repo = TreeItem::Repo {
        repo_id: gitcroc_core::RepoId("repo".into()),
        group_id: None,
    };
    let entries = tree_menu(&repo, Some("missing"), true);
    assert!(
        entries
            .iter()
            .filter(|entry| matches!(
                entry.action,
                TreeAction::OpenFileManager | TreeAction::OpenTerminal | TreeAction::RemoveRepo
            ))
            .all(|entry| !entry.item.disabled)
    );
    assert!(
        entries
            .iter()
            .filter(|entry| !matches!(
                entry.action,
                TreeAction::OpenFileManager | TreeAction::OpenTerminal | TreeAction::RemoveRepo
            ))
            .all(|entry| entry.item.disabled)
    );
}

#[test]
fn ui_7_tree_menu_entries_match_rc1_row_kinds() {
    use TreeAction as A;
    use gitcroc_core::{GroupId, RepoId};
    let repo_id = RepoId("r1".into());
    let group_id = GroupId("g1".into());
    let group = TreeItem::Group {
        group_id: group_id.clone(),
        summary: gitcroc_core::GroupSummary {
            workspace_id: gitcroc_core::WorkspaceId("workspace".into()),
            group_id: group_id.clone(),
            branch: gitcroc_core::GroupBranch::None,
            members: 0,
            dirty: 0,
            conflicts: 0,
        },
    };
    let entries = tree_menu(&group, None, false);
    assert_eq!(entries.len(), 14);
    assert!(entries.iter().all(|entry| entry.item.disabled));
    let cases = vec![
        (
            TreeItem::RemoteBranch {
                repo_id: repo_id.clone(),
                name: "origin/main".into(),
                remote: "origin".into(),
                branch: "main".into(),
            },
            vec![A::CheckoutRemote, A::Create, A::DeleteRemote],
        ),
        (
            TreeItem::Tag {
                repo_id: repo_id.clone(),
                name: "v1".into(),
                annotated: false,
            },
            vec![A::CheckoutDetached, A::Create, A::TagPush, A::TagDelete],
        ),
        (
            TreeItem::Remotes {
                repo_id: repo_id.clone(),
                group_id: None,
            },
            vec![A::RemoteAdd],
        ),
        (
            TreeItem::Remote {
                repo_id: repo_id.clone(),
                remote: "origin".into(),
            },
            vec![A::RemoteRename, A::RemoteUrl, A::RemoteRemove],
        ),
        (
            TreeItem::Stash {
                repo_id: repo_id.clone(),
                index: 0,
                hash: "abc".into(),
                message: "saved".into(),
                seconds: 0,
            },
            vec![A::StashApply, A::StashPop, A::StashDrop],
        ),
        (
            TreeItem::GroupBranch {
                group_id,
                name: "main".into(),
                present: 1,
                members: 1,
            },
            vec![A::GroupCheckout, A::GroupDeleteBranch],
        ),
        (TreeItem::Ungrouped, vec![A::NewGroup]),
        (
            TreeItem::Local {
                repo_id,
                group_id: None,
            },
            vec![],
        ),
    ];
    for (row, actions) in cases {
        let entries = tree_menu(&row, None, false);
        assert_eq!(
            entries.iter().map(|entry| entry.action).collect::<Vec<_>>(),
            actions
        );
        for entry in &entries {
            if entry.item.disabled {
                assert_eq!(entry.item.description.as_deref(), Some("Not available yet"));
            }
        }
        assert!(
            tree_menu(&row, Some("conflict"), false)
                .iter()
                .all(|entry| entry.item.disabled)
        );
    }
}

#[test]
fn ui_13_commit_context_menu() {
    let entries = commit_menu(1, true, None);
    assert_eq!(entries.len(), 7);
    assert!(!entries[0].disabled);
    assert!(!entries[1].disabled);
    assert!(!entries[6].disabled);
    assert!(entries[2..6].iter().all(|entry| entry.disabled));
    let blocked = commit_menu(1, true, Some("conflict"));
    assert!(blocked[..6].iter().all(|entry| entry.disabled));
    assert!(!blocked[6].disabled);
}

#[test]
fn ui_13_cherry_pick_needs_rows_of_one_repo() {
    for same_repo in [false, true] {
        let entries = commit_menu(3, same_repo, None);
        assert!(entries.iter().all(|entry| entry.disabled));
        assert_eq!(entries[3].text.as_ref(), "Cherry-pick 3 commits");
        assert!(entries[3].description.is_some());
    }
}
