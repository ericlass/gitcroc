//! The group operations on a name X typed or picked once (section 5
//! "Checkout branch X", "Create branch X", "Delete branch X", "Merge X into
//! current", "Rebase current onto X" and "Create tag X" rows), a port of
//! rc.1's `ui/src/groups/GroupNameDialog.tsx`. X is picked from the group's
//! branch names (UI-5's list) or typed. The core checks it as the user types
//! (SEC-4): a branch or tag name as one, a ref to merge or rebase onto as
//! something git would not read as an option. Each repo resolves X on its
//! own (GRP-14). Running goes through the preflight like any group
//! operation, which predicts conflicts (GRP-13) and asks before a
//! destructive one (GRP-9): this dialog only picks the name and the
//! options. Names are shown as text (SEC-3).

use gitcroc_core::{
    GroupOp, MergeFastForward, MissingBranch, StartPoint, TargetSet, TreeItem, TreeNode,
    WorkspaceId,
};
use gpui_component::button::{Button, ButtonVariants};
use gpui_component::checkbox::Checkbox;
use gpui_component::input::{Input, InputEvent, InputState, Textarea, TextareaState};
use gpui_component::{Disableable, FocusTrapElement, Selectable};
use gpui_kit::{
    App, AppContext, ClickEvent, Context, Entity, EventEmitter, FocusHandle, Focusable,
    InteractiveElement, IntoElement, ParentElement, Render, Role, SharedString,
    StatefulInteractiveElement, Styled, Subscription, TestSupportExt, Window, div, px,
};

use crate::dialog::{self, Cancel, Closed, Modal};
use crate::group_sync_dialog::RunGroupOp;
use crate::services::Services;
use crate::strings::group_ops::names as text;
use crate::strings::groups::STOPPED;
use crate::theme::ActiveTokens;

/// Which operation the dialog picks X for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NameKind {
    Checkout,
    Create,
    Delete,
    Merge,
    Rebase,
    Tag,
}

impl NameKind {
    fn title(self, target: &str) -> String {
        match self {
            Self::Checkout => text::checkout_title(target),
            Self::Create => text::create_title(target),
            Self::Delete => text::delete_title(target),
            Self::Merge => text::merge_title(target),
            Self::Rebase => text::rebase_title(target),
            Self::Tag => text::tag_title(target),
        }
    }

    fn submit(self) -> &'static str {
        match self {
            Self::Checkout => text::CHECK_OUT,
            Self::Create | Self::Tag => text::CREATE,
            Self::Delete => text::DELETE,
            Self::Merge => text::MERGE,
            Self::Rebase => text::REBASE,
        }
    }

    /// Merge and rebase are one dialog, either picked.
    fn integrating(self) -> bool {
        matches!(self, Self::Merge | Self::Rebase)
    }

    /// X is picked from the group's names, not only typed.
    fn picks(self) -> bool {
        !matches!(self, Self::Create | Self::Tag)
    }
}

/// A group name dialog to open: what it runs on and the name it starts with.
#[derive(Debug, Clone)]
pub struct AskGroupName {
    pub workspace: WorkspaceId,
    pub target: TargetSet,
    /// Such as `Backend` or `3 repositories`.
    pub target_name: String,
    pub kind: NameKind,
    pub name: String,
}

/// Where a new branch starts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Start {
    Head,
    DefaultBranch,
    Revision,
}

pub struct GroupNameDialog {
    services: Services,
    target_name: String,
    kind: NameKind,
    name: Entity<InputState>,
    revision: Entity<InputState>,
    message: Entity<TextareaState>,
    /// The group's branch names; `None` while they load or for several
    /// repos, which have no Branches node.
    names: Option<Vec<String>>,
    names_error: bool,
    missing: MissingBranch,
    start: Start,
    switch: bool,
    force: bool,
    on_remote: bool,
    fast_forward: MergeFastForward,
    annotated: bool,
    push: bool,
    name_error: Option<SharedString>,
    validating: bool,
    validation: u64,
    run: RunGroupOp,
    focus: FocusHandle,
    cancel_focus: FocusHandle,
    _inputs: Vec<Subscription>,
}

impl EventEmitter<Closed> for GroupNameDialog {}

/// Every branch name under `nodes`, a group's Branches node's children.
fn names_of(nodes: &[TreeNode]) -> Vec<String> {
    nodes
        .iter()
        .flat_map(|node| {
            let own = match &node.item {
                TreeItem::GroupBranch { name, .. } => Some(name.clone()),
                _ => None,
            };
            own.into_iter()
                .chain(names_of(node.children.as_deref().unwrap_or_default()))
        })
        .collect()
}

