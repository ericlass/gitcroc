//! What the tab bar's "+" offers (UI-28), a port of
//! `ui/src/workspaces/WorkspaceMenu.tsx`: every workspace file, open ones
//! marked, to open or switch to (UI-27); Delete for one that is not open;
//! New workspace; and Import workspace (DATA-2). The same entries show as
//! a menu below "+" and as buttons when no tab is open. The core lists,
//! sorts and opens; this shows what it says.

use gitcroc_core::{ConfigWarning, WorkspaceId, WorkspaceListItem};
use gpui_component::button::Button;
use gpui_kit::{
    Context, ElementId, EventEmitter, FocusHandle, Focusable, InteractiveElement, IntoElement,
    ParentElement, Render, Role, SharedString, StatefulInteractiveElement, Styled, TestSupportExt,
    Window, div,
};

use crate::menu::MenuItem;
use crate::strings::{app, workspaces as text};
use crate::theme::ActiveTokens;
use crate::workspaces::Workspaces;

/// What an entry does, or a tab menu's item.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Choice {
    /// Opens the workspace, or switches to its tab.
    Open(WorkspaceId, SharedString),
    /// Asks before deleting a workspace that is not open.
    Delete(WorkspaceId, SharedString),
    New,
    Import,
    /// Asks for a tab's new name.
    Rename(WorkspaceId, SharedString),
    Export(WorkspaceId),
    Close(WorkspaceId),
}

/// Why a workspace file opens read-only (DATA-1).
fn warning_text(warning: &ConfigWarning) -> String {
    match warning {
        ConfigWarning::NewerSchema { found, .. } => text::newer_schema(*found),
        ConfigWarning::Unreadable { reason, .. } => text::unreadable(reason),
        ConfigWarning::NoConfigDir => text::NO_CONFIG_DIR.to_owned(),
    }
}

/// The entries for the workspace files `items`, in the core's order.
pub fn entries(items: &[WorkspaceListItem]) -> Vec<(MenuItem, Choice)> {
    let mut entries = Vec::new();
    for item in items {
        let name: SharedString = item.name.clone().into();
        let mut shown = item.name.clone();
        if item.open {
            shown = text::marked(&shown, text::OPEN);
        }
        if item.warning.is_some() {
            shown = text::marked(&shown, text::READ_ONLY);
        }
        let description = item
            .warning
            .as_ref()
            .map(|warning| warning_text(warning).into());
        entries.push((
            MenuItem::new(shown).description(description),
            Choice::Open(item.id.clone(), name.clone()),
        ));
        if !item.open {
            entries.push((
                MenuItem::new(text::DELETE_ENTRY).label(text::delete_label(&name)),
                Choice::Delete(item.id.clone(), name),
            ));
        }
    }
    entries.push((MenuItem::new(text::NEW_ENTRY).separated(), Choice::New));
    entries.push((MenuItem::new(text::IMPORT_ENTRY), Choice::Import));
    entries
}

/// The entries in place, as buttons, when no tab is open (UI-27).
pub struct WorkspaceStart {
    entries: Vec<(MenuItem, Choice)>,
    /// Why the list could not be read, or a workspace not opened.
    error: Option<SharedString>,
    focus: FocusHandle,
}

impl EventEmitter<Choice> for WorkspaceStart {}

impl WorkspaceStart {
    /// Reads the workspace files anew, so files made elsewhere show.
    pub fn new(workspaces: &gpui_kit::Entity<Workspaces>, cx: &mut Context<Self>) -> Self {
        let call = workspaces.read(cx).list();
        cx.spawn(async move |this, cx| {
            let result = call.await;
            let _ = this.update(cx, |this, cx| {
                match result {
                    Some(Ok(items)) => this.entries = entries(&items),
                    Some(Err(error)) => {
                        this.entries = entries(&[]);
                        this.error = Some(text::cannot_list(&error.message).into());
                    }
                    None => {}
                }
                cx.notify();
            });
        })
        .detach();
        Self {
            entries: Vec::new(),
            error: None,
            focus: cx.focus_handle(),
        }
    }

    /// Says why the last choice failed.
    pub fn set_error(&mut self, error: SharedString, cx: &mut Context<Self>) {
        self.error = Some(error);
        cx.notify();
    }
}

impl Focusable for WorkspaceStart {
    fn focus_handle(&self, _: &gpui_kit::App) -> FocusHandle {
        self.focus.clone()
    }
}

impl Render for WorkspaceStart {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let tokens = *cx.tokens();
        let buttons = self.entries.iter().enumerate().map(|(at, (item, choice))| {
            let choice = choice.clone();
            Button::new(ElementId::NamedInteger(
                "workspace-start-item".into(),
                at as u64,
            ))
            .label(item.text.clone())
            .accessibility_label(item.name())
            .on_click(cx.listener(move |_, _, _, cx| cx.emit(choice.clone())))
        });
        let error = self.error.clone().map(|error| {
            div()
                .id("workspace-start-error")
                .test_support()
                .role(Role::Alert)
                .aria_label(error.clone())
                .text_color(tokens.danger)
                .child(error)
        });
        div()
            .id("workspace-start")
            .test_support()
            .track_focus(&self.focus)
            .role(Role::Main)
            .size_full()
            .flex()
            .flex_col()
            .items_center()
            .justify_center()
            .gap_3()
            .p_8()
            .child(
                div()
                    .id("no-workspace")
                    .test_support()
                    .role(Role::Paragraph)
                    .aria_label(app::NO_WORKSPACE)
                    .text_color(tokens.text_secondary)
                    .child(app::NO_WORKSPACE),
            )
            .children(error)
            .child(
                div()
                    .id("workspace-start-entries")
                    .test_support()
                    .role(Role::Group)
                    .aria_label(text::LABEL)
                    .flex()
                    .flex_col()
                    .items_stretch()
                    .gap_1()
                    .min_w(gpui_kit::px(240.))
                    .children(buttons),
            )
    }
}
