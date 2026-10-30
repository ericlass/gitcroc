//! rc.1's operation dialogs. Forms own only transient input; the core
//! supplies choices, validates names and decides when confirmation is needed.
mod form;
mod send;
use crate::{
    dialog::{Closed, Modal},
    services::Services,
    strings::operations as text,
};
use gitcroc_core::{
    IntegrateOp, PushChoices, RemoteOp, RepoId, ResetMode, StashOp, SyncOp, TagOp, WorkspaceId,
};
use gpui_component::input::{InputEvent, InputState, TextareaState};
use gpui_kit::{
    App, AppContext, Context, Entity, EventEmitter, FocusHandle, Focusable, SharedString,
    Subscription, Window,
};

#[derive(Debug, Clone)]
pub enum Operation {
    Sync(SyncOp),
    Integrate(IntegrateOp),
    Stash(StashOp),
    Tag(TagOp),
    Reset { commit: String, mode: ResetMode },
    Remote(RemoteOp),
}
#[derive(Debug, Clone)]
pub enum OperationMode {
    Fetch,
    Pull,
    Push,
    Merge,
    StashPush,
    TagCreate { commit: Option<String> },
    TagDelete { name: String },
    Reset { commit: String, mode: ResetMode },
    RemoteAdd,
    RemoteRename { name: String },
    RemoteUrl { name: String },
    Run(Operation),
}
impl OperationMode {
    pub fn immediate(&self) -> bool {
        matches!(self, Self::Run(_))
    }
}
/// A started operation, used to follow retained stashes even after the dialog closes.
#[derive(Clone)]
pub struct Started {
    pub workspace: WorkspaceId,
    pub repo: RepoId,
    pub job: gitcroc_core::JobId,
    pub op: Operation,
}

