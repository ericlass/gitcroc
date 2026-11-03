//! What each palette operation runs (UI-22), a port of
//! `ui/src/palette/commands.ts`. Each place of the window that has an
//! operation's toolbar button, menu entry or pane control gives the
//! palette the same function that control calls, and the rule that
//! enables it: the palette adds no dialog, no way to run an operation and
//! no rule of its own. A place gives commands only while it is shown, for
//! the workspace it shows (ARCH-9); an operation no place gives now lacks
//! its subject, and the palette names what to select.

use gitcroc_core::{PaletteOp, ResetMode, WorkspaceId};
use gpui_kit::{App, Window};

use super::GitcrocWindow;
use crate::operation_dialog::OperationMode;
use crate::palette::Command;
use crate::row_context::{AskRowMenu, RowChoice};
use crate::row_menus::TreeAction;
use crate::settings_dialog::SettingsTab;
use crate::strings::palette as text;
use crate::toolbar::ToolbarButton;
use crate::workspace_menu::Choice;

/// The places of the window an operation lives in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Place {
    /// The toolbar, the status bar, Settings and the tabs' menus.
    App,
    /// The tab bar's keys.
    Tabs,
    /// The tree's add menu and its rows' context menus.
    Tree,
    /// The history's commit menu.
    History,
    /// The changes pane's file lists.
    Files,
    /// The changes pane's commit box.
    Commit,
    /// The conflict banner above the history.
    Banner,
    /// The conflict editor's toolbar and hunk keys.
    Editor,
    /// The choice of a file that cannot be merged line by line (OPS-6).
    Choice,
}

/// The place of every operation. A new operation in the core's catalog
/// fails to compile here until it has one.
pub fn place(op: PaletteOp) -> Place {
    use PaletteOp as Op;
    match op {
        Op::AddFolder
        | Op::Clone
        | Op::RemoveRepo
        | Op::OpenInFileManager
        | Op::OpenInTerminal
        | Op::Checkout
        | Op::RenameBranch
        | Op::DeleteBranch
        | Op::SetUpstream
        | Op::CheckoutRemoteBranch
        | Op::StashApply
        | Op::StashPop
        | Op::StashDrop
        | Op::CreateTag
        | Op::DeleteTag
        | Op::PushTag
        | Op::AddRemote
        | Op::RenameRemote
        | Op::RemoveRemote
        | Op::EditRemoteUrl
        | Op::NewGroup => Place::Tree,
        Op::StageFile | Op::UnstageFile | Op::DiscardFile | Op::StageAll | Op::UnstageAll => {
            Place::Files
        }
        Op::Commit | Op::Amend => Place::Commit,
        Op::CheckoutCommit
        | Op::CherryPick
        | Op::Revert
        | Op::ResetSoft
        | Op::ResetMixed
        | Op::ResetHard => Place::History,
        Op::Abort | Op::Continue | Op::ResolveAllWithAi => Place::Banner,
        Op::MarkResolved
        | Op::TakeOurs
        | Op::TakeTheirs
        | Op::TakeBoth
        | Op::ResolveWithAi
        | Op::AcceptHighConfidence => Place::Editor,
        Op::KeepOurs | Op::KeepTheirs => Place::Choice,
        Op::CreateBranch
        | Op::Fetch
        | Op::FetchWithOptions
        | Op::Pull
        | Op::PullWithOptions
        | Op::Push
        | Op::PushWithOptions
        | Op::Merge
        | Op::Rebase
        | Op::Stash
        | Op::Settings
        | Op::NewWorkspace
        | Op::ImportWorkspace
        | Op::ExportWorkspace
        | Op::JobList
        | Op::CloseTab
        | Op::CheckForUpdates => Place::App,
        Op::NextTab | Op::PreviousTab => Place::Tabs,
    }
}

/// The tree menu entries an operation runs: the first of them the
/// selected row's menu has.
fn tree_actions(op: PaletteOp) -> &'static [TreeAction] {
    use PaletteOp as Op;
    use TreeAction as A;
    match op {
        Op::RemoveRepo => &[A::RemoveRepo],
        Op::OpenInFileManager => &[A::OpenFileManager],
        Op::OpenInTerminal => &[A::OpenTerminal],
        Op::Checkout => &[A::Checkout, A::GroupCheckout],
        Op::RenameBranch => &[A::Rename],
        Op::DeleteBranch => &[A::Delete, A::GroupDeleteBranch],
        Op::SetUpstream => &[A::SetUpstream],
        Op::CheckoutRemoteBranch => &[A::CheckoutRemote],
        Op::StashApply => &[A::StashApply],
        Op::StashPop => &[A::StashPop],
        Op::StashDrop => &[A::StashDrop],
        Op::CreateTag => &[A::TagCreate, A::GroupTag],
        Op::DeleteTag => &[A::TagDelete],
        Op::PushTag => &[A::TagPush],
        Op::AddRemote => &[A::RemoteAdd],
        Op::RenameRemote => &[A::RemoteRename],
        Op::RemoveRemote => &[A::RemoteRemove],
        Op::EditRemoteUrl => &[A::RemoteUrl],
        Op::NewGroup => &[A::NewGroup],
        _ => &[],
    }
}

