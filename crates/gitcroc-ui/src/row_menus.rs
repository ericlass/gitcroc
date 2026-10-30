//! Row menu presentations, ported from rc.1's tree/menus.ts and
//! history/CommitMenu.tsx (UI-7, UI-13). The core still authorizes writes.
use crate::menu::MenuItem;
use crate::strings::{commit_menu as commit, tree_menu as text};
use gitcroc_core::TreeItem;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TreeAction {
    Checkout,
    CheckoutRemote,
    CheckoutDetached,
    Create,
    Rename,
    Delete,
    DeleteRemote,
    SetUpstream,
    UnsetUpstream,
    OpenFileManager,
    OpenTerminal,
    RemoveRepo,
    CloneMissing,
    TagCreate,
    TagPush,
    TagDelete,
    RemoteAdd,
    RemoteRename,
    RemoteUrl,
    RemoteRemove,
    StashApply,
    StashPop,
    StashDrop,
    AddToGroup,
    RemoveFromGroup,
    MoveUp,
    MoveDown,
    NewGroup,
    GroupFetch,
    GroupPull,
    GroupPush,
    GroupCheckout,
    GroupCreate,
    GroupDeleteBranch,
    GroupMerge,
    GroupRebase,
    GroupTag,
    GroupRename,
    GroupDelete,
}

impl TreeAction {
    fn available(self) -> bool {
        matches!(
            self,
            Self::Checkout
                | Self::CheckoutRemote
                | Self::CheckoutDetached
                | Self::Create
                | Self::Rename
                | Self::Delete
                | Self::DeleteRemote
                | Self::SetUpstream
                | Self::UnsetUpstream
                | Self::OpenFileManager
                | Self::OpenTerminal
                | Self::RemoveRepo
                | Self::TagCreate
                | Self::TagPush
                | Self::TagDelete
                | Self::RemoteAdd
                | Self::RemoteRename
                | Self::RemoteUrl
                | Self::RemoteRemove
                | Self::StashApply
                | Self::StashPop
                | Self::StashDrop
        )
    }
    fn read_only(self) -> bool {
        matches!(
            self,
            Self::OpenFileManager | Self::OpenTerminal | Self::RemoveRepo
        )
    }
}

#[derive(Debug, Clone)]
pub struct TreeEntry {
    pub action: TreeAction,
    pub item: MenuItem,
}

