//! Requests from row menus; the window owns their overlays and dialogs.
use crate::{branch_dialog::BranchMode, menu::MenuItem, row_menus::TreeAction, strings};
use gitcroc_core::{RepoId, SummaryView, WorkspaceId};
use gpui_kit::{FocusHandle, Pixels, Point};
#[derive(Clone)]
pub enum RowChoice {
    Branch(BranchMode),
    Operation(crate::operation_dialog::OperationMode),
    Repo(TreeAction),
    Copy(String),
}
#[derive(Clone)]
pub struct AskRowMenu {
    pub label: &'static str,
    pub workspace: WorkspaceId,
    pub repo: Option<RepoId>,
    pub repo_name: String,
    pub items: Vec<(MenuItem, Option<RowChoice>)>,
    pub at: Point<Pixels>,
    pub focus: FocusHandle,
}
pub fn blocked(view: Option<&SummaryView>) -> Option<&'static str> {
    let view = view?;
    if view.missing {
        Some(strings::tree::FOLDER_MISSING)
    } else if view.conflict {
        Some(strings::tree::CONFLICT)
    } else if view.busy {
        Some(strings::branches::BUSY)
    } else {
        None
    }
}
