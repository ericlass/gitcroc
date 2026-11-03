//! Resolve with AI in the conflict editor's header (AI-3), a port of the AI
//! parts of rc.1's `ConflictView.tsx`, `ConflictEditor.tsx` and
//! `stores/conflicts.ts`. The core decides everything (ARCH-6):
//! `ai_eligibility` gives the shown file's reason up front (AI-4),
//! `ai_resolve` refuses or starts the job for the hunks still untouched in
//! the buffer (AI-5), and `ai_proposal_get` reads the result after
//! `AiProposalReady`, which goes to the file's kept editor and its cards
//! (AI-11). Each file's AI state is kept with its editor, so it survives
//! switching files, repos and tabs and goes with it (OPS-9, UI-29); a job
//! runs on in a background tab (UI-30). With AI features off nothing of it
//! shows and every proposal goes (AI-16).

use gitcroc_app::app::AppEvent;
use gitcroc_core::{
    AiEligibility, AiEligibilityRead, AiProposalGet, AiProposalReady, AiRefused, AiResolveRequest,
    AiResolveStart, ConsentRequest, FileProposal, JobId, JobView, Outcome,
};
use gpui_component::button::Button;
use gpui_component::{Disableable, Sizable};
use gpui_kit::{
    AnyElement, App, Context, Div, Entity, InteractiveElement, IntoElement, ParentElement, Role,
    SharedString, StatefulInteractiveElement, Styled, TestSupportExt, div,
};

use super::ChangesPane;
use crate::conflict::EditorKey;
use crate::strings::conflict as text;
use crate::theme::ActiveTokens;

/// What resolving with AI asks of the window (AI-3, AI-6).
#[derive(Debug, Clone)]
pub enum AskAi {
    /// No provider is the default: open Settings at its AI tab.
    Settings,
    /// The consent dialog for `request`; after Allow the file `key` is
    /// asked for again.
    Consent {
        request: ConsentRequest,
        key: EditorKey,
    },
    /// The consent dialog for `request`; after Allow Resolve all with AI
    /// for `repo` is sent again (AI-13).
    ConsentAll {
        request: ConsentRequest,
        repo: gitcroc_core::RepoId,
    },
}

/// One unmerged file's AI state, kept with its editor (AI-3, AI-11).
#[derive(Default)]
pub struct FileAi {
    /// Whether the file may go to AI (AI-4), and the exclude list it was
    /// read for; `None` until read.
    eligibility: Option<(AiEligibility, Vec<String>)>,
    /// An eligibility read is on its way.
    checking: bool,
    /// `ai_resolve` is on its way.
    starting: bool,
    /// The proposal job running for the file, with its provider's name.
    job: Option<(JobId, String)>,
    /// The core kept a result: read it while the file can be named.
    ready: bool,
    /// `ai_proposal_get` is on its way.
    reading: bool,
    proposal: Option<FileProposal>,
    /// "AI failed", with the reason (AI-9).
    failed: Option<String>,
    /// Why the last request was refused or did not go out.
    refusal: Option<String>,
}

impl FileAi {
    /// A file just read: a result kept before it was opened, as Resolve
    /// all keeps them, is read once it is shown.
    pub fn opened() -> Self {
        Self {
            ready: true,
            ..Self::default()
        }
    }
}

/// A refusal the button shows beside it, in words.
pub(super) fn refusal_text(refused: &AiRefused) -> String {
    match refused {
        AiRefused::AiOff => text::refused::AI_OFF.to_owned(),
        AiRefused::AiDisabled => text::AI_OFF_HERE.to_owned(),
        AiRefused::NoProvider => text::refused::NO_PROVIDER.to_owned(),
        AiRefused::ProviderUnusable { reason } => text::refused::provider_unusable(reason),
        AiRefused::Ineligible { message, .. } => text::not_sent_to_ai(message),
        AiRefused::ConsentNeeded { .. } => text::refused::CONSENT_NEEDED.to_owned(),
    }
}

/// The reason of a failed job's message, without the prefix the header
/// shows itself.
fn failure(message: Option<&str>) -> String {
    let message = message.unwrap_or(text::JOB_FAILED);
    message
        .strip_prefix("AI failed: ")
        .unwrap_or(message)
        .to_owned()
}