impl GroupNameDialog {
    pub fn new(
        services: Services,
        ask: AskGroupName,
        run: RunGroupOp,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let name = cx.new(|cx| InputState::new(window, cx).default_value(ask.name.clone()));
        let revision = cx.new(|cx| InputState::new(window, cx));
        let message = cx.new(|cx| TextareaState::new(window, cx).rows(3));
        let inputs = vec![
            cx.subscribe_in(&name, window, |this, _, event, window, cx| match event {
                InputEvent::Change => this.check_name(cx),
                InputEvent::PressEnter { .. } => this.submit(window, cx),
                _ => {}
            }),
            cx.subscribe(&revision, |this, _, event, cx| {
                if let InputEvent::Change = event {
                    this.start = Start::Revision;
                    cx.notify();
                }
            }),
            cx.subscribe(&message, |_, _, _, cx| cx.notify()),
        ];
        let mut this = Self {
            services,
            target_name: ask.target_name,
            kind: ask.kind,
            name,
            revision,
            message,
            names: None,
            names_error: false,
            missing: MissingBranch::Skip,
            start: Start::Head,
            switch: true,
            force: false,
            on_remote: false,
            fast_forward: MergeFastForward::Config,
            annotated: true,
            push: false,
            name_error: None,
            validating: false,
            validation: 0,
            run,
            focus: cx.focus_handle(),
            cancel_focus: cx.focus_handle(),
            _inputs: inputs,
        };
        this.check_name(cx);
        // Only a group has a Branches node; several repos get no list.
        if let TargetSet::Group { group_id } = ask.target {
            let workspace = ask.workspace;
            let call = this
                .services
                .with_app(move |app| async move { app.group_branches(workspace, group_id).await });
            cx.spawn(async move |this, cx| {
                let answer = call.await;
                let _ = this.update(cx, |this, cx| {
                    match answer {
                        Some(Ok(nodes)) => this.names = Some(names_of(&nodes)),
                        _ => this.names_error = true,
                    }
                    cx.notify();
                });
            })
            .detach();
        }
        this
    }

    /// Asks the core whether X is a name the operation takes (SEC-4).
    fn check_name(&mut self, cx: &mut Context<Self>) {
        self.validation += 1;
        let generation = self.validation;
        self.validating = true;
        let value = self.name.read(cx).value().to_string();
        let kind = self.kind;
        let call = self.services.with_app(move |app| async move {
            match kind {
                NameKind::Tag => app.tag_name_check(&value),
                NameKind::Merge | NameKind::Rebase => app.revision_check(value.trim()),
                NameKind::Checkout | NameKind::Create | NameKind::Delete => {
                    app.branch_name_check(&value)
                }
            }
        });
        cx.spawn(async move |this, cx| {
            let answer = call.await;
            let _ = this.update(cx, |this, cx| {
                if this.validation == generation {
                    this.name_error = match answer {
                        Some(error) => error.map(Into::into),
                        None => Some(STOPPED.into()),
                    };
                    this.validating = false;
                    cx.notify();
                }
            });
        })
        .detach();
        cx.notify();
    }

    /// The operation with the picked options, or `None` while it cannot
    /// run.
    fn op(&self, cx: &App) -> Option<GroupOp> {
        if self.validating || self.name_error.is_some() {
            return None;
        }
        let name = self.name.read(cx).value().to_string();
        Some(match self.kind {
            NameKind::Checkout => GroupOp::Checkout {
                name,
                missing: self.missing,
            },
            NameKind::Create => GroupOp::CreateBranch {
                name,
                start: match self.start {
                    Start::Head => StartPoint::Head,
                    Start::DefaultBranch => StartPoint::DefaultBranch,
                    Start::Revision => {
                        let revision = self.revision.read(cx).value().trim().to_owned();
                        if revision.is_empty() {
                            return None;
                        }
                        StartPoint::Revision { revision }
                    }
                },
                switch: self.switch,
            },
            NameKind::Delete => GroupOp::DeleteBranch {
                name,
                force: self.force,
                on_remote: self.on_remote,
            },
            NameKind::Merge => GroupOp::Merge {
                name: name.trim().to_owned(),
                fast_forward: self.fast_forward,
            },
            NameKind::Rebase => GroupOp::Rebase {
                onto: name.trim().to_owned(),
            },
            NameKind::Tag => {
                let message = self.message.read(cx).value().to_string();
                if self.annotated && message.trim().is_empty() {
                    return None;
                }
                GroupOp::CreateTag {
                    name,
                    annotated: self.annotated,
                    message: if self.annotated {
                        message
                    } else {
                        String::new()
                    },
                    push: self.push,
                }
            }
        })
    }

    pub fn submit(&mut self, _: &mut Window, cx: &mut Context<Self>) {
        let Some(op) = self.op(cx) else {
            return;
        };
        (self.run)(op, cx);
        cx.emit(Closed);
    }

