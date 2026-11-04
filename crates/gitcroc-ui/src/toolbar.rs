//! rc.1 Toolbar.tsx: defaults on the primary buttons, forms on the arrows.
//! On a group or a multi-selection Fetch, Pull and Push run through the
//! preflight (section 5, UI-20); Branch and Merge open the group name
//! dialog, and Stash is single-repo only.
use crate::{
    group_name_dialog::{AskGroupName, NameKind},
    group_ops::SyncKind,
    operation_dialog::{Operation, OperationDialog, OperationMode},
    palette::Command,
    strings::{group_ops as group_text, operations as text},
    window::GitcrocWindow,
};
use gitcroc_core::{Prune, PullStrategy, StartPoint, SyncOp, TargetSet, WorkspaceId};
use gpui_component::{Disableable, button::Button};
use gpui_kit::{AnyElement, Context, IntoElement, ParentElement, Styled, div};

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

/// The toolbar's operation buttons, in order: the label, what a click
/// does on one repo, what the arrow beside it opens, and whether it needs
/// a branch checked out.
fn toolbar_buttons() -> [(&'static str, OperationMode, Option<OperationMode>, bool); 6] {
    [
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
}

/// The buttons the palette runs (UI-22), by their place in [`toolbar_buttons`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ToolbarButton {
    Fetch = 0,
    Pull = 1,
    Push = 2,
    Branch = 3,
    Merge = 4,
    Stash = 5,
}

/// Whether button `at` is enabled on `scope`, and its tooltip: its label,
/// or why it is disabled.
fn button_state(scope: &Scope, at: usize, label: &str, needs_branch: bool) -> (bool, String) {
    let enabled = match (scope, at) {
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
    let reason = match scope {
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
    (enabled, reason)
}

impl GitcrocWindow {
    /// What toolbar button `button`, or with `options` the arrow beside
    /// it, does now, as a click on it (UI-22); `None` while nothing is
    /// selected for it to act on.
    pub(crate) fn toolbar_command(
        &self,
        button: ToolbarButton,
        options: bool,
        cx: &gpui_kit::App,
    ) -> Option<Command> {
        let scope = self.toolbar_scope(cx);
        if matches!(scope, Scope::None) {
            return None;
        }
        let at = button as usize;
        let (label, mode, options_mode, needs_branch) = toolbar_buttons().into_iter().nth(at)?;
        let (enabled, reason) = button_state(&scope, at, label, needs_branch);
        if !enabled {
            return Some(Command::cannot(reason));
        }
        Some(match (options, options_mode) {
            (true, Some(options_mode)) => Command::run(move |this, window, cx| {
                this.toolbar_options(scope, at, options_mode, window, cx)
            }),
            (true, None) => return None,
            (false, _) => {
                Command::run(move |this, window, cx| this.toolbar_run(scope, at, mode, window, cx))
            }
        })
    }

    /// Rebase onto a ref, by the rules of Merge: the merge dialog with
    /// rebase chosen for one repo, or the group rebase dialog (GRP-13).
    pub(crate) fn rebase_command(&self, cx: &gpui_kit::App) -> Option<Command> {
        let scope = self.toolbar_scope(cx);
        if matches!(scope, Scope::None) {
            return None;
        }
        let at = ToolbarButton::Merge as usize;
        let (enabled, reason) = button_state(&scope, at, text::MERGE, true);
        if !enabled {
            return Some(Command::cannot(reason));
        }
        Some(Command::run(move |this, window, cx| match scope {
            Scope::None => {}
            Scope::Group {
                workspace,
                target,
                name,
            } => this.open_group_name(
                AskGroupName {
                    workspace,
                    target,
                    target_name: name,
                    kind: NameKind::Rebase,
                    name: String::new(),
                },
                window,
                cx,
            ),
            Scope::Repo {
                workspace,
                repo,
                name,
                ..
            } => {
                this.ask_operation(workspace, repo, name, OperationMode::Merge, window, cx);
                if let Some(dialog) = this.dialog_of::<OperationDialog>() {
                    dialog.update(cx, |dialog, cx| dialog.choose_rebase(cx));
                }
            }
        }))
    }

    /// What the toolbar acts on for the active tab.
    fn toolbar_scope(&self, cx: &gpui_kit::App) -> Scope {
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
        for (at, (label, mode, options, needs_branch)) in toolbar_buttons().into_iter().enumerate()
        {
            let (enabled, reason) = button_state(&scope, at, label, needs_branch);
            let scope_for_click = self.scope_clone(&scope);
            let button = Button::new(gpui_kit::ElementId::Name(format!("toolbar-{at}").into()))
                .label(label)
                .disabled(!enabled)
                .tooltip(reason.clone())
                .on_click(cx.listener(move |this, _, window, cx| {
                    this.toolbar_run(scope_for_click.clone(), at, mode.clone(), window, cx);
                }));
            let Some(options) = options else {
                buttons.push(button.into_any_element());
                continue;
            };
            let scope_for_options = self.scope_clone(&scope);
            let options_label = text::options(label);
            // A button and its arrow stay together when the toolbar wraps.
            buttons.push(
                div()
                    .flex()
                    .flex_row()
                    .flex_shrink_0()
                    .gap_1()
                    .child(button)
                    .child(
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
                        .on_click(cx.listener(
                            move |this, _, window, cx| {
                                this.toolbar_options(
                                    scope_for_options.clone(),
                                    at,
                                    options.clone(),
                                    window,
                                    cx,
                                );
                            },
                        )),
                    )
                    .into_any_element(),
            );
        }
        buttons.push(
            Button::new("toolbar-palette")
                .label(text::PALETTE)
                .tooltip(text::PALETTE)
                .on_click(cx.listener(|this, _, window, cx| this.toggle_palette(window, cx)))
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
