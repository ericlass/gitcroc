//! What the app tells its UI (ARCH-6): one [`AppEvent`] per Tauri event the
//! old shell sent, on one broadcast channel, and the [`Hub`] that the core's
//! sinks send through.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, PoisonError};

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
    /// The order of the group rows worked out, so a stale one is not sent
    /// after a newer one (GRP-4).
    pub(crate) group_rows: Arc<GroupRows>,
}

/// The tickets of the group rows: the next to give, and the last sent per
/// group.
#[derive(Default)]
pub(crate) struct GroupRows {
    next: AtomicU64,
    sent: Mutex<HashMap<GroupId, u64>>,
}

/// Group rows worked out with the ticket taken before they were read.
struct Ticketed {
    ticket: u64,
    rows: Vec<GroupSummary>,
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
        if let Some(rows) = self.group_rows(changed) {
            self.send_group_rows(rows);
        }
    }

    /// Works out the rows of the groups `changed` is a member of, from the
    /// summaries stored now, with a ticket taken before they are read.
    fn group_rows(&self, changed: &RepoRef) -> Option<Ticketed> {
        let groups = self.workspaces.groups(&changed.workspace_id).ok()?;
        let member: Vec<GroupId> = groups
            .into_iter()
            .filter(|group| group.repos.contains(&changed.repo_id))
            .map(|group| group.id)
            .collect();
        if member.is_empty() {
            return None;
        }
        let ticket = self.group_rows.next.fetch_add(1, Ordering::SeqCst);
        let rows = group_summaries(&self.workspaces, &self.summaries, &changed.workspace_id)
            .ok()?
            .into_iter()
            .filter(|row| member.contains(&row.group_id))
            .collect();
        Some(Ticketed { ticket, rows })
    }

    /// Sends `rows`, except a group's row that a row with a later ticket
    /// was sent for already. Summaries are read in parallel, so rows can
    /// get here out of order; one with a later ticket was read after every
    /// summary stored before this one's, so it is never older.
    fn send_group_rows(&self, rows: Ticketed) {
        let mut sent = self
            .group_rows
            .sent
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        for row in rows.rows {
            let last = sent.entry(row.group_id.clone()).or_insert(rows.ticket);
            if *last > rows.ticket {
                continue;
            }
            *last = rows.ticket;
            self.send(AppEvent::GroupSummary(row));
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

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use gitcroc_core::{CacheDir, ConfigDir, GroupBranch, JobId, SummaryCache, add_path};

    use super::*;

    /// A config dir and a cache dir in one temporary folder.
    struct Dirs(repo_gen::Fixture);

    impl ConfigDir for Dirs {
        fn config_dir(&self) -> Option<PathBuf> {
            Some(self.0.path().join("config"))
        }
    }

    impl CacheDir for Dirs {
        fn cache_dir(&self) -> Option<PathBuf> {
            Some(self.0.path().join("cache"))
        }
    }

    fn git() -> GitRunner {
        GitRunner::new(PathBuf::from("git"))
    }

    fn last_row(events: &mut broadcast::Receiver<AppEvent>) -> Option<GroupSummary> {
        let mut last = None;
        while let Ok(event) = events.try_recv() {
            if let AppEvent::GroupSummary(row) = event {
                last = Some(row);
            }
        }
        last
    }

    /// GRP-4: a group's row worked out before a member's newer summary was
    /// stored is never sent after the row that has it, so the UI's last
    /// row is the current one. The two reads are interleaved by hand: the
    /// first row is worked out, then the newer summary is read and sent in
    /// full, then the first row is sent.
    #[test]
    fn grp_4_a_stale_group_row_is_never_sent_after_a_newer_one() {
        let runtime = tokio::runtime::Runtime::new().unwrap();
        let made = repo_gen::Fixture::new().unwrap();
        for name in ["api", "web"] {
            made.repo(name)
                .unwrap()
                .commit_file("README.md", "hello\n", "Initial")
                .unwrap();
        }
        let dirs = Dirs(repo_gen::Fixture::new().unwrap());
        let workspaces = Arc::new(Workspaces::load(&dirs));
        let summaries = Arc::new(Summaries::load(SummaryCache::open(&dirs), &workspaces));
        let id = workspaces.views()[0].id.clone();
        let report = runtime
            .block_on(add_path(
                workspaces.clone(),
                id.clone(),
                made.path().to_owned(),
                git(),
                JobId(1),
                |_| {},
            ))
            .unwrap();
        let repos: Vec<RepoId> = report.added.into_iter().map(|repo| repo.id).collect();
        let group = workspaces.create_group(&id, "Backend", &repos).unwrap();
        let (events, mut heard) = broadcast::channel(EVENT_CAPACITY);
        let hub = Hub {
            events,
            workspaces,
            summaries,
            trees: Arc::new(Trees::new()),
            group_rows: Arc::default(),
        };
        runtime.block_on(hub.refresh(id.clone(), repos.clone(), git()));
        assert_eq!(
            last_row(&mut heard).map(|row| row.branch),
            Some(GroupBranch::Branch {
                name: "main".into()
            })
        );

        // The first row is worked out while both members are on main.
        let api = RepoRef {
            workspace_id: id.clone(),
            repo_id: repos[0].clone(),
        };
        let stale = hub.group_rows(&api);
        // Meanwhile web switches, and its summary is read and sent in full.
        made.repo("web")
            .unwrap()
            .git(["switch", "--quiet", "-c", "topic"])
            .unwrap();
        runtime.block_on(hub.refresh(id.clone(), vec![repos[1].clone()], git()));
        let mixed = GroupBranch::Mixed { count: 2 };
        assert_eq!(
            last_row(&mut heard).map(|row| row.branch),
            Some(mixed.clone())
        );

        // The first row is sent last; it must not take the newer one's place.
        if let Some(stale) = stale {
            hub.send_group_rows(stale);
        }
        let last = last_row(&mut heard);
        assert!(
            last.as_ref().is_none_or(|row| row.branch == mixed),
            "{last:?} for {group:?}"
        );
    }
}
