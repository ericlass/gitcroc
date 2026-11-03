//! The consent dialog (AI-6), a port of `ui/src/ai/ConsentDialog.tsx`:
//! before a provider's first use, and again when its host or program
//! changed, it says what is sent for each file (AI-5) and to which host or
//! program, in the core's words. Allow keeps the consent with
//! `ai_provider_consent` and goes on; Cancel sends nothing. Cancel has the
//! focus and Escape cancels. Turning AI features off closes it unanswered
//! (AI-16). Text only (SEC-3).

use gitcroc_core::ConsentRequest;
use gpui_component::button::{Button, ButtonVariants};
use gpui_component::{Disableable, FocusTrapElement};
use gpui_kit::{
    App, ClickEvent, Context, Entity, EventEmitter, FocusHandle, Focusable, InteractiveElement,
    IntoElement, ParentElement, Render, Role, SharedString, StatefulInteractiveElement, Styled,
    Subscription, TestSupportExt, Window, div,
};

use crate::ai::Ai;
use crate::dialog::{self, Cancel, Closed, Modal};
use crate::strings::{self, ai::consent as text, common};
use crate::theme::{ActiveTokens, AppTheme};

/// What Allow goes on with once the consent is kept.
pub type Allowed = Box<dyn FnOnce(&mut Window, &mut App)>;

pub struct ConsentDialog {
    request: ConsentRequest,
    ai: Entity<Ai>,
    allowed: Option<Allowed>,
    error: Option<SharedString>,
    sending: bool,
    focus: FocusHandle,
    cancel: FocusHandle,
    _ai: Subscription,
}

impl EventEmitter<Closed> for ConsentDialog {}

impl ConsentDialog {
    pub fn new(
        request: ConsentRequest,
        ai: Entity<Ai>,
        allowed: Allowed,
        cx: &mut Context<Self>,
    ) -> Self {
        let watch = cx.observe(&ai, |this, ai, cx| {
            if !ai.read(cx).enabled() {
                this.allowed = None;
                cx.emit(Closed);
            }
        });
        Self {
            request,
            ai,
            allowed: Some(allowed),
            error: None,
            sending: false,
            focus: cx.focus_handle(),
            cancel: cx.focus_handle(),
            _ai: watch,
        }
    }

    fn allow(&mut self, _: &ClickEvent, window: &mut Window, cx: &mut Context<Self>) {
        if self.sending {
            return;
        }
        self.sending = true;
        self.error = None;
        cx.notify();
        let (id, target) = (
            self.request.provider_id.clone(),
            self.request.to.value().to_owned(),
        );
        let sent = self.ai.update(cx, |ai, cx| ai.consent(id, target, cx));
        cx.spawn_in(window, async move |this, cx| {
            let result = sent.await;
            let _ = this.update_in(cx, |this, window, cx| {
                this.sending = false;
                match result {
                    Ok(()) => {
                        if let Some(allowed) = this.allowed.take() {
                            allowed(window, cx);
                        }
                        cx.emit(Closed);
                    }
                    Err(error) => this.error = Some(error),
                }
                cx.notify();
            });
        })
        .detach();
    }

    fn cancel(&mut self, _: &Cancel, _: &mut Window, cx: &mut Context<Self>) {
        self.allowed = None;
        cx.emit(Closed);
    }

    fn on_cancel(&mut self, _: &ClickEvent, _: &mut Window, cx: &mut Context<Self>) {
        self.allowed = None;
        cx.emit(Closed);
    }
}

impl Modal for ConsentDialog {
    /// Cancel has the focus.
    fn focus_first(&self, window: &mut Window, cx: &mut App) {
        window.focus(&self.cancel, cx);
    }
}

impl Focusable for ConsentDialog {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus.clone()
    }
}

impl Render for ConsentDialog {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let tokens = *cx.tokens();
        let target: SharedString = self.request.to.value().to_owned().into();
        let title: SharedString = text::title(&target).into();
        let (before, after) = text::around(&self.request);
        let said: SharedString = text::text(&self.request).into();
        let sent = self.request.sent.iter().enumerate().map(|(at, item)| {
            let item: SharedString = item.clone().into();
            div()
                .id(("consent-sent-item", at))
                .test_support()
                .role(Role::ListItem)
                .aria_label(item.clone())
                .child(text::item(&item))
        });
        dialog::frame("consent-frame", Role::AlertDialog, &title, cx)
            .aria_description(strings::a11y::describe(&[&said, text::ASKED_AGAIN]))
            .test_support()
            .track_focus(&self.focus)
            .key_context(dialog::CONTEXT)
            .on_action(cx.listener(Self::cancel))
            .child(
                div()
                    .id("consent-text")
                    .test_support()
                    .role(Role::Paragraph)
                    .aria_label(said)
                    .flex()
                    .flex_row()
                    .flex_wrap()
                    .child(before)
                    .child(
                        div()
                            .id("consent-target")
                            .test_support()
                            .role(Role::Code)
                            .aria_label(target.clone())
                            .px_1()
                            .bg(tokens.bg_2)
                            .font_family(AppTheme::global(cx).code_font().clone())
                            .child(target),
                    )
                    .child(after),
            )
            .child(
                div()
                    .id("consent-sent")
                    .test_support()
                    .role(Role::List)
                    .aria_label(text::SENT)
                    .flex()
                    .flex_col()
                    .gap_1()
                    .children(sent),
            )
            .child(
                div()
                    .id("consent-again")
                    .test_support()
                    .role(Role::Paragraph)
                    .aria_label(text::ASKED_AGAIN)
                    .text_sm()
                    .text_color(tokens.text_secondary)
                    .child(text::ASKED_AGAIN),
            )
            .children(dialog::refusal(&self.error, cx))
            .child(
                dialog::actions_row()
                    .child(
                        Button::new("consent-cancel")
                            .label(common::CANCEL)
                            .track_focus(&self.cancel)
                            .on_click(cx.listener(Self::on_cancel)),
                    )
                    .child(
                        Button::new("consent-allow")
                            .primary()
                            .label(text::ALLOW)
                            .disabled(self.sending)
                            .on_click(cx.listener(Self::allow)),
                    ),
            )
            // The trap takes the place of the frame, under the dialog's id.
            .focus_trap("consent-dialog", &self.focus)
    }
}
