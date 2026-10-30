//! The commit message box under the file lists of "Uncommitted changes"
//! (UI-15), a port of rc.1's `changes/CommitBox.tsx`: a subject line, a
//! body, then Amend and Commit. Ctrl or Cmd and Enter commits from either
//! field. Each disabled button says why. The unsent message is kept per
//! repo of the workspace while the app runs, so it survives switching repos
//! and tabs (UI-29); [`Drafts`] keeps it. The core composes the message and
//! refuses what cannot be committed (ARCH-6); this forwards input and shows
//! the answer.

use std::rc::Rc;

use gitcroc_app::app::AppEvent;
use gitcroc_core::{CommitMessage, CommitRequest, WorkspaceId};
use gpui_component::Disableable;
use gpui_component::button::{Button, ButtonVariants};
use gpui_component::input::{Input, InputEvent, InputState, Textarea, TextareaState};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::{
    App, AppContext, Context, Entity, EventEmitter, InteractiveElement, IntoElement, KeyBinding,
    KeyBindingContextPredicate, ParentElement, Render, Role, SharedString,
    StatefulInteractiveElement, Styled, Subscription, TestSupportExt, Window, actions, div, px,
};

use crate::changes::Committable;
use crate::commit::{Drafts, Press};
use crate::services::Services;
use crate::strings::changes as text;
use crate::theme::ActiveTokens;

actions!(
    commit_box,
    [
        /// Commits from either field (Ctrl or Cmd and Enter).
        CommitNow,
    ]
);

/// The key context of the box.
const CONTEXT: &str = "GitcrocCommitBox";

/// The body's height, about two lines.
const BODY_HEIGHT: f32 = 56.;

/// Ctrl or Cmd and Enter in either field commits. The binding is the
/// fields', in the box, so it comes before theirs, which would add a line.
pub fn bind(cx: &mut App) {
    let keys = if cx.platform_info().is_mac() {
        "cmd-enter"
    } else {
        "ctrl-enter"
    };
    let context = KeyBindingContextPredicate::parse(&format!("{CONTEXT} > Input"))
        .ok()
        .map(Rc::new);
    if let Ok(binding) = KeyBinding::load(keys, Box::new(CommitNow), context, false, None, cx) {
        cx.bind_keys([binding]);
    }
}

/// The commit of the box was overtaken by a newer read of the lists: the
/// pane reads them again.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ReadAgain;

/// The commit box of one workspace.
pub struct CommitBox {
    services: Services,
    workspace_id: WorkspaceId,
    /// The repo whose changes are shown, while they can be committed.
    target: Option<Committable>,
    drafts: Drafts,
    subject: Entity<InputState>,
    body: Entity<TextareaState>,
    /// The fields are to show the draft of the shown repo again, as after
    /// switching repos or a commit that cleared it.
    sync: bool,
    _subscriptions: Vec<Subscription>,
}

impl EventEmitter<ReadAgain> for CommitBox {}

impl CommitBox {
    pub fn new(
        services: Services,
        workspace_id: WorkspaceId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let subject = cx.new(|cx| InputState::new(window, cx).placeholder(text::SUBJECT));
        let body = cx.new(|cx| {
            TextareaState::new(window, cx)
                .rows(2)
                .placeholder(text::BODY_PLACEHOLDER)
        });
        let subscriptions = vec![
            cx.subscribe(&subject, |this, _, event: &InputEvent, cx| {
                if let InputEvent::Change = event {
                    this.edited(cx);
                }
            }),
            cx.subscribe(&body, |this, _, event: &InputEvent, cx| {
                if let InputEvent::Change = event {
                    this.edited(cx);
                }
            }),
        ];
        Self {
            services,
            workspace_id,
            target: None,
            drafts: Drafts::default(),
            subject,
            body,
            sync: false,
            _subscriptions: subscriptions,
        }
    }

    /// The repo whose changes are shown, or none; the fields show its
    /// draft.
    pub fn set_target(&mut self, target: Option<Committable>, cx: &mut Context<Self>) {
        let repo_changed = self.target.as_ref().map(|target| &target.repo)
            != target.as_ref().map(|target| &target.repo);
        if repo_changed {
            self.sync = true;
        }
        if self.target != target {
            self.target = target;
            cx.notify();
        }
    }

    pub fn target(&self) -> Option<&Committable> {
        self.target.as_ref()
    }

    /// The unsent message of the repo shown.
    pub fn draft(&self) -> Option<CommitMessage> {
        Some(self.drafts.draft(&self.target.as_ref()?.repo))
    }

    /// Why the last commit of the repo shown was refused or failed.
    pub fn error(&self) -> Option<&str> {
        self.drafts.error(&self.target.as_ref()?.repo)
    }

    /// Follows the end of the box's commit job.
    pub fn on_event(&mut self, event: &AppEvent, cx: &mut Context<Self>) {
        if let AppEvent::JobEnded(job) = event
            && job.workspace_id == self.workspace_id
            && self.drafts.job_ended(job)
        {
            self.sync = true;
            cx.notify();
        }
    }

    /// The user typed in a field: the shown repo's draft is what the fields
    /// hold.
    fn edited(&mut self, cx: &mut Context<Self>) {
        let Some(target) = &self.target else {
            return;
        };
        let message = CommitMessage {
            subject: self.subject.read(cx).value().to_string(),
            body: self.body.read(cx).value().to_string(),
        };
        if self.drafts.draft(&target.repo) != message {
            self.drafts.edit(&target.repo.clone(), message);
            cx.notify();
        }
    }

