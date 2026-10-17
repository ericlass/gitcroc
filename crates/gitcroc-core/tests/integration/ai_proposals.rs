//! Proposal jobs (D9, AI-7, AI-16, section 3 "Proposal") with a scripted
//! proposer in place of a provider, on conflicts made with `git` as a
//! terminal user would. Each test waits for events; nothing waits for a
//! time, and no provider is called.

use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use gitcroc_core::{
    AiEvents, AiFileContext, AiJobs, AiOptions, AiProposalGet, AiProposalReady, AiRefused,
    AiResolveOnce, AiResolveRequest, AiResolveStart, AiUsage, Changes, ChangesContent,
    ChangesTarget, ChooseRequest, ConfigDir, ConflictChoice, ConsentRequest, ConsentTarget,
    FileProposal, GitRunner, Ineligible, IntegrateOp, IntegrateRequest, IntegrateStart, JobEvents,
    JobId, JobView, Jobs, Kill, Outcome, ProposalConfidence, ProposalStatus, ProposeFuture,
    ProposedHunk, Proposer, ProviderAccess, ProviderDraft, ProviderEntry, ProviderKind, RepoId,
    ResolveRequest, SettingsStore, SkipReason, Summaries, SummaryCache, SummaryView, WorkspaceId,
    Workspaces, add_path, ai_proposal_get, ai_resolve, ai_resolve_once, changes_get,
    conflict_choose, conflict_resolve, op_integrate,
};
use repo_gen::{Fixture, Repo};
use tokio::sync::{Semaphore, mpsc};

struct Dirs {
    root: tempfile::TempDir,
}

impl ConfigDir for Dirs {
    fn config_dir(&self) -> Option<PathBuf> {
        Some(self.root.path().join("config"))
    }
}

/// git with no user or system config, so no hook or setting of the machine
/// takes part.
fn git(dirs: &Dirs) -> GitRunner {
    let home = dirs.root.path().join("home");
    GitRunner::new(PathBuf::from("git")).with_env([
        ("HOME", home.clone().into_os_string()),
        ("XDG_CONFIG_HOME", home.into_os_string()),
        ("GIT_CONFIG_NOSYSTEM", "1".into()),
    ])
}

/// Sends each job's end and each kept result to the test.
struct Events {
    ends: mpsc::UnboundedSender<JobView>,
    ready: mpsc::UnboundedSender<AiProposalReady>,
}

impl JobEvents for Events {
    fn progress(&self, _job: &JobView) {}

    fn ended(&self, job: &JobView) {
        let _ = self.ends.send(job.clone());
    }

    fn summary(&self, _view: SummaryView, _read: bool) {}
}

impl AiEvents for Events {
    fn proposal_ready(&self, ready: &AiProposalReady) {
        let _ = self.ready.send(ready.clone());
    }
}

/// A proposal that resolves hunk 1 with `text`.
fn proposal(text: &str) -> FileProposal {
    FileProposal {
        hunks: vec![ProposedHunk {
            id: 1,
            status: ProposalStatus::Resolved,
            resolution: Some(text.to_owned()),
            explanation: "Kept both.".to_owned(),
            confidence: ProposalConfidence::High,
        }],
        usage: vec![
            Some(AiUsage {
                input_tokens: 1200,
                output_tokens: 80,
            }),
            None,
        ],
    }
}

/// Lets a test see a held request arrive and let it answer.
#[derive(Clone)]
struct Gate {
    arrived: Arc<Semaphore>,
    released: Arc<Semaphore>,
}

impl Gate {
    fn new() -> Self {
        Self {
            arrived: Arc::new(Semaphore::new(0)),
            released: Arc::new(Semaphore::new(0)),
        }
    }

    async fn arrived(&self) {
        self.arrived.acquire().await.unwrap().forget();
    }
}

/// Answers from a queue of scripted results and records every context it
/// got; held, it waits for its gate or the cancel request.
#[derive(Default)]
struct Scripted {
    results: Mutex<VecDeque<Result<FileProposal, String>>>,
    contexts: Mutex<Vec<AiFileContext>>,
    gate: Option<Gate>,
}

impl Proposer for Scripted {
    fn propose(&self, context: AiFileContext, cancel: Kill) -> ProposeFuture<'_> {
        Box::pin(async move {
            self.contexts.lock().unwrap().push(context);
            if let Some(gate) = &self.gate {
                gate.arrived.add_permits(1);
                tokio::select! {
                    permit = gate.released.acquire() => permit.unwrap().forget(),
                    () = cancel.requested() => return Err("cancelled".to_owned()),
                }
            }
            self.results
                .lock()
                .unwrap()
                .pop_front()
                .unwrap_or_else(|| Err("no scripted result left".to_owned()))
        })
    }
}

