//! The open workspaces as the app reports them (ARCH-6, ARCH-9), as rc.1's
//! `stores/workspaces.ts` kept them: the window passes on the
//! `WorkspacesChanged` and `WorkspaceChanged` events, and the views send
//! the tab changes (UI-27, UI-28). The core checks names, decides and
//! saves; this only shows what it says.

use gitcroc_app::app::AppError;
use gitcroc_core::{WorkspaceId, WorkspaceListItem, WorkspaceTabs, WorkspaceView};
use gpui_kit::{App, Context, Entity, SharedString, Task};

use crate::services::{Call, Services};

/// Why a call failed, as the user reads it.
pub fn refusal(result: Option<Result<impl Sized, AppError>>) -> Result<(), SharedString> {
    match result {
        Some(Ok(_)) => Ok(()),
        Some(Err(error)) => Err(error.message.into()),
        // The runtime is shutting down: nothing is left to show.
        None => Ok(()),
    }
}

/// The open workspaces and the active one.
pub struct Workspaces {
    services: Services,
    tabs: WorkspaceTabs,
}

impl Workspaces {
    /// The tabs the app loaded at start, which are in memory.
    pub fn new(services: Services) -> Self {
        let tabs = services.app().workspace_get();
        Self { services, tabs }
    }

    pub fn tabs(&self) -> &WorkspaceTabs {
        &self.tabs
    }

    /// The active workspace's view, if one is open.
    pub fn active(&self) -> Option<&WorkspaceView> {
        let active = self.tabs.active.as_ref()?;
        self.tabs.open.iter().find(|view| &view.id == active)
    }

    /// The open workspace `id`.
    pub fn view(&self, id: &WorkspaceId) -> Option<&WorkspaceView> {
        self.tabs.open.iter().find(|view| &view.id == id)
    }

    pub fn set_tabs(&mut self, tabs: WorkspaceTabs, cx: &mut Context<Self>) {
        self.tabs = tabs;
        cx.notify();
    }

    /// One workspace's fresh view; a workspace that is not open is left out.
    pub fn set_view(&mut self, view: WorkspaceView, cx: &mut Context<Self>) {
        if let Some(open) = self.tabs.open.iter_mut().find(|open| open.id == view.id) {
            *open = view;
            cx.notify();
        }
    }

    /// Every workspace file, open ones marked, sorted by name.
    pub fn list(&self) -> Call<Result<Vec<WorkspaceListItem>, AppError>> {
        self.services
            .with_app(|app| async move { app.workspace_list().await })
    }

    /// Creates a workspace, which opens as the active last tab.
    pub fn create(
        this: &Entity<Self>,
        name: String,
        cx: &mut App,
    ) -> Task<Result<(), SharedString>> {
        Self::change(
            this,
            cx,
            |app| async move { app.workspace_create(name).await },
        )
    }

    /// Opens a workspace as the last tab, or switches to it when it is open
    /// (UI-27).
    pub fn open(
        this: &Entity<Self>,
        id: WorkspaceId,
        cx: &mut App,
    ) -> Task<Result<(), SharedString>> {
        Self::change(this, cx, |app| async move { app.workspace_open(id).await })
    }

    /// Closes a tab; the workspace file stays (UI-28).
    pub fn close(
        this: &Entity<Self>,
        id: WorkspaceId,
        cx: &mut App,
    ) -> Task<Result<(), SharedString>> {
        Self::change(this, cx, |app| async move { app.workspace_close(id).await })
    }

    pub fn rename(
        this: &Entity<Self>,
        id: WorkspaceId,
        name: String,
        cx: &mut App,
    ) -> Task<Result<(), SharedString>> {
        Self::change(this, cx, |app| async move {
            app.workspace_rename(id, name).await
        })
    }

    pub fn activate(
        this: &Entity<Self>,
        id: WorkspaceId,
        cx: &mut App,
    ) -> Task<Result<(), SharedString>> {
        Self::change(
            this,
            cx,
            |app| async move { app.workspace_activate(id).await },
        )
    }

    pub fn reorder(
        this: &Entity<Self>,
        order: Vec<WorkspaceId>,
        cx: &mut App,
    ) -> Task<Result<(), SharedString>> {
        Self::change(
            this,
            cx,
            |app| async move { app.workspace_reorder(order).await },
        )
    }

    /// Moves open workspace `id` by `step` places in the tab order; past
    /// either end it stays.
    pub fn move_tab(
        this: &Entity<Self>,
        id: &WorkspaceId,
        step: isize,
        cx: &mut App,
    ) -> Task<Result<(), SharedString>> {
        let mut order: Vec<WorkspaceId> = this
            .read(cx)
            .tabs
            .open
            .iter()
            .map(|view| view.id.clone())
            .collect();
        let Some(from) = order.iter().position(|open| open == id) else {
            return Task::ready(Ok(()));
        };
        let to = from as isize + step;
        if to < 0 || to >= order.len() as isize {
            return Task::ready(Ok(()));
        }
        let moved = order.remove(from);
        order.insert(to as usize, moved);
        Self::reorder(this, order, cx)
    }

    /// Deletes the file of a workspace that is not open, after the user
    /// confirmed.
    pub fn delete(
        this: &Entity<Self>,
        id: WorkspaceId,
        cx: &mut App,
    ) -> Task<Result<(), SharedString>> {
        let call = this
            .read(cx)
            .services
            .with_app(|app| async move { app.workspace_delete(id, true).await });
        cx.spawn(async move |_| refusal(call.await))
    }

    /// Asks the app for the save dialog; how the export went arrives as an
    /// event (DATA-2).
    pub fn export(
        this: &Entity<Self>,
        id: WorkspaceId,
        cx: &mut App,
    ) -> Task<Result<(), SharedString>> {
        let call = this
            .read(cx)
            .services
            .with_app(|app| async move { app.workspace_export(id) });
        cx.spawn(async move |_| refusal(call.await))
    }

    /// Asks the app for the open dialog; how the import went arrives as an
    /// event (DATA-2).
    pub fn import(this: &Entity<Self>, cx: &mut App) -> Task<Result<(), SharedString>> {
        let call = this
            .read(cx)
            .services
            .with_app(|app| async move { app.workspace_import().await });
        cx.spawn(async move |_| refusal(call.await))
    }

    /// Sends a tab change. The tabs it answers with are shown at once, as
    /// its event would show them.
    fn change<F>(
        this: &Entity<Self>,
        cx: &mut App,
        call: impl FnOnce(gitcroc_app::app::App) -> F,
    ) -> Task<Result<(), SharedString>>
    where
        F: Future<Output = Result<WorkspaceTabs, AppError>> + Send + 'static,
    {
        let call = this.read(cx).services.with_app(call);
        let this = this.downgrade();
        cx.spawn(async move |cx| match call.await {
            Some(Ok(tabs)) => {
                let _ = this.update(cx, |this, cx| this.set_tabs(tabs, cx));
                Ok(())
            }
            result => refusal(result),
        })
    }
}
