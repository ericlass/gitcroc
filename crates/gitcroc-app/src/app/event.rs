//! What the app tells its UI (ARCH-6): one [`AppEvent`] per Tauri event the
//! old shell sent, on one broadcast channel, and the [`Hub`] that the core's
//! sinks send through.

use std::path::PathBuf;
use std::sync::Arc;

use gitcroc_core::{
    AddReport, AiProposalReady, CloneReport, ExportReport, GitRunner, GitStatus, GroupId,
    GroupSummary, ImportReport, JobId, JobView, PreflightReady, RepoId, Summaries, SummaryView,
    Trees, UpdateView, WatchWarning, WorkspaceId, WorkspaceTabs, WorkspaceView, Workspaces,
    group_summaries, refresh_summaries,
};
use tokio::sync::broadcast;

use crate::ai::{AiModelsReply, AiSelfTestReply, AiView};
use crate::askpass::AskpassPrompt;

/// How many events a subscriber may fall behind before it gets `Lagged` and
/// has to read the state again (ARCH-6). A startup refresh sends one
/// `RepoSummary` per repo, and a group job one `JobProgress` per sub-job
/// step, so a few thousand cover the largest workspace of section 9 while a
/// window that stops reading is told soon.
pub const EVENT_CAPACITY: usize = 4096;

/// A repo of a workspace.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RepoRef {
    pub workspace_id: WorkspaceId,
    pub repo_id: RepoId,
}

/// Something the UI shows changed. Each variant is what the Tauri event of
/// the same name carried.
#[derive(Debug, Clone)]
pub enum AppEvent {
    /// The AI settings or providers changed, or a CLI provider's self-test
    /// finished. Never carries a key (SEC-1).
    AiChanged(AiView),
    /// The models `ai_models_get` asked for (AI-1).
    AiModels(AiModelsReply),
    /// The result of `ai_self_test` (AI-2).
    AiSelfTest(AiSelfTestReply),
    /// A proposal job's result was kept; `ai_proposal_get` reads it. Never
    /// carries a prompt or a reply.
    AiProposalReady(AiProposalReady),
    /// A job was queued, began to run, or made progress (ARCH-7, UI-21).
    JobProgress(JobView),
    /// Git or ssh asks for something it cannot get silently (D10), answered
    /// with `askpass_answer`.
    AskpassPrompt(AskpassPrompt),
    /// A prompt was withdrawn before it was answered, as when its job ended.
    AskpassClosed { prompt_id: u32 },
    /// A group operation's preflight finished (section 5, step 1).
    PreflightReady(PreflightReady),
    /// A job ended, with its outcome and git's output, after its repos'
    /// fresh summaries.
    JobEnded(JobView),
    /// The result of git detection: `Checking` when it starts, then the
    /// result (PLAT-3).
    GitStatus(GitStatus),
    /// The updater's view changed (PLAT-4).
    UpdateStatusChanged(UpdateView),
    /// A group's row after a member's summary changed (GRP-4, UI-4).
    GroupSummary(GroupSummary),
    /// A workspace's fresh state, after it changed.
    WorkspaceChanged(WorkspaceView),
    /// The open workspaces and the active one changed (UI-27).
    WorkspacesChanged(WorkspaceTabs),
    /// What an add job did, when it ends, after its `WorkspaceChanged`.
    WorkspaceAddReport(AddReport),
    /// The parent folder the user picked for a clone. `key` is the target
    /// of the missing repos dialog it is for; `None` for the clone dialog.
    CloneParentPicked {
        workspace_id: WorkspaceId,
        path: PathBuf,
        key: Option<String>,
    },
    /// How an export ended: `report`, or `error` when it failed.
    WorkspaceExported {
        workspace_id: WorkspaceId,
        report: Option<ExportReport>,
        error: Option<String>,
    },
    /// How an import ended, after the `WorkspacesChanged` that opens the new
    /// workspace: `report`, or `error` when it was refused or failed.
    WorkspaceImported {
        report: Option<ImportReport>,
        error: Option<String>,
    },
    /// How a clone ended, after its `WorkspaceChanged`.
    CloneReport(CloneReport),
    /// One repo's fresh summary, each time a repo has been read (PERF-3).
    RepoSummary(SummaryView),
    /// A repo's loaded refs were dropped because its summary was read again;
    /// `refs_get` reads them anew.
    RepoRefsChanged(RepoRef),
    /// Watching a workspace's files failed, sent once.
    WatchWarning(WatchWarning),
    /// A commit-graph write job ended: `error` says why it failed, with
    /// git's stderr when git ran.
    CommitGraphWritten {
        workspace_id: WorkspaceId,
        repo_id: RepoId,
        job_id: JobId,
        error: Option<String>,
        git_stderr: Option<String>,
    },
}

/// The channel and the stores that the core's sinks need to send events:
/// cloned into the job engine's events and the watchers' callbacks.
#[derive(Clone)]
pub(crate) struct Hub {
    pub(crate) events: broadcast::Sender<AppEvent>,
    pub(crate) workspaces: Arc<Workspaces>,
    pub(crate) summaries: Arc<Summaries>,
    pub(crate) trees: Arc<Trees>,
}

impl Hub {
    /// Sends `event` to every subscriber. With none, it is dropped: the
    /// state it reports can be read at any time.
    pub(crate) fn send(&self, event: AppEvent) {
        let _ = self.events.send(event);
    }

    /// Sends `view` as `RepoSummary`, then its groups' rows; when the repo
    /// was `read` again, its loaded refs are dropped too, announced as
    /// `RepoRefsChanged`.
    pub(crate) fn summary(&self, view: SummaryView, read: bool) {
        let changed = RepoRef {
            workspace_id: view.workspace_id.clone(),
            repo_id: view.repo_id.clone(),
        };
        self.send(AppEvent::RepoSummary(view));
        self.group_summaries(&changed);
        if read
            && self
                .trees
                .invalidate(&changed.workspace_id, &changed.repo_id)
        {
            self.send(AppEvent::RepoRefsChanged(changed));
        }
    }

    /// Sends the row of each group `changed` is a member of: its branch and
    /// totals follow its members (GRP-4).
    fn group_summaries(&self, changed: &RepoRef) {
        let Ok(groups) = self.workspaces.groups(&changed.workspace_id) else {
            return;
        };
        let member: Vec<&GroupId> = groups
            .iter()
            .filter(|group| group.repos.contains(&changed.repo_id))
            .map(|group| &group.id)
            .collect();
        if member.is_empty() {
            return;
        }
        let Ok(rows) = group_summaries(&self.workspaces, &self.summaries, &changed.workspace_id)
        else {
            return;
        };
        for row in rows {
            if member.contains(&&row.group_id) {
                self.send(AppEvent::GroupSummary(row));
            }
        }
    }

    /// Sends each summary a refresh reads; see [`Self::summary`].
    pub(crate) fn summary_sink(&self) -> impl Fn(SummaryView) + Send + Sync + 'static {
        let hub = self.clone();
        move |view| {
            let read = !view.refreshing;
            hub.summary(view, read);
        }
    }

    /// Reads the summaries of `repos` again, sending each.
    pub(crate) async fn refresh(
        &self,
        workspace_id: WorkspaceId,
        repos: Vec<RepoId>,
        git: GitRunner,
    ) {
        if repos.is_empty() {
            return;
        }
        refresh_summaries(
            self.workspaces.clone(),
            self.summaries.clone(),
            workspace_id,
            repos,
            git,
            self.summary_sink(),
        )
        .await;
    }
}