/// What the provider checks of the app answer.
#[derive(Default)]
struct Access {
    problem: Option<String>,
    consent: bool,
    unusable: Option<String>,
    proposer: Arc<Scripted>,
    /// The `debug_log` each proposer was asked with.
    debug_logs: Mutex<Vec<bool>>,
}

impl ProviderAccess for Access {
    fn problem(&self, _entry: &ProviderEntry) -> Option<String> {
        self.problem.clone()
    }

    fn consent_needed(&self, entry: &ProviderEntry) -> Option<ConsentRequest> {
        self.consent.then(|| ConsentRequest {
            provider_id: entry.id.clone(),
            provider_name: entry.name.clone(),
            to: ConsentTarget::Host {
                host: "api.example.com".to_owned(),
            },
            sent: vec!["The file's path".to_owned()],
        })
    }

    fn proposer(
        &self,
        _entry: &ProviderEntry,
        debug_log: bool,
    ) -> Result<Arc<dyn Proposer>, String> {
        self.debug_logs.lock().unwrap().push(debug_log);
        match &self.unusable {
            Some(reason) => Err(reason.clone()),
            None => Ok(self.proposer.clone()),
        }
    }
}

/// `main` and `feature` each changed `a.txt` and `b.txt` since "Base", and
/// `.env` too; `main` checked out and merging `feature` in conflict.
fn conflicted(fixture: &Fixture) -> Repo {
    let repo = fixture.repo("app").unwrap();
    repo.write("b.txt", b"b\n").unwrap();
    repo.write(".env", b"KEY=base\n").unwrap();
    repo.commit_file("a.txt", "a\n", "Base").unwrap();
    repo.branch("feature").unwrap();
    repo.write("b.txt", b"b main\n").unwrap();
    repo.write(".env", b"KEY=main\n").unwrap();
    repo.commit_file("a.txt", "main\n", "Main").unwrap();
    repo.switch("feature").unwrap();
    repo.write("b.txt", b"b feature\n").unwrap();
    repo.write(".env", b"KEY=feature\n").unwrap();
    repo.commit_file("a.txt", "feature\n", "Feature").unwrap();
    repo.switch("main").unwrap();
    repo.merge_with_conflicts("feature").unwrap();
    repo
}

/// A workspace with the repo, its changes view, the job engine and the AI
/// jobs, with one default provider.
struct Run {
    dirs: Dirs,
    settings: Arc<SettingsStore>,
    workspaces: Arc<Workspaces>,
    changes: Arc<Changes>,
    jobs: Arc<Jobs>,
    ai: Arc<AiJobs>,
    ends: mpsc::UnboundedReceiver<JobView>,
    ready: mpsc::UnboundedReceiver<AiProposalReady>,
    id: WorkspaceId,
    repo_id: RepoId,
    repo_path: PathBuf,
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
        let (ends_sender, ends) = mpsc::unbounded_channel();
        let (ready_sender, ready) = mpsc::unbounded_channel();
        let events = Arc::new(Events {
            ends: ends_sender,
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
            ends,
            ready,
            id,
            repo_id,
            repo_path: repo.path().to_owned(),
        }
    }

    /// The view's handle and the id of unmerged `path` in it.
    async fn file(&self, path: &str) -> (u32, u32) {
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
        let id = unmerged
            .iter()
            .find(|file| file.path == path)
            .unwrap_or_else(|| panic!("{path} is not unmerged"))
            .id;
        (view.handle, u32::try_from(id).unwrap())
    }

    async fn resolve_with(&self, access: Arc<Access>, path: &str) -> AiResolveStart {
        let (handle, file) = self.file(path).await;
        let access: Arc<dyn ProviderAccess> = access;
        ai_resolve(
            self.changes.clone(),
            self.ai.clone(),
            access,
            git(&self.dirs),
            self.id.clone(),
            AiResolveRequest {
                handle,
                file,
                hunks: None,
            },
        )
        .await
        .unwrap()
        .unwrap()
    }

    /// Why `path` was refused with `access`.
    async fn refused(&self, access: Access, path: &str) -> AiRefused {
        match self.resolve_with(Arc::new(access), path).await {
            AiResolveStart::Refused { refused } => refused,
            other => panic!("not refused: {other:?}"),
        }
    }