/// Entry `at` of a row menu, as choosing it in the menu does: disabled
/// with its reason, or its choice.
fn menu_entry(ask: AskRowMenu, at: usize) -> Option<Command> {
    let (item, choice) = ask.items.get(at)?.clone();
    if item.disabled {
        return Some(Command::of(None, item.description.as_deref()));
    }
    let choice = choice?;
    Some(Command::run(move |this, window, cx| {
        this.row_choice(ask, choice, window, cx)
    }))
}

/// The first entry of `menu` that is one of `wanted`.
fn menu_command(menu: (AskRowMenu, Vec<TreeAction>), wanted: &[TreeAction]) -> Option<Command> {
    let (ask, actions) = menu;
    let at = actions.iter().position(|action| wanted.contains(action))?;
    menu_entry(ask, at)
}

impl GitcrocWindow {
    /// What `op` does now in `workspace`, as its place in the window does
    /// it; `None` while its place is not shown or the selection lacks its
    /// subject.
    pub(crate) fn palette_command(
        &self,
        workspace: &WorkspaceId,
        op: PaletteOp,
        cx: &App,
    ) -> Option<Command> {
        if self.shown.as_ref() != Some(workspace) {
            return None;
        }
        match place(op) {
            Place::App => self.app_command(workspace, op, cx),
            Place::Tabs => self.tabs_command(op, cx),
            Place::Tree => self.tree_command(workspace, op, cx),
            Place::History => self.history_command(workspace, op, cx),
            Place::Banner => self
                .histories
                .get(workspace)?
                .read(cx)
                .banner_command(op, cx),
            Place::Files | Place::Commit | Place::Editor | Place::Choice => {
                let pane = self.changes.get(workspace)?;
                pane.read(cx).palette_command(pane, op, cx)
            }
        }
    }

    /// The toolbar's operations and the window's own.
    fn app_command(&self, workspace: &WorkspaceId, op: PaletteOp, cx: &App) -> Option<Command> {
        use PaletteOp as Op;
        use ToolbarButton as B;
        let workspace = workspace.clone();
        Some(match op {
            Op::Fetch => return self.toolbar_command(B::Fetch, false, cx),
            Op::FetchWithOptions => return self.toolbar_command(B::Fetch, true, cx),
            Op::Pull => return self.toolbar_command(B::Pull, false, cx),
            Op::PullWithOptions => return self.toolbar_command(B::Pull, true, cx),
            Op::Push => return self.toolbar_command(B::Push, false, cx),
            Op::PushWithOptions => return self.toolbar_command(B::Push, true, cx),
            Op::CreateBranch => return self.toolbar_command(B::Branch, false, cx),
            Op::Merge => return self.toolbar_command(B::Merge, false, cx),
            Op::Stash => return self.toolbar_command(B::Stash, false, cx),
            Op::Rebase => return self.rebase_command(cx),
            // The AI tab until M6-26 brings the others.
            Op::Settings => {
                Command::run(|this, window, cx| this.open_settings(SettingsTab::Ai, window, cx))
            }
            Op::NewWorkspace => {
                Command::run(|this, window, cx| this.choose(Choice::New, window, cx))
            }
            Op::ImportWorkspace => {
                Command::run(|this, window, cx| this.choose(Choice::Import, window, cx))
            }
            Op::ExportWorkspace => Command::run(move |this, window, cx| {
                this.choose(Choice::Export(workspace), window, cx)
            }),
            Op::JobList => Command::run(|this, window, cx| this.open_job_list(None, window, cx)),
            Op::CloseTab => {
                Command::run(move |this, window, cx| this.close_tab(workspace, window, cx))
            }
            // Until the Updates tab of Settings (M6-26) can show a check's
            // result, only a version on offer has a place to go.
            Op::CheckForUpdates => {
                let offered = self.updates.read(cx).view().is_some_and(|view| {
                    matches!(view.status, gitcroc_core::UpdateStatus::Available { .. })
                });
                if offered {
                    Command::run(|this, window, cx| this.open_update(window, cx))
                } else {
                    Command::cannot(text::NO_UPDATE)
                }
            }
            _ => return None,
        })
    }

