//! rc.1 Toolbar.tsx: defaults on the primary buttons, forms on the arrows.
use crate::{
    operation_dialog::{Operation, OperationMode},
    strings::operations as text,
    window::GitcrocWindow,
};
use gitcroc_core::{Prune, PullStrategy, StartPoint, SyncOp, TargetSet};
use gpui_component::{Disableable, button::Button};
use gpui_kit::{AnyElement, Context, IntoElement};
impl GitcrocWindow {
    pub(crate) fn operation_toolbar(&self, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let target = self.workspaces.read(cx).active().and_then(|workspace| {
            let tree = self.trees.get(&workspace.id)?.read(cx);
            let TargetSet::Repo { repo_id } = tree.target()? else {
                return None;
            };
            let view = tree.tree().summary(&repo_id);
            let fetch = !view.is_some_and(|v| v.busy || v.missing);
            let blocked = crate::row_context::blocked(view);
            let write = blocked.is_none();
            let branch = !view
                .and_then(|v| v.summary.as_ref())
                .is_some_and(|s| matches!(s.head, gitcroc_core::Head::Detached { .. }));
            let name = tree
                .tree()
                .repo_names()
                .get(&repo_id)
                .cloned()
                .unwrap_or_default();
            Some((
                workspace.id.clone(),
                repo_id,
                name,
                fetch,
                write,
                branch,
                blocked,
            ))
        });
        let mut buttons = Vec::new();
        for (at, (label, mode, options, needs_branch)) in [
            (
                text::FETCH,
                OperationMode::Run(Operation::Sync(SyncOp::Fetch {
                    prune: Prune::Config,
                })),
                Some(OperationMode::Fetch),
                false,
            ),
            (
                text::PULL,
                OperationMode::Run(Operation::Sync(SyncOp::Pull {
                    strategy: PullStrategy::Config,
                })),
                Some(OperationMode::Pull),
                true,
            ),
            (
                text::PUSH,
                OperationMode::Run(Operation::Sync(SyncOp::Push {
                    set_upstream: true,
                    remote: None,
                    force_with_lease: false,
                })),
                Some(OperationMode::Push),
                true,
            ),
            (text::BRANCH, OperationMode::Merge, None, false),
            (text::MERGE, OperationMode::Merge, None, true),
            (text::STASH, OperationMode::StashPush, None, false),
        ]
        .into_iter()
        .enumerate()
        {
            let enabled = target
                .as_ref()
                .is_some_and(|(_, _, _, fetch, write, branch, _)| {
                    if at == 0 {
                        *fetch
                    } else {
                        *write && (!needs_branch || *branch)
                    }
                });
            let reason = target.as_ref().map_or(
                text::SINGLE_REPO,
                |(_, _, _, fetch, _, branch, blocked)| {
                    if at == 0 && *fetch {
                        label
                    } else if let Some(reason) = blocked {
                        reason
                    } else if needs_branch && !branch {
                        text::DETACHED
                    } else {
                        label
                    }
                },
            );
            let target = target.clone();
            let button = Button::new(gpui_kit::ElementId::Name(format!("toolbar-{at}").into()))
                .label(label)
                .disabled(!enabled)
                .tooltip(reason)
                .on_click(cx.listener(move |this, _, window, cx| {
                    if let Some((workspace, repo, name, _, _, _, _)) = target.clone() {
                        if at == 3 {
                            this.ask_branch(
                                crate::branch_dialog::AskBranch {
                                    workspace,
                                    repo,
                                    repo_name: name,
                                    mode: crate::branch_dialog::BranchMode::Create {
                                        start: StartPoint::Head,
                                    },
                                    immediate: false,
                                },
                                window,
                                cx,
                            );
                        } else {
                            this.ask_operation(workspace, repo, name, mode.clone(), window, cx);
                        }
                    }
                }));
            buttons.push(button.into_any_element());
            if let Some(mode) = options {
                let target = self.workspaces.read(cx).active().and_then(|w| {
                    let tree = self.trees.get(&w.id)?.read(cx);
                    let TargetSet::Repo { repo_id } = tree.target()? else {
                        return None;
                    };
                    Some((
                        w.id.clone(),
                        repo_id.clone(),
                        tree.tree()
                            .repo_names()
                            .get(&repo_id)
                            .cloned()
                            .unwrap_or_default(),
                    ))
                });
                buttons.push(
                    Button::new(gpui_kit::ElementId::Name(
                        format!("toolbar-options-{at}").into(),
                    ))
                    .icon(gpui_component::IconName::ChevronDown)
                    .accessibility_label(text::options(label))
                    .tooltip(if enabled {
                        text::options(label)
                    } else {
                        reason.to_owned()
                    })
                    .disabled(!enabled)
                    .on_click(cx.listener(move |this, _, window, cx| {
                        if let Some((workspace, repo, name)) = target.clone() {
                            this.ask_operation(workspace, repo, name, mode.clone(), window, cx);
                        }
                    }))
                    .into_any_element(),
                );
            }
        }
        for (id, label) in [
            ("toolbar-palette", text::PALETTE),
            ("toolbar-settings", text::SETTINGS),
        ] {
            buttons.push(
                Button::new(id)
                    .label(label)
                    .disabled(true)
                    .tooltip(text::LATER)
                    .into_any_element(),
            );
        }
        buttons
    }
}
