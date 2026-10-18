//! Resolve all with AI (AI-3, AI-13, AI-16, D9) with a proposer that answers
//! per file and holds each request until the test releases it, on a merge
//! left in conflict by `git`. Each test waits for events; nothing waits for
//! a time, and no provider is called.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use gitcroc_core::{
    AiEvents, AiFileContext, AiFileProgress, AiFileState, AiJobs, AiOptions, AiProposalGet,
    AiProposalReady, AiRefused, AiResolveAllFile, AiResolveAllRequest, AiResolveAllStart, Changes,
    ChangesContent, ChangesTarget, ConsentRequest, FileProposal, JobEvents, JobId, JobView, Jobs,
    Kill, Outcome, ProposalConfidence, ProposalStatus, ProposeFuture, ProposedHunk, Proposer,
    ProviderAccess, ProviderDraft, ProviderEntry, ProviderKind, RepoId, SettingsStore, Summaries,
    SummaryCache, SummaryView, WorkspaceId, Workspaces, add_path, ai_proposal_get, ai_resolve_all,
    changes_get,
};
use repo_gen::{Fixture, Repo};
use tokio::sync::{Semaphore, mpsc};

use crate::ai_proposals::{Dirs, git, snapshot};

/// Sends each job's views and each kept result to the test.
struct Events {
    views: mpsc::UnboundedSender<JobView>,
    ready: mpsc::UnboundedSender<AiProposalReady>,
}

impl JobEvents for Events {
    fn progress(&self, job: &JobView) {
        let _ = self.views.send(job.clone());
    }

    fn ended(&self, job: &JobView) {
        let _ = self.views.send(job.clone());
    }

    fn summary(&self, _view: SummaryView, _read: bool) {}
}

impl AiEvents for Events {
    fn proposal_ready(&self, ready: &AiProposalReady) {
        let _ = self.ready.send(ready.clone());
    }
}

/// Answers every hunk it is asked for with "ai <path>", high confidence,
/// and records each context; held, each request waits until the test
/// releases its file or it is cancelled.
#[derive(Default)]
struct PerFile {
    held: bool,
    contexts: Mutex<Vec<AiFileContext>>,
    /// The requests held now, and the most held at once.
    holding: Mutex<(usize, usize)>,
    arrived: Option<mpsc::UnboundedSender<String>>,
    releases: Mutex<HashMap<String, Arc<Semaphore>>>,
}

impl PerFile {
    fn release_of(&self, path: &str) -> Arc<Semaphore> {
        self.releases
            .lock()
            .unwrap()
            .entry(path.to_owned())
            .or_insert_with(|| Arc::new(Semaphore::new(0)))
            .clone()
    }

    fn release(&self, path: &str) {
        self.release_of(path).add_permits(1);
    }

    fn most_held(&self) -> usize {
        self.holding.lock().unwrap().1
    }

    fn sent(&self) -> Vec<String> {
        let mut paths: Vec<String> = self
            .contexts
            .lock()
            .unwrap()
            .iter()
            .map(|context| context.path.clone())
            .collect();
        paths.sort();
        paths
    }
}

fn answer(context: &AiFileContext) -> FileProposal {
    FileProposal {
        hunks: context
            .hunks
            .iter()
            .map(|hunk| ProposedHunk {
                id: hunk.id,
                status: ProposalStatus::Resolved,
                resolution: Some(format!("ai {}\n", context.path)),
                explanation: "Kept both.".to_owned(),
                confidence: ProposalConfidence::High,
            })
            .collect(),
        usage: vec![None],
    }
}

impl Proposer for PerFile {
    fn propose(&self, context: AiFileContext, cancel: Kill) -> ProposeFuture<'_> {
        Box::pin(async move {
            let proposal = answer(&context);
            let path = context.path.clone();
            self.contexts.lock().unwrap().push(context);
            if !self.held {
                return Ok(proposal);
            }
            {
                let mut holding = self.holding.lock().unwrap();
                holding.0 += 1;
                holding.1 = holding.1.max(holding.0);
            }
            if let Some(arrived) = &self.arrived {
                let _ = arrived.send(path.clone());
            }
            let release = self.release_of(&path);
            let result = tokio::select! {
                permit = release.acquire() => {
                    permit.unwrap().forget();
                    Ok(proposal)
                }
                () = cancel.requested() => Err("cancelled".to_owned()),
            };
            self.holding.lock().unwrap().0 -= 1;
            result
        })
    }
}