impl ChangesPane {
    /// AI features on, and AI for this workspace (AI-6, AI-16).
    fn workspace_ai(&self, cx: &App) -> bool {
        self.workspaces
            .read(cx)
            .view(&self.workspace_id)
            .is_none_or(|view| view.ai)
    }

    /// AI features changed: turned off, every proposal goes from the
    /// editors and nothing of AI is kept; the buffers stay (AI-16).
    pub(super) fn ai_changed(&mut self, cx: &mut Context<Self>) {
        if !self.ai.read(cx).enabled() {
            for file in self.conflicts.files.values_mut() {
                file.ai = FileAi::default();
                if let Some(editor) = &file.editor {
                    editor.update(cx, |editor, cx| editor.set_proposal(None, cx));
                }
            }
        }
        cx.notify();
    }

    /// Resolve with AI for the hunks of `key` still untouched (AI-3,
    /// AI-5): the job's result arrives as `AiProposalReady`.
    pub fn resolve_with_ai(&mut self, key: &EditorKey, cx: &mut Context<Self>) {
        if !self.ai.read(cx).enabled() {
            return;
        }
        let Some(file) = self.conflicts.files.get(key) else {
            return;
        };
        if file.ai.starting || file.ai.job.is_some() {
            return;
        }
        let Some(editor) = file.editor.clone() else {
            return;
        };
        let hunks = editor.read(cx).untouched_hunks(cx);
        if hunks.is_empty() {
            return;
        }
        let Some((handle, id)) = self.conflict_target(key) else {
            self.reload(cx);
            return;
        };
        if let Some(file) = self.conflicts.files.get_mut(key) {
            file.ai.starting = true;
            file.ai.refusal = None;
        }
        let workspace = self.workspace_id.clone();
        let request = AiResolveRequest {
            handle,
            file: id,
            hunks: Some(hunks),
        };
        let call = self
            .services
            .with_app(move |app| async move { app.ai_resolve(workspace, request).await });
        let key = key.clone();
        cx.spawn(async move |this, cx| {
            let result = call.await;
            let _ = this.update(cx, |this, cx| {
                this.started(&key, result, cx);
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }

    /// What `ai_resolve` answered for `key`.
    fn started(
        &mut self,
        key: &EditorKey,
        result: Option<Result<Option<AiResolveStart>, gitcroc_app::app::AppError>>,
        cx: &mut Context<Self>,
    ) {
        let provider = self.ai.read(cx).default_name();
        let exclude = self.exclude(cx);
        let ended = |job_id: JobId, this: &Self| {
            this.conflicts
                .ended
                .iter()
                .find(|(job, _, _)| *job == job_id)
                .map(|(_, outcome, message)| (outcome.clone(), message.clone()))
        };
        let refusal = match result {
            Some(Ok(Some(AiResolveStart::Started { job_id }))) => {
                let early = ended(job_id, self);
                if let Some(file) = self.conflicts.files.get_mut(key) {
                    file.ai.starting = false;
                    file.ai.failed = None;
                    match early {
                        // It ended before its start was answered.
                        Some((outcome, message)) => {
                            if matches!(outcome, Some(Outcome::Failed)) {
                                file.ai.failed = Some(failure(message.as_deref()));
                            }
                        }
                        None => file.ai.job = Some((job_id, provider)),
                    }
                }
                None
            }
            Some(Ok(Some(AiResolveStart::AlreadyResolved))) => {
                self.conflicts.forget(key);
                self.reload(cx);
                return;
            }
            Some(Ok(Some(AiResolveStart::Refused { refused }))) => match refused {
                AiRefused::NoProvider => {
                    cx.emit(AskAi::Settings);
                    None
                }
                AiRefused::ConsentNeeded { consent } => {
                    cx.emit(AskAi::Consent {
                        request: consent,
                        key: key.clone(),
                    });
                    None
                }
                AiRefused::Ineligible { reason, message } => {
                    if let Some(file) = self.conflicts.files.get_mut(key) {
                        file.ai.eligibility =
                            Some((AiEligibility::Ineligible { reason, message }, exclude));
                    }
                    None
                }
                refused => Some(refusal_text(&refused)),
            },
            // A newer view: read the lists.
            Some(Ok(None)) => {
                self.reload(cx);
                None
            }
            Some(Err(error)) => Some(error.to_string()),
            None => Some(super::conflicts::stopped()),
        };
        if let Some(file) = self.conflicts.files.get_mut(key) {
            file.ai.starting = false;
            if refusal.is_some() {
                file.ai.refusal = refusal;
            }
        }
    }

    /// Cancels the proposal job of `key`; its end arrives as `JobEnded`.
    fn cancel_ai(&mut self, key: &EditorKey, cx: &mut Context<Self>) {
        let Some((job, _)) = self.conflicts.get(key).and_then(|file| file.ai.job.clone()) else {
            return;
        };
        drop(
            self.services
                .with_app(move |app| async move { app.job_cancel(job) }),
        );
        cx.notify();
    }

    fn exclude(&self, cx: &App) -> Vec<String> {
        self.ai
            .read(cx)
            .view()
            .map(|view| view.exclude.clone())
            .unwrap_or_default()
    }

    /// The AI events: a result kept for a file, and the end of its job.
    pub(super) fn ai_event(&mut self, event: &AppEvent, cx: &mut Context<Self>) {
        let AppEvent::AiProposalReady(AiProposalReady {
            workspace_id,
            repo_id,
            path,
            ..
        }) = event
        else {
            return;
        };
        if workspace_id != &self.workspace_id {
            return;
        }
        let key = EditorKey {
            workspace: workspace_id.clone(),
            repo: repo_id.clone(),
            path: path.clone(),
        };
        if let Some(file) = self.conflicts.files.get_mut(&key) {
            file.ai.ready = true;
            file.ai.job = None;
            self.read_proposal(&key, cx);
            cx.notify();
        }
    }

    /// A job ended: a proposal job's failure is said by its file (AI-9).
    pub(super) fn ai_job_ended(&mut self, job: &JobView) {
        for file in self.conflicts.files.values_mut() {
            if file
                .ai
                .job
                .as_ref()
                .is_some_and(|(id, _)| *id == job.job_id)
            {
                file.ai.job = None;
                if matches!(job.outcome, Some(Outcome::Failed)) {
                    file.ai.failed = Some(failure(job.message.as_deref()));
                }
            }
        }
    }

    /// Reads the result the core kept for `key`, while its file can be
    /// named in the view shown; otherwise once it is shown.
    fn read_proposal(&mut self, key: &EditorKey, cx: &mut Context<Self>) {
        if !self.ai.read(cx).enabled() {
            return;
        }
        let Some(file) = self.conflicts.files.get(key) else {
            return;
        };
        if !file.ai.ready || file.ai.reading || file.editor.is_none() {
            return;
        }
        let Some((handle, id)) = self.conflict_target(key) else {
            return;
        };
        if let Some(file) = self.conflicts.files.get_mut(key) {
            file.ai.ready = false;
            file.ai.reading = true;
        }
        let workspace = self.workspace_id.clone();
        let call = self
            .services
            .with_app(move |app| async move { app.ai_proposal_get(workspace, handle, id).await });
        let key = key.clone();
        cx.spawn(async move |this, cx| {
            let result = call.await;
            let _ = this.update(cx, |this, cx| {
                let enabled = this.ai.read(cx).enabled();
                let Some(file) = this.conflicts.files.get_mut(&key) else {
                    return;
                };
                file.ai.reading = false;
                let supply = match result {
                    _ if !enabled => None,
                    Some(Ok(Some(AiProposalGet::Proposal { proposal }))) => {
                        file.ai.failed = None;
                        file.ai.proposal = Some(proposal.clone());
                        Some(Some(proposal))
                    }
                    Some(Ok(Some(AiProposalGet::Failed { reason }))) => {
                        file.ai.failed = Some(reason);
                        file.ai.proposal = None;
                        Some(None)
                    }
                    Some(Ok(Some(AiProposalGet::NoProposal))) => None,
                    Some(Ok(Some(AiProposalGet::AlreadyResolved))) => {
                        this.conflicts.forget(&key);
                        this.reload(cx);
                        None
                    }
                    // A newer view: the next shows it.
                    Some(Ok(None)) => {
                        file.ai.ready = true;
                        None
                    }
                    Some(Err(error)) => {
                        file.ai.refusal = Some(text::cannot_read_proposal(&error.to_string()));
                        None
                    }
                    None => None,
                };
                if let Some(proposal) = supply
                    && let Some(editor) = this
                        .conflicts
                        .files
                        .get(&key)
                        .and_then(|file| file.editor.clone())
                {
                    editor.update(cx, |editor, cx| editor.set_proposal(proposal, cx));
                }
                cx.notify();
            });
        })
        .detach();
    }

    /// Asks what the shown file needs: its eligibility for the exclude
    /// list as it is now (AI-4), and a result kept for it.
    pub(super) fn sync_ai(&mut self, key: &EditorKey, cx: &mut Context<Self>) {
        if !self.ai.read(cx).enabled() {
            return;
        }
        self.read_proposal(key, cx);
        let exclude = self.exclude(cx);
        let Some(file) = self.conflicts.files.get(key) else {
            return;
        };
        if file.editor.is_none()
            || file.ai.checking
            || file
                .ai
                .eligibility
                .as_ref()
                .is_some_and(|(_, read_for)| read_for == &exclude)
        {
            return;
        }
        let Some((handle, id)) = self.conflict_target(key) else {
            return;
        };
        if let Some(file) = self.conflicts.files.get_mut(key) {
            file.ai.checking = true;
        }
        let workspace = self.workspace_id.clone();
        let call = self
            .services
            .with_app(move |app| async move { app.ai_eligibility(workspace, handle, id).await });
        let key = key.clone();
        cx.spawn(async move |this, cx| {
            let result = call.await;
            let _ = this.update(cx, |this, cx| {
                let Some(file) = this.conflicts.files.get_mut(&key) else {
                    return;
                };
                file.ai.checking = false;
                match result {
                    Some(Ok(Some(AiEligibilityRead::Read { eligibility }))) => {
                        file.ai.eligibility = Some((eligibility, exclude));
                    }
                    Some(Ok(Some(AiEligibilityRead::AlreadyResolved))) => {
                        this.conflicts.forget(&key);
                        this.reload(cx);
                    }
                    // A failed or stale read leaves the check to
                    // `ai_resolve`, which says why.
                    _ => file.ai.eligibility = Some((AiEligibility::Eligible, exclude)),
                }
                cx.notify();
            });
        })
        .detach();
    }

    /// The AI part of the editor's header: Resolve with AI or Cancel, why
    /// it cannot be pressed, AI failed with Try again, Accept all
    /// high-confidence and the token usage (AI-3, AI-4, AI-9, AI-11,
    /// AI-14). Nothing while AI features are off (AI-16).
    /// Why the file cannot go to AI: AI is off for the workspace, or the
    /// file is not eligible (AI-4, AI-6).
    fn ai_blocked(&self, ai: &FileAi, cx: &App) -> Option<String> {
        if !self.workspace_ai(cx) {
            Some(text::AI_OFF_HERE.to_owned())
        } else {
            match &ai.eligibility {
                Some((AiEligibility::Ineligible { message, .. }, _)) => {
                    Some(text::not_sent_to_ai(message))
                }
                _ => None,
            }
        }
    }

    /// Resolve with AI and Accept all high-confidence, as the editor's
    /// buttons, for the palette (UI-22); none while AI features are off
    /// (AI-16).
    pub(super) fn ai_command(
        &self,
        pane: &Entity<Self>,
        key: &EditorKey,
        editor: &Entity<crate::conflict::ConflictEditor>,
        op: gitcroc_core::PaletteOp,
        cx: &App,
    ) -> Option<crate::palette::Command> {
        use crate::palette::Command;
        if !self.ai.read(cx).enabled() {
            return None;
        }
        let ai = &self.conflicts.get(key)?.ai;
        if op == gitcroc_core::PaletteOp::AcceptHighConfidence {
            if ai.proposal.is_none() || editor.read(cx).high_confidence(cx) == 0 {
                return Some(Command::cannot(crate::strings::palette::CANNOT_APPLY));
            }
            let editor = editor.clone();
            return Some(Command::run(move |_, window, cx| {
                editor.update(cx, |editor, cx| {
                    editor.accept_all_high(window, cx);
                })
            }));
        }
        let blocked = self.ai_blocked(ai, cx);
        let untouched = !editor.read(cx).untouched_hunks(cx).is_empty();
        if let Some(why) = blocked {
            return Some(Command::cannot(why));
        }
        if ai.job.is_some() || ai.starting || !untouched {
            return Some(Command::cannot(crate::strings::palette::CANNOT_APPLY));
        }
        let (pane, key) = (pane.clone(), key.clone());
        Some(Command::run(move |_, _, cx| {
            pane.update(cx, |pane, cx| pane.resolve_with_ai(&key, cx))
        }))
    }

    pub(super) fn ai_header(&self, key: &EditorKey, cx: &mut Context<Self>) -> Option<Div> {
        if !self.ai.read(cx).enabled() {
            return None;
        }
        let file = self.conflicts.get(key)?;
        let editor = file.editor.clone()?;
        let ai = &file.ai;
        let tokens = *cx.tokens();
        let (untouched, high) = {
            let editor = editor.read(cx);
            (
                !editor.untouched_hunks(cx).is_empty(),
                editor.high_confidence(cx),
            )
        };
        let blocked = self.ai_blocked(ai, cx);
        let can_resolve = blocked.is_none() && !ai.starting && untouched;
        let note = |id: &'static str, role: Role, said: String| {
            let said: SharedString = said.into();
            div()
                .id(id)
                .test_support()
                .role(role)
                .aria_label(said.clone())
                .text_sm()
                .child(said)
        };
        let button: AnyElement = match &ai.job {
            Some((_, provider)) => {
                let cancelled = key.clone();
                Button::new("conflict-cancel-ai")
                    .small()
                    .label(text::cancel_ai(provider))
                    .on_click(cx.listener(move |this, _, _, cx| this.cancel_ai(&cancelled, cx)))
                    .into_any_element()
            }
            None => {
                let asked = key.clone();
                Button::new("conflict-resolve-ai")
                    .small()
                    .label(text::RESOLVE_WITH_AI)
                    .tooltip(text::RESOLVE_WITH_AI_TITLE)
                    .disabled(!can_resolve)
                    .on_click(cx.listener(move |this, _, _, cx| this.resolve_with_ai(&asked, cx)))
                    .into_any_element()
            }
        };
        let reason = blocked.or_else(|| ai.refusal.clone()).map(|reason| {
            note("conflict-ai-reason", Role::Status, reason).text_color(tokens.text_secondary)
        });
        let failed = ai.failed.as_ref().map(|reason| {
            let retried = key.clone();
            div()
                .min_w_0()
                .flex()
                .flex_row()
                .flex_wrap()
                .items_center()
                .gap_2()
                .child(
                    note("conflict-ai-failed", Role::Alert, text::ai_failed(reason))
                        .min_w_0()
                        .text_color(tokens.danger),
                )
                .children(ai.job.is_none().then(|| {
                    Button::new("conflict-ai-retry")
                        .small()
                        .label(text::TRY_AGAIN)
                        .disabled(!can_resolve)
                        .on_click(
                            cx.listener(move |this, _, _, cx| this.resolve_with_ai(&retried, cx)),
                        )
                }))
        });
        let proposal = ai.proposal.as_ref().map(|proposal| {
            let usage: Vec<String> = proposal
                .usage
                .iter()
                .enumerate()
                .filter_map(|(at, usage)| {
                    let usage = usage.as_ref()?;
                    Some(text::request(
                        at + 1,
                        &text::tokens(usage.input_tokens, usage.output_tokens),
                    ))
                })
                .collect();
            let accepting = editor.clone();
            div()
                .flex()
                .flex_row()
                .flex_wrap()
                .items_center()
                .gap_2()
                .child(
                    Button::new("conflict-accept-high")
                        .small()
                        .label(text::ACCEPT_ALL_HIGH)
                        .tooltip(text::ACCEPT_ALL_HIGH_TITLE)
                        .disabled(high == 0)
                        .on_click(move |_, window, cx| {
                            accepting.update(cx, |editor, cx| {
                                editor.accept_all_high(window, cx);
                            });
                        }),
                )
                .children((!usage.is_empty()).then(|| {
                    div()
                        .id("conflict-ai-usage")
                        .test_support()
                        .role(Role::List)
                        .aria_label(text::TOKEN_USAGE)
                        .flex()
                        .flex_row()
                        .gap_2()
                        .text_sm()
                        .text_color(tokens.text_secondary)
                        .children(usage.into_iter().enumerate().map(|(at, line)| {
                            let line: SharedString = line.into();
                            div()
                                .id(("conflict-ai-usage-line", at))
                                .test_support()
                                .role(Role::ListItem)
                                .aria_label(line.clone())
                                .child(line)
                        }))
                }))
        });
        Some(
            div()
                .flex_1()
                .min_w_0()
                .flex()
                .flex_row()
                .flex_wrap()
                .items_center()
                .gap_2()
                .child(button)
                .children(reason)
                .children(failed)
                .children(proposal),
        )
    }
}