/// All entries remain visible. Later tasks supply the currently disabled
/// group operations.
pub fn tree_menu(row: &TreeItem, blocked: Option<&str>, missing: bool) -> Vec<TreeEntry> {
    use TreeAction as A;
    let mut entries = Vec::new();
    let mut add = |action: TreeAction, label: &'static str, disabled: bool| {
        let reason = if !action.read_only() && blocked.is_some() {
            blocked
        } else if !action.available() {
            Some(text::NOT_AVAILABLE)
        } else {
            None
        };
        entries.push(TreeEntry {
            action,
            item: MenuItem::new(label)
                .disabled(disabled || reason.is_some())
                .description(reason.map(|value| value.to_owned().into())),
        });
    };
    match row {
        TreeItem::Repo { group_id, .. } => {
            if missing {
                add(A::CloneMissing, text::CLONE_FROM_REMOTE, false);
            }
            add(A::Create, text::NEW_BRANCH, false);
            add(A::TagCreate, text::NEW_TAG, false);
            add(A::AddToGroup, text::ADD_TO_GROUP, false);
            if group_id.is_some() {
                add(A::RemoveFromGroup, text::REMOVE_FROM_GROUP, false);
                add(A::MoveUp, text::MOVE_UP, false);
                add(A::MoveDown, text::MOVE_DOWN, false);
            }
            add(A::OpenFileManager, text::OPEN_IN_FILE_MANAGER, false);
            add(A::OpenTerminal, text::OPEN_IN_TERMINAL, false);
            add(A::RemoveRepo, text::REMOVE_FROM_WORKSPACE, false);
        }
        TreeItem::Branch {
            current, upstream, ..
        } => {
            add(A::Checkout, text::CHECK_OUT, *current);
            add(A::Create, text::NEW_BRANCH_FROM_HERE, false);
            add(A::Rename, text::RENAME, false);
            add(A::Delete, text::DELETE, *current);
            add(A::SetUpstream, text::SET_UPSTREAM, false);
            add(A::UnsetUpstream, text::UNSET_UPSTREAM, upstream.is_none());
        }
        TreeItem::RemoteBranch { .. } => {
            add(A::CheckoutRemote, text::CHECK_OUT, false);
            add(A::Create, text::NEW_BRANCH_FROM_HERE, false);
            add(A::DeleteRemote, text::DELETE_ON_REMOTE, false);
        }
        TreeItem::Tag { .. } => {
            add(A::CheckoutDetached, text::CHECK_OUT_DETACHED, false);
            add(A::Create, text::NEW_BRANCH_FROM_HERE, false);
            add(A::TagPush, text::PUSH, false);
            add(A::TagDelete, text::DELETE, false);
        }
        TreeItem::Remotes { .. } => add(A::RemoteAdd, text::ADD_REMOTE, false),
        TreeItem::Remote { .. } => {
            add(A::RemoteRename, text::RENAME, false);
            add(A::RemoteUrl, text::EDIT_URL, false);
            add(A::RemoteRemove, text::REMOVE, false);
        }
        TreeItem::Stash { .. } => {
            add(A::StashApply, text::APPLY, false);
            add(A::StashPop, text::POP, false);
            add(A::StashDrop, text::DROP, false);
        }
        TreeItem::Group { .. } => {
            for (action, label) in [
                (A::GroupFetch, text::FETCH),
                (A::GroupPull, text::PULL),
                (A::GroupPush, text::PUSH),
                (A::GroupCheckout, text::CHECK_OUT_BRANCH),
                (A::GroupCreate, text::NEW_BRANCH),
                (A::GroupDeleteBranch, text::DELETE_BRANCH),
                (A::GroupMerge, text::MERGE),
                (A::GroupRebase, text::REBASE),
                (A::GroupTag, text::CREATE_TAG),
                (A::GroupRename, text::RENAME),
                (A::GroupDelete, text::DELETE),
                (A::NewGroup, text::NEW_GROUP),
                (A::MoveUp, text::MOVE_UP),
                (A::MoveDown, text::MOVE_DOWN),
            ] {
                add(action, label, false);
            }
        }
        TreeItem::GroupBranch { .. } => {
            add(A::GroupCheckout, text::CHECK_OUT, false);
            add(A::GroupDeleteBranch, text::DELETE, false);
        }
        TreeItem::Ungrouped => add(A::NewGroup, text::NEW_GROUP, false),
        _ => {}
    }
    entries
}

/// In rc.1 action order: checkout, branch, tag, cherry-pick, revert,
/// reset, copy. Only cherry-pick accepts several commits from one repo.
pub fn commit_menu(count: usize, same_repo: bool, blocked: Option<&str>) -> Vec<MenuItem> {
    [
        commit::CHECKOUT,
        commit::CREATE_BRANCH,
        commit::CREATE_TAG,
        commit::CHERRY_PICK,
        commit::REVERT,
        commit::RESET,
        commit::COPY_HASH,
    ]
    .into_iter()
    .enumerate()
    .map(|(at, label)| {
        let reason = if at == 3 && !same_repo {
            Some(text::ONE_REPO)
        } else if at != 6 {
            blocked
        } else {
            None
        };
        let label = if at == 3 && count != 1 {
            commit::cherry_pick(count)
        } else {
            label.to_owned()
        };
        MenuItem::new(label)
            .disabled((count != 1 && at != 3) || count == 0 || reason.is_some())
            .description(reason.map(|value| value.to_owned().into()))
    })
    .collect()
}
