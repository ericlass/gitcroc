//! The updater as the app reports it (PLAT-4, D12), a port of
//! `ui/src/stores/update.ts`: read with `update_status` at start, then kept
//! current by `UpdateStatusChanged`. The app decides when to check, what a
//! package does with an update and whether Later still hides the notice;
//! this forwards the user's choices. When an installed update is ready, the
//! app restarts into it.

use gitcroc_app::app::AppEvent;
use gitcroc_core::{UpdateStatus, UpdateView};
use gpui_kit::{Context, SharedString, Task};

use crate::services::Services;
use crate::workspaces::refusal;

pub struct Updates {
    services: Services,
    /// The app's view; `None` until the first read.
    view: Option<UpdateView>,
    /// An `UpdateStatusChanged` has arrived, so the first read is older.
    heard: bool,
    /// The restart into an installed update was asked for.
    restarting: bool,
}

impl Updates {
    pub fn new(services: Services, cx: &mut Context<Self>) -> Self {
        let mut this = Self {
            services,
            view: None,
            heard: false,
            restarting: false,
        };
        this.read_again(cx);
        this
    }

    /// Reads the updater's view; an event that arrives first is newer.
    pub fn read_again(&mut self, cx: &mut Context<Self>) {
        self.heard = false;
        let call = self
            .services
            .with_app(|app| async move { app.update_status() });
        cx.spawn(async move |this, cx| {
            if let Some(view) = call.await {
                let _ = this.update(cx, |this, cx| {
                    if !this.heard {
                        this.set_view(view, cx);
                    }
                });
            }
        })
        .detach();
    }

    pub fn on_event(&mut self, event: &AppEvent, cx: &mut Context<Self>) {
        if let AppEvent::UpdateStatusChanged(view) = event {
            self.heard = true;
            self.set_view(view.clone(), cx);
        }
    }

    fn set_view(&mut self, view: UpdateView, cx: &mut Context<Self>) {
        let ready = matches!(view.status, UpdateStatus::ReadyToRestart { .. });
        self.view = Some(view);
        cx.notify();
        // The update is installed: the app restarts into it (PLAT-4).
        if ready && !self.restarting {
            self.restarting = true;
            cx.restart();
        }
    }

    pub fn view(&self) -> Option<&UpdateView> {
        self.view.as_ref()
    }

    /// The version on offer, while Later has not put its notice off.
    pub fn notice(&self) -> Option<&str> {
        let view = self.view.as_ref().filter(|view| !view.later)?;
        match &view.status {
            UpdateStatus::Available { version, .. } => Some(version),
            _ => None,
        }
    }

    /// Hides the notice until the next start or a newer version.
    pub fn later(&self) {
        // The view the app then sends hides the notice. The call runs on
        // the runtime whether or not anyone awaits it.
        drop(
            self.services
                .with_app(|app| async move { app.update_later() }),
        );
    }

    /// Starts the install job; resolves to the app's refusal.
    pub fn install(&self, cx: &mut Context<Self>) -> Task<Result<(), SharedString>> {
        let call = self
            .services
            .with_app(|app| async move { app.update_install() });
        cx.spawn(async move |_, _| refusal(call.await))
    }

    /// Opens the releases page, for the packages that only notify.
    pub fn open_download_page(&self, cx: &mut Context<Self>) -> Task<Result<(), SharedString>> {
        let call = self
            .services
            .with_app(|app| async move { app.update_open_download_page().await });
        cx.spawn(async move |_, _| refusal(call.await))
    }
}