/// Provider access that lets every request go to `proposer`.
struct Access {
    proposer: Arc<PerFile>,
}

impl ProviderAccess for Access {
    fn problem(&self, _entry: &ProviderEntry) -> Option<String> {
        None
    }

    fn consent_needed(&self, _entry: &ProviderEntry) -> Option<ConsentRequest> {
        None
    }

    fn proposer(
        &self,
        _entry: &ProviderEntry,
        _debug_log: bool,
    ) -> Result<Arc<dyn Proposer>, String> {
        Ok(self.proposer.clone())
    }
}

/// `main` and `feature` each changed every file of `names`, `.env` and
/// `two.txt` since "Base", `two.txt` in two places far apart; `main`
/// checked out and merging `feature` in conflict.
fn conflicted(fixture: &Fixture, names: &[&str]) -> Repo {
    let repo = fixture.repo("app").unwrap();
    let two = |first: &str, last: &str| format!("{first}\n2\n3\n4\n5\n6\n7\n8\n9\n{last}\n");
    let write_all = |side: &str| {
        for name in names {
            repo.write(name, format!("{name} {side}\n")).unwrap();
        }
        repo.write(".env", format!("KEY={side}\n")).unwrap();
        repo.write("two.txt", two(&format!("1 {side}"), &format!("10 {side}")))
            .unwrap();
    };
    write_all("base");
    repo.commit_all("Base").unwrap();
    repo.branch("feature").unwrap();
    write_all("main");
    repo.commit_all("Main").unwrap();
    repo.switch("feature").unwrap();
    write_all("feature");
    repo.commit_all("Feature").unwrap();
    repo.switch("main").unwrap();
    repo.merge_with_conflicts("feature").unwrap();
    repo
}

/// With `two.txt`, six eligible files.
const FIVE: [&str; 5] = ["f1.txt", "f2.txt", "f3.txt", "f4.txt", "f5.txt"];

/// A workspace with the repo, its changes view, the job engine and the AI
/// jobs, with one default provider.
struct Run {
    dirs: Dirs,
    settings: Arc<SettingsStore>,
    workspaces: Arc<Workspaces>,
    changes: Arc<Changes>,
    jobs: Arc<Jobs>,
    ai: Arc<AiJobs>,
    views: mpsc::UnboundedReceiver<JobView>,
    ready: mpsc::UnboundedReceiver<AiProposalReady>,
    id: WorkspaceId,
    repo_id: RepoId,
}

impl Run {
    async fn new(repo: &Repo) -> Self {
        let dirs = Dirs {
            root: tempfile::tempdir().unwrap(),
        };
        std::fs::create_dir_all(dirs.root.path().join("home")).unwrap();
        let settings = Arc::new(SettingsStore::load(&dirs));
        let entry = settings
            .save_provider(
                ProviderDraft {
                    id: None,
                    name: "Local".to_owned(),
                    kind: ProviderKind::Openai,
                    model: Some("m".to_owned()),
                    base_url: Some("http://localhost:11434/v1".to_owned()),
                    path: None,
                    context_budget: None,
                },
                |_, _| Ok::<_, gitcroc_core::SettingsError>(()),
            )
            .unwrap();
        settings.set_default_provider(&entry.id).unwrap();
        let workspaces = Arc::new(Workspaces::load_with(&dirs, settings.clone()));
        let id = workspaces.views()[0].id.clone();
        let summaries = Arc::new(Summaries::load(SummaryCache::none(), &workspaces));
        let (views_sender, views) = mpsc::unbounded_channel();
        let (ready_sender, ready) = mpsc::unbounded_channel();
        let events = Arc::new(Events {
            views: views_sender,
            ready: ready_sender,
        });
        let jobs = Jobs::new(workspaces.clone(), summaries, events.clone());
        let ai = AiJobs::new(jobs.clone(), workspaces.clone(), settings.clone(), events);
        add_path(
            workspaces.clone(),
            id.clone(),
            repo.path().to_owned(),
            git(&dirs),
            JobId(0),
            |_| {},
        )
        .await
        .unwrap();
        let repo_id = workspaces.view(&id).unwrap().repos[0].id.clone();
        Self {
            dirs,
            settings,
            workspaces,
            changes: Arc::new(Changes::new()),
            jobs,
            ai,
            views,
            ready,
            id,
            repo_id,
        }
    }

