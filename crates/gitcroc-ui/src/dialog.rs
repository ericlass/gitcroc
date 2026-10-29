//! The dialogs every area shares, ports of `ui/src/dialogs/parts.tsx`: a
//! name to give, and a question that cannot be undone. A dialog is modal:
//! the window draws it over everything, with its title as its name; the
//! focus stays inside it and Escape closes it. What the dialog asks for is
//! sent by its owner; the core's refusal shows inside it, and it closes
//! when the request succeeds.

use std::rc::Rc;

use gpui_component::button::{Button, ButtonVariants};
use gpui_component::input::{Input, InputEvent, InputState};
use gpui_component::{Disableable, FocusTrapElement};
use gpui_kit::{
    App, AppContext, ClickEvent, Context, Div, Entity, EventEmitter, FocusHandle, Focusable,
    InteractiveElement, IntoElement, KeyBinding, KeyBindingContextPredicate, ParentElement, Render,
    Role, SharedString, Stateful, StatefulInteractiveElement, Styled, Subscription, Task,
    TestSupportExt, Window, actions, div, px,
};

use crate::strings;
use crate::theme::ActiveTokens;

actions!(
    dialog,
    [
        /// Closes the dialog without doing anything.
        Cancel,
    ]
);

/// The key context of an open dialog.
pub(crate) const CONTEXT: &str = "GitcrocDialog";

/// The keys of an open dialog.
pub fn bind(cx: &mut App) {
    let context = KeyBindingContextPredicate::parse(CONTEXT).ok().map(Rc::new);
    if let Ok(binding) = KeyBinding::load("escape", Box::new(Cancel), context, false, None, cx) {
        cx.bind_keys([binding]);
    }
}

/// A dialog's end: it was answered or cancelled.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Closed;

/// A dialog the window can show.
pub trait Modal: Render + EventEmitter<Closed> {
    /// Gives the focus to where the dialog starts.
    fn focus_first(&self, window: &mut Window, cx: &mut App);

    /// A click beside the dialog closes it, as for a list only to look at.
    fn closes_on_backdrop(&self) -> bool {
        false
    }
}

/// Sends what a dialog asked for; resolves to the core's refusal.
pub type Send<T> = Box<dyn Fn(T, &mut App) -> Task<Result<(), SharedString>>>;

/// What every dialog shares: a modal surface named by its title. The
/// caller adds its test support, then its focus and its key context
/// ([`CONTEXT`]), which closes it on Escape.
pub(crate) fn frame(id: &'static str, role: Role, title: &SharedString, cx: &App) -> Stateful<Div> {
    let tokens = cx.tokens();
    div()
        .id(id)
        .role(role)
        .aria_label(title.clone())
        .occlude()
        .flex()
        .flex_col()
        .gap_3()
        .w(px(440.))
        .max_w_full()
        .p_4()
        .bg(tokens.bg_1)
        .text_color(tokens.text_primary)
        .border_1()
        .border_color(tokens.border)
        .rounded(px(8.))
        .shadow_lg()
        .child(
            div()
                .id("dialog-title")
                .test_support()
                .role(Role::Heading)
                .aria_level(2)
                .aria_label(title.clone())
                .text_lg()
                .font_weight(gpui_kit::FontWeight::SEMIBOLD)
                .child(title.clone()),
        )
}

/// The core's refusal, under what it refused.
pub(crate) fn refusal(error: &Option<SharedString>, cx: &App) -> Option<impl IntoElement + use<>> {
    let danger = cx.tokens().danger;
    error.clone().map(|error| {
        div()
            .id("dialog-error")
            .test_support()
            .role(Role::Alert)
            .aria_label(error.clone())
            .text_color(danger)
            .child(error)
    })
}

/// The buttons at the bottom, at the right end.
pub(crate) fn actions_row() -> Div {
    div().flex().flex_row().justify_end().gap_2().pt_1()
}

/// Asks for a name: a new one, or a changed one (`ui/src/dialogs/parts.tsx`'s
/// `NameDialog`). Enter submits.
pub struct NameDialog {
    title: SharedString,
    submit: SharedString,
    input: Entity<InputState>,
    error: Option<SharedString>,
    sending: bool,
    send: Send<String>,
    focus: FocusHandle,
    _input_events: Subscription,
}

impl EventEmitter<Closed> for NameDialog {}

impl NameDialog {
    /// A dialog named `title` whose field starts with `initial`; `submit`
    /// names its button.
    pub fn new(
        title: impl Into<SharedString>,
        submit: impl Into<SharedString>,
        initial: &str,
        send: Send<String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let input = cx.new(|cx| InputState::new(window, cx).default_value(initial.to_owned()));
        let input_events = cx.subscribe_in(&input, window, |this, _, event, window, cx| {
            match event {
                InputEvent::PressEnter { .. } => this.submit(window, cx),
                // Typing clears the refusal.
                InputEvent::Change => {
                    if this.error.take().is_some() {
                        cx.notify();
                    }
                }
                InputEvent::Focus | InputEvent::Blur => {}
            }
        });
        Self {
            title: title.into(),
            submit: submit.into(),
            input,
            error: None,
            sending: false,
            send,
            focus: cx.focus_handle(),
            _input_events: input_events,
        }
    }

