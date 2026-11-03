//! rc.1 Toolbar.tsx: defaults on the primary buttons, forms on the arrows.
//! On a group or a multi-selection Fetch, Pull and Push run through the
//! preflight (section 5, UI-20); Branch and Merge open the group name
//! dialog, and Stash is single-repo only.
use crate::{
    group_name_dialog::{AskGroupName, NameKind},
    group_ops::SyncKind,
    operation_dialog::{Operation, OperationMode},
    strings::{group_ops as group_text, operations as text},
    window::GitcrocWindow,
};
use gitcroc_core::{Prune, PullStrategy, StartPoint, SyncOp, TargetSet, WorkspaceId};
use gpui_component::{Disableable, button::Button};
use gpui_kit::{AnyElement, Context, IntoElement};

/// What the toolbar's Fetch, Pull and Push act on (GRP-5).
#[derive(Clone)]
enum Scope {
    None,
    /// A group or a multi-selection: the preflight decides per repo.
    Group {
        workspace: WorkspaceId,
        target: TargetSet,
        /// Such as `Backend` or `3 repositories`.
        name: String,
    },
    /// One repo, with what its summaries allow.
    Repo {
        workspace: WorkspaceId,
        repo: gitcroc_core::RepoId,
        name: String,
        fetch: bool,
        write: bool,
        branch: bool,
        blocked: Option<&'static str>,
    },
}

impl GitcrocWindow {
    /// What the toolbar acts on for the active tab.
    fn toolbar_scope(&self, cx: &Context<Self>) -> Scope {
        let Some(workspace) = self
            .workspaces
            .read(cx)
            .active()
            .map(|view| view.id.clone())
        else {
            return Scope::None;
        };
        let Some(tree) = self.trees.get(&workspace).map(|tree| tree.read(cx)) else {
            return Scope::None;
        };
        let Some(target) = tree.target() else {
            return Scope::None;
        };
        match &target {
            TargetSet::Group { group_id } => {
                let name = self
                    .workspaces
                    .read(cx)
                    .view(&workspace)
                    .and_then(|view| {
                        view.groups
                            .iter()
                            .find(|group| &group.id == group_id)
                            .map(|group| group.name.clone())
                    })
                    .unwrap_or_default();
                Scope::Group {
                    workspace,
                    target,
                    name,
                }
            }
            TargetSet::Repos { repo_ids } => Scope::Group {
                workspace,
                name: group_text::repositories(u32::try_from(repo_ids.len()).unwrap_or(u32::MAX)),
                target: target.clone(),
            },
            TargetSet::Repo { repo_id } => {
                let view = tree.tree().summary(repo_id);
                let fetch = !view.is_some_and(|view| view.busy || view.missing);
                let blocked = crate::row_context::blocked(view);
                let write = blocked.is_none();
                let branch = !view
                    .and_then(|view| view.summary.as_ref())
                    .is_some_and(|summary| {
                        matches!(summary.head, gitcroc_core::Head::Detached { .. })
                    });
                let name = tree
                    .tree()
                    .repo_names()
                    .get(repo_id)
                    .cloned()
                    .unwrap_or_default();
                Scope::Repo {
                    workspace,
                    repo: repo_id.clone(),
                    name,
                    fetch,
                    write,
                    branch,
                    blocked,
                }
            }
        }
    }