    /// The view's handle and the ids of its unmerged files by path.
    async fn unmerged(&self) -> (u32, HashMap<String, u32>) {
        let view = changes_get(
            self.changes.clone(),
            self.workspaces.clone(),
            git(&self.dirs),
            self.id.clone(),
            self.repo_id.clone(),
            ChangesTarget::Uncommitted,
        )
        .await
        .unwrap()
        .unwrap();
        let ChangesContent::Uncommitted { unmerged, .. } = view.content else {
            panic!("not the working copy");
        };
        let ids = unmerged
            .into_iter()
            .map(|file| (file.path, u32::try_from(file.id).unwrap()))
            .collect();
        (view.handle, ids)
    }

    async fn resolve_all_with(
        &self,
        proposer: Arc<PerFile>,
        buffers: &[(&str, Vec<u32>)],
    ) -> AiResolveAllStart {
        let (handle, ids) = self.unmerged().await;
        let files = buffers
            .iter()
            .map(|(path, hunks)| AiResolveAllFile {
                file: ids[*path],
                hunks: hunks.clone(),
            })
            .collect();
        let access: Arc<dyn ProviderAccess> = Arc::new(Access { proposer });
        ai_resolve_all(
            self.changes.clone(),
            self.ai.clone(),
            access,
            git(&self.dirs),
            self.id.clone(),
            AiResolveAllRequest {
                repo: self.repo_id.clone(),
                handle,
                files,
            },
        )
        .await
        .unwrap()
        .unwrap()
    }

    async fn start(&self, proposer: Arc<PerFile>, buffers: &[(&str, Vec<u32>)]) -> JobId {
        match self.resolve_all_with(proposer, buffers).await {
            AiResolveAllStart::Started { job_id } => job_id,
            other => panic!("not started: {other:?}"),
        }
    }

    /// Waits for a view of job `job_id` that lists its files and that
    /// `wanted` accepts.
    async fn view_where(&mut self, job_id: JobId, wanted: impl Fn(&JobView) -> bool) -> JobView {
        loop {
            let view = self.views.recv().await.unwrap();
            if view.job_id == job_id && !view.ai_files.is_empty() && wanted(&view) {
                return view;
            }
        }
    }

    /// Waits for the end of job `job_id`, gathering every view of it.
    async fn ended(&mut self, job_id: JobId) -> (JobView, Vec<JobView>) {
        let mut seen = Vec::new();
        loop {
            let view = self.views.recv().await.unwrap();
            if view.job_id != job_id {
                continue;
            }
            if view.outcome.is_some() {
                return (view, seen);
            }
            seen.push(view);
        }
    }

    async fn proposal(&self, path: &str) -> AiProposalGet {
        let (handle, ids) = self.unmerged().await;
        ai_proposal_get(
            self.changes.clone(),
            self.ai.clone(),
            self.id.clone(),
            handle,
            ids[path],
        )
        .await
        .unwrap()
        .unwrap()
    }

    fn set_ai(&self, enabled: bool) {
        let ai = self.settings.ai();
        self.ai
            .set_options(AiOptions {
                enabled,
                exclude: ai.exclude,
                debug_log: ai.debug_log,
            })
            .unwrap();
    }
}

fn held() -> (Arc<PerFile>, mpsc::UnboundedReceiver<String>) {
    let (arrived, arrivals) = mpsc::unbounded_channel();
    let proposer = Arc::new(PerFile {
        held: true,
        arrived: Some(arrived),
        ..PerFile::default()
    });
    (proposer, arrivals)
}

fn answering() -> Arc<PerFile> {
    Arc::new(PerFile::default())
}

fn state_of<'a>(files: &'a [AiFileProgress], path: &str) -> &'a AiFileState {
    &files
        .iter()
        .find(|file| file.path == path)
        .unwrap_or_else(|| panic!("{path} is not listed: {files:#?}"))
        .state
}

fn done(hunks: u32) -> AiFileState {
    AiFileState::Done {
        high: hunks,
        medium: 0,
        low: 0,
        unresolved: 0,
    }
}

fn excluded() -> AiFileState {
    AiFileState::Ineligible {
        message: "matches the exclude list (.env*)".to_owned(),
    }
}

