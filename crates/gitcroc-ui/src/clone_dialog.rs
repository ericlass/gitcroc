//! The clone dialog (section 6 "Repos"), a port of rc.1's
//! `ui/src/clone/CloneDialog.tsx` and `ui/src/stores/clone.ts`: the URL,
//! the parent folder picked with the folder dialog the core opens (SEC-2),
//! and the folder name, which follows the core's default for the URL until
//! the user edits it. The core checks the target as the user types: a
//! folder that exists and is not empty disables Clone and says why. Add to
//! group picks none, a group of the workspace, or a new group whose name
//! the core checks when Clone is pressed (GRP-2); a refusal shows inside
//! the dialog, which stays open. A started job closes it: its progress is
//! in the status bar, and the tree selects the repo it cloned. Paths and
//! names are text (SEC-3); the only path made from typed text is the
//! request's `parent` and `name`, which the core checks (PLAT-2).

use std::path::{MAIN_SEPARATOR, PathBuf};

use gitcroc_app::app::AppEvent;
use gitcroc_core::{CloneGroup, CloneRequest, GroupView, WorkspaceId};
use gpui_component::button::{Button, ButtonVariants};
use gpui_component::input::{Input, InputEvent, InputState};
use gpui_component::{Disableable, FocusTrapElement, Selectable};
use gpui_kit::{
    App, AppContext, ClickEvent, Context, ElementId, Entity, EventEmitter, FocusHandle, Focusable,
    InteractiveElement, IntoElement, ParentElement, Render, Role, SharedString,
    StatefulInteractiveElement, Styled, Subscription, Task, TestSupportExt, Window, div,
};
use tokio::sync::broadcast::error::RecvError;

use crate::dialog::{self, Cancel, Closed, Modal};
use crate::services::Services;
use crate::strings::{self, clone as text};
use crate::theme::ActiveTokens;

/// What Add to group is set to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum GroupChoice {
    None,
    /// The group at this index of the workspace's groups.
    Existing(usize),
    New,
}

/// The core's verdict on a target: the parent and name it checked, and
/// why it refuses them.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Verdict {
    parent: PathBuf,
    name: String,
    error: Option<String>,
}

pub struct CloneDialog {
    services: Services,
    workspace: WorkspaceId,
    groups: Vec<GroupView>,
    url: Entity<InputState>,
    name: Entity<InputState>,
    group_name: Entity<InputState>,
    parent: Option<PathBuf>,
    /// The user typed a folder name: the URL no longer sets it.
    edited: bool,
    verdict: Option<Verdict>,
    group: GroupChoice,
    error: Option<SharedString>,
    sending: bool,
    focus: FocusHandle,
    _subscriptions: Vec<Subscription>,
    _events: Task<()>,
}

impl EventEmitter<Closed> for CloneDialog {}

impl CloneDialog {
    /// The dialog for `workspace`, whose groups are `groups`, starting with
    /// the parent folder last picked there.
    pub fn new(
        services: Services,
        workspace: WorkspaceId,
        groups: Vec<GroupView>,
        parent: Option<PathBuf>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let url = cx.new(|cx| InputState::new(window, cx));
        let name = cx.new(|cx| InputState::new(window, cx));
        let group_name = cx.new(|cx| InputState::new(window, cx));
        let subscriptions = vec![
            cx.subscribe_in(&url, window, |this, _, event, window, cx| match event {
                InputEvent::Change => this.url_changed(window, cx),
                InputEvent::PressEnter { .. } => this.submit(cx),
                InputEvent::Focus | InputEvent::Blur => {}
            }),
            cx.subscribe_in(&name, window, |this, _, event, _, cx| match event {
                // `set_value` sends no change: this is the user's typing.
                InputEvent::Change => {
                    this.edited = true;
                    this.error = None;
                    this.check_target(cx);
                }
                InputEvent::PressEnter { .. } => this.submit(cx),
                InputEvent::Focus | InputEvent::Blur => {}
            }),
            cx.subscribe_in(&group_name, window, |this, _, event, _, cx| match event {
                InputEvent::Change => {
                    this.error = None;
                    cx.notify();
                }
                InputEvent::PressEnter { .. } => this.submit(cx),
                InputEvent::Focus | InputEvent::Blur => {}
            }),
        ];
        // The folder picked arrives as an event (SEC-2).
        let mut events = services.app().subscribe();
        let listening = cx.spawn(async move |this, cx| {
            loop {
                match events.recv().await {
                    Ok(AppEvent::CloneParentPicked {
                        workspace_id,
                        path,
                        key: None,
                    }) => {
                        let picked = this.update(cx, |this, cx| {
                            if workspace_id == this.workspace {
                                this.parent = Some(path);
                                this.check_target(cx);
                            }
                        });
                        if picked.is_err() {
                            break;
                        }
                    }
                    Ok(_) | Err(RecvError::Lagged(_)) => {}
                    Err(RecvError::Closed) => break,
                }
            }
        });
        Self {
            services,
            workspace,
            groups,
            url,
            name,
            group_name,
            parent,
            edited: false,
            verdict: None,
            group: GroupChoice::None,
            error: None,
            sending: false,
            focus: cx.focus_handle(),
            _subscriptions: subscriptions,
            _events: listening,
        }
    }