    /// The tab bar's keys: the tab beside the active one.
    fn tabs_command(&self, op: PaletteOp, cx: &App) -> Option<Command> {
        let (step, last) = match op {
            PaletteOp::NextTab => (1, text::LAST_TAB),
            PaletteOp::PreviousTab => (-1, text::FIRST_TAB),
            _ => return None,
        };
        if !self.tab_bar.read(cx).has_beside_active(step, cx) {
            return Some(Command::cannot(last));
        }
        Some(Command::run(move |this, window, cx| {
            this.tab_bar.update(cx, |tab_bar, cx| {
                tab_bar.activate_beside_active(step, window, cx)
            })
        }))
    }

    /// The tree's add menu, and the selected row's menu.
    fn tree_command(&self, workspace: &WorkspaceId, op: PaletteOp, cx: &App) -> Option<Command> {
        let tree = self.trees.get(workspace)?.read(cx);
        match op {
            PaletteOp::AddFolder => {
                let menu = tree.add_menu().clone();
                Some(Command::run(move |_, _, cx| {
                    menu.update(cx, |menu, cx| menu.add_folder(cx))
                }))
            }
            PaletteOp::Clone => {
                let ask = tree.add_menu().read(cx).clone_ask();
                Some(Command::run(move |this, window, cx| {
                    this.open_clone(ask, window, cx)
                }))
            }
            // New group as the selected row's menu has it, which takes a
            // selected repo along, or as Ungrouped's.
            PaletteOp::NewGroup => {
                let menu = tree
                    .selected_menu()
                    .filter(|(_, actions)| actions.contains(&TreeAction::NewGroup))
                    .or_else(|| tree.ungrouped_menu())?;
                menu_command(menu, tree_actions(op))
            }
            _ => menu_command(tree.selected_menu()?, tree_actions(op)),
        }
    }

    /// The history's commit menu on the selected commit; Reset with the
    /// mode chosen in the palette.
    fn history_command(&self, workspace: &WorkspaceId, op: PaletteOp, cx: &App) -> Option<Command> {
        use PaletteOp as Op;
        let (at, reset) = match op {
            Op::CheckoutCommit => (0, None),
            Op::CherryPick => (3, None),
            Op::Revert => (4, None),
            Op::ResetSoft => (5, Some(ResetMode::Soft)),
            Op::ResetMixed => (5, Some(ResetMode::Mixed)),
            Op::ResetHard => (5, Some(ResetMode::Hard)),
            _ => return None,
        };
        let mut ask = self
            .histories
            .get(workspace)?
            .read(cx)
            .selected_commit_menu(cx)?;
        if let (
            Some(chosen),
            Some((_, Some(RowChoice::Operation(OperationMode::Reset { mode, .. })))),
        ) = (reset, ask.items.get_mut(at))
        {
            *mode = chosen;
        }
        menu_entry(ask, at)
    }

    /// Runs what the palette chose, after closing it: an operation as its
    /// place does, or a repo, group or branch selected in the tree, which
    /// gets the focus (UI-22).
    pub(super) fn palette_chosen(
        &mut self,
        item: gitcroc_core::PaletteItem,
        window: &mut Window,
        cx: &mut gpui_kit::Context<Self>,
    ) {
        use crate::tree::Reveal;
        use gitcroc_core::PaletteItem;
        if let Some(open) = self.dialog.take()
            && let Some(restore) = open.restore
        {
            window.focus(&restore, cx);
        }
        self.palette_events = None;
        cx.notify();
        let Some(workspace) = self.shown.clone() else {
            return;
        };
        let place = match item {
            PaletteItem::Operation { op, .. } => {
                if let Some(Command::Run(run)) = self.palette_command(&workspace, op, cx) {
                    run(self, window, cx);
                }
                self.sync_preflight(window, cx);
                return;
            }
            PaletteItem::Repo { repo_id, .. } => Reveal::Repo(repo_id),
            PaletteItem::Group { group_id, .. } => Reveal::Group(group_id),
            PaletteItem::Branch {
                repo_id,
                name,
                remote,
                ..
            } => Reveal::Branch {
                repo: repo_id,
                name,
                remote: remote.is_some(),
            },
        };
        if let Some(tree) = self.trees.get(&workspace).cloned() {
            tree.update(cx, |tree, cx| tree.reveal(place, window, cx));
            let focus = gpui_kit::Focusable::focus_handle(tree.read(cx), cx);
            window.focus(&focus, cx);
        }
    }
}
