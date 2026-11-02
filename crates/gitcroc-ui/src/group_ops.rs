//! A group operation as the user follows it (section 5), a port of rc.1's
//! `ui/src/stores/groupOps.ts`: the flow asks the core for a preflight,
//! shows its dialog when the core says the user must confirm, and starts
//! the job, confirmed or at once. It decides nothing (ARCH-6): the classes,
//! whether to confirm and Retry failed's target set (GRP-11, GRP-5) are the
//! core's. One preflight is open at a time; the one open belongs to the
//! workspace it started in (ARCH-9). Cancel drops it, before or after its
//! result.

use std::collections::HashMap;

use gitcroc_app::app::AppError;
use gitcroc_core::{
    GroupOp, JobId, PreflightId, PreflightReady, Prune, PullStrategy, TargetSet, WorkspaceId,
};
use gpui_kit::{App, Context, SharedString};

use crate::services::{Call, Services};
use crate::strings::group_ops::sync;
use crate::strings::groups::STOPPED;

/// The group operations of the toolbar's Fetch, Pull and Push (UI-20).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SyncKind {
    Fetch,
    Pull,
    Push,
}

impl SyncKind {
    /// What the operation is called, as its dialog's title starts.
    pub fn name(self) -> &'static str {
        match self {
            Self::Fetch => sync::FETCH,
            Self::Pull => sync::PULL,
            Self::Push => sync::PUSH,
        }
    }

    /// The defaults of section 5 for a click on the toolbar: Prune as git
    /// config says, the pull strategy of git config, set upstream on and no
    /// force.
    pub fn default_op(self) -> GroupOp {
        match self {
            Self::Fetch => GroupOp::Fetch {
                prune: Prune::Config,
            },
            Self::Pull => GroupOp::Pull {
                strategy: PullStrategy::Config,
            },
            Self::Push => GroupOp::Push {
                set_upstream: true,
                force_with_lease: false,
            },
        }
    }
}

/// A group operation a view asks the window to run through the preflight,
/// as a group branch's double-click does (UI-5).
#[derive(Debug, Clone)]
pub struct RunGroup {
    pub workspace: WorkspaceId,
    pub target: TargetSet,
    pub op: GroupOp,
}

/// A group checkout of branch `name` on `group`'s members, with the skip
/// policy (UI-5).
pub fn group_checkout(group: gitcroc_core::GroupId, name: String) -> (TargetSet, GroupOp) {
    (
        TargetSet::Group { group_id: group },
        GroupOp::Checkout {
            name,
            missing: gitcroc_core::MissingBranch::Skip,
        },
    )
}

/// The preflight the user follows, as its dialog shows it.
#[derive(Debug, Clone)]
pub struct PendingPreflight {
    pub workspace_id: WorkspaceId,
    /// `None` until the core answered the request.
    pub preflight_id: Option<PreflightId>,
    /// The core's classes, once the preflight finished and the user must
    /// confirm.
    pub ready: Option<PreflightReady>,
    /// The job is being started.
    pub starting: bool,
    /// Why the preflight or the start was refused.
    pub error: Option<SharedString>,
}

/// Told the job's id when the operation it was given with starts, as the
/// group commit box is, to follow its job (UI-34).
pub type Started = Box<dyn FnOnce(JobId, &mut App)>;

/// The group operation flow of the window.
pub struct GroupOps {
    services: Services,
    pending: Option<PendingPreflight>,
    /// Told when the pending operation's job starts.
    started: Option<Started>,
    /// Results that arrived before their request's answer, by preflight.
    early: HashMap<PreflightId, PreflightReady>,
    /// Counts requests, so an answer to one the user left is dropped.
    request: u64,
}

impl GroupOps {
    pub fn new(services: Services, _: &mut Context<Self>) -> Self {
        Self {
            services,
            pending: None,
            started: None,
            early: HashMap::new(),
            request: 0,
        }
    }

    /// The preflight the user follows, if any.
    pub fn pending(&self) -> Option<&PendingPreflight> {
        self.pending.as_ref()
    }

    /// Runs `op` on `target` in `workspace`, after the preflight dialog
    /// when the core asks for it (section 5, steps 1 and 2).
    pub fn run(
        &mut self,
        workspace: WorkspaceId,
        target: TargetSet,
        op: GroupOp,
        cx: &mut Context<Self>,
    ) {
        let asked_workspace = workspace.clone();
        let asked = self.services.with_app(move |app| async move {
            app.group_preflight(asked_workspace, target, op).await
        });
        self.begin(workspace, asked, cx);
    }

    /// Runs `op` as [`Self::run`] does, and tells `started` its job's id
    /// when it starts; never when the user cancels.
    pub fn run_then(
        &mut self,
        workspace: WorkspaceId,
        target: TargetSet,
        op: GroupOp,
        started: Started,
        cx: &mut Context<Self>,
    ) {
        self.run(workspace, target, op, cx);
        self.started = Some(started);
    }

