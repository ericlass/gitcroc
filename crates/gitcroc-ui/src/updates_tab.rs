//! The Updates tab of Settings, a port of rc.1's `ui/src/update/UpdateSection.tsx`
//! (D12, SEC-7): the automatic check, on by default and saved with the
//! settings; the last result with its time; and Check now, which also runs
//! while automatic checks are off. A failed check shows here and nowhere
//! else. Reasons are text (SEC-3).

use chrono::{Local, TimeZone};
use gitcroc_core::{UpdateStatus, UpdateView};
use gpui_component::Disableable;
use gpui_component::button::Button;
use gpui_component::checkbox::Checkbox;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::{
    Context, Entity, FontWeight, InteractiveElement, IntoElement, ParentElement, Render, Role,
    SharedString, StatefulInteractiveElement, Styled, Subscription, TestSupportExt, Window, div,
};

use crate::settings::SettingsModel;
use crate::strings::settings as text;
use crate::theme::ActiveTokens;
use crate::updates::Updates;

/// A check's time, milliseconds since the Unix epoch, as the local date and
/// time.
pub fn checked_at(at: u64) -> String {
    i64::try_from(at)
        .ok()
        .and_then(|at| Local.timestamp_millis_opt(at).single())
        .map_or_else(
            || at.to_string(),
            |time| time.format("%Y-%m-%d %H:%M").to_string(),
        )
}

/// Where the updater is, as the tab says it: the last result with its
/// time.
pub fn status_text(view: &UpdateView) -> String {
    if !view.checks {
        return text::NO_CHECKS.to_owned();
    }
    match &view.status {
        UpdateStatus::Idle => text::NEVER_CHECKED.to_owned(),
        UpdateStatus::Checking => text::CHECKING.to_owned(),
        UpdateStatus::UpToDate { checked_at: at } => {
            text::up_to_date(&view.current_version, &checked_at(*at))
        }
        UpdateStatus::Available {
            version,
            checked_at: at,
            ..
        } => text::update_available(version, &checked_at(*at)),
        UpdateStatus::Failed {
            reason,
            checked_at: at,
        } => text::update_failed(reason, &checked_at(*at)),
        UpdateStatus::Downloading { version, .. } => text::downloading(version),
        UpdateStatus::ReadyToRestart { version } => text::ready_to_restart(version),
    }
}

pub struct UpdatesTab {
    settings: Entity<SettingsModel>,
    updates: Entity<Updates>,
    error: Option<SharedString>,
    _subscriptions: Vec<Subscription>,
}

impl UpdatesTab {
    pub fn new(
        settings: Entity<SettingsModel>,
        updates: Entity<Updates>,
        cx: &mut Context<Self>,
    ) -> Self {
        let subscriptions = vec![
            cx.observe(&settings, |_, _, cx| cx.notify()),
            cx.observe(&updates, |_, _, cx| cx.notify()),
        ];
        Self {
            settings,
            updates,
            error: None,
            _subscriptions: subscriptions,
        }
    }

    fn set_check(&mut self, on: bool, cx: &mut Context<Self>) {
        self.error = None;
        let request = self
            .settings
            .update(cx, |settings, cx| settings.set_update_check(on, cx));
        cx.spawn(async move |this, cx| {
            if let Err(error) = request.await {
                let _ = this.update(cx, |this, cx| {
                    this.error = Some(error);
                    cx.notify();
                });
            }
        })
        .detach();
        cx.notify();
    }
}

impl Render for UpdatesTab {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let settings = self.settings.read(cx);
        let (read_only, on) = (settings.read_only(), settings.view().update_check);
        let view = self.updates.read(cx).view().cloned();
        let busy = view.as_ref().is_none_or(|view| {
            !view.checks
                || matches!(
                    view.status,
                    UpdateStatus::Checking
                        | UpdateStatus::Downloading { .. }
                        | UpdateStatus::ReadyToRestart { .. }
                )
        });
        let danger = cx.tokens().danger;
        div()
            .id("settings-updates")
            .test_support()
            .role(Role::Group)
            .aria_label(text::UPDATES_SECTION)
            .flex()
            .flex_col()
            .gap_2()
            .child(
                div()
                    .font_weight(FontWeight::SEMIBOLD)
                    .child(text::UPDATES_SECTION),
            )
            .child(
                Checkbox::new("settings-update-check")
                    .label(text::CHECK_AUTOMATICALLY)
                    .checked(on)
                    .disabled(read_only)
                    .on_click(cx.listener(|this, value: &bool, _, cx| this.set_check(*value, cx))),
            )
            .when_some(view.as_ref().map(status_text), |this, said| {
                let said: SharedString = said.into();
                this.child(
                    div()
                        .id("settings-update-status")
                        .test_support()
                        .role(Role::Paragraph)
                        .aria_label(said.clone())
                        .child(said),
                )
            })
            .when_some(self.error.clone(), |this, error| {
                this.child(
                    div()
                        .id("settings-updates-error")
                        .test_support()
                        .role(Role::Alert)
                        .aria_label(error.clone())
                        .text_color(danger)
                        .child(error),
                )
            })
            .child(
                div().child(
                    Button::new("settings-check-now")
                        .label(text::CHECK_NOW)
                        .disabled(busy)
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.updates.read(cx).check_now();
                        })),
                ),
            )
    }
}