#[tokio::test]
async fn ai_13_at_most_4_files_at_a_time() {
    let fixture = Fixture::new().unwrap();
    let repo = conflicted(&fixture, &FIVE);
    let mut run = Run::new(&repo).await;
    let (proposer, mut arrivals) = held();
    // Five files and `two.txt` are eligible; `.env` is not.
    let job_id = run.start(proposer.clone(), &[]).await;

    let mut arrived = Vec::new();
    for _ in 0..4 {
        arrived.push(arrivals.recv().await.unwrap());
    }
    let running = run.jobs.all_views()[0].clone();
    assert_eq!(running.label, "Resolve all with AI · app (6 files)");
    let count = |state: AiFileState| {
        running
            .ai_files
            .iter()
            .filter(|file| file.state == state)
            .count()
    };
    assert_eq!(count(AiFileState::Running), 4, "{running:#?}");
    assert_eq!(count(AiFileState::Waiting), 2, "{running:#?}");
    // Each released file lets one waiting file in.
    for _ in 0..2 {
        proposer.release(&arrived.remove(0));
        arrived.push(arrivals.recv().await.unwrap());
    }
    for path in arrived {
        proposer.release(&path);
    }
    let (ended, _) = run.ended(job_id).await;

    assert_eq!(proposer.most_held(), 4);
    assert_eq!(ended.outcome, Some(Outcome::Succeeded), "{ended:#?}");
    assert_eq!(proposer.sent().len(), 6);
}

#[tokio::test]
async fn ai_13_cancel_keeps_finished_proposals_and_leaves_the_rest_untouched() {
    let fixture = Fixture::new().unwrap();
    let repo = conflicted(&fixture, &FIVE);
    let mut run = Run::new(&repo).await;
    let before = snapshot(repo.path());
    let (proposer, mut arrivals) = held();
    let job_id = run.start(proposer.clone(), &[]).await;
    let first = arrivals.recv().await.unwrap();
    let second = arrivals.recv().await.unwrap();
    proposer.release(&first);
    proposer.release(&second);
    run.view_where(job_id, |view| {
        let finished =
            |path: &str| matches!(state_of(&view.ai_files, path), AiFileState::Done { .. });
        finished(&first) && finished(&second)
    })
    .await;

    run.jobs.cancel(job_id);
    let (ended, _) = run.ended(job_id).await;

    // Succeeded, as two proposals are kept.
    assert_eq!(ended.outcome, Some(Outcome::Succeeded), "{ended:#?}");
    for file in &ended.ai_files {
        let expected = if file.path == first || file.path == second {
            done(if file.path == "two.txt" { 2 } else { 1 })
        } else if file.path == ".env" {
            excluded()
        } else {
            AiFileState::Cancelled
        };
        assert_eq!(file.state, expected, "{ended:#?}");
    }
    for path in ["f1.txt", "f2.txt", "f3.txt", "f4.txt", "f5.txt", "two.txt"] {
        let kept = run.proposal(path).await;
        if path == first || path == second {
            assert!(
                matches!(kept, AiProposalGet::Proposal { .. }),
                "{path}: {kept:?}"
            );
        } else {
            assert_eq!(kept, AiProposalGet::NoProposal, "{path}");
        }
    }
    // Nothing was accepted, written or staged for the user (D9).
    assert_eq!(snapshot(repo.path()), before);
}

#[tokio::test]
async fn ai_13_progress_per_file_in_order_and_ineligible_never_sent() {
    let fixture = Fixture::new().unwrap();
    let repo = conflicted(&fixture, &["f1.txt"]);
    let mut run = Run::new(&repo).await;
    let proposer = answering();
    let job_id = run.start(proposer.clone(), &[]).await;
    let (ended, seen) = run.ended(job_id).await;

    for path in ["f1.txt", "two.txt"] {
        let mut states: Vec<AiFileState> = Vec::new();
        for view in seen.iter().chain([&ended]) {
            if view.ai_files.is_empty() {
                continue;
            }
            let state = state_of(&view.ai_files, path).clone();
            if states.last() != Some(&state) {
                states.push(state);
            }
        }
        let hunks = if path == "two.txt" { 2 } else { 1 };
        assert_eq!(
            states,
            [AiFileState::Waiting, AiFileState::Running, done(hunks)],
            "{path}"
        );
    }
    for view in seen.iter().chain([&ended]) {
        if !view.ai_files.is_empty() {
            assert_eq!(state_of(&view.ai_files, ".env"), &excluded());
        }
    }
    assert_eq!(proposer.sent(), ["f1.txt", "two.txt"]);
    assert_eq!(ended.label, "Resolve all with AI · app (2 files)");
    assert_eq!(ended.outcome, Some(Outcome::Succeeded));
    // Each file's result is kept and announced as Resolve with AI's.
    let mut ready = [
        run.ready.recv().await.unwrap(),
        run.ready.recv().await.unwrap(),
    ]
    .map(|ready| (ready.path, ready.job_id.0));
    ready.sort();
    assert_eq!(
        ready,
        [
            ("f1.txt".to_owned(), job_id.0),
            ("two.txt".to_owned(), job_id.0)
        ]
    );
    let AiProposalGet::Proposal { proposal } = run.proposal("two.txt").await else {
        panic!("no proposal kept");
    };
    assert_eq!(proposal.hunks.len(), 2);
}

