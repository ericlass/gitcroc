//! The options of Fetch, Pull and Push on a target set (section 5 "Fetch",
//! "Pull" and "Push" rows, UI-20), a port of rc.1's
//! `ui/src/groups/GroupSyncDialog.tsx`: opened by the arrow beside each
//! toolbar button. Running goes through the preflight like a click does
//! (GRP-9): this dialog only picks the options. The target's name is shown
//! as text (SEC-3).

use gitcroc_core::{GroupOp, Prune, PullStrategy};
use gpui_component::button::Button;
use gpui_component::button::ButtonVariants;
use gpui_component::checkbox::Checkbox;
use gpui_component::{FocusTrapElement, Selectable};
use gpui_kit::{
    App, Context, EventEmitter, FocusHandle, Focusable, InteractiveElement, IntoElement,
    ParentElement, Render, Role, SharedString, Styled, TestSupportExt, Window, div,
};

use crate::dialog::{self, Cancel, Closed, Modal};
use crate::group_ops::SyncKind;
use crate::strings::group_ops::sync;

/// What the dialog does with the picked options.
pub type RunGroupOp = Box<dyn Fn(GroupOp, &mut App)>;

pub struct GroupSyncDialog {
    kind: SyncKind,
    target_name: String,
    prune: Prune,
    strategy: PullStrategy,
    set_upstream: bool,
    force: bool,
    run: RunGroupOp,
    focus: FocusHandle,
    cancel_focus: FocusHandle,
}

impl EventEmitter<Closed> for GroupSyncDialog {}

impl GroupSyncDialog {
    pub fn new(
        kind: SyncKind,
        target_name: String,
        run: RunGroupOp,
        cx: &mut Context<Self>,
    ) -> Self {
        Self {
            kind,
            target_name,
            prune: Prune::Config,
            strategy: PullStrategy::Config,
            set_upstream: true,
            force: false,
            run,
            focus: cx.focus_handle(),
            cancel_focus: cx.focus_handle(),
        }
    }

    fn op(&self) -> GroupOp {
        match self.kind {
            SyncKind::Fetch => GroupOp::Fetch { prune: self.prune },
            SyncKind::Pull => GroupOp::Pull {
                strategy: self.strategy,
            },
            SyncKind::Push => GroupOp::Push {
                set_upstream: self.set_upstream,
                force_with_lease: self.force,
            },
        }
    }

    fn submit(&mut self, _: &gpui_kit::ClickEvent, _: &mut Window, cx: &mut Context<Self>) {
        (self.run)(self.op(), cx);
        cx.emit(Closed);
    }

    fn cancel(&mut self, _: &gpui_kit::ClickEvent, _: &mut Window, cx: &mut Context<Self>) {
        cx.emit(Closed);
    }

    fn on_cancel(&mut self, _: &Cancel, _: &mut Window, cx: &mut Context<Self>) {
        cx.emit(Closed);
    }

    fn choices(
        &self,
        labels: &[&'static str],
        selected: usize,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let mut list = div().flex().flex_col().gap_1();
        for (at, label) in labels.iter().enumerate() {
            let is_selected = at == selected;
            list = list.child(
                Button::new(gpui_kit::ElementId::Name(
                    format!("group-sync-choice-{at}").into(),
                ))
                .label(*label)
                .selected(is_selected)
                .on_click(cx.listener(move |this, _, _, cx| {
                    match this.kind {
                        SyncKind::Fetch => {
                            this.prune = [Prune::Config, Prune::Prune, Prune::NoPrune][at];
                        }
                        SyncKind::Pull => {
                            this.strategy = [
                                PullStrategy::Config,
                                PullStrategy::FfOnly,
                                PullStrategy::Merge,
                                PullStrategy::Rebase,
                            ][at];
                        }
                        SyncKind::Push => {}
                    }
                    cx.notify();
                })),
            );
        }
        list
    }
}

impl Modal for GroupSyncDialog {
    fn focus_first(&self, window: &mut Window, cx: &mut App) {
        window.focus(&self.cancel_focus, cx);
    }
}

impl Focusable for GroupSyncDialog {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus.clone()
    }
}

impl Render for GroupSyncDialog {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let title: SharedString = sync::title(self.kind.name(), &self.target_name).into();
        let selected = match self.kind {
            SyncKind::Fetch => match self.prune {
                Prune::Config => 0,
                Prune::Prune => 1,
                Prune::NoPrune => 2,
            },
            SyncKind::Pull => match self.strategy {
                PullStrategy::Config => 0,
                PullStrategy::FfOnly => 1,
                PullStrategy::Merge => 2,
                PullStrategy::Rebase => 3,
            },
            SyncKind::Push => 0,
        };
        let mut body = div().id("group-sync-body").flex().flex_col().gap_2();
        match self.kind {
            SyncKind::Fetch => {
                body = body.child(sync::PRUNE.to_owned()).child(self.choices(
                    &[sync::PRUNE_CONFIG, sync::PRUNE_ON, sync::PRUNE_OFF],
                    selected,
                    cx,
                ));
            }
            SyncKind::Pull => {
                body = body
                    .child(sync::STRATEGY.to_owned())
                    .child(self.choices(
                        &[
                            sync::STRATEGY_CONFIG,
                            sync::FF_ONLY,
                            sync::MERGE,
                            sync::REBASE,
                        ],
                        selected,
                        cx,
                    ))
                    .child(div().id("group-sync-hint").child(sync::PULLS_OWN));
            }
            SyncKind::Push => {
                body = body
                    .child(
                        Checkbox::new("group-sync-upstream")
                            .label(sync::SET_UPSTREAM)
                            .checked(self.set_upstream)
                            .on_click(cx.listener(|this, value: &bool, _, cx| {
                                this.set_upstream = *value;
                                cx.notify();
                            })),
                    )
                    .child(
                        Checkbox::new("group-sync-force")
                            .label(sync::FORCE)
                            .checked(self.force)
                            .on_click(cx.listener(|this, value: &bool, _, cx| {
                                this.force = *value;
                                cx.notify();
                            })),
                    );
            }
        }
        dialog::frame("group-sync-frame", Role::Dialog, &title, cx)
            .test_support()
            .track_focus(&self.focus)
            .key_context(dialog::CONTEXT)
            .on_action(cx.listener(Self::on_cancel))
            .child(body)
            .child(
                dialog::actions_row()
                    .child(
                        Button::new("group-sync-cancel")
                            .label(crate::strings::common::CANCEL)
                            .track_focus(&self.cancel_focus)
                            .on_click(cx.listener(Self::cancel)),
                    )
                    .child(
                        Button::new("group-sync-submit")
                            .label(self.kind.name())
                            .primary()
                            .on_click(cx.listener(Self::submit)),
                    ),
            )
            // The trap takes the place of the frame, under the dialog's id.
            .focus_trap("group-sync", &self.focus)
    }
}