    fn submit(&mut self, _: &mut Window, cx: &mut Context<Self>) {
        if self.sending {
            return;
        }
        self.sending = true;
        self.error = None;
        cx.notify();
        let name = self.input.read(cx).value().to_string();
        let sent = (self.send)(name, cx);
        cx.spawn(async move |this, cx| {
            let result = sent.await;
            let _ = this.update(cx, |this, cx| {
                this.sending = false;
                match result {
                    Ok(()) => cx.emit(Closed),
                    Err(error) => this.error = Some(error),
                }
                cx.notify();
            });
        })
        .detach();
    }

    fn on_submit(&mut self, _: &ClickEvent, window: &mut Window, cx: &mut Context<Self>) {
        self.submit(window, cx);
    }

    fn cancel(&mut self, _: &Cancel, _: &mut Window, cx: &mut Context<Self>) {
        cx.emit(Closed);
    }

    fn on_cancel(&mut self, _: &ClickEvent, _: &mut Window, cx: &mut Context<Self>) {
        cx.emit(Closed);
    }
}

impl Modal for NameDialog {
    /// The field has the focus.
    fn focus_first(&self, window: &mut Window, cx: &mut App) {
        self.input.update(cx, |input, cx| input.focus(window, cx));
    }
}

impl Focusable for NameDialog {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus.clone()
    }
}

impl Render for NameDialog {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        frame("name-dialog-frame", Role::Dialog, &self.title, cx)
            .test_support()
            .track_focus(&self.focus)
            .key_context(CONTEXT)
            .on_action(cx.listener(Self::cancel))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap_1()
                    .child(strings::dialogs::NAME)
                    .child(
                        Input::new(&self.input)
                            .id("name-field")
                            .aria_label(strings::dialogs::NAME),
                    ),
            )
            .children(refusal(&self.error, cx))
            .child(
                actions_row()
                    .child(
                        Button::new("dialog-cancel")
                            .label(strings::common::CANCEL)
                            .on_click(cx.listener(Self::on_cancel)),
                    )
                    .child(
                        Button::new("dialog-submit")
                            .primary()
                            .label(self.submit.clone())
                            .disabled(self.sending)
                            .on_click(cx.listener(Self::on_submit)),
                    ),
            )
            // The trap takes the place of the frame, under the dialog's id.
            .focus_trap("name-dialog", &self.focus)
    }
}

/// A question about something that cannot be undone
/// (`ui/src/dialogs/parts.tsx`'s `Question`): Cancel has the focus, and
/// the danger button does it.
pub struct Question {
    title: SharedString,
    /// What it does, one paragraph each.
    text: Vec<SharedString>,
    submit: SharedString,
    error: Option<SharedString>,
    sending: bool,
    send: Send<()>,
    focus: FocusHandle,
    cancel: FocusHandle,
}

impl EventEmitter<Closed> for Question {}

impl Question {
    pub fn new(
        title: impl Into<SharedString>,
        text: Vec<SharedString>,
        submit: impl Into<SharedString>,
        send: Send<()>,
        cx: &mut Context<Self>,
    ) -> Self {
        Self {
            title: title.into(),
            text,
            submit: submit.into(),
            error: None,
            sending: false,
            send,
            focus: cx.focus_handle(),
            cancel: cx.focus_handle(),
        }
    }

    fn confirm(&mut self, _: &ClickEvent, _: &mut Window, cx: &mut Context<Self>) {
        if self.sending {
            return;
        }
        self.sending = true;
        self.error = None;
        cx.notify();
        let sent = (self.send)((), cx);
        cx.spawn(async move |this, cx| {
            let result = sent.await;
            let _ = this.update(cx, |this, cx| {
                this.sending = false;
                match result {
                    Ok(()) => cx.emit(Closed),
                    Err(error) => this.error = Some(error),
                }
                cx.notify();
            });
        })
        .detach();
    }

    fn cancel(&mut self, _: &Cancel, _: &mut Window, cx: &mut Context<Self>) {
        cx.emit(Closed);
    }

    fn on_cancel(&mut self, _: &ClickEvent, _: &mut Window, cx: &mut Context<Self>) {
        cx.emit(Closed);
    }
}

impl Modal for Question {
    /// Cancel has the focus.
    fn focus_first(&self, window: &mut Window, cx: &mut App) {
        window.focus(&self.cancel, cx);
    }
}

impl Focusable for Question {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus.clone()
    }
}

impl Render for Question {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let text = self.text.iter().enumerate().map(|(at, text)| {
            div()
                .id(("question-text", at))
                .test_support()
                .role(Role::Paragraph)
                .aria_label(text.clone())
                .child(text.clone())
        });
        frame("question-frame", Role::AlertDialog, &self.title, cx)
            .test_support()
            .track_focus(&self.focus)
            .key_context(CONTEXT)
            .on_action(cx.listener(Self::cancel))
            .children(text)
            .children(refusal(&self.error, cx))
            .child(
                actions_row()
                    .child(
                        Button::new("dialog-cancel")
                            .label(strings::common::CANCEL)
                            .track_focus(&self.cancel)
                            .on_click(cx.listener(Self::on_cancel)),
                    )
                    .child(
                        Button::new("dialog-submit")
                            .danger()
                            .label(self.submit.clone())
                            .disabled(self.sending)
                            .on_click(cx.listener(Self::confirm)),
                    ),
            )
            // The trap takes the place of the frame, under the dialog's id.
            .focus_trap("question", &self.focus)
    }
}
