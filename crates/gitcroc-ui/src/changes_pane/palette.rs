//! What the changes pane's controls do now, for the command palette
//! (UI-22): the file lists' buttons and keys, the commit box's buttons,
//! and the conflict editor's, as they are enabled.

use gitcroc_core::{ChangeAction, ChangesContent, PaletteOp};
use gpui_kit::{App, Entity};

use super::{ChangesPane, row_actions};
use crate::changes::{List, move_action};
use crate::history::Selection;
use crate::palette::Command;
use crate::strings::palette as text;

impl ChangesPane {
    /// What `op` does now in this pane, `pane`; `None` while its control
    /// is not shown.
    pub fn palette_command(&self, pane: &Entity<Self>, op: PaletteOp, cx: &App) -> Option<Command> {
        match op {
            PaletteOp::StageFile => self.file_command(pane, ChangeAction::Stage),
            PaletteOp::UnstageFile => self.file_command(pane, ChangeAction::Unstage),
            PaletteOp::DiscardFile => self.file_command(pane, ChangeAction::Discard),
            PaletteOp::StageAll => self.list_command(pane, List::Unstaged),
            PaletteOp::UnstageAll => self.list_command(pane, List::Staged),
            PaletteOp::Commit | PaletteOp::Amend => self.commit_command(op, cx),
            PaletteOp::KeepOurs | PaletteOp::KeepTheirs => self.choice_command(pane, op),
            _ => self.editor_command(pane, op, cx),
        }
    }

    /// The uncommitted changes are shown, whose lists have the controls.
    fn uncommitted(&self) -> bool {
        matches!(
            self.changes.selection(),
            Some(Selection::Uncommitted { .. } | Selection::MergedUncommitted { .. })
        )
    }

    /// `action` on the selected file, as its row's control does it.
    fn file_command(&self, pane: &Entity<Self>, action: ChangeAction) -> Option<Command> {
        if !self.uncommitted() {
            return None;
        }
        let file = self.changes.file()?.clone();
        let has = self
            .changes
            .changeable(&file)
            .is_some_and(|list| row_actions(list).contains(&action));
        if !has {
            return Some(Command::cannot(text::CANNOT_APPLY));
        }
        if let Some(why) = self.changes.blocked(file.repo.as_ref()) {
            return Some(Command::cannot(why.to_owned()));
        }
        let pane = pane.clone();
        Some(Command::run(move |_, _, cx| {
            pane.update(cx, |pane, cx| {
                let files = pane.changes.targets(&file);
                pane.apply_files(action, &files, cx);
            })
        }))
    }

    /// Stage all or Unstage all: the button of one repo's `list`.
    fn list_command(&self, pane: &Entity<Self>, list: List) -> Option<Command> {
        if !self.uncommitted() {
            return None;
        }
        let merged = self
            .changes
            .view()
            .is_some_and(|view| matches!(view.content, ChangesContent::Merged { .. }));
        if merged {
            return Some(Command::cannot(text::ONE_REPO));
        }
        let files = self.changes.list_files(list, None);
        let action = move_action(list)?;
        if files.is_empty() {
            return Some(Command::cannot(text::CANNOT_APPLY));
        }
        if let Some(why) = self.changes.blocked(None) {
            return Some(Command::cannot(why.to_owned()));
        }
        let pane = pane.clone();
        Some(Command::run(move |_, _, cx| {
            pane.update(cx, |pane, cx| pane.apply_files(action, &files, cx))
        }))
    }

    /// Commit or Amend, as the commit box's button.
    fn commit_command(&self, op: PaletteOp, cx: &App) -> Option<Command> {
        let commit_box = self.commit_box.read(cx);
        let group = commit_box.group_repos().is_some();
        if commit_box.target().is_none() && !group {
            return None;
        }
        let amend = op == PaletteOp::Amend;
        let why = if amend && group {
            Some(text::ONE_REPO)
        } else if amend {
            commit_box.amend_why()
        } else {
            commit_box.commit_why()
        };
        if let Some(why) = why {
            return Some(Command::cannot(why));
        }
        let commit_box = self.commit_box.clone();
        Some(Command::run(move |_, _, cx| {
            commit_box.update(cx, |commit_box, cx| commit_box.press(amend, cx))
        }))
    }
}