#[tokio::test]
async fn ai_13_only_the_untouched_hunks_of_a_buffer_are_sent() {
    let fixture = Fixture::new().unwrap();
    let repo = conflicted(&fixture, &["f1.txt", "f2.txt"]);
    let mut run = Run::new(&repo).await;
    let proposer = answering();
    // `two.txt` has hunk 1 edited, `f1.txt` its only hunk; `f2.txt` has no
    // buffer.
    let buffers = [("two.txt", vec![2]), ("f1.txt", vec![])];
    let job_id = run.start(proposer.clone(), &buffers).await;
    let (ended, _) = run.ended(job_id).await;

    assert_eq!(proposer.sent(), ["f2.txt", "two.txt"]);
    let contexts = proposer.contexts.lock().unwrap().clone();
    let ids = |path: &str| -> Vec<u32> {
        let context = contexts
            .iter()
            .find(|context| context.path == path)
            .unwrap();
        context.hunks.iter().map(|hunk| hunk.id).collect()
    };
    assert_eq!(ids("two.txt"), [2]);
    assert_eq!(ids("f2.txt"), [1]);
    assert_eq!(state_of(&ended.ai_files, "f1.txt"), &AiFileState::AllEdited);
    assert_eq!(state_of(&ended.ai_files, "two.txt"), &done(1));
    assert_eq!(ended.label, "Resolve all with AI · app (2 files)");
}

#[tokio::test]
async fn ai_16_resolve_all_refused_when_off() {
    let fixture = Fixture::new().unwrap();
    let repo = conflicted(&fixture, &["f1.txt"]);
    let run = Run::new(&repo).await;
    run.set_ai(false);
    let proposer = answering();

    let started = run.resolve_all_with(proposer.clone(), &[]).await;

    assert_eq!(
        started,
        AiResolveAllStart::Refused {
            refused: AiRefused::AiOff
        }
    );
    assert!(run.jobs.all_views().is_empty());
    assert!(proposer.sent().is_empty());
}

#[tokio::test]
async fn ai_3_resolve_all_refused_without_a_provider() {
    let fixture = Fixture::new().unwrap();
    let repo = conflicted(&fixture, &["f1.txt"]);
    let run = Run::new(&repo).await;
    let default = run.settings.ai().default.unwrap();
    run.settings.delete_provider(&default).unwrap();

    let started = run.resolve_all_with(answering(), &[]).await;

    assert_eq!(
        started,
        AiResolveAllStart::Refused {
            refused: AiRefused::NoProvider
        }
    );
}

#[tokio::test]
async fn ai_6_resolve_all_refused_when_the_workspace_switch_is_off() {
    let fixture = Fixture::new().unwrap();
    let repo = conflicted(&fixture, &["f1.txt"]);
    let run = Run::new(&repo).await;
    run.workspaces.set_ai(&run.id, false).unwrap();

    let started = run.resolve_all_with(answering(), &[]).await;

    assert_eq!(
        started,
        AiResolveAllStart::Refused {
            refused: AiRefused::AiDisabled
        }
    );
}

#[tokio::test]
async fn ai_16_turning_off_cancels_resolve_all_keeping_no_proposal() {
    let fixture = Fixture::new().unwrap();
    let repo = conflicted(&fixture, &FIVE);
    let mut run = Run::new(&repo).await;
    let (proposer, mut arrivals) = held();
    let job_id = run.start(proposer.clone(), &[]).await;
    let first = arrivals.recv().await.unwrap();
    proposer.release(&first);
    run.view_where(job_id, |view| {
        matches!(state_of(&view.ai_files, &first), AiFileState::Done { .. })
    })
    .await;

    run.set_ai(false);
    let (ended, _) = run.ended(job_id).await;

    for file in &ended.ai_files {
        if file.path != first && file.path != ".env" {
            assert_eq!(file.state, AiFileState::Cancelled, "{ended:#?}");
        }
    }
    for path in ["f1.txt", "f2.txt", "f3.txt", "f4.txt", "f5.txt", "two.txt"] {
        assert_eq!(
            run.proposal(path).await,
            AiProposalGet::NoProposal,
            "{path}"
        );
    }
}