pub struct OperationDialog {
    services: Services,
    workspace: WorkspaceId,
    repo: RepoId,
    repo_name: String,
    head_name: Option<String>,
    mode: OperationMode,
    name: Entity<InputState>,
    revision: Entity<InputState>,
    message: Entity<TextareaState>,
    url: Entity<InputState>,
    push_url: Entity<InputState>,
    choice: usize,
    flag: bool,
    secondary: bool,
    push_choices: Option<PushChoices>,
    urls_loaded: bool,
    has_push_url: bool,
    targets: Vec<String>,
    filter_generation: u64,
    remotes: Option<(Operation, Vec<String>)>,
    picked: Option<usize>,
    parents: Option<(String, Vec<String>)>,
    confirm: Option<(Operation, String, Vec<String>)>,
    error: Option<SharedString>,
    read_error: Option<SharedString>,
    name_error: Option<SharedString>,
    validating: bool,
    validation: u64,
    sending: bool,
    focus: FocusHandle,
    cancel_focus: FocusHandle,
    _inputs: Vec<Subscription>,
}
impl EventEmitter<Closed> for OperationDialog {}
impl EventEmitter<Started> for OperationDialog {}
impl OperationDialog {
    pub fn new(
        services: Services,
        workspace: WorkspaceId,
        repo: RepoId,
        repo_name: String,
        mode: OperationMode,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let initial = if let OperationMode::RemoteRename { name } = &mode {
            name.clone()
        } else {
            String::new()
        };
        let name = cx.new(|cx| InputState::new(window, cx).default_value(initial));
        let revision = cx.new(|cx| InputState::new(window, cx));
        let message = cx.new(|cx| TextareaState::new(window, cx).rows(3));
        let url = cx.new(|cx| InputState::new(window, cx));
        let push_url = cx.new(|cx| InputState::new(window, cx));
        let mut inputs = Vec::new();
        for field in [&name, &revision, &url, &push_url] {
            inputs.push(cx.subscribe_in(
                field,
                window,
                |this, input, event, window, cx| match event {
                    InputEvent::Change if *input == this.name => this.check_name(cx),
                    InputEvent::Change if *input == this.revision => this.filter(cx),
                    InputEvent::Change => cx.notify(),
                    InputEvent::PressEnter { .. } if this.confirm.is_none() => {
                        this.submit(window, cx)
                    }
                    _ => {}
                },
            ));
        }
        inputs.push(cx.subscribe(&message, |_, _, _, cx| cx.notify()));
        let choice = match mode {
            OperationMode::Reset {
                mode: ResetMode::Mixed,
                ..
            } => 1,
            OperationMode::Reset {
                mode: ResetMode::Hard,
                ..
            } => 2,
            _ => 0,
        };
        let flag = matches!(mode, OperationMode::Push | OperationMode::TagCreate { .. });
        let mut this = Self {
            services,
            workspace,
            repo,
            repo_name,
            head_name: None,
            mode,
            name,
            revision,
            message,
            url,
            push_url,
            choice,
            flag,
            secondary: false,
            push_choices: None,
            urls_loaded: false,
            has_push_url: false,
            targets: Vec::new(),
            filter_generation: 0,
            remotes: None,
            picked: None,
            parents: None,
            confirm: None,
            error: None,
            read_error: None,
            name_error: None,
            validating: false,
            validation: 0,
            sending: false,
            focus: cx.focus_handle(),
            cancel_focus: cx.focus_handle(),
            _inputs: inputs,
        };
        this.check_name(cx);
        this.load(window, cx);
        this
    }
    fn checks_name(&self) -> bool {
        matches!(
            self.mode,
            OperationMode::TagCreate { .. }
                | OperationMode::RemoteAdd
                | OperationMode::RemoteRename { .. }
        )
    }
    fn check_name(&mut self, cx: &mut Context<Self>) {
        if !self.checks_name() {
            return;
        }
        self.validation += 1;
        let generation = self.validation;
        self.validating = true;
        let name = self.name.read(cx).value().to_string();
        let tag = matches!(self.mode, OperationMode::TagCreate { .. });
        let call = self.services.with_app(move |app| async move {
            if tag {
                app.tag_name_check(&name)
            } else {
                app.remote_name_check(&name)
            }
        });
        cx.spawn(async move |this, cx| {
            let result = call.await;
            let _ = this.update(cx, |this, cx| {
                if generation == this.validation {
                    this.validating = false;
                    this.name_error = match result {
                        Some(error) => error.map(Into::into),
                        None => Some(text::STOPPED.into()),
                    };
                    cx.notify();
                }
            });
        })
        .detach();
        cx.notify();
    }
    fn load(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if matches!(
            self.mode,
            OperationMode::Merge | OperationMode::Reset { .. }
        ) {
            let workspace = self.workspace.clone();
            let repo = self.repo.clone();
            let call = self.services.with_app(move |app| async move {
                app.workspace_summaries(&workspace)
                    .ok()
                    .and_then(|views| views.into_iter().find(|v| v.repo_id == repo))
                    .and_then(|v| v.summary)
                    .and_then(|s| match s.head {
                        gitcroc_core::Head::Branch { name, .. } => Some(name),
                        _ => None,
                    })
            });
            cx.spawn(async move |this, cx| {
                let result = call.await;
                let _ = this.update(cx, |this, cx| {
                    this.head_name = result.flatten();
                    cx.notify();
                });
            })
            .detach();
        }
        match self.mode.clone() {
            OperationMode::Push => {
                let workspace = self.workspace.clone();
                let repo = self.repo.clone();
                let call = self
                    .services
                    .with_app(move |app| async move { app.push_choices(workspace, repo).await });
                cx.spawn(async move |this, cx| {
                    let result = call.await;
                    let _ = this.update(cx, |this, cx| {
                        match result {
                            Some(Ok(choices)) => this.push_choices = Some(choices),
                            Some(Err(e)) => this.read_error = Some(e.to_string().into()),
                            None => this.read_error = Some(text::STOPPED.into()),
                        };
                        cx.notify();
                    });
                })
                .detach();
            }
            OperationMode::RemoteUrl { name } => {
                let workspace = self.workspace.clone();
                let repo = self.repo.clone();
                let call = self.services.with_app(move |app| async move {
                    app.remote_urls(workspace, repo, name).await
                });
                cx.spawn_in(window, async move |this, cx| {
                    let result = call.await;
                    let _ = this.update_in(cx, |this, window, cx| {
                        match result {
                            Some(Ok(urls)) => {
                                this.has_push_url = urls.push_url.is_some();
                                this.url.update(cx, |input, cx| {
                                    input.set_value(urls.url.unwrap_or_default(), window, cx)
                                });
                                this.push_url.update(cx, |input, cx| {
                                    input.set_value(urls.push_url.unwrap_or_default(), window, cx)
                                });
                                this.urls_loaded = true;
                            }
                            Some(Err(e)) => this.read_error = Some(e.to_string().into()),
                            None => this.read_error = Some(text::STOPPED.into()),
                        };
                        cx.notify();
                    });
                })
                .detach();
            }
            OperationMode::Merge => self.filter(cx),
            _ => {}
        }
    }
    fn filter(&mut self, cx: &mut Context<Self>) {
        self.filter_generation += 1;
        let generation = self.filter_generation;
        let filter = self.revision.read(cx).value().to_string();
        let workspace = self.workspace.clone();
        let repo = self.repo.clone();
        let call = self
            .services
            .with_app(move |app| async move { app.merge_targets(workspace, repo, filter).await });
        cx.spawn(async move |this, cx| {
            let result = call.await;
            let _ = this.update(cx, |this, cx| {
                if generation == this.filter_generation {
                    match result {
                        Some(Ok(targets)) => {
                            this.targets = targets;
                            this.read_error = None;
                        }
                        Some(Err(e)) => this.read_error = Some(e.to_string().into()),
                        None => this.read_error = Some(text::STOPPED.into()),
                    };
                    cx.notify();
                }
            });
        })
        .detach();
    }
    fn operation(&self, cx: &App) -> Option<Operation> {
        if self.sending {
            return None;
        }
        if let Some((op, _, _)) = &self.confirm {
            return Some(op.clone());
        }
        if let Some((op, remotes)) = &self.remotes {
            let remote = remotes.get(self.picked?)?.clone();
            let mut op = op.clone();
            match &mut op {
                Operation::Tag(
                    TagOp::Create { remote: picked, .. }
                    | TagOp::Push { remote: picked, .. }
                    | TagOp::Delete { remote: picked, .. },
                ) => *picked = Some(remote),
                Operation::Sync(SyncOp::Push { remote: picked, .. }) => *picked = Some(remote),
                _ => return None,
            }
            return Some(op);
        }
        if let Some((commit, _)) = &self.parents {
            return Some(Operation::Integrate(IntegrateOp::Revert {
                commit: commit.clone(),
                mainline: Some(self.choice as u32 + 1),
            }));
        }
        let name = self.name.read(cx).value().to_string();
        if self.checks_name() && (name.is_empty() || self.validating || self.name_error.is_some()) {
            return None;
        }
        let message = self.message.read(cx).value().to_string();
        Some(match &self.mode {
            OperationMode::Fetch => Operation::Sync(SyncOp::Fetch {
                prune: [
                    gitcroc_core::Prune::Config,
                    gitcroc_core::Prune::Prune,
                    gitcroc_core::Prune::NoPrune,
                ][self.choice],
            }),
            OperationMode::Pull => Operation::Sync(SyncOp::Pull {
                strategy: [
                    gitcroc_core::PullStrategy::Config,
                    gitcroc_core::PullStrategy::FfOnly,
                    gitcroc_core::PullStrategy::Merge,
                    gitcroc_core::PullStrategy::Rebase,
                ][self.choice],
            }),
            OperationMode::Push => {
                let choices = self.push_choices.as_ref()?;
                let remote = self
                    .picked
                    .and_then(|at| choices.remotes.get(at).cloned())
                    .or_else(|| choices.remote.clone());
                if choices.upstream.is_none() && remote.is_none() {
                    return None;
                }
                Operation::Sync(SyncOp::Push {
                    set_upstream: self.flag,
                    remote: if choices.upstream.is_some() {
                        None
                    } else {
                        remote
                    },
                    force_with_lease: self.secondary,
                })
            }
            OperationMode::Merge => {
                let revision = self.revision.read(cx).value().trim().to_owned();
                if revision.is_empty() {
                    return None;
                }
                Operation::Integrate(if self.flag {
                    IntegrateOp::Rebase { onto: revision }
                } else {
                    IntegrateOp::Merge {
                        revision,
                        fast_forward: [
                            gitcroc_core::MergeFastForward::Config,
                            gitcroc_core::MergeFastForward::FfOnly,
                            gitcroc_core::MergeFastForward::NoFf,
                        ][self.choice],
                    }
                })
            }
            OperationMode::StashPush => Operation::Stash(StashOp::Push {
                message: (!message.trim().is_empty()).then_some(message),
                include_untracked: self.flag,
            }),
            OperationMode::TagCreate { commit } => {
                if self.flag && message.trim().is_empty() {
                    return None;
                }
                Operation::Tag(TagOp::Create {
                    name,
                    commit: commit.clone(),
                    annotated: self.flag,
                    message: if self.flag { message } else { String::new() },
                    push: self.secondary,
                    remote: None,
                })
            }
            OperationMode::TagDelete { name } => Operation::Tag(TagOp::Delete {
                name: name.clone(),
                on_remote: self.flag,
                remote: None,
            }),
            OperationMode::Reset { commit, .. } => Operation::Reset {
                commit: commit.clone(),
                mode: [ResetMode::Soft, ResetMode::Mixed, ResetMode::Hard][self.choice],
            },
            OperationMode::RemoteAdd => {
                let url = self.url.read(cx).value().trim().to_owned();
                if url.is_empty() {
                    return None;
                }
                Operation::Remote(RemoteOp::Add {
                    name,
                    url,
                    fetch: self.flag,
                })
            }
            OperationMode::RemoteRename { name: old } => {
                if *old == name {
                    return None;
                }
                Operation::Remote(RemoteOp::Rename {
                    name: old.clone(),
                    new_name: name,
                })
            }
            OperationMode::RemoteUrl { name } => {
                let url = self.url.read(cx).value().trim().to_owned();
                if !self.urls_loaded || url.is_empty() {
                    return None;
                }
                let push_url = self
                    .has_push_url
                    .then(|| self.push_url.read(cx).value().trim().to_owned());
                if push_url.as_ref().is_some_and(|url| url.is_empty()) {
                    return None;
                }
                Operation::Remote(RemoteOp::SetUrl {
                    name: name.clone(),
                    url,
                    push_url,
                })
            }
            OperationMode::Run(op) => op.clone(),
        })
    }
}
impl Modal for OperationDialog {
    fn focus_first(&self, window: &mut Window, cx: &mut App) {
        let input = match self.mode {
            OperationMode::TagCreate { .. }
            | OperationMode::RemoteAdd
            | OperationMode::RemoteRename { .. } => Some(&self.name),
            OperationMode::Merge => Some(&self.revision),
            _ => None,
        };
        if matches!(self.mode, OperationMode::StashPush) {
            self.message.update(cx, |input, cx| input.focus(window, cx));
            return;
        }
        if let Some(input) = input {
            input.update(cx, |input, cx| input.focus(window, cx));
        } else {
            window.focus(&self.cancel_focus, cx);
        }
    }
}
impl Focusable for OperationDialog {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus.clone()
    }
}
