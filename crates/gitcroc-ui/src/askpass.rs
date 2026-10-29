//! Git's prompt that it cannot answer silently (D10), a port of
//! `ui/src/AskpassDialog.tsx`: a credential, or an SSH host key question.
//! It names the job, its repos and the workspace; the prompt is shown as
//! text (SEC-3), and a password or passphrase is masked. Escape and Cancel
//! give git no answer. The answer is sent and not kept: the field and its
//! undo history are emptied as it goes, and the window drops the dialog
//! (D10, SEC-1).

use gitcroc_app::askpass::{AskpassPrompt, PromptKind};
use gpui_component::FocusTrapElement;
use gpui_component::button::{Button, ButtonVariants};
use gpui_component::input::{Input, InputContentType, InputEvent, InputState};
use gpui_kit::{
    App, AppContext, ClickEvent, Context, Entity, EventEmitter, FocusHandle, Focusable,
    InteractiveElement, IntoElement, ParentElement, Render, Role, SharedString,
    StatefulInteractiveElement, Subscription, TestSupportExt, Window, div,
};

use crate::dialog::{self, Cancel, Closed, Modal};
use crate::strings::askpass as text;

/// Sends an answer to prompt `prompt_id`; `None` gives git none.
pub type SendAnswer = Box<dyn Fn(u32, Option<String>, &mut App)>;

pub struct AskpassDialog {
    prompt: AskpassPrompt,
    title: SharedString,
    input: Entity<InputState>,
    send: SendAnswer,
    answered: bool,
    focus: FocusHandle,
    yes: FocusHandle,
    _input_events: Subscription,
}

impl EventEmitter<Closed> for AskpassDialog {}

impl AskpassDialog {
    pub fn new(
        prompt: AskpassPrompt,
        send: SendAnswer,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let masked = prompt.kind == PromptKind::Secret;
        let input = cx.new(|cx| InputState::new(window, cx).masked(masked));
        let input_events = cx.subscribe_in(&input, window, |this, _, event, window, cx| {
            if let InputEvent::PressEnter { .. } = event {
                this.answer_typed(window, cx);
            }
        });
        let title = text::title(
            &prompt.job_label,
            &prompt.repo_names.join(", "),
            &prompt.workspace_name,
        )
        .into();
        Self {
            prompt,
            title,
            input,
            send,
            answered: false,
            focus: cx.focus_handle(),
            yes: cx.focus_handle(),
            _input_events: input_events,
        }
    }

    pub fn prompt_id(&self) -> u32 {
        self.prompt.prompt_id
    }

    /// The field git's answer is typed in.
    pub fn field(&self) -> &Entity<InputState> {
        &self.input
    }

    /// Whether the field hides what is typed.
    pub fn masked(&self, cx: &App) -> bool {
        self.input.read(cx).presentation().is_masked()
    }

    /// Sends `answer` to git, once, empties the field and closes.
    fn answer(&mut self, answer: Option<String>, window: &mut Window, cx: &mut Context<Self>) {
        if self.answered {
            return;
        }
        self.answered = true;
        (self.send)(self.prompt.prompt_id, answer, cx);
        self.input
            .update(cx, |input, cx| input.set_value("", window, cx));
        cx.emit(Closed);
    }

    /// Sends what was typed.
    fn answer_typed(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let value = self.input.read(cx).value().to_string();
        self.answer(Some(value), window, cx);
    }

    fn on_ok(&mut self, _: &ClickEvent, window: &mut Window, cx: &mut Context<Self>) {
        self.answer_typed(window, cx);
    }

    fn cancel(&mut self, _: &Cancel, window: &mut Window, cx: &mut Context<Self>) {
        self.answer(None, window, cx);
    }

    fn on_cancel(&mut self, _: &ClickEvent, window: &mut Window, cx: &mut Context<Self>) {
        self.answer(None, window, cx);
    }
}

impl Modal for AskpassDialog {
    /// The field has the focus, or Yes for a question.
    fn focus_first(&self, window: &mut Window, cx: &mut App) {
        if self.prompt.kind == PromptKind::YesNo {
            window.focus(&self.yes, cx);
        } else {
            self.input.update(cx, |input, cx| input.focus(window, cx));
        }
    }
}

impl Focusable for AskpassDialog {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus.clone()
    }
}

impl Render for AskpassDialog {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let prompt: SharedString = self.prompt.text.clone().into();
        let frame = dialog::frame("askpass-frame", Role::Dialog, &self.title, cx)
            .test_support()
            .track_focus(&self.focus)
            .key_context(dialog::CONTEXT)
            .on_action(cx.listener(Self::cancel))
            .child(
                div()
                    .id("askpass-prompt")
                    .test_support()
                    .role(Role::Paragraph)
                    .aria_label(prompt.clone())
                    .child(prompt.clone()),
            );
        let frame = match self.prompt.kind {
            PromptKind::YesNo => frame.child(
                dialog::actions_row()
                    .child(
                        Button::new("askpass-yes")
                            .primary()
                            .label(text::YES)
                            .track_focus(&self.yes)
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.answer(Some("yes".to_owned()), window, cx);
                            })),
                    )
                    .child(
                        Button::new("askpass-no")
                            .label(text::NO)
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.answer(Some("no".to_owned()), window, cx);
                            })),
                    ),
            ),
            PromptKind::Text | PromptKind::Secret => {
                let field = Input::new(&self.input)
                    .id("askpass-field")
                    .aria_label(prompt);
                // A password field: screen readers do not read its text.
                let field = if self.prompt.kind == PromptKind::Secret {
                    field.content_type(InputContentType::Password)
                } else {
                    field
                };
                frame.child(field).child(
                    dialog::actions_row()
                        .child(
                            Button::new("askpass-cancel")
                                .label(text::CANCEL)
                                .on_click(cx.listener(Self::on_cancel)),
                        )
                        .child(
                            Button::new("askpass-ok")
                                .primary()
                                .label(text::OK)
                                .on_click(cx.listener(Self::on_ok)),
                        ),
                )
            }
        };
        // The trap takes the place of the frame, under the dialog's id.
        frame.focus_trap("askpass-dialog", &self.focus)
    }
}
