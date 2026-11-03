//! The conflict banner of OPS-2 above a single repo's history (UI-10), a
//! port of rc.1's `ConflictBanner.tsx`. It
//! names the operation the repo's fresh summary shows, so a conflict
//! started in a terminal looks like one started in the app (OPS-1), with
//! what the core reads of it: what is applied onto what, and a rebase's or
//! mailbox's step. Abort and Continue ask the window, which sends them
//! through M6-18's operation dialog, where the core's confirmation of a
//! stash-apply Abort and its refusals appear (GRP-9). With AI features on
//! (AI-16) it has Resolve all with AI (AI-3), Cancel while the run goes,
//! and under it the files of the repo's last run and where each is
//! (AI-13), from the workspace's [`ResolveAll`]; a file's name opens it in
//! the conflict editor. Names, paths and reasons are text only (SEC-3).
use std::collections::HashMap;

use gitcroc_core::{ConflictDetails, IntegrateOp, Operation, RepoId, SummaryView, WorkspaceId};
use gpui_component::button::{Button, ButtonVariants};
use gpui_component::{Disableable, Sizable};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::{
    AnyElement, Context, Entity, InteractiveElement, IntoElement, ParentElement, Role,
    SharedString, StatefulInteractiveElement, Styled, TestSupportExt, div,
};

use crate::history_pane::HistoryPane;
use crate::resolve_all::{ListIds, OpenFile, ResolveAll};
use crate::strings::{conflict as conflict_text, integrate as text};
use crate::theme::ActiveTokens;

/// The banner's list of a run's files.
const FILE_IDS: ListIds = ListIds {
    list: "conflict-ai-files",
    row: "conflict-ai-file",
    file: "conflict-ai-file-name",
    state: "conflict-ai-file-state",
};

/// Abort or Continue was pressed: the window sends `op` for `repo`.
#[derive(Debug, Clone)]
pub struct AskIntegrate {
    pub workspace: WorkspaceId,
    pub repo: RepoId,
    pub repo_name: String,
    pub op: IntegrateOp,
}

/// Why Continue is disabled for `operation` in `view`'s repo; a stash
/// apply has none to continue.
pub fn continue_why(operation: Operation, view: &SummaryView) -> Option<&'static str> {
    let unmerged = view
        .summary
        .as_ref()
        .map_or(0, |summary| summary.state.unmerged_paths);
    if operation == Operation::StashApply {
        Some(text::STASH_APPLY_ENDS)
    } else if view.busy {
        Some(text::BUSY)
    } else if unmerged > 0 {
        Some(text::RESOLVE_FIRST)
    } else {
        None
    }
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
        resolve_all: &Entity<ResolveAll>,
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
        let continue_why = continue_why(operation, view);
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
        let ai = AiPart::of(repo, resolve_all, cx);
        Some(
            div()
                .id("conflict-banner")
                .test_support()
                .role(Role::Region)
                .aria_label(text::CONFLICT_STATE)
                .flex_shrink_0()
                .flex()
                .flex_col()
                .gap_1()
                .px_2()
                .py_1()
                .bg(tokens.warning_bg)
                .border_b_1()
                .border_color(tokens.border)
                .child(
                    div()
                        .flex()
                        .flex_row()
                        .flex_wrap()
                        .items_center()
                        .gap_2()
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
                                .children(error)
                                .children(ai.as_ref().and_then(|ai| ai.notes(cx))),
                        )
                        .children(ai.as_ref().map(AiPart::button))
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
                        ),
                )
                .children(ai.and_then(|ai| ai.progress(cx)))
                .into_any_element(),
        )
    }
}

/// The banner's AI part for one repo, while AI features are on (AI-16).
struct AiPart {
    repo: RepoId,
    runs: Entity<ResolveAll>,
    /// AI for this workspace (AI-6).
    workspace_on: bool,
    busy: bool,
    refusal: Option<SharedString>,
}