    /// Why Commit is disabled: what is missing first.
    pub fn commit_why(&self) -> Option<&'static str> {
        let target = self.target.as_ref()?;
        if target.in_conflict {
            Some(text::IN_CONFLICT)
        } else if self.drafts.busy(&target.repo) {
            Some(text::COMMIT_RUNNING)
        } else if !target.staged {
            Some(text::NOTHING_STAGED)
        } else if self.drafts.draft(&target.repo).subject.trim().is_empty() {
            Some(text::NO_SUBJECT)
        } else {
            None
        }
    }

    /// Why Amend is disabled.
    pub fn amend_why(&self) -> Option<&'static str> {
        let target = self.target.as_ref()?;
        if target.in_conflict {
            Some(text::IN_CONFLICT)
        } else if self.drafts.busy(&target.repo) {
            Some(text::COMMIT_RUNNING)
        } else if target.last_message.is_none() {
            Some(text::NOTHING_TO_AMEND)
        } else {
            None
        }
    }

    /// Commit (`amend` false) or Amend, as its button does when enabled.
    pub fn press(&mut self, amend: bool, cx: &mut Context<Self>) {
        let why = if amend {
            self.amend_why()
        } else {
            self.commit_why()
        };
        let Some(target) = self.target.clone().filter(|_| why.is_none()) else {
            return;
        };
        let message = match self
            .drafts
            .press(&target.repo, amend, target.last_message.as_ref())
        {
            Press::Nothing => return,
            Press::Filled => {
                self.sync = true;
                cx.notify();
                return;
            }
            Press::Send(message) => message,
        };
        cx.notify();
        let request = CommitRequest {
            handle: target.handle,
            message,
            amend,
        };
        let workspace = self.workspace_id.clone();
        let call = self
            .services
            .with_app(move |app| async move { app.changes_commit(workspace, request).await });
        let repo = target.repo;
        cx.spawn(async move |this, cx| {
            let result = call.await;
            let _ = this.update(cx, |this, cx| {
                let answer = match result {
                    Some(Ok(Some(job))) => Ok(job),
                    // A newer read of the lists took over; the core ran
                    // nothing. Say so rather than drop the click; the
                    // message stays.
                    Some(Ok(None)) => {
                        cx.emit(ReadAgain);
                        Err(text::LIST_CHANGED.to_owned())
                    }
                    Some(Err(error)) => Err(error.message),
                    // The app is going.
                    None => return,
                };
                if this.drafts.answered(&repo, answer) {
                    this.sync = true;
                }
                cx.notify();
            });
        })
        .detach();
    }

    fn commit_now(&mut self, _: &CommitNow, _: &mut Window, cx: &mut Context<Self>) {
        self.press(false, cx);
    }

    /// Puts the shown repo's draft in the fields.
    fn sync_fields(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.sync {
            return;
        }
        self.sync = false;
        let draft = self.draft().unwrap_or_default();
        self.subject
            .update(cx, |field, cx| field.set_value(draft.subject, window, cx));
        self.body
            .update(cx, |field, cx| field.set_value(draft.body, window, cx));
    }
}

impl Render for CommitBox {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.sync_fields(window, cx);
        let tokens = *cx.tokens();
        let root = div()
            .id("commit-box")
            .test_support()
            .key_context(CONTEXT)
            .on_action(cx.listener(Self::commit_now))
            .role(Role::Group)
            .aria_label(text::COMMIT_MESSAGE)
            .flex_shrink_0()
            .flex()
            .flex_col()
            .gap_1()
            .p_2()
            .border_t_1()
            .border_color(tokens.border);
        let Some(target) = self.target.clone() else {
            return root;
        };
        let commit_why = self.commit_why();
        let amend_why = self.amend_why();
        let busy = self.drafts.busy(&target.repo);
        let error = self.drafts.error(&target.repo).map(|error| {
            let error = SharedString::from(error.to_owned());
            div()
                .id("commit-error")
                .test_support()
                .role(Role::Alert)
                .aria_label(error.clone())
                .text_color(tokens.danger)
                .child(error)
        });
        let why = commit_why.map(|why| {
            div()
                .id("commit-why")
                .test_support()
                .role(Role::Label)
                .aria_label(why)
                .text_color(tokens.text_secondary)
                .child(why)
        });
        root.child(
            Input::new(&self.subject)
                .id("commit-subject")
                .aria_label(text::SUBJECT),
        )
        .child(
            div().id("commit-body").test_support().child(
                Textarea::new(&self.body)
                    .h(px(BODY_HEIGHT))
                    .aria_label(text::BODY),
            ),
        )
        .children(error)
        .child(
            div()
                .flex()
                .flex_row()
                .items_center()
                .gap_2()
                .child(div().flex_1().min_w_0().children(why))
                .child(
                    Button::new("commit-amend")
                        .label(text::AMEND)
                        .tooltip(amend_why.unwrap_or(text::AMEND_TITLE))
                        .disabled(amend_why.is_some())
                        .on_click(cx.listener(|this, _, _, cx| this.press(true, cx))),
                )
                .child(
                    Button::new("commit-commit")
                        .primary()
                        .label(if busy { text::COMMITTING } else { text::COMMIT })
                        .when_some(commit_why, |button, why| button.tooltip(why))
                        .disabled(commit_why.is_some())
                        .on_click(cx.listener(|this, _, _, cx| this.press(false, cx))),
                ),
        )
    }
}
