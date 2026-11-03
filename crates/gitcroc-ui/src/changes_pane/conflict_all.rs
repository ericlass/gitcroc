//! Resolve all with AI as the changes pane sends it (AI-3, AI-13), a port
//! of `start` and `open` in rc.1's `stores/resolveAll.ts`. The banner asks
//! through the workspace's [`ResolveAll`]; the pane has the repo's
//! unmerged list and its kept editors, so it names every file with an
//! editor and its untouched hunks (OPS-9), and the core reads the rest,
//! decides what is sent (AI-4) and runs the files. When the repo's lists are
//! not shown, its uncommitted changes are selected first and the request
//! goes once they are read. Each file's result reaches its kept editor as
//! Resolve with AI's does; a file opened later reads its own.

use gitcroc_app::app::AppError;
use gitcroc_core::{
    AiRefused, AiResolveAllFile, AiResolveAllRequest, AiResolveAllStart, ChangesContent, RepoId,
};
use gpui_kit::Context;

use super::{AskAi, ChangesPane};
use crate::changes::{FileKey, List};
use crate::conflict::EditorKey;
use crate::history::Selection;
use crate::resolve_all::ResolveAllAsk;

/// A repo's unmerged list as the shown view names it.
struct Unmerged {
    handle: u32,
    /// Its files' ids and paths.
    files: Vec<(usize, String)>,
    /// The repo as the view's file keys name it: `None` in a repo's own
    /// view, the repo in a merged one (UI-34).
    key_repo: Option<RepoId>,
}

impl ChangesPane {
    /// `repo`'s unmerged list, when the shown view has it read.
    fn unmerged_of(&self, repo: &RepoId) -> Option<Unmerged> {
        if self.changes.loading() {
            return None;
        }
        let view = self.changes.view()?;
        let (files, key_repo) = match &view.content {
            ChangesContent::Uncommitted { unmerged, .. } => match self.changes.selection()? {
                Selection::Uncommitted { repo: shown } if shown == repo => (unmerged, None),
                _ => return None,
            },
            ChangesContent::Merged { repos, .. } => (
                &repos.iter().find(|shown| &shown.repo_id == repo)?.unmerged,
                Some(repo.clone()),
            ),
            ChangesContent::Commit { .. } => return None,
        };
        Some(Unmerged {
            handle: self.changes.handle_of(Some(repo))?,
            files: files
                .iter()
                .map(|file| (file.id, file.path.clone()))
                .collect(),
            key_repo,
        })
    }

    /// What the banner asked: wait for `repo`'s lists if they are not
    /// shown, selecting its uncommitted changes, then act.
    pub(super) fn resolve_all_ask(&mut self, ask: &ResolveAllAsk, cx: &mut Context<Self>) {
        if !self.ai.read(cx).enabled() {
            return;
        }
        let repo = match ask {
            ResolveAllAsk::Start { repo } => {
                self.resolve_all
                    .update(cx, |runs, cx| runs.starting(repo, cx));
                repo
            }
            ResolveAllAsk::Open { repo, .. } => repo,
        };
        if self.unmerged_of(repo).is_some() {
            self.act_on(ask.clone(), cx);
            return;
        }
        let selected = self
            .history
            .update(cx, |history, cx| history.select_uncommitted(repo, cx));
        if selected {
            self.waiting_lists = Some(ask.clone());
        } else if let ResolveAllAsk::Start { repo } = ask {
            self.resolve_all
                .update(cx, |runs, cx| runs.settled(repo, None, cx));
        }
    }

    /// The view was read: a request waiting for its lists goes now, or
    /// gives up when they are not in it.
    pub(super) fn lists_read(&mut self, cx: &mut Context<Self>) {
        let Some(ask) = self.waiting_lists.take() else {
            return;
        };
        let repo = match &ask {
            ResolveAllAsk::Start { repo } | ResolveAllAsk::Open { repo, .. } => repo.clone(),
        };
        if self.unmerged_of(&repo).is_some() {
            self.act_on(ask, cx);
        } else if matches!(ask, ResolveAllAsk::Start { .. }) {
            self.resolve_all
                .update(cx, |runs, cx| runs.settled(&repo, None, cx));
        }
    }

