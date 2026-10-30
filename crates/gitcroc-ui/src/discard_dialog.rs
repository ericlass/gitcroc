//! The confirmation before a discard (GRP-9), a port of rc.1's
//! `changes/DiscardDialog.tsx` on the shared dialog parts: it names the repo
//! and the files the core said it would discard, as text (SEC-3), up to
//! [`LISTED`] of them and a count of the rest. The core refuses a discard
//! sent without it; this only asks. Cancel has the focus, so Enter alone
//! never discards, and Escape and Cancel keep the changes.

use gpui_component::Disableable;
use gpui_component::FocusTrapElement;
use gpui_component::button::{Button, ButtonVariants};
use gpui_kit::{
    App, ClickEvent, Context, EventEmitter, FocusHandle, Focusable, InteractiveElement,
    IntoElement, ParentElement, Render, Role, SharedString, StatefulInteractiveElement, Styled,
    TestSupportExt, Window, div,
};

use crate::dialog::{self, Cancel, Closed, Modal, Send};
use crate::strings::{self, changes as text};

/// Files named one by one; more are counted.
pub const LISTED: usize = 10;

pub struct DiscardDialog {
    title: SharedString,
    /// What is lost.
    lost: SharedString,
    files: Vec<SharedString>,
    error: Option<SharedString>,
    sending: bool,
    send: Send<()>,
    focus: FocusHandle,
    cancel: FocusHandle,
}

impl EventEmitter<Closed> for DiscardDialog {}

impl DiscardDialog {
    /// Asks to discard `files` of `repo`, or one hunk of the file when
    /// `hunk`; `send` runs the discard.
    pub fn new(
        repo: &str,
        files: Vec<String>,
        hunk: bool,
        send: Send<()>,
        cx: &mut Context<Self>,
    ) -> Self {
        let lost = if hunk {
            text::HUNK_LOST.to_owned()
        } else if files.len() == 1 {
            text::FILE_LOST.to_owned()
        } else {
            text::files_lost(files.len())
        };
        Self {
            title: text::discard_dialog(repo).into(),
            lost: lost.into(),
            files: files.into_iter().map(SharedString::from).collect(),
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

impl Modal for DiscardDialog {
    /// Cancel has the focus.
    fn focus_first(&self, window: &mut Window, cx: &mut App) {
        window.focus(&self.cancel, cx);
    }
}

impl Focusable for DiscardDialog {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus.clone()
    }
}

impl Render for DiscardDialog {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let more = self.files.len().saturating_sub(LISTED);
        let names = self
            .files
            .iter()
            .take(LISTED)
            .enumerate()
            .map(|(at, name)| {
                div()
                    .id(("discard-file", at))
                    .test_support()
                    .role(Role::ListItem)
                    .aria_label(name.clone())
                    .min_w_0()
                    .truncate()
                    .child(name.clone())
            });
        let more = (more > 0).then(|| {
            let label: SharedString = text::and_more(more).into();
            div()
                .id("discard-more")
                .test_support()
                .role(Role::ListItem)
                .aria_label(label.clone())
                .child(label)
        });
        dialog::frame("discard-frame", Role::AlertDialog, &self.title, cx)
            .test_support()
            .track_focus(&self.focus)
            .key_context(dialog::CONTEXT)
            .on_action(cx.listener(Self::cancel))
            .child(
                div()
                    .id("discard-text")
                    .test_support()
                    .role(Role::Paragraph)
                    .aria_label(self.lost.clone())
                    .child(self.lost.clone()),
            )
            .child(
                div()
                    .id("discard-files")
                    .test_support()
                    .role(Role::List)
                    .aria_label(text::DISCARDED_FILES)
                    .flex()
                    .flex_col()
                    .pl_3()
                    .children(names)
                    .children(more),
            )
            .children(dialog::refusal(&self.error, cx))
            .child(
                dialog::actions_row()
                    .child(
                        Button::new("dialog-cancel")
                            .label(strings::common::CANCEL)
                            .track_focus(&self.cancel)
                            .on_click(cx.listener(Self::on_cancel)),
                    )
                    .child(
                        Button::new("dialog-submit")
                            .danger()
                            .label(text::DISCARD)
                            .disabled(self.sending)
                            .on_click(cx.listener(Self::confirm)),
                    ),
            )
            // The trap takes the place of the frame, under the dialog's id.
            .focus_trap("discard-dialog", &self.focus)
    }
}