    /// Starts a job for `path` with `access` and returns its id.
    async fn start(&self, access: Arc<Access>, path: &str) -> JobId {
        match self.resolve_with(access, path).await {
            AiResolveStart::Started { job_id } => job_id,
            other => panic!("not started: {other:?}"),
        }
    }

    /// Waits for the end of job `job_id`.
    async fn ended(&mut self, job_id: JobId) -> JobView {
        let ended = self.ends.recv().await.unwrap();
        assert_eq!(ended.job_id, job_id, "{ended:#?}");
        ended
    }

    async fn proposal(&self, path: &str) -> AiProposalGet {
        let (handle, file) = self.file(path).await;
        ai_proposal_get(
            self.changes.clone(),
            self.ai.clone(),
            self.id.clone(),
            handle,
            file,
        )
        .await
        .unwrap()
        .unwrap()
    }

    /// Runs a job for `path` that ends with a proposal, and checks it is
    /// kept.
    async fn proposed(&mut self, path: &str) {
        let access = scripted([Ok(proposal("both\n"))]);
        let job_id = self.start(access, path).await;
        assert_eq!(self.ended(job_id).await.outcome, Some(Outcome::Succeeded));
        assert_eq!(self.ready.recv().await.unwrap().job_id, job_id);
        assert_eq!(
            self.proposal(path).await,
            AiProposalGet::Proposal {
                proposal: proposal("both\n")
            }
        );
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

/// Provider access whose proposer answers `results` in order.
fn scripted(results: impl IntoIterator<Item = Result<FileProposal, String>>) -> Arc<Access> {
    Arc::new(Access {
        proposer: Arc::new(Scripted {
            results: Mutex::new(results.into_iter().collect()),
            ..Scripted::default()
        }),
        ..Access::default()
    })
}

/// Provider access whose proposer holds every request at the gate.
fn held(results: impl IntoIterator<Item = Result<FileProposal, String>>) -> (Arc<Access>, Gate) {
    let gate = Gate::new();
    let access = Arc::new(Access {
        proposer: Arc::new(Scripted {
            results: Mutex::new(results.into_iter().collect()),
            gate: Some(gate.clone()),
            ..Scripted::default()
        }),
        ..Access::default()
    });
    (access, gate)
}

/// Everything a write could change: every file of the working tree and
/// of `.git` but the logs, byte for byte.
fn snapshot(root: &Path) -> Vec<(PathBuf, Vec<u8>)> {
    fn walk(root: &Path, dir: &Path, files: &mut Vec<(PathBuf, Vec<u8>)>) {
        for entry in std::fs::read_dir(dir).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                walk(root, &path, files);
            } else {
                let bytes = std::fs::read(&path).unwrap();
                files.push((path.strip_prefix(root).unwrap().to_owned(), bytes));
            }
        }
    }
    let mut files = Vec::new();
    walk(root, root, &mut files);
    files.sort();
    files
}

#[tokio::test]
async fn d9_proposal_job_writes_nothing() {
    let fixture = Fixture::new().unwrap();
    let repo = conflicted(&fixture);
    let mut run = Run::new(&repo).await;
    let (handle, file) = run.file("a.txt").await;
    let before = snapshot(repo.path());

    let access: Arc<dyn ProviderAccess> = scripted([Ok(proposal("both\n"))]);
    let started = ai_resolve(
        run.changes.clone(),
        run.ai.clone(),
        access,
        git(&run.dirs),
        run.id.clone(),
        AiResolveRequest {
            handle,
            file,
            hunks: None,
        },
    )
    .await
    .unwrap()
    .unwrap();
    let AiResolveStart::Started { job_id } = started else {
        panic!("not started: {started:?}");
    };
    assert_eq!(run.ended(job_id).await.outcome, Some(Outcome::Succeeded));
    run.ready.recv().await.unwrap();

    // The working file, the index, `HEAD`, the refs and the conflict state
    // are all under the repo's folder.
    assert_eq!(snapshot(repo.path()), before);
}