impl AiPart {
    fn of(repo: &RepoId, runs: &Entity<ResolveAll>, cx: &Context<HistoryPane>) -> Option<Self> {
        let read = runs.read(cx);
        if !read.enabled(cx) {
            return None;
        }
        let run = read.run(repo);
        Some(Self {
            repo: repo.clone(),
            runs: runs.clone(),
            workspace_on: read.workspace_on(cx),
            busy: run.is_some_and(crate::resolve_all::RepoRun::busy),
            refusal: run
                .and_then(|run| run.refusal())
                .map(|refusal| refusal.to_owned().into()),
        })
    }

    /// Why Resolve all cannot be pressed, or why it was refused.
    fn notes(&self, cx: &Context<HistoryPane>) -> Option<gpui_kit::Div> {
        let tokens = *cx.tokens();
        if !self.workspace_on {
            return Some(
                div().child(
                    div()
                        .id("conflict-banner-ai-reason")
                        .test_support()
                        .role(Role::Status)
                        .aria_label(conflict_text::AI_OFF_HERE)
                        .text_sm()
                        .text_color(tokens.text_secondary)
                        .child(conflict_text::AI_OFF_HERE),
                ),
            );
        }
        self.refusal.clone().map(|refusal| {
            div().child(
                div()
                    .id("conflict-banner-ai-refusal")
                    .test_support()
                    .role(Role::Alert)
                    .aria_label(refusal.clone())
                    .text_sm()
                    .text_color(tokens.danger)
                    .child(refusal),
            )
        })
    }

    /// Resolve all with AI, or Cancel while the run goes (AI-3, AI-13).
    fn button(&self) -> AnyElement {
        let (runs, repo) = (self.runs.clone(), self.repo.clone());
        if self.busy {
            return Button::new("conflict-banner-cancel-ai")
                .small()
                .label(text::CANCEL_ALL)
                .tooltip(text::CANCEL_ALL_TITLE)
                .on_click(move |_, _, cx| runs.update(cx, |runs, cx| runs.cancel(&repo, cx)))
                .into_any_element();
        }
        Button::new("conflict-banner-resolve-ai")
            .small()
            .label(text::RESOLVE_ALL)
            .tooltip(if self.workspace_on {
                text::RESOLVE_ALL_TITLE
            } else {
                text::AI_OFF_TITLE
            })
            .disabled(!self.workspace_on)
            .on_click(move |_, _, cx| runs.update(cx, |runs, cx| runs.ask(&repo, cx)))
            .into_any_element()
    }

    /// The files of the repo's last run and where each is (AI-13).
    fn progress(&self, cx: &mut Context<HistoryPane>) -> Option<AnyElement> {
        let tokens = *cx.tokens();
        let job = self.runs.read(cx).run(&self.repo)?.view()?.clone();
        let (runs, repo) = (self.runs.clone(), self.repo.clone());
        let open: OpenFile = std::rc::Rc::new(move |path, _, cx| {
            runs.update(cx, |runs, cx| runs.open(&repo, path, cx));
        });
        let label: SharedString = job.label.clone().into();
        let state: SharedString = crate::jobs::state(&job).into();
        Some(
            div()
                .id("conflict-ai-run")
                .test_support()
                .role(Role::Region)
                .aria_label(text::RESOLVE_ALL)
                .flex()
                .flex_col()
                .gap_1()
                .child(
                    div()
                        .flex()
                        .flex_row()
                        .flex_wrap()
                        .gap_2()
                        .child(
                            div()
                                .id("conflict-ai-run-label")
                                .test_support()
                                .role(Role::Label)
                                .aria_label(label.clone())
                                .font_weight(gpui_kit::FontWeight::SEMIBOLD)
                                .child(label),
                        )
                        .child(
                            div()
                                .id("conflict-ai-run-state")
                                .test_support()
                                .role(Role::Status)
                                .aria_label(state.clone())
                                .text_color(tokens.text_secondary)
                                .child(state),
                        ),
                )
                .child(crate::resolve_all::file_list(
                    FILE_IDS,
                    &job,
                    Some(open),
                    &tokens,
                    cx,
                ))
                .into_any_element(),
        )
    }
}