    fn on_submit(&mut self, _: &ClickEvent, window: &mut Window, cx: &mut Context<Self>) {
        self.submit(window, cx);
    }

    fn on_cancel(&mut self, _: &ClickEvent, _: &mut Window, cx: &mut Context<Self>) {
        cx.emit(Closed);
    }

    fn cancel(&mut self, _: &Cancel, _: &mut Window, cx: &mut Context<Self>) {
        cx.emit(Closed);
    }

    /// Puts a known name in the field and checks it.
    fn pick(&mut self, name: String, window: &mut Window, cx: &mut Context<Self>) {
        // `set_value` sends no change: the check is asked here.
        self.name
            .update(cx, |input, cx| input.set_value(name, window, cx));
        self.check_name(cx);
    }

    /// Radio-like buttons: `labels` in order, the one at `selected` chosen.
    fn choices(
        &self,
        id: &'static str,
        legend: &'static str,
        labels: &[&'static str],
        selected: usize,
        choose: fn(&mut Self, usize),
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let mut list = div()
            .id(id)
            .role(Role::RadioGroup)
            .aria_label(legend)
            .flex()
            .flex_col()
            .gap_1()
            .child(legend);
        for (at, label) in labels.iter().enumerate() {
            list = list.child(
                Button::new(gpui_kit::ElementId::Name(format!("{id}-{at}").into()))
                    .label(*label)
                    .selected(at == selected)
                    .on_click(cx.listener(move |this, _, _, cx| {
                        choose(this, at);
                        cx.notify();
                    })),
            );
        }
        list
    }

    /// The group's names that contain what is typed, to pick from.
    fn known(&self, cx: &mut Context<Self>) -> Option<impl IntoElement + use<>> {
        let names = self.names.as_ref()?;
        let typed = self.name.read(cx).value().to_string();
        let mut list = div()
            .id("group-name-known")
            .test_support()
            .role(Role::List)
            .aria_label(text::KNOWN)
            .flex()
            .flex_col()
            .gap_1()
            .max_h(px(160.))
            .overflow_y_scroll();
        for (at, known) in names.iter().enumerate() {
            if !known.contains(typed.trim()) || *known == typed {
                continue;
            }
            let picked = known.clone();
            list = list.child(
                Button::new(gpui_kit::ElementId::Name(
                    format!("group-name-known-{at}").into(),
                ))
                .label(known.clone())
                .on_click(
                    cx.listener(move |this, _, window, cx| this.pick(picked.clone(), window, cx)),
                ),
            );
        }
        Some(list)
    }

    fn flag(
        &self,
        id: &'static str,
        label: &'static str,
        checked: bool,
        set: fn(&mut Self, bool),
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        Checkbox::new(id)
            .label(label)
            .checked(checked)
            .on_click(cx.listener(move |this, value: &bool, _, cx| {
                set(this, *value);
                cx.notify();
            }))
    }
}

/// A line that says what the operation does in each repo.
fn hint(id: &'static str, line: &'static str) -> impl IntoElement {
    div()
        .id(id)
        .test_support()
        .role(Role::Paragraph)
        .aria_label(line)
        .child(line)
}

impl Modal for GroupNameDialog {
    fn focus_first(&self, window: &mut Window, cx: &mut App) {
        self.name.update(cx, |input, cx| input.focus(window, cx));
    }
}

impl Focusable for GroupNameDialog {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus.clone()
    }
}