#[tokio::test]
async fn d9_ai_jobs_never_reach_the_write_runner() {
    let fixture = Fixture::new().unwrap();
    let repo = conflicted(&fixture);
    let mut run = Run::new(&repo).await;
    // A runner that fails on every git it would start, writes first: the
    // job still ends with its proposal, so it started none.
    let failing = GitRunner::new(run.dirs.root.path().join("no-git"));
    let (handle, file) = run.file("a.txt").await;
    let access: Arc<dyn ProviderAccess> = scripted([Ok(proposal("both\n"))]);
    let started = ai_resolve(
        run.changes.clone(),
        run.ai.clone(),
        access,
        failing,
        run.id.clone(),
        AiResolveRequest {
            handle,
            file,
            hunks: None,
        },
    )
    .await
    .unwrap()
    .unwrap();
    let AiResolveStart::Started { job_id } = started else {
        panic!("not started: {started:?}");
    };
    let ended = run.ended(job_id).await;
    assert_eq!(ended.outcome, Some(Outcome::Succeeded), "{ended:#?}");
    // The job is the AI's, on the repo, outside the repo's write queue.
    assert_eq!(ended.label, "Resolve with AI · a.txt");
    assert_eq!(ended.repos, [run.repo_id.clone()]);
}

#[tokio::test]
async fn ai_7_the_job_shows_usage_never_the_prompt_or_the_reply() {
    let fixture = Fixture::new().unwrap();
    let repo = conflicted(&fixture);
    let mut run = Run::new(&repo).await;
    let access = scripted([Ok(proposal("SECRET-REPLY\n"))]);
    let job_id = run.start(access.clone(), "a.txt").await;
    let ended = run.ended(job_id).await;

    assert_eq!(
        ended.output,
        "Request 1: 1200 input tokens, 80 output tokens\nRequest 2: no token usage reported\n"
    );
    let job_list = format!("{:?}", run.jobs.all_views());
    assert!(!job_list.contains("SECRET-REPLY"), "{job_list}");
    // The context sent holds the file's text; the job list does not.
    let sent = access.proposer.contexts.lock().unwrap()[0].clone();
    assert_eq!(sent.hunks[0].theirs, "feature\n");
    assert!(!job_list.contains("feature\\n"), "{job_list}");
    // debug_log is off by default, and the proposer is asked with it.
    assert_eq!(*access.debug_logs.lock().unwrap(), [false]);
}

#[tokio::test]
async fn ai_7_the_proposer_is_asked_to_log_when_debug_log_is_on() {
    let fixture = Fixture::new().unwrap();
    let repo = conflicted(&fixture);
    let mut run = Run::new(&repo).await;
    run.ai
        .set_options(AiOptions {
            enabled: true,
            exclude: run.settings.ai().exclude,
            debug_log: true,
        })
        .unwrap();
    let access = scripted([Ok(proposal("both\n"))]);
    let job_id = run.start(access.clone(), "a.txt").await;
    run.ended(job_id).await;
    assert_eq!(*access.debug_logs.lock().unwrap(), [true]);
}

#[tokio::test]
async fn ai_9_a_failed_file_keeps_the_reason() {
    let fixture = Fixture::new().unwrap();
    let repo = conflicted(&fixture);
    let mut run = Run::new(&repo).await;
    let access = scripted([Err("invalid reply after one retry: no JSON".to_owned())]);
    let job_id = run.start(access, "a.txt").await;
    let ended = run.ended(job_id).await;

    assert_eq!(ended.outcome, Some(Outcome::Failed));
    assert_eq!(
        ended.message.as_deref(),
        Some("AI failed: invalid reply after one retry: no JSON")
    );
    assert_eq!(run.ready.recv().await.unwrap().job_id, job_id);
    assert_eq!(
        run.proposal("a.txt").await,
        AiProposalGet::Failed {
            reason: "invalid reply after one retry: no JSON".to_owned()
        }
    );
}

#[tokio::test]
async fn ai_16_off_refuses_before_any_other_check() {
    let fixture = Fixture::new().unwrap();
    let repo = conflicted(&fixture);
    let run = Run::new(&repo).await;
    run.workspaces.set_ai(&run.id, false).unwrap();
    run.set_ai(false);
    let access = Access {
        problem: Some("broken".to_owned()),
        consent: true,
        unusable: Some("no key".to_owned()),
        ..Access::default()
    };
    assert_eq!(run.refused(access, ".env").await, AiRefused::AiOff);
}

#[tokio::test]
async fn ai_6_workspace_switch_refuses_before_the_provider_checks() {
    let fixture = Fixture::new().unwrap();
    let repo = conflicted(&fixture);
    let run = Run::new(&repo).await;
    run.workspaces.set_ai(&run.id, false).unwrap();
    let access = Access {
        problem: Some("broken".to_owned()),
        consent: true,
        ..Access::default()
    };
    assert_eq!(run.refused(access, ".env").await, AiRefused::AiDisabled);
}

