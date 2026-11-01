//! The conflict banner of OPS-2 above a single repo's history (UI-10), a
//! port of rc.1's `ConflictBanner.tsx` without its AI parts (M6-23). It
//! names the operation the repo's fresh summary shows, so a conflict
//! started in a terminal looks like one started in the app (OPS-1), with
//! what the core reads of it: what is applied onto what, and a rebase's or
//! mailbox's step. Abort and Continue ask the window, which sends them
//! through M6-18's operation dialog, where the core's confirmation of a
//! stash-apply Abort and its refusals appear (GRP-9). Names are text only
//! (SEC-3).
use std::collections::HashMap;

use gitcroc_core::{ConflictDetails, IntegrateOp, Operation, RepoId, SummaryView, WorkspaceId};
use gpui_component::button::{Button, ButtonVariants};
use gpui_component::{Disableable, Sizable};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::{
    AnyElement, Context, InteractiveElement, IntoElement, ParentElement, Role, SharedString,
    StatefulInteractiveElement, Styled, TestSupportExt, div,
};

use crate::history_pane::HistoryPane;
use crate::strings::integrate as text;
use crate::theme::ActiveTokens;

/// Abort or Continue was pressed: the window sends `op` for `repo`.
#[derive(Debug, Clone)]
pub struct AskIntegrate {
    pub workspace: WorkspaceId,
    pub repo: RepoId,
    pub repo_name: String,
    pub op: IntegrateOp,
}

/// What the banner read of each repo, per workspace (ARCH-9).
#[derive(Default)]
pub struct Banner {
    details: HashMap<RepoId, ConflictDetails>,
    errors: HashMap<RepoId, String>,
    /// The latest read of each repo; older answers are dropped.
    reads: HashMap<RepoId, u64>,
}

impl Banner {
    pub fn details(&self, repo: &RepoId) -> Option<&ConflictDetails> {
        self.details.get(repo)
    }

    /// A read of `repo`'s details starts (OPS-2); its number, which only
    /// the latest read's answer may show.
    pub fn start_read(&mut self, repo: &RepoId) -> u64 {
        let read = self.reads.entry(repo.clone()).or_default();
        *read = read.wrapping_add(1);
        *read
    }

    /// The answer to read `read` of `repo`; an older read's is dropped, and
    /// a failure keeps the banner with its error. `false` when dropped.
    pub fn answer(
        &mut self,
        repo: RepoId,
        read: u64,
        result: Result<ConflictDetails, String>,
    ) -> bool {
        if self.reads.get(&repo) != Some(&read) {
            return false;
        }
        match result {
            Ok(details) => {
                self.errors.remove(&repo);
                self.details.insert(repo, details);
            }
            Err(error) => {
                self.errors.insert(repo, error);
            }
        }
        true
    }

    /// The banner of `repo`, from its fresh summary; `None` outside
    /// conflict state.
    pub fn render(
        &self,
        workspace: &WorkspaceId,
        repo: &RepoId,
        repo_name: &str,
        view: &SummaryView,
        cx: &mut Context<HistoryPane>,
    ) -> Option<AnyElement> {
        let state = view.summary.as_ref()?.state;
        let operation = state.operation?;
        let tokens = *cx.tokens();
        let unmerged = state.unmerged_paths;
        let unknown = ConflictDetails {
            operation: None,
            applying: None,
            onto: None,
            step: None,
            unmerged_paths: unmerged,
        };
        let details = self.details.get(repo).unwrap_or(&unknown);
        let line: SharedString = text::in_progress(operation, details).into();
        let count: SharedString = if unmerged == 0 {
            text::ALL_RESOLVED.into()
        } else if operation == Operation::StashApply {
            text::mark_resolved(unmerged).into()
        } else {
            text::resolve_and_stage(unmerged).into()
        };
        // Continue's reason when disabled; a stash apply has none to give.
        let continue_why = if operation == Operation::StashApply {
            Some(text::STASH_APPLY_ENDS)
        } else if view.busy {
            Some(text::BUSY)
        } else if unmerged > 0 {
            Some(text::RESOLVE_FIRST)
        } else {
            None
        };
        let abort_title = if operation == Operation::StashApply {
            text::ABORT_STASH_APPLY.to_owned()
        } else {
            text::abort_title(operation)
        };
        let error = self.errors.get(repo).map(|error| {
            let error: SharedString = text::cannot_read(error).into();
            div()
                .id("conflict-banner-error")
                .test_support()
                .role(Role::Alert)
                .aria_label(error.clone())
                .text_color(tokens.danger)
                .child(error)
        });
        let ask = |op: IntegrateOp| AskIntegrate {
            workspace: workspace.clone(),
            repo: repo.clone(),
            repo_name: repo_name.to_owned(),
            op,
        };
        let (abort, proceed) = (ask(IntegrateOp::Abort), ask(IntegrateOp::Continue));
        Some(
            div()
                .id("conflict-banner")
                .test_support()
                .role(Role::Region)
                .aria_label(text::CONFLICT_STATE)
                .flex_shrink_0()
                .flex()
                .flex_row()
                .flex_wrap()
                .items_center()
                .gap_2()
                .px_2()
                .py_1()
                .bg(tokens.warning_bg)
                .border_b_1()
                .border_color(tokens.border)
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .flex()
                        .flex_col()
                        .child(
                            div()
                                .id("conflict-banner-text")
                                .test_support()
                                .role(Role::Heading)
                                .aria_level(2)
                                .aria_label(line.clone())
                                .font_weight(gpui_kit::FontWeight::SEMIBOLD)
                                .child(line),
                        )
                        .child(
                            div()
                                .id("conflict-banner-unmerged")
                                .test_support()
                                .role(Role::Label)
                                .aria_label(count.clone())
                                .child(count),
                        )
                        .children(error),
                )
                .child(
                    Button::new("conflict-banner-abort")
                        .small()
                        .label(text::ABORT)
                        .tooltip(abort_title)
                        .disabled(view.busy)
                        .on_click(cx.listener(move |_, _, _, cx| cx.emit(abort.clone()))),
                )
                .child(
                    Button::new("conflict-banner-continue")
                        .small()
                        .primary()
                        .label(text::CONTINUE)
                        .when_some(continue_why, |button, why| button.tooltip(why))
                        .disabled(continue_why.is_some())
                        .on_click(cx.listener(move |_, _, _, cx| cx.emit(proceed.clone()))),
                )
                .into_any_element(),
        )
    }
}