    /// The folder the clone goes into, as the dialog has it.
    pub fn parent(&self) -> Option<&PathBuf> {
        self.parent.as_ref()
    }

    /// Asks the core for the URL's default folder name, unless the user
    /// typed one; only the answer for the URL still typed counts.
    fn url_changed(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.error = None;
        cx.notify();
        if self.edited {
            return;
        }
        let asked = self.url.read(cx).value().to_string();
        let sent = asked.clone();
        let call = self
            .services
            .with_app(|app| async move { app.clone_default_name(&sent) });
        cx.spawn_in(window, async move |this, cx| {
            let Some(named) = call.await else {
                return;
            };
            let _ = this.update_in(cx, |this, window, cx| {
                if this.edited || this.url.read(cx).value() != asked {
                    return;
                }
                let named = named.unwrap_or_default();
                this.name
                    .update(cx, |name, cx| name.set_value(named, window, cx));
                this.check_target(cx);
            });
        })
        .detach();
    }

    /// The folder name as typed.
    fn name(&self, cx: &App) -> String {
        self.name.read(cx).value().to_string()
    }

    /// Asks the core about the target as it is now; only the answer for
    /// the target still shown counts.
    fn check_target(&mut self, cx: &mut Context<Self>) {
        cx.notify();
        let name = self.name(cx);
        let Some(parent) = self.parent.clone().filter(|_| !name.is_empty()) else {
            return;
        };
        let (asked_parent, asked_name) = (parent.clone(), name.clone());
        let call = self.services.with_app(|app| async move {
            app.clone_target_check(asked_parent, asked_name, false)
                .await
        });
        cx.spawn(async move |this, cx| {
            let error = call.await.unwrap_or_else(|| Some(text::STOPPED.to_owned()));
            let _ = this.update(cx, |this, cx| {
                if this.parent.as_ref() == Some(&parent) && this.name(cx) == name {
                    this.verdict = Some(Verdict {
                        parent,
                        name,
                        error,
                    });
                    cx.notify();
                }
            });
        })
        .detach();
    }

    /// The core's verdict on the target shown, once it answered.
    fn verdict(&self, cx: &App) -> Option<&Verdict> {
        let name = self.name(cx);
        self.verdict
            .as_ref()
            .filter(|verdict| Some(&verdict.parent) == self.parent.as_ref() && verdict.name == name)
    }

    /// The request Clone sends, when the dialog is ready to send it.
    fn request(&self, cx: &App) -> Option<CloneRequest> {
        let url = self.url.read(cx).value().to_string();
        if self.sending || url.trim().is_empty() {
            return None;
        }
        let parent = self.parent.clone()?;
        let verdict = self.verdict(cx)?;
        if verdict.error.is_some() {
            return None;
        }
        let group = match self.group {
            GroupChoice::None => None,
            GroupChoice::Existing(at) => Some(CloneGroup::Existing {
                group_id: self.groups.get(at)?.id.clone(),
            }),
            GroupChoice::New => {
                let name = self.group_name.read(cx).value().to_string();
                if name.trim().is_empty() {
                    return None;
                }
                Some(CloneGroup::New { name })
            }
        };
        Some(CloneRequest {
            url,
            parent,
            name: verdict.name.clone(),
            repo: None,
            group,
        })
    }

    /// Starts the clone; started, the dialog closes.
    fn submit(&mut self, cx: &mut Context<Self>) {
        let Some(request) = self.request(cx) else {
            return;
        };
        self.sending = true;
        self.error = None;
        cx.notify();
        let workspace = self.workspace.clone();
        let call = self
            .services
            .with_app(|app| async move { app.clone_start(workspace, request).await });
        cx.spawn(async move |this, cx| {
            let result = call.await;
            let _ = this.update(cx, |this, cx| {
                this.sending = false;
                match result {
                    Some(Ok(_)) => cx.emit(Closed),
                    Some(Err(error)) => this.error = Some(error.message.into()),
                    None => this.error = Some(text::STOPPED.into()),
                }
                cx.notify();
            });
        })
        .detach();
    }

    /// Asks the core for the folder dialog; the pick arrives as an event.
    fn choose(&mut self, _: &ClickEvent, _: &mut Window, cx: &mut Context<Self>) {
        self.error = None;
        cx.notify();
        let workspace = self.workspace.clone();
        let call = self
            .services
            .with_app(|app| async move { app.clone_pick_parent(workspace, None) });
        cx.spawn(async move |this, cx| {
            if let Some(Err(error)) = call.await {
                let _ = this.update(cx, |this, cx| {
                    this.error = Some(error.message.into());
                    cx.notify();
                });
            }
        })
        .detach();
    }

    fn pick_group(&mut self, group: GroupChoice, window: &mut Window, cx: &mut Context<Self>) {
        self.group = group;
        self.error = None;
        if group == GroupChoice::New {
            self.group_name
                .update(cx, |input, cx| input.focus(window, cx));
        }
        cx.notify();
    }