#[tokio::test]
async fn ai_3_no_provider_refuses_before_the_provider_problem() {
    let fixture = Fixture::new().unwrap();
    let repo = conflicted(&fixture);
    let run = Run::new(&repo).await;
    let id = run.settings.ai().default.unwrap();
    run.settings.delete_provider(&id).unwrap();
    let access = Access {
        problem: Some("broken".to_owned()),
        consent: true,
        ..Access::default()
    };
    assert_eq!(run.refused(access, ".env").await, AiRefused::NoProvider);
}

#[tokio::test]
async fn ai_2_unusable_provider_refuses_before_eligibility() {
    let fixture = Fixture::new().unwrap();
    let repo = conflicted(&fixture);
    let run = Run::new(&repo).await;
    let access = Access {
        problem: Some("claude 1.0 lacks --tools".to_owned()),
        consent: true,
        ..Access::default()
    };
    assert_eq!(
        run.refused(access, ".env").await,
        AiRefused::ProviderUnusable {
            reason: "claude 1.0 lacks --tools".to_owned()
        }
    );
}

#[tokio::test]
async fn ai_4_ineligible_refuses_before_consent() {
    let fixture = Fixture::new().unwrap();
    let repo = conflicted(&fixture);
    let run = Run::new(&repo).await;
    let access = Access {
        consent: true,
        ..Access::default()
    };
    assert_eq!(
        run.refused(access, ".env").await,
        AiRefused::Ineligible {
            reason: Ineligible::Excluded {
                pattern: ".env*".to_owned()
            },
            message: "matches the exclude list (.env*)".to_owned(),
        }
    );
}

#[tokio::test]
async fn ai_6_consent_needed_is_the_last_refusal() {
    let fixture = Fixture::new().unwrap();
    let repo = conflicted(&fixture);
    let run = Run::new(&repo).await;
    let access = Access {
        consent: true,
        unusable: Some("no key".to_owned()),
        ..Access::default()
    };
    let AiRefused::ConsentNeeded { consent } = run.refused(access, "a.txt").await else {
        panic!("no consent asked");
    };
    assert_eq!(consent.provider_name, "Local");
    // With consent, a provider that cannot be made is unusable.
    let access = Access {
        unusable: Some("no key".to_owned()),
        ..Access::default()
    };
    assert_eq!(
        run.refused(access, "a.txt").await,
        AiRefused::ProviderUnusable {
            reason: "no key".to_owned()
        }
    );
}

#[tokio::test]
async fn ai_16_turning_off_cancels_jobs_and_drops_proposals() {
    let fixture = Fixture::new().unwrap();
    let repo = conflicted(&fixture);
    let mut run = Run::new(&repo).await;
    run.proposed("b.txt").await;
    let (access, gate) = held([Ok(proposal("late\n"))]);
    let job_id = run.start(access, "a.txt").await;
    gate.arrived().await;

    run.set_ai(false);

    let ended = run.ended(job_id).await;
    assert_eq!(
        ended.outcome,
        Some(Outcome::Skipped {
            reason: SkipReason::Cancelled
        })
    );
    run.set_ai(true);
    assert_eq!(run.proposal("b.txt").await, AiProposalGet::NoProposal);
    assert_eq!(run.proposal("a.txt").await, AiProposalGet::NoProposal);
    assert!(run.ready.try_recv().is_err());
}

#[tokio::test]
async fn d9_cancel_keeps_no_proposal_and_leaves_the_file_untouched() {
    let fixture = Fixture::new().unwrap();
    let repo = conflicted(&fixture);
    let mut run = Run::new(&repo).await;
    let before = snapshot(repo.path());
    let (access, gate) = held([Ok(proposal("late\n"))]);
    let job_id = run.start(access, "a.txt").await;
    gate.arrived().await;

    run.jobs.cancel(job_id);

    let ended = run.ended(job_id).await;
    assert_eq!(
        ended.outcome,
        Some(Outcome::Skipped {
            reason: SkipReason::Cancelled
        })
    );
    assert_eq!(run.proposal("a.txt").await, AiProposalGet::NoProposal);
    assert!(run.ready.try_recv().is_err());
    assert_eq!(snapshot(repo.path()), before);
}