    fn act_on(&mut self, ask: ResolveAllAsk, cx: &mut Context<Self>) {
        match ask {
            ResolveAllAsk::Start { repo } => self.resolve_all_with_ai(&repo, false, cx),
            ResolveAllAsk::Open { repo, path } => self.open_unmerged(&repo, &path, cx),
        }
    }

    /// Sends Resolve all with AI for `repo` (AI-3, AI-13): the files with a
    /// kept editor go with their untouched hunks (OPS-9), every other with
    /// all of its. `consented` once the consent dialog allowed it (AI-6).
    pub fn resolve_all_with_ai(&mut self, repo: &RepoId, consented: bool, cx: &mut Context<Self>) {
        if !self.ai.read(cx).enabled() {
            return;
        }
        let Some(unmerged) = self.unmerged_of(repo) else {
            self.resolve_all
                .update(cx, |runs, cx| runs.settled(repo, None, cx));
            return;
        };
        let files = unmerged
            .files
            .iter()
            .filter_map(|(id, path)| {
                let key = EditorKey {
                    workspace: self.workspace_id.clone(),
                    repo: repo.clone(),
                    path: path.clone(),
                };
                let editor = self.conflicts.get(&key)?.editor.clone()?;
                Some(AiResolveAllFile {
                    file: u32::try_from(*id).ok()?,
                    hunks: editor.read(cx).untouched_hunks(cx),
                })
            })
            .collect();
        self.resolve_all
            .update(cx, |runs, cx| runs.starting(repo, cx));
        let workspace = self.workspace_id.clone();
        let request = AiResolveAllRequest {
            repo: repo.clone(),
            handle: unmerged.handle,
            files,
        };
        let call = self
            .services
            .with_app(move |app| async move { app.ai_resolve_all(workspace, request).await });
        let repo = repo.clone();
        cx.spawn(async move |this, cx| {
            let result = call.await;
            let _ = this.update(cx, |this, cx| {
                this.all_started(&repo, consented, result, cx)
            });
        })
        .detach();
    }

    /// What `ai_resolve_all` answered for `repo`.
    fn all_started(
        &mut self,
        repo: &RepoId,
        consented: bool,
        result: Option<Result<Option<AiResolveAllStart>, AppError>>,
        cx: &mut Context<Self>,
    ) {
        let refusal = match result {
            Some(Ok(Some(AiResolveAllStart::Started { job_id }))) => {
                self.resolve_all
                    .update(cx, |runs, cx| runs.started(repo, job_id, cx));
                return;
            }
            Some(Ok(Some(AiResolveAllStart::AlreadyResolved))) | Some(Ok(None)) => {
                self.reload(cx);
                None
            }
            Some(Ok(Some(AiResolveAllStart::Refused { refused }))) => match refused {
                AiRefused::NoProvider => {
                    cx.emit(AskAi::Settings);
                    None
                }
                // The dialog is asked for once, before the run.
                AiRefused::ConsentNeeded { consent } if !consented => {
                    cx.emit(AskAi::ConsentAll {
                        request: consent,
                        repo: repo.clone(),
                    });
                    None
                }
                refused => Some(super::conflict_ai::refusal_text(&refused)),
            },
            Some(Err(error)) => Some(error.to_string()),
            None => Some(super::conflicts::stopped()),
        };
        self.resolve_all
            .update(cx, |runs, cx| runs.settled(repo, refusal, cx));
    }

    /// Selects unmerged `path` of `repo`, which opens it in the conflict
    /// editor with its proposal (AI-13).
    fn open_unmerged(&mut self, repo: &RepoId, path: &str, cx: &mut Context<Self>) {
        let Some(unmerged) = self.unmerged_of(repo) else {
            return;
        };
        let Some((id, _)) = unmerged.files.iter().find(|(_, shown)| shown == path) else {
            return;
        };
        self.changes.select(FileKey {
            list: List::Unmerged,
            id: *id,
            repo: unmerged.key_repo,
        });
        self.scroll_to_selected();
        self.sync_diff(cx);
        cx.notify();
    }
}
