//! The update dialog (PLAT-4), a port of `ui/src/update/UpdateDialog.tsx`:
//! the running and the new version, the release notes as plain text
//! (SEC-3), and what the package does with an update: Install and restart
//! for the Windows installer, the macOS app and the AppImage, Open download
//! page for the others. Later hides the notice until the next start or a
//! newer version. Rc.1 asked before restarting with unsaved conflict edits
//! (OPS-9); the conflict editor, and that question, come with M6-19.

use chrono::{DateTime, Local};
use gitcroc_core::UpdateStatus;
use gpui_component::button::{Button, ButtonVariants};
use gpui_component::{Disableable, FocusTrapElement};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::{
    App, ClickEvent, Context, Entity, EventEmitter, FocusHandle, Focusable, InteractiveElement,
    IntoElement, ParentElement, Render, Role, SharedString, StatefulInteractiveElement, Styled,
    Subscription, Task, TestSupportExt, Window, div, px,
};

use crate::dialog::{self, Cancel, Closed, Modal};
use crate::strings::update as text;
use crate::theme::{ActiveTokens, AppTheme};
use crate::updates::Updates;

const NOTES_HEIGHT: gpui_kit::Pixels = px(200.);

pub struct UpdateDialog {
    updates: Entity<Updates>,
    error: Option<SharedString>,
    sending: bool,
    focus: FocusHandle,
    /// Install, or Open download page.
    primary: FocusHandle,
    _updates_changed: Subscription,
}

impl EventEmitter<Closed> for UpdateDialog {}

/// A manifest's RFC 3339 date as the local date; one that does not parse
/// is shown as it is.
pub fn format_date(date: &str) -> String {
    DateTime::parse_from_rfc3339(date).map_or_else(
        |_| date.to_owned(),
        |date| date.with_timezone(&Local).date_naive().to_string(),
    )
}

impl UpdateDialog {
    pub fn new(updates: Entity<Updates>, cx: &mut Context<Self>) -> Self {
        let updates_changed = cx.observe(&updates, |_, _, cx| cx.notify());
        Self {
            updates,
            error: None,
            sending: false,
            focus: cx.focus_handle(),
            primary: cx.focus_handle(),
            _updates_changed: updates_changed,
        }
    }

    /// Sends `request`; closes when it succeeds, else shows why not.
    fn send(&mut self, request: Task<Result<(), SharedString>>, cx: &mut Context<Self>) {
        if self.sending {
            return;
        }
        self.sending = true;
        self.error = None;
        cx.notify();
        cx.spawn(async move |this, cx| {
            let result = request.await;
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

    fn install(&mut self, _: &ClickEvent, _: &mut Window, cx: &mut Context<Self>) {
        let request = self.updates.update(cx, |updates, cx| updates.install(cx));
        self.send(request, cx);
    }

    fn download(&mut self, _: &ClickEvent, _: &mut Window, cx: &mut Context<Self>) {
        let request = self
            .updates
            .update(cx, |updates, cx| updates.open_download_page(cx));
        self.send(request, cx);
    }

    fn later(&mut self, _: &ClickEvent, _: &mut Window, cx: &mut Context<Self>) {
        self.updates.read(cx).later();
        cx.emit(Closed);
    }

    fn close(&mut self, _: &Cancel, _: &mut Window, cx: &mut Context<Self>) {
        cx.emit(Closed);
    }
}

impl Modal for UpdateDialog {
    /// Install, or Open download page, has the focus.
    fn focus_first(&self, window: &mut Window, cx: &mut App) {
        window.focus(&self.primary, cx);
    }
}

impl Focusable for UpdateDialog {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus.clone()
    }
}

fn paragraph(id: &'static str, text: impl Into<SharedString>) -> impl IntoElement {
    let text = text.into();
    div()
        .id(id)
        .test_support()
        .role(Role::Paragraph)
        .aria_label(text.clone())
        .child(text)
}

impl Render for UpdateDialog {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let tokens = *cx.tokens();
        let code_font = AppTheme::global(cx).code_font().clone();
        let title = SharedString::from(text::TITLE);
        let view = self.updates.read(cx).view().cloned();
        let installs = view.as_ref().is_some_and(|view| view.installs);
        let offer = view.as_ref().and_then(|view| match &view.status {
            UpdateStatus::Available {
                version,
                date,
                notes,
                ..
            } => Some((
                view.current_version.clone(),
                version.clone(),
                date.clone(),
                notes.clone(),
            )),
            _ => None,
        });
        let has_offer = offer.is_some();
        let mut frame = dialog::frame("update-frame", Role::Dialog, &title, cx)
            .test_support()
            .track_focus(&self.focus)
            .key_context(dialog::CONTEXT)
            .on_action(cx.listener(Self::close))
            .w(px(560.));
        if let Some((current, version, date, notes)) = offer {
            let notes = if notes.is_empty() {
                paragraph("update-notes", text::NO_NOTES).into_any_element()
            } else {
                div()
                    .id("update-notes")
                    .test_support()
                    .role(Role::Document)
                    .aria_label(notes.clone())
                    .max_h(NOTES_HEIGHT)
                    .overflow_y_scroll()
                    .p_2()
                    .bg(tokens.bg_2)
                    .border_1()
                    .border_color(tokens.border)
                    .font_family(code_font)
                    .text_sm()
                    .child(notes)
                    .into_any_element()
            };
            frame = frame
                .child(paragraph(
                    "update-versions",
                    text::versions(&current, &version),
                ))
                .children(date.map(|date| {
                    paragraph("update-published", text::published(&format_date(&date)))
                }))
                .child(
                    div()
                        .id("update-notes-title")
                        .test_support()
                        .role(Role::Heading)
                        .aria_level(3)
                        .aria_label(text::NOTES)
                        .font_weight(gpui_kit::FontWeight::SEMIBOLD)
                        .child(text::NOTES),
                )
                .child(notes)
                .when(!installs, |frame| {
                    frame.child(paragraph("update-notify-only", text::NOTIFY_ONLY))
                });
        }
        let primary = if installs {
            Button::new("update-install")
                .primary()
                .label(text::INSTALL)
                .disabled(!has_offer || self.sending)
                .track_focus(&self.primary)
                .on_click(cx.listener(Self::install))
        } else {
            Button::new("update-download")
                .primary()
                .label(text::OPEN_DOWNLOAD_PAGE)
                .disabled(!has_offer || self.sending)
                .track_focus(&self.primary)
                .on_click(cx.listener(Self::download))
        };
        frame
            .children(dialog::refusal(&self.error, cx))
            .child(
                dialog::actions_row()
                    .child(
                        Button::new("update-later")
                            .label(text::LATER)
                            .on_click(cx.listener(Self::later)),
                    )
                    .child(primary),
            )
            // The trap takes the place of the frame, under the dialog's id.
            .focus_trap("update-dialog", &self.focus)
    }
}

#[cfg(test)]
mod tests {
    use super::format_date;

    #[test]
    fn plat_4_release_date_is_shown_as_a_date() {
        let date = format_date("2026-10-01T12:00:00Z");
        assert_eq!(date.len(), 10, "{date}");
        assert!(date.starts_with("2026-"), "{date}");
        assert_eq!(format_date("soon"), "soon");
    }
}