impl Render for GroupNameDialog {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let title: SharedString = self.kind.title(&self.target_name).into();
        let mut body = div().id("group-name-body").flex().flex_col().gap_2();
        if self.kind.integrating() {
            body = body.child(self.choices(
                "group-name-operation",
                text::OPERATION,
                &[text::MERGE_IT, text::REBASE_ONTO],
                usize::from(self.kind == NameKind::Rebase),
                |this, at| {
                    this.kind = if at == 0 {
                        NameKind::Merge
                    } else {
                        NameKind::Rebase
                    }
                },
                cx,
            ));
        }
        let label = match self.kind {
            NameKind::Tag => text::TAG,
            NameKind::Merge | NameKind::Rebase => text::REF,
            _ => text::BRANCH,
        };
        body = body.child(
            div().child(label).child(
                Input::new(&self.name)
                    .id("group-name-field")
                    .aria_label(label),
            ),
        );
        if let Some(error) = &self.name_error
            && !self.name.read(cx).value().is_empty()
        {
            body = body.child(
                div()
                    .id("group-name-error")
                    .test_support()
                    .role(Role::Alert)
                    .aria_label(error.clone())
                    .text_color(cx.tokens().danger)
                    .child(error.clone()),
            );
        }
        if self.kind.picks() {
            body = body.children(self.known(cx));
        }
        match self.kind {
            NameKind::Checkout => {
                let selected = match self.missing {
                    MissingBranch::Skip => 0,
                    MissingBranch::DefaultBranch => 1,
                    MissingBranch::Create => 2,
                };
                body = body.child(self.choices(
                    "group-name-missing",
                    text::WHERE_MISSING,
                    &[
                        text::MISSING_SKIP,
                        text::MISSING_DEFAULT,
                        text::MISSING_CREATE,
                    ],
                    selected,
                    |this, at| {
                        this.missing = [
                            MissingBranch::Skip,
                            MissingBranch::DefaultBranch,
                            MissingBranch::Create,
                        ][at]
                    },
                    cx,
                ));
            }
            NameKind::Create => {
                let selected = match self.start {
                    Start::Head => 0,
                    Start::DefaultBranch => 1,
                    Start::Revision => 2,
                };
                use crate::strings::branches;
                body = body
                    .child(self.choices(
                        "group-name-start",
                        branches::START_POINT,
                        &[
                            branches::CURRENT_HEAD,
                            branches::DEFAULT_BRANCH,
                            branches::REVISION,
                        ],
                        selected,
                        |this, at| {
                            this.start = [Start::Head, Start::DefaultBranch, Start::Revision][at]
                        },
                        cx,
                    ))
                    .child(
                        Input::new(&self.revision)
                            .id("group-name-revision")
                            .aria_label(branches::REVISION_LABEL),
                    )
                    .child(hint("group-name-hint", text::CREATE_SKIPS))
                    .child(self.flag(
                        "group-name-switch",
                        branches::CHECK_OUT_NEW,
                        self.switch,
                        |this, value| this.switch = value,
                        cx,
                    ));
            }
            NameKind::Delete => {
                body = body
                    .child(self.flag(
                        "group-name-force",
                        text::FORCE,
                        self.force,
                        |this, value| this.force = value,
                        cx,
                    ))
                    .child(self.flag(
                        "group-name-on-remote",
                        text::ON_REMOTE,
                        self.on_remote,
                        |this, value| this.on_remote = value,
                        cx,
                    ));
            }
            NameKind::Merge => {
                let selected = match self.fast_forward {
                    MergeFastForward::Config => 0,
                    MergeFastForward::FfOnly => 1,
                    MergeFastForward::NoFf => 2,
                };
                body = body.child(self.choices(
                    "group-name-ff",
                    text::FAST_FORWARD,
                    &[text::FF_CONFIG, text::FF_ONLY, text::NO_FF],
                    selected,
                    |this, at| {
                        this.fast_forward = [
                            MergeFastForward::Config,
                            MergeFastForward::FfOnly,
                            MergeFastForward::NoFf,
                        ][at]
                    },
                    cx,
                ));
            }
            NameKind::Rebase => {}
            NameKind::Tag => {
                body = body.child(self.choices(
                    "group-name-tag-kind",
                    text::TAG_KIND,
                    &[text::ANNOTATED, text::LIGHTWEIGHT],
                    usize::from(!self.annotated),
                    |this, at| this.annotated = at == 0,
                    cx,
                ));
                if self.annotated {
                    body = body.child(div().child(text::MESSAGE).child(
                        crate::text_area::tab_moves_on(
                            Textarea::new(&self.message).aria_label(text::MESSAGE),
                        ),
                    ));
                }
                body = body
                    .child(hint("group-name-hint", text::TAG_SKIPS))
                    .child(self.flag(
                        "group-name-push",
                        text::PUSH_TAG,
                        self.push,
                        |this, value| this.push = value,
                        cx,
                    ));
            }
        }
        if self.kind.integrating() {
            body = body.child(hint("group-name-predicts", text::PREDICTS_CONFLICTS));
        }
        if self.names_error {
            body = body.child(
                div()
                    .id("group-name-names-error")
                    .test_support()
                    .role(Role::Alert)
                    .aria_label(text::CANNOT_READ_BRANCHES)
                    .child(text::CANNOT_READ_BRANCHES),
            );
        }
        let ready = self.op(cx).is_some();
        dialog::frame("group-name-frame", Role::Dialog, &title, cx)
            .test_support()
            .track_focus(&self.focus)
            .key_context(dialog::CONTEXT)
            .on_action(cx.listener(Self::cancel))
            .child(body)
            .child(
                dialog::actions_row()
                    .child(
                        Button::new("group-name-cancel")
                            .label(crate::strings::common::CANCEL)
                            .track_focus(&self.cancel_focus)
                            .on_click(cx.listener(Self::on_cancel)),
                    )
                    .child(
                        Button::new("group-name-submit")
                            .label(self.kind.submit())
                            .primary()
                            .disabled(!ready)
                            .on_click(cx.listener(Self::on_submit)),
                    ),
            )
            // The trap takes the place of the frame, under the dialog's id.
            .focus_trap("group-name", &self.focus)
    }
}
