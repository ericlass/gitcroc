//! The settings as the core last reported them (ARCH-6), a port of
//! rc.1's `ui/src/stores/settings.ts`: read with `settings_get` when the window
//! opens, then replaced by the view each change returns. The window applies
//! the theme, the fonts and the title bar from it (UI-23, UI-24, UI-26,
//! UI-35) and shows its warning (DATA-1); the Settings dialog shows it and
//! sends the user's changes. A refused change keeps the view as it was and
//! resolves to the core's message.

use gitcroc_app::app::AppError;
use gitcroc_core::{Appearance, GroupSettings, SettingsView, Theme};
use gpui_kit::{Context, SharedString, Task};

use crate::services::{Call, Services};

pub struct SettingsModel {
    services: Services,
    view: SettingsView,
}

impl SettingsModel {
    pub fn new(services: Services) -> Self {
        let view = services.app().settings_get();
        Self { services, view }
    }

    pub fn view(&self) -> &SettingsView {
        &self.view
    }

    /// Changes are refused, as the view's warning says (DATA-1).
    pub fn read_only(&self) -> bool {
        self.view.read_only
    }

    pub fn set_theme(
        &self,
        theme: Theme,
        cx: &mut Context<Self>,
    ) -> Task<Result<(), SharedString>> {
        let call = self
            .services
            .with_app(move |app| async move { app.settings_set_theme(theme).await });
        self.send(call, cx)
    }

    /// Saves `change` applied to the appearance in use.
    pub fn set_appearance(
        &self,
        change: impl FnOnce(&mut Appearance),
        cx: &mut Context<Self>,
    ) -> Task<Result<(), SharedString>> {
        let mut appearance = self.view.settings.appearance.clone();
        change(&mut appearance);
        let call = self
            .services
            .with_app(move |app| async move { app.settings_set_appearance(appearance).await });
        self.send(call, cx)
    }

    /// Saves `change` applied to the group settings in use.
    pub fn set_groups(
        &self,
        change: impl FnOnce(&mut GroupSettings),
        cx: &mut Context<Self>,
    ) -> Task<Result<(), SharedString>> {
        let mut groups = self.view.settings.groups;
        change(&mut groups);
        let call = self
            .services
            .with_app(move |app| async move { app.settings_set_groups(groups).await });
        self.send(call, cx)
    }

    pub fn set_update_check(
        &self,
        enabled: bool,
        cx: &mut Context<Self>,
    ) -> Task<Result<(), SharedString>> {
        let call = self
            .services
            .with_app(move |app| async move { app.settings_set_update_check(enabled).await });
        self.send(call, cx)
    }

    /// Waits for `call`, and keeps the view it returns.
    fn send(
        &self,
        call: Call<Result<SettingsView, AppError>>,
        cx: &mut Context<Self>,
    ) -> Task<Result<(), SharedString>> {
        cx.spawn(async move |this, cx| match call.await {
            Some(Ok(view)) => {
                let _ = this.update(cx, |this, cx| {
                    this.view = view;
                    cx.notify();
                });
                Ok(())
            }
            Some(Err(error)) => {
                // The view stays as it was; it is notified so fields that
                // showed the refused value show the saved one again.
                let _ = this.update(cx, |_, cx| cx.notify());
                Err(error.message.into())
            }
            None => Ok(()),
        })
    }
}