    pub(crate) fn operation_toolbar(&self, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let scope = self.toolbar_scope(cx);
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
            let enabled = match (&scope, at) {
                (Scope::Repo { fetch, .. }, 0) => *fetch,
                (Scope::Repo { write, branch, .. }, 1 | 2) => *write && (!needs_branch || *branch),
                (Scope::Repo { write, .. }, 3) => *write,
                (Scope::Repo { write, branch, .. }, 4) => *write && *branch,
                (Scope::Repo { write, .. }, 5) => *write,
                // Everything but Stash runs through the preflight, which
                // skips or blocks each repo that cannot take it (GRP-8).
                (Scope::Group { .. }, hit) => hit < 5,
                _ => false,
            };
            let reason = match &scope {
                Scope::None => text::SINGLE_REPO.to_owned(),
                Scope::Group { .. } => match at {
                    5 => text::STASH_ONE_REPO.to_owned(),
                    _ => label.to_owned(),
                },
                Scope::Repo {
                    fetch,
                    branch,
                    blocked,
                    ..
                } => {
                    if at == 0 && *fetch {
                        label.to_owned()
                    } else if let Some(reason) = blocked {
                        (*reason).to_owned()
                    } else if needs_branch && !branch {
                        text::DETACHED.to_owned()
                    } else {
                        label.to_owned()
                    }
                }
            };
            let scope_for_click = self.scope_clone(&scope);
            let button = Button::new(gpui_kit::ElementId::Name(format!("toolbar-{at}").into()))
                .label(label)
                .disabled(!enabled)
                .tooltip(reason.clone())
                .on_click(cx.listener(move |this, _, window, cx| {
                    this.toolbar_run(scope_for_click.clone(), at, mode.clone(), window, cx);
                }));
            buttons.push(button.into_any_element());
            if let Some(options) = options {
                let scope_for_options = self.scope_clone(&scope);
                let options_label = text::options(label);
                buttons.push(
                    Button::new(gpui_kit::ElementId::Name(
                        format!("toolbar-options-{at}").into(),
                    ))
                    .icon(gpui_component::IconName::ChevronDown)
                    .accessibility_label(options_label.clone())
                    .tooltip(if enabled {
                        options_label
                    } else {
                        reason.clone()
                    })
                    .disabled(!enabled)
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.toolbar_options(
                            scope_for_options.clone(),
                            at,
                            options.clone(),
                            window,
                            cx,
                        );
                    }))
                    .into_any_element(),
                );
            }
        }
        buttons.push(
            Button::new("toolbar-palette")
                .label(text::PALETTE)
                .disabled(true)
                .tooltip(text::LATER)
                .into_any_element(),
        );
        buttons.push(
            Button::new("toolbar-settings")
                .label(text::SETTINGS)
                .tooltip(text::SETTINGS)
                .on_click(cx.listener(|this, _, window, cx| {
                    this.open_settings(crate::settings_dialog::SettingsTab::Ai, window, cx);
                }))
                .into_any_element(),
        );
        buttons
    }

    /// A clone of what a button acts on, for its listener.
    fn scope_clone(&self, scope: &Scope) -> Scope {
        match scope {
            Scope::None => Scope::None,
            Scope::Group {
                workspace,
                target,
                name,
            } => Scope::Group {
                workspace: workspace.clone(),
                target: target.clone(),
                name: name.clone(),
            },
            Scope::Repo {
                workspace,
                repo,
                name,
                fetch,
                write,
                branch,
                blocked,
            } => Scope::Repo {
                workspace: workspace.clone(),
                repo: repo.clone(),
                name: name.clone(),
                fetch: *fetch,
                write: *write,
                branch: *branch,
                blocked: *blocked,
            },
        }
    }

    /// A click on the toolbar: Fetch, Pull and Push run with the defaults on
    /// a group or several repos, or open the single repo's dialog; Branch,
    /// Merge and Stash open their dialogs for one repo.
    fn toolbar_run(
        &mut self,
        scope: Scope,
        at: usize,
        mode: OperationMode,
        window: &mut gpui_kit::Window,
        cx: &mut Context<Self>,
    ) {
        match scope {
            Scope::None => {}
            Scope::Group {
                workspace,
                target,
                name,
            } => {
                // Branch and Merge open the group name dialog (UI-20).
                let named = match at {
                    3 => Some(NameKind::Create),
                    4 => Some(NameKind::Merge),
                    _ => None,
                };
                if let Some(kind) = named {
                    self.open_group_name(
                        AskGroupName {
                            workspace,
                            target,
                            target_name: name,
                            kind,
                            name: String::new(),
                        },
                        window,
                        cx,
                    );
                    return;
                }
                let kind = match at {
                    0 => SyncKind::Fetch,
                    1 => SyncKind::Pull,
                    2 => SyncKind::Push,
                    _ => return,
                };
                self.group_ops().update(cx, |ops, cx| {
                    ops.run(workspace, target, kind.default_op(), cx)
                });
            }
            Scope::Repo {
                workspace,
                repo,
                name,
                ..
            } => {
                if at == 3 {
                    self.ask_branch(
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
                    self.ask_operation(workspace, repo, name, mode, window, cx);
                }
            }
        }
    }

    /// The arrow beside a button: a group's options, or the single repo's
    /// dialog.
    fn toolbar_options(
        &mut self,
        scope: Scope,
        at: usize,
        mode: OperationMode,
        window: &mut gpui_kit::Window,
        cx: &mut Context<Self>,
    ) {
        match scope {
            Scope::None => {}
            Scope::Group {
                workspace,
                target,
                name,
            } => {
                let kind = match at {
                    0 => SyncKind::Fetch,
                    1 => SyncKind::Pull,
                    2 => SyncKind::Push,
                    _ => return,
                };
                self.open_group_sync(workspace, target, kind, name, window, cx);
            }
            Scope::Repo {
                workspace,
                repo,
                name,
                ..
            } => self.ask_operation(workspace, repo, name, mode, window, cx),
        }
    }
}
