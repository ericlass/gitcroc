//! The commit message box under the file lists of "Uncommitted changes"
//! (UI-15), a port of rc.1's `changes/CommitBox.tsx`: a subject line, a
//! body, then Amend and Commit. Ctrl or Cmd and Enter commits from either
//! field. Each disabled button says why. The unsent message is kept per
//! repo of the workspace while the app runs, so it survives switching repos
//! and tabs (UI-29); [`Drafts`] keeps it. The core composes the message and
//! refuses what cannot be committed (ARCH-6); this forwards input and shows
//! the answer. For a group or several repos (UI-34) the box keeps one
//! message per target set, has the group Commit's options above Commit,
//! names how many repos it commits and has no Amend; Commit asks the
//! window to run the group Commit through the preflight (GRP-15).

use std::rc::Rc;

use gitcroc_app::app::AppEvent;
use gitcroc_core::{CommitMessage, CommitRequest, GroupOp, JobId, WorkspaceId};
use gpui_component::Disableable;
use gpui_component::button::{Button, ButtonVariants};
use gpui_component::checkbox::Checkbox;
use gpui_component::input::{Input, InputEvent, InputState, Textarea, TextareaState};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::{
    App, AppContext, Context, Entity, EventEmitter, InteractiveElement, IntoElement, KeyBinding,
    KeyBindingContextPredicate, ParentElement, Render, Role, SharedString,
    StatefulInteractiveElement, Styled, Subscription, TestSupportExt, Window, actions, div, px,
    relative,
};

use crate::changes::{Committable, GroupCommittable};
use crate::commit::{DraftKey, Drafts, GroupDrafts, GroupOptions, Press};
use crate::group_ops::RunGroup;
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

/// The body's least height, about one line.
const BODY_MIN_HEIGHT: f32 = 28.;

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
    /// The group or several repos whose changes are shown (UI-34).
    group: Option<GroupCommittable>,
    group_drafts: GroupDrafts,
    subject: Entity<InputState>,
    body: Entity<TextareaState>,
    /// The fields are to show the draft of the shown repo again, as after
    /// switching repos or a commit that cleared it.
    sync: bool,
    _subscriptions: Vec<Subscription>,
}