    fn on_submit(&mut self, _: &ClickEvent, _: &mut Window, cx: &mut Context<Self>) {
        self.submit(cx);
    }

    fn cancel(&mut self, _: &Cancel, _: &mut Window, cx: &mut Context<Self>) {
        cx.emit(Closed);
    }

    fn on_cancel(&mut self, _: &ClickEvent, _: &mut Window, cx: &mut Context<Self>) {
        cx.emit(Closed);
    }

    /// A field with its label above it.
    fn field(label: &'static str, input: &Entity<InputState>, id: &'static str) -> gpui_kit::Div {
        div()
            .flex()
            .flex_col()
            .gap_1()
            .child(label)
            .child(Input::new(input).id(id).aria_label(label))
    }
}

impl Modal for CloneDialog {
    /// The URL has the focus.
    fn focus_first(&self, window: &mut Window, cx: &mut App) {
        self.url.update(cx, |input, cx| input.focus(window, cx));
    }
}

impl Focusable for CloneDialog {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus.clone()
    }
}

impl Render for CloneDialog {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let title: SharedString = text::TITLE.into();
        let danger = cx.tokens().danger;
        let name = self.name(cx);
        let target_error = self
            .verdict(cx)
            .and_then(|verdict| verdict.error.clone())
            .filter(|_| !name.is_empty())
            .map(|error| {
                let error: SharedString = error.into();
                div()
                    .id("clone-target-error")
                    .test_support()
                    .role(Role::Alert)
                    .aria_label(error.clone())
                    .text_color(danger)
                    .child(error)
            });
        let parent: SharedString = self.parent.as_ref().map_or_else(
            || text::NO_FOLDER.into(),
            |parent| parent.display().to_string().into(),
        );
        // For display only: the core joins the request's parent and name.
        let target = self
            .parent
            .as_ref()
            .filter(|_| !name.is_empty())
            .map(|parent| {
                let shown: SharedString =
                    text::clones_into(&format!("{}{MAIN_SEPARATOR}{name}", parent.display()))
                        .into();
                div()
                    .id("clone-target")
                    .test_support()
                    .role(Role::Paragraph)
                    .aria_label(shown.clone())
                    .child(shown)
            });
        let mut groups = div()
            .id("clone-groups")
            .flex()
            .flex_row()
            .flex_wrap()
            .gap_1()
            .child(
                Button::new("clone-group-none")
                    .label(text::NO_GROUP)
                    .selected(self.group == GroupChoice::None)
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.pick_group(GroupChoice::None, window, cx)
                    })),
            );
        for (at, group) in self.groups.iter().enumerate() {
            groups = groups.child(
                Button::new(ElementId::NamedInteger("clone-group".into(), at as u64))
                    .label(group.name.clone())
                    .selected(self.group == GroupChoice::Existing(at))
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.pick_group(GroupChoice::Existing(at), window, cx)
                    })),
            );
        }
        groups =
            groups.child(
                Button::new("clone-group-new")
                    .label(text::NEW_GROUP)
                    .selected(self.group == GroupChoice::New)
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.pick_group(GroupChoice::New, window, cx)
                    })),
            );
        let group_name = (self.group == GroupChoice::New)
            .then(|| Self::field(text::GROUP_NAME, &self.group_name, "clone-group-name"));
        let ready = self.request(cx).is_some();
        dialog::frame("clone-frame", Role::Dialog, &title, cx)
            .test_support()
            .track_focus(&self.focus)
            .key_context(dialog::CONTEXT)
            .on_action(cx.listener(Self::cancel))
            .child(Self::field(text::URL, &self.url, "clone-url"))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap_1()
                    .child(text::INTO_FOLDER)
                    .child(
                        div()
                            .flex()
                            .flex_row()
                            .items_center()
                            .gap_2()
                            .child(
                                div()
                                    .id("clone-parent")
                                    .test_support()
                                    .role(Role::Label)
                                    .aria_label(parent.clone())
                                    .flex_1()
                                    .min_w_0()
                                    .truncate()
                                    .child(parent),
                            )
                            .child(
                                Button::new("clone-choose")
                                    .label(text::CHOOSE)
                                    .accessibility_label(text::CHOOSE_FOLDER)
                                    .on_click(cx.listener(Self::choose)),
                            ),
                    ),
            )
            .child(Self::field(text::FOLDER_NAME, &self.name, "clone-name"))
            .children(target_error)
            .children(target)
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap_1()
                    .child(text::ADD_TO_GROUP)
                    .child(groups),
            )
            .children(group_name)
            .children(dialog::refusal(&self.error, cx))
            .child(
                dialog::actions_row()
                    .child(
                        Button::new("dialog-cancel")
                            .label(strings::common::CANCEL)
                            .on_click(cx.listener(Self::on_cancel)),
                    )
                    .child(
                        Button::new("dialog-submit")
                            .primary()
                            .label(text::CLONE)
                            .disabled(!ready)
                            .on_click(cx.listener(Self::on_submit)),
                    ),
            )
            // The trap takes the place of the frame, under the dialog's id.
            .focus_trap("clone-dialog", &self.focus)
    }
}
