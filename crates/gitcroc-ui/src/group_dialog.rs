//! The dialogs of the group menus (UI-7), a port of rc.1's
//! `ui/src/tree/GroupDialogs.tsx`: a new group's name, a rename, and
//! deleting a group, which says its repos stay and nothing on disk is
//! touched (GRP-3). The core checks names (GRP-2); its refusal shows inside
//! the open dialog. Names are text (SEC-3). Each belongs to the workspace
//! it was opened in (ARCH-9).

use std::future::Future;

use gitcroc_app::app::AppError;
use gitcroc_core::WorkspaceId;
use gpui_kit::{App, AppContext, Entity, SharedString, Task, Window};

use crate::dialog::{NameDialog, Question, Send};
use crate::groups::GroupDialog;
use crate::services::Services;
use crate::strings::groups as text;

/// A group dialog, made and not shown yet.
pub enum Made {
    Name(Entity<NameDialog>),
    Question(Entity<Question>),
}

/// Sends `call`; resolves to the core's refusal.
fn sent<T, F>(
    services: &Services,
    call: impl FnOnce(gitcroc_app::app::App) -> F,
    cx: &mut App,
) -> Task<Result<(), SharedString>>
where
    T: std::marker::Send + 'static,
    F: Future<Output = Result<T, AppError>> + std::marker::Send + 'static,
{
    let call = services.with_app(call);
    cx.spawn(async move |_| match call.await {
        Some(Ok(_)) => Ok(()),
        Some(Err(error)) => Err(error.message.into()),
        None => Err(text::STOPPED.into()),
    })
}

/// Makes the dialog `dialog` of workspace `workspace`.
pub fn make(
    services: Services,
    workspace: WorkspaceId,
    dialog: GroupDialog,
    window: &mut Window,
    cx: &mut App,
) -> Made {
    match dialog {
        GroupDialog::New { repos } => {
            let send: Send<String> = Box::new(move |name: String, cx: &mut App| {
                let (workspace, repos) = (workspace.clone(), repos.clone());
                sent(
                    &services,
                    move |app| async move { app.group_create(workspace, name, repos).await },
                    cx,
                )
            });
            Made::Name(
                cx.new(|cx| NameDialog::new(text::NEW_DIALOG, text::CREATE, "", send, window, cx)),
            )
        }
        GroupDialog::Rename { group, name } => {
            let send: Send<String> = Box::new(move |new_name: String, cx: &mut App| {
                let (workspace, group) = (workspace.clone(), group.clone());
                sent(
                    &services,
                    move |app| async move { app.group_rename(workspace, group, new_name).await },
                    cx,
                )
            });
            Made::Name(cx.new(|cx| {
                NameDialog::new(
                    text::rename_dialog(&name),
                    text::RENAME,
                    &name,
                    send,
                    window,
                    cx,
                )
            }))
        }
        GroupDialog::Delete { group, name } => {
            let send: Send<()> = Box::new(move |(), cx: &mut App| {
                let (workspace, group) = (workspace.clone(), group.clone());
                sent(
                    &services,
                    move |app| async move { app.group_delete(workspace, group).await },
                    cx,
                )
            });
            Made::Question(cx.new(|cx| {
                Question::new(
                    text::delete_dialog(&name),
                    vec![text::DELETE_WHAT.into()],
                    text::DELETE,
                    send,
                    cx,
                )
            }))
        }
    }
}