impl EventEmitter<ReadAgain> for CommitBox {}
impl EventEmitter<RunGroup> for CommitBox {}

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
            group: None,
            group_drafts: GroupDrafts::default(),
            subject,
            body,
            sync: false,
            _subscriptions: subscriptions,
        }
    }

    /// The repo, or the group or several repos, whose changes are shown,
    /// or none; the fields show its draft.
    pub fn set_target(
        &mut self,
        target: Option<Committable>,
        group: Option<GroupCommittable>,
        cx: &mut Context<Self>,
    ) {
        let repo_changed = self.target.as_ref().map(|target| &target.repo)
            != target.as_ref().map(|target| &target.repo);
        let key = |group: &Option<GroupCommittable>| {
            group.as_ref().map(|group| DraftKey::of(&group.target))
        };
        if repo_changed || key(&self.group) != key(&group) {
            self.sync = true;
        }
        if self.target != target || self.group != group {
            self.target = target;
            self.group = group;
            cx.notify();
        }
    }

    /// The key of the group or several repos shown.
    fn group_key(&self) -> Option<DraftKey> {
        Some(DraftKey::of(&self.group.as_ref()?.target))
    }

    /// The options of the group commit shown.
    pub fn group_options(&self) -> Option<GroupOptions> {
        Some(self.group_drafts.options(&self.group_key()?))
    }

    fn set_group_options(
        &mut self,
        change: impl FnOnce(&mut GroupOptions),
        cx: &mut Context<Self>,
    ) {
        let Some(key) = self.group_key() else {
            return;
        };
        let mut options = self.group_drafts.options(&key);
        change(&mut options);
        self.group_drafts.set_options(&key, options);
        cx.notify();
    }

    /// How many repos Commit commits, for a group or several repos.
    pub fn group_repos(&self) -> Option<usize> {
        let group = self.group.as_ref()?;
        Some(group.repos(self.group_options()?.stage_all))
    }

    pub fn target(&self) -> Option<&Committable> {
        self.target.as_ref()
    }

    /// The unsent message of the repo, or the target set, shown.
    pub fn draft(&self) -> Option<CommitMessage> {
        if let Some(key) = self.group_key() {
            return Some(self.group_drafts.draft(&key));
        }
        Some(self.drafts.draft(&self.target.as_ref()?.repo))
    }

    /// Why the last commit of the repo shown was refused or failed, or
    /// that a repo of the target set shown did not commit.
    pub fn error(&self) -> Option<&str> {
        if let Some(key) = self.group_key() {
            return self.group_drafts.error(&key);
        }
        self.drafts.error(&self.target.as_ref()?.repo)
    }

    /// Follows the end of the box's commit job.
    pub fn on_event(&mut self, event: &AppEvent, cx: &mut Context<Self>) {
        if let AppEvent::JobEnded(job) = event
            && job.workspace_id == self.workspace_id
        {
            let single = self.drafts.job_ended(job);
            if self.group_drafts.job_ended(job) || single {
                self.sync = true;
                cx.notify();
            }
        }
    }

    /// The group commit the box sent started as `job`.
    pub fn group_started(&mut self, job: JobId, cx: &mut Context<Self>) {
        if self.group_drafts.started(job) {
            self.sync = true;
        }
        cx.notify();
    }

    /// The user typed in a field: the shown repo's draft is what the fields
    /// hold.
    fn edited(&mut self, cx: &mut Context<Self>) {
        let message = CommitMessage {
            subject: self.subject.read(cx).value().to_string(),
            body: self.body.read(cx).value().to_string(),
        };
        if let Some(key) = self.group_key() {
            if self.group_drafts.draft(&key) != message {
                self.group_drafts.edit(&key, message);
                cx.notify();
            }
            return;
        }
        let Some(target) = &self.target else {
            return;
        };
        if self.drafts.draft(&target.repo) != message {
            self.drafts.edit(&target.repo.clone(), message);
            cx.notify();
        }
    }

    /// Why Commit is disabled: what is missing first.
    pub fn commit_why(&self) -> Option<&'static str> {
        if let Some(key) = self.group_key() {
            return if self.group_drafts.busy(&key) {
                Some(text::GROUP_COMMIT_RUNNING)
            } else if self.group_repos() == Some(0) {
                Some(text::NOTHING_STAGED)
            } else if self.group_drafts.draft(&key).subject.trim().is_empty() {
                Some(text::NO_SUBJECT)
            } else {
                None
            };
        }
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
        if self.group.is_some() {
            if !amend {
                self.press_group(cx);
            }
            return;
        }
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

    /// Commit of a group or several repos: the window runs the group
    /// Commit through the preflight, which always asks first (GRP-15).
    fn press_group(&mut self, cx: &mut Context<Self>) {
        let (Some(group), Some(key)) = (self.group.clone(), self.group_key()) else {
            return;
        };
        if self.commit_why().is_some() {
            return;
        }
        let options = self.group_drafts.options(&key);
        let Some(message) = self.group_drafts.press(&key) else {
            return;
        };
        cx.notify();
        cx.emit(RunGroup {
            workspace: self.workspace_id.clone(),
            target: group.target,
            op: GroupOp::Commit {
                message,
                stage_all: options.stage_all,
                push: options.push,
            },
        });
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
            .size_full()
            .flex()
            .flex_col()
            .gap_1()
            .p_2();
        let group = self.group_key();
        let busy = match (&group, &self.target) {
            (Some(key), _) => self.group_drafts.busy(key),
            (None, Some(target)) => self.drafts.busy(&target.repo),
            (None, None) => return root,
        };
        let commit_why = self.commit_why();
        let amend_why = self.amend_why();
        let options = self.group_options().map(|options| {
            div()
                .id("commit-options")
                .test_support()
                .role(Role::Group)
                .aria_label(text::GROUP_COMMIT_OPTIONS)
                .flex()
                .flex_row()
                .flex_wrap()
                .gap_3()
                .child(
                    Checkbox::new("commit-stage-all")
                        .label(text::STAGE_ALL_FIRST)
                        .checked(options.stage_all)
                        .on_click(cx.listener(|this, value: &bool, _, cx| {
                            let value = *value;
                            this.set_group_options(|options| options.stage_all = value, cx);
                        })),
                )
                .child(
                    Checkbox::new("commit-push")
                        .label(text::PUSH_AFTER)
                        .checked(options.push)
                        .on_click(cx.listener(|this, value: &bool, _, cx| {
                            let value = *value;
                            this.set_group_options(|options| options.push = value, cx);
                        })),
                )
        });
        let label: SharedString = match (busy, self.group_repos()) {
            (true, _) => text::COMMITTING.into(),
            (false, Some(repos)) => text::commit_repos(repos).into(),
            (false, None) => text::COMMIT.into(),
        };
        let amend = group.is_none().then(|| {
            Button::new("commit-amend")
                .label(text::AMEND)
                .tooltip(amend_why.unwrap_or(text::AMEND_TITLE))
                .disabled(amend_why.is_some())
                .on_click(cx.listener(|this, _, _, cx| this.press(true, cx)))
        });
        let error = self.error().map(|error| {
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
        root.children(options)
            .child(
                Input::new(&self.subject)
                    .id("commit-subject")
                    .aria_label(text::SUBJECT),
            )
            // The body takes the height the splitter above the box gives it.
            .child(
                div()
                    .id("commit-body")
                    .test_support()
                    .flex_1()
                    .min_h(px(BODY_MIN_HEIGHT))
                    .child(
                        Textarea::new(&self.body)
                            .h(relative(1.))
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
                    .children(amend)
                    .child(
                        Button::new("commit-commit")
                            .primary()
                            .label(label)
                            .when_some(commit_why, |button, why| button.tooltip(why))
                            .disabled(commit_why.is_some())
                            .on_click(cx.listener(|this, _, _, cx| this.press(false, cx))),
                    ),
            )
    }
}