    /// Retry failed (GRP-11) of ended group job `job`; the core takes its
    /// failed repos from the report, never from the UI (GRP-5). `workspace`
    /// is the one the job ran in.
    pub fn retry(&mut self, workspace: WorkspaceId, job: JobId, cx: &mut Context<Self>) {
        let asked = self
            .services
            .with_app(move |app| async move { app.group_retry_failed(job).await });
        self.begin(workspace, asked, cx);
    }

    /// The user confirmed the dialog (GRP-9).
    pub fn confirm(&mut self, cx: &mut Context<Self>) {
        let Some(ready) = self
            .pending
            .as_ref()
            .and_then(|pending| pending.ready.clone())
        else {
            return;
        };
        self.start(ready.preflight_id, true, cx);
    }

    /// The user cancelled the dialog: the core drops the preflight, also
    /// when its result arrives after this.
    pub fn cancel(&mut self, cx: &mut Context<Self>) {
        let id = self.pending.take().and_then(|pending| pending.preflight_id);
        self.started = None;
        self.request += 1;
        self.early.clear();
        if let Some(id) = id {
            self.services.app().group_drop(id);
        }
        cx.notify();
    }

    /// A preflight finished: the dialog shows when the core says so, else
    /// the job starts at once.
    pub fn on_ready(&mut self, ready: PreflightReady, cx: &mut Context<Self>) {
        let Some(pending) = self.pending.as_mut() else {
            return;
        };
        if pending.preflight_id.is_none() {
            // The result came before the request's answer.
            self.early.insert(ready.preflight_id, ready);
            return;
        }
        if pending.preflight_id != Some(ready.preflight_id) {
            return;
        }
        if ready.needs_confirmation {
            pending.ready = Some(ready);
            cx.notify();
        } else {
            self.start(ready.preflight_id, false, cx);
        }
    }

    fn begin(
        &mut self,
        workspace: WorkspaceId,
        asked: Call<Result<PreflightId, AppError>>,
        cx: &mut Context<Self>,
    ) {
        self.request += 1;
        let mine = self.request;
        self.early.clear();
        self.started = None;
        self.pending = Some(PendingPreflight {
            workspace_id: workspace,
            preflight_id: None,
            ready: None,
            starting: false,
            error: None,
        });
        cx.notify();
        cx.spawn(async move |this, cx| {
            let answer = asked.await;
            let _ = this.update(cx, |this, cx| this.answered(mine, answer, cx));
        })
        .detach();
    }

    fn answered(
        &mut self,
        mine: u64,
        answer: Option<Result<PreflightId, AppError>>,
        cx: &mut Context<Self>,
    ) {
        if mine != self.request {
            // Cancelled before the core answered: its preflight is dropped.
            if let Some(Ok(id)) = answer {
                self.services.app().group_drop(id);
            }
            return;
        }
        match answer {
            Some(Ok(id)) => {
                let Some(pending) = self.pending.as_mut() else {
                    // The answer belongs to a preflight the user left.
                    self.services.app().group_drop(id);
                    return;
                };
                pending.preflight_id = Some(id);
                let early = self.early.remove(&id);
                self.early.clear();
                cx.notify();
                if let Some(ready) = early {
                    self.on_ready(ready, cx);
                }
            }
            Some(Err(error)) => {
                if let Some(pending) = self.pending.as_mut() {
                    pending.error = Some(error.message.into());
                }
                cx.notify();
            }
            None => {
                if let Some(pending) = self.pending.as_mut() {
                    pending.error = Some(STOPPED.into());
                }
                cx.notify();
            }
        }
    }

    fn start(&mut self, id: PreflightId, confirmed: bool, cx: &mut Context<Self>) {
        let Some(pending) = self.pending.as_mut() else {
            return;
        };
        let mine = self.request;
        pending.starting = true;
        pending.error = None;
        cx.notify();
        let call = self
            .services
            .with_app(move |app| async move { app.group_start(id, confirmed).await });
        cx.spawn(async move |this, cx| {
            let answer = call.await;
            let _ = this.update(cx, |this, cx| {
                if mine != this.request {
                    return;
                }
                match answer {
                    Some(Ok(job)) => {
                        this.pending = None;
                        if let Some(started) = this.started.take() {
                            started(job, cx);
                        }
                    }
                    Some(Err(error)) => {
                        if let Some(pending) = this.pending.as_mut() {
                            pending.starting = false;
                            pending.error = Some(error.message.into());
                        }
                    }
                    None => {
                        if let Some(pending) = this.pending.as_mut() {
                            pending.starting = false;
                            pending.error = Some(STOPPED.into());
                        }
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }
}