#[tokio::test]
async fn d9_proposal_dropped_on_mark_resolved_and_on_choose() {
    let fixture = Fixture::new().unwrap();
    let repo = conflicted(&fixture);
    let mut run = Run::new(&repo).await;
    run.proposed("a.txt").await;
    run.proposed("b.txt").await;
    let (handle, file) = run.file("a.txt").await;
    let request = ResolveRequest {
        handle,
        file,
        text: "both\n".to_owned(),
        override_markers: false,
    };
    conflict_resolve(
        run.changes.clone(),
        run.jobs.clone(),
        git(&run.dirs),
        run.id.clone(),
        request,
    )
    .await
    .unwrap()
    .unwrap();
    run.ends.recv().await.unwrap();
    // Unmerged again, as `git checkout -m` makes it: the same stages, but
    // the proposal went with Mark resolved.
    repo.git(["checkout", "-m", "--", "a.txt"]).unwrap();
    assert_eq!(run.proposal("a.txt").await, AiProposalGet::NoProposal);
    assert!(matches!(
        run.proposal("b.txt").await,
        AiProposalGet::Proposal { .. }
    ));

    let (handle, file) = run.file("b.txt").await;
    let request = ChooseRequest {
        handle,
        file,
        side: ConflictChoice::Ours,
    };
    conflict_choose(
        run.changes.clone(),
        run.jobs.clone(),
        git(&run.dirs),
        run.id.clone(),
        request,
    )
    .await
    .unwrap()
    .unwrap();
    run.ends.recv().await.unwrap();
    repo.git(["checkout", "-m", "--", "b.txt"]).unwrap();
    assert_eq!(run.proposal("b.txt").await, AiProposalGet::NoProposal);
}

#[tokio::test]
async fn d9_proposal_dropped_on_abort() {
    let fixture = Fixture::new().unwrap();
    let repo = conflicted(&fixture);
    let mut run = Run::new(&repo).await;
    run.proposed("a.txt").await;
    let start = op_integrate(
        run.workspaces.clone(),
        Arc::new(Summaries::load(SummaryCache::none(), &run.workspaces)),
        run.jobs.clone(),
        git(&run.dirs),
        run.id.clone(),
        IntegrateRequest {
            repo_id: run.repo_id.clone(),
            op: IntegrateOp::Abort,
            confirmed: true,
        },
    )
    .await
    .unwrap();
    assert!(matches!(start, IntegrateStart::Started { .. }), "{start:?}");
    run.ends.recv().await.unwrap();
    // The same conflict again: the proposal went with Abort.
    repo.merge_with_conflicts("feature").unwrap();
    assert_eq!(run.proposal("a.txt").await, AiProposalGet::NoProposal);
}

#[tokio::test]
async fn d9_proposal_dropped_when_the_stages_change() {
    let fixture = Fixture::new().unwrap();
    let repo = conflicted(&fixture);
    let mut run = Run::new(&repo).await;
    run.proposed("a.txt").await;
    // In a terminal: the merge aborted, `feature` changed again, and merged
    // again, so the file is in conflict with another theirs.
    repo.git(["merge", "--abort"]).unwrap();
    repo.switch("feature").unwrap();
    repo.commit_file("a.txt", "feature again\n", "Again")
        .unwrap();
    repo.switch("main").unwrap();
    repo.merge_with_conflicts("feature").unwrap();
    assert_eq!(run.proposal("a.txt").await, AiProposalGet::NoProposal);
}

#[tokio::test]
async fn d9_resolve_once_runs_the_same_job_outside_a_workspace() {
    let fixture = Fixture::new().unwrap();
    let repo = conflicted(&fixture);
    let run = Run::new(&repo).await;
    let access: Arc<dyn ProviderAccess> = scripted([Ok(proposal("both\n"))]);
    let once = ai_resolve_once(
        &run.settings,
        access,
        git(&run.dirs),
        run.repo_path.clone(),
        b"a.txt".to_vec(),
        None,
    )
    .await
    .unwrap();
    assert_eq!(
        once,
        AiResolveOnce::Proposal {
            proposal: proposal("both\n")
        }
    );
    let access: Arc<dyn ProviderAccess> = scripted([]);
    let once = ai_resolve_once(
        &run.settings,
        access,
        git(&run.dirs),
        run.repo_path.clone(),
        b".env".to_vec(),
        None,
    )
    .await
    .unwrap();
    assert!(
        matches!(
            once,
            AiResolveOnce::Refused {
                refused: AiRefused::Ineligible { .. }
            }
        ),
        "{once:?}"
    );
}
