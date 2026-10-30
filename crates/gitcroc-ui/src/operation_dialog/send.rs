use super::*;
use gitcroc_core::{
    IntegrateRequest, IntegrateStart, RemoteRequest, RemoteStart, ResetRequest, ResetStart,
    StashRequest, StashStart, SyncRequest, SyncStart, TagRequest, TagStart,
};
enum Reply {
    Started(gitcroc_core::JobId),
    Confirm(String, Vec<String>),
    Remote(Vec<String>),
    Mainline(String, Vec<String>),
}
impl OperationDialog {
    pub fn submit(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(op) = self.operation(cx) else {
            return;
        };
        let sent = op.clone();
        let confirmed = self.confirm.is_some();
        self.sending = true;
        self.error = None;
        let workspace = self.workspace.clone();
        let repo = self.repo.clone();
        let call = self.services.with_app(move |app| async move {
            Ok::<_, gitcroc_app::app::AppError>(match sent {
                Operation::Sync(op) => match app
                    .op_sync(
                        workspace,
                        SyncRequest {
                            repo_id: repo,
                            op,
                            confirmed,
                        },
                    )
                    .await?
                {
                    SyncStart::Started { job_id } => Reply::Started(job_id),
                    SyncStart::Confirm { repo, actions } => Reply::Confirm(repo, actions),
                    SyncStart::ChooseRemote { remotes } => Reply::Remote(remotes),
                },
                Operation::Integrate(op) => match app
                    .op_integrate(
                        workspace,
                        IntegrateRequest {
                            repo_id: repo,
                            op,
                            confirmed,
                        },
                    )
                    .await?
                {
                    IntegrateStart::Started { job_id } => Reply::Started(job_id),
                    IntegrateStart::Confirm { repo, actions } => Reply::Confirm(repo, actions),
                    IntegrateStart::ChooseMainline { commit, parents } => {
                        Reply::Mainline(commit, parents)
                    }
                },
                Operation::Stash(op) => match app
                    .op_stash(
                        workspace,
                        StashRequest {
                            repo_id: repo,
                            op,
                            confirmed,
                        },
                    )
                    .await?
                {
                    StashStart::Started { job_id } => Reply::Started(job_id),
                    StashStart::Confirm { repo, actions } => Reply::Confirm(repo, actions),
                },
                Operation::Tag(op) => match app
                    .op_tag(
                        workspace,
                        TagRequest {
                            repo_id: repo,
                            op,
                            confirmed,
                        },
                    )
                    .await?
                {
                    TagStart::Started { job_id } => Reply::Started(job_id),
                    TagStart::Confirm { repo, actions } => Reply::Confirm(repo, actions),
                    TagStart::ChooseRemote { remotes } => Reply::Remote(remotes),
                },
                Operation::Reset { commit, mode } => match app
                    .op_reset(
                        workspace,
                        ResetRequest {
                            repo_id: repo,
                            commit,
                            mode,
                            confirmed,
                        },
                    )
                    .await?
                {
                    ResetStart::Started { job_id } => Reply::Started(job_id),
                    ResetStart::Confirm { repo, actions } => Reply::Confirm(repo, actions),
                },
                Operation::Remote(op) => match app
                    .op_remote(
                        workspace,
                        RemoteRequest {
                            repo_id: repo,
                            op,
                            confirmed,
                        },
                    )
                    .await?
                {
                    RemoteStart::Started { job_id } => Reply::Started(job_id),
                    RemoteStart::Confirm { repo, actions } => Reply::Confirm(repo, actions),
                },
            })
        });
        cx.spawn_in(window, async move |this, cx| {
            let result = call.await;
            let _ = this.update_in(cx, |this, window, cx| {
                this.sending = false;
                match result {
                    Some(Ok(Reply::Started(job))) => {
                        cx.emit(Started {
                            workspace: this.workspace.clone(),
                            repo: this.repo.clone(),
                            job,
                            op,
                        });
                        cx.emit(Closed);
                    }
                    Some(Ok(Reply::Confirm(repo, actions))) => {
                        this.confirm = Some((op, repo, actions));
                        window.focus(&this.cancel_focus, cx);
                    }
                    Some(Ok(Reply::Remote(remotes))) => {
                        this.confirm = None;
                        this.remotes = Some((op, remotes));
                        this.picked = None;
                        window.focus(&this.cancel_focus, cx);
                    }
                    Some(Ok(Reply::Mainline(commit, parents))) => {
                        this.parents = Some((commit, parents));
                        this.choice = 0;
                        window.focus(&this.cancel_focus, cx);
                    }
                    Some(Err(error)) => this.error = Some(error.to_string().into()),
                    None => this.error = Some(text::STOPPED.into()),
                }
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }
}
