//! Proposal jobs (D9, AI-7, section 3 "Proposal"): one conflicted file sent
//! to the default provider as a job, its answer kept in memory per
//! workspace (ARCH-9) until the conflict it was made for is gone. A job
//! only reads the repo, with a read-only runner, and calls the provider
//! through [`Proposer`]: it never writes, stages, commits or continues.
//! Accepting a proposal, Mark resolved and Continue stay the user's
//! actions (AI-12, OPS-5). Prompts and replies never reach the job list;
//! the job shows its outcome and its token usage (AI-7, AI-14).

use std::collections::HashMap;
use std::future::Future;
use std::path::PathBuf;
use std::pin::Pin;
use std::sync::{Arc, Mutex};

use gitcroc_git::{ConflictStages, GitRepo, GitRunner, Kill};
use serde::{Deserialize, Serialize};
use tokio::sync::Semaphore;

use super::{AiContextRead, AiFileContext, Ineligible, ai_eligibility, file_context};
use crate::conflict::unmerged_of;
use crate::{
    AiEligibility, AiOptions, AiSettings, Changes, ChangesError, JobError, JobId, Jobs, Outcome,
    ProviderEntry, ProviderId, ProviderKind, RepoId, SettingsError, SettingsStore, SkipReason,
    TrackedJob, WorkspaceId, Workspaces, lock, url_host,
};

/// The most AI requests running at once, over every workspace (AI-13).
pub const AI_MAX_REQUESTS: usize = 4;

/// The tokens of one request, when the provider reports them (AI-14).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
pub struct AiUsage {
    pub input_tokens: u32,
    pub output_tokens: u32,
}

/// Whether the model resolved a hunk (AI-8).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "snake_case")]
pub enum ProposalStatus {
    Resolved,
    Unresolved,
}

/// How sure the model is of a resolution (AI-8).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "snake_case")]
pub enum ProposalConfidence {
    High,
    Medium,
    Low,
}

/// The proposal for one hunk (AI-8, AI-11).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
pub struct ProposedHunk {
    /// The hunk's id, counted from 1 over all hunks of the file (AI-5).
    pub id: u32,
    pub status: ProposalStatus,
    /// The exact replacement text; `None` for an unresolved hunk.
    pub resolution: Option<String>,
    pub explanation: String,
    pub confidence: ProposalConfidence,
}

/// The proposal for a file (section 3 "Proposal"): an answer for every
/// hunk asked for, and the usage of each request sent for it, in order,
/// `None` where the provider reported none (AI-14).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
pub struct FileProposal {
    pub hunks: Vec<ProposedHunk>,
    pub usage: Vec<Option<AiUsage>>,
}

/// The future a [`Proposer`] returns: the proposal, or the reason the file
/// is marked "AI failed" (AI-9).
pub type ProposeFuture<'a> =
    Pin<Box<dyn Future<Output = Result<FileProposal, String>> + Send + 'a>>;

/// The seam to `gitcroc-ai`, which the core does not depend on: the app and
/// the CLI each ask a provider through its ask loop (AI-9, AI-10).
pub trait Proposer: Send + Sync {
    /// Proposes resolutions for the hunks of `context`. Once `cancel` is
    /// requested the request is dropped, or the CLI killed, and the answer
    /// does not matter: nothing of it is kept.
    fn propose(&self, context: AiFileContext, cancel: Kill) -> ProposeFuture<'_>;
}

/// The provider checks the core cannot make: the self-tests, the consent
/// and the keys are the app's, or the CLI's (AI-2, AI-6, SEC-1).
pub trait ProviderAccess: Send + Sync {
    /// Why `entry` cannot be used, from what is known now, such as a failed
    /// self-test (AI-2); `None` when nothing is known against it.
    fn problem(&self, entry: &ProviderEntry) -> Option<String>;

    /// The consent `entry` needs before it is used (AI-6).
    fn consent_needed(&self, entry: &ProviderEntry) -> Option<ConsentRequest>;

    /// A proposer that asks `entry`, writing each prompt and reply to the
    /// log when `debug_log` is on, never a key (AI-7, SEC-1). `Err` with
    /// the reason when it cannot be used, such as a missing key. May block
    /// on the keychain.
    fn proposer(&self, entry: &ProviderEntry, debug_log: bool)
    -> Result<Arc<dyn Proposer>, String>;
}

/// Where a provider sends what AI-5 allows: an API's host, or the program a
/// CLI provider runs (AI-6).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ConsentTarget {
    /// With its port when the URL names one, such as `localhost:11434`.
    Host {
        host: String,
    },
    Program {
        path: String,
    },
}

impl ConsentTarget {
    /// What the consent keeps: the host or the program's path.
    pub fn value(&self) -> &str {
        match self {
            Self::Host { host } => host,
            Self::Program { path } => path,
        }
    }
}

/// The text of the consent dialog, shown before a provider's first use and
/// again when its host or program changed (AI-6).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
pub struct ConsentRequest {
    pub provider_id: ProviderId,
    pub provider_name: String,
    /// Where it is sent; `ai_provider_consent` takes its value back.
    pub to: ConsentTarget,
    /// What is sent for each file, one item per line.
    pub sent: Vec<String>,
}

/// What is sent for each file (AI-5), in plain words for the consent
/// dialog.
const SENT: [&str; 5] = [
    "The file's path in the repository",
    "The operation, such as merge, rebase, cherry-pick, revert or stash apply",
    "The labels of the two sides, such as the branch names",
    "Each conflicting part: its base, ours and theirs text, with the 40 lines around it",
    "The messages of up to 10 commits per side that changed the file, each cut at 2,000 characters",
];

/// Where `entry` sends (AI-6): its API's host, the Anthropic API at
/// `anthropic_url`, or its program.
pub fn consent_target(entry: &ProviderEntry, anthropic_url: &str) -> Option<ConsentTarget> {
    let host = |url: &str| url_host(url).map(|host| ConsentTarget::Host { host });
    match entry.kind {
        ProviderKind::Anthropic => host(anthropic_url),
        ProviderKind::Openai => entry.base_url.as_deref().and_then(host),
        ProviderKind::ClaudeCode | ProviderKind::Codex => {
            entry.path.as_ref().map(|path| ConsentTarget::Program {
                path: path.display().to_string(),
            })
        }
    }
}

/// The consent `entry` still needs (AI-6): none when the user consented to
/// where it sends now. `anthropic_url` as in [`consent_target`].
pub fn consent_request(entry: &ProviderEntry, anthropic_url: &str) -> Option<ConsentRequest> {
    let to = consent_target(entry, anthropic_url)?;
    (entry.consent.as_deref() != Some(to.value())).then(|| ConsentRequest {
        provider_id: entry.id.clone(),
        provider_name: entry.name.clone(),
        to,
        sent: SENT.map(str::to_owned).to_vec(),
    })
}

/// Why a file was not sent to AI, in the order the checks run.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, specta::Type)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum AiRefused {
    /// AI features are off (AI-16).
    AiOff,
    /// The workspace's switch is off (AI-6).
    AiDisabled,
    /// No provider is the default: open the provider settings (AI-3).
    NoProvider,
    ProviderUnusable {
        reason: String,
    },
    /// The file may not go to AI (AI-4).
    Ineligible {
        reason: Ineligible,
        message: String,
    },
    /// Ask for consent, then try again (AI-6).
    ConsentNeeded {
        consent: ConsentRequest,
    },
}

/// Resolve with AI for one unmerged file of a changes view (AI-3).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
pub struct AiResolveRequest {
    /// The changes view whose unmerged list `file` is from.
    pub handle: u32,
    pub file: u32,
    /// The hunks to send, counted from 1; all when `None`, so hunks
    /// resolved in the editor need not be sent.
    pub hunks: Option<Vec<u32>>,
}

/// What `ai_resolve` did.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, specta::Type)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum AiResolveStart {
    /// The job runs; `ai_proposal_ready` tells when its result is kept.
    Started {
        job_id: JobId,
    },
    /// The file is no longer in conflict; read the lists again.
    AlreadyResolved,
    Refused {
        refused: AiRefused,
    },
}

/// What `ai_proposal_get` found for a file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, specta::Type)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum AiProposalGet {
    Proposal {
        proposal: FileProposal,
    },
    /// "AI failed", with the reason (AI-9).
    Failed {
        reason: String,
    },
    /// None is kept for the conflict as it is now.
    NoProposal,
    /// The file is no longer in conflict; read the lists again.
    AlreadyResolved,
}

/// A proposal job's result was kept: read it with `ai_proposal_get`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
pub struct AiProposalReady {
    pub workspace_id: WorkspaceId,
    pub repo_id: RepoId,
    /// The path from the repo top, for display.
    pub path: String,
    pub job_id: JobId,
}

/// Where the AI jobs tell the app that a result is kept.
pub trait AiEvents: Send + Sync {
    fn proposal_ready(&self, ready: &AiProposalReady);
}

/// What `ai_resolve_once` ended with, for the CLI.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum AiResolveOnce {
    Proposal { proposal: FileProposal },
    Failed { reason: String },
    AlreadyResolved,
    Refused { refused: AiRefused },
}

/// A proposal's place: a file of a repo of a workspace, as git stores its
/// path.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct Key {
    workspace: WorkspaceId,
    repo: RepoId,
    git_path: Vec<u8>,
}

/// What a job left for a file.
#[derive(Debug, Clone)]
enum Kept {
    Proposal(FileProposal),
    Failed(String),
}

/// A kept result and the stages of the conflict it was made for.
#[derive(Debug)]
struct Stored {
    stages: ConflictStages,
    kept: Kept,
}

#[derive(Default)]
struct State {
    proposals: HashMap<Key, Stored>,
    /// The running jobs and their files. A job whose entry is gone was
    /// cancelled, and keeps nothing.
    running: HashMap<JobId, Key>,
}

/// How a job's work ended.
enum Ending {
    Proposal(ConflictStages, FileProposal),
    /// "AI failed"; kept with the stages when the conflict was read.
    Failed(Option<ConflictStages>, String),
    Cancelled,
}

/// The proposal jobs and their results, for the whole app, each keyed by
/// workspace, repo and file (ARCH-9).
pub struct AiJobs {
    jobs: Arc<Jobs>,
    workspaces: Arc<Workspaces>,
    settings: Arc<SettingsStore>,
    events: Arc<dyn AiEvents>,
    /// Shared by every AI job: at most [`AI_MAX_REQUESTS`] ask at once.
    requests: Arc<Semaphore>,
    state: Mutex<State>,
}

impl std::fmt::Debug for AiJobs {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AiJobs").finish_non_exhaustive()
    }
}

/// The default provider of `ai`, if it is configured (AI-1).
fn default_provider(ai: &AiSettings) -> Option<ProviderEntry> {
    ai.default.as_ref().and_then(|id| ai.provider(id)).cloned()
}

/// What the checks before a job found.
enum Prepared {
    Ready {
        proposer: Arc<dyn Proposer>,
        path: String,
    },
    AlreadyResolved,
    Refused(AiRefused),
}

/// Runs the checks of `ai_resolve` in their order (AI-16, AI-6, AI-3,
/// AI-2, AI-4, AI-6): `workspace_on` is the workspace's switch.
async fn prepare(
    ai: &AiSettings,
    workspace_on: bool,
    access: &Arc<dyn ProviderAccess>,
    repo_path: PathBuf,
    git_path: Vec<u8>,
) -> Result<Prepared, ChangesError> {
    if !ai.enabled {
        return Ok(Prepared::Refused(AiRefused::AiOff));
    }
    if !workspace_on {
        return Ok(Prepared::Refused(AiRefused::AiDisabled));
    }
    let Some(entry) = default_provider(ai) else {
        return Ok(Prepared::Refused(AiRefused::NoProvider));
    };
    if let Some(reason) = access.problem(&entry) {
        return Ok(Prepared::Refused(AiRefused::ProviderUnusable { reason }));
    }
    let exclude = ai.exclude.clone();
    let conflict = tokio::task::spawn_blocking(move || {
        let repo = GitRepo::open(&repo_path)?;
        Ok::<_, ChangesError>(repo.conflict(&git_path)?.map(|conflict| {
            let eligibility = ai_eligibility(&git_path, &conflict.content, &exclude);
            (conflict.path, eligibility)
        }))
    })
    .await??;
    let Some((path, eligibility)) = conflict else {
        return Ok(Prepared::AlreadyResolved);
    };
    if let AiEligibility::Ineligible { reason, message } = eligibility {
        return Ok(Prepared::Refused(AiRefused::Ineligible { reason, message }));
    }
    if let Some(consent) = access.consent_needed(&entry) {
        return Ok(Prepared::Refused(AiRefused::ConsentNeeded { consent }));
    }
    let debug_log = ai.debug_log;
    let access = access.clone();
    let proposer = tokio::task::spawn_blocking(move || access.proposer(&entry, debug_log)).await?;
    Ok(match proposer {
        Ok(proposer) => Prepared::Ready { proposer, path },
        Err(reason) => Prepared::Refused(AiRefused::ProviderUnusable { reason }),
    })
}

/// What a job sends and how it ends: reads the context of `git_path` with
/// a read-only `git` (D9), and asks `proposer` once one of `requests` is
/// free, until `cancel`.
#[allow(clippy::too_many_arguments)]
async fn propose(
    proposer: Arc<dyn Proposer>,
    requests: Arc<Semaphore>,
    git: GitRunner,
    repo_path: PathBuf,
    git_path: Vec<u8>,
    exclude: Vec<String>,
    hunks: Option<Vec<u32>>,
    cancel: Kill,
) -> Ending {
    let git = git.read_only();
    let read = tokio::task::spawn_blocking(move || {
        let repo = GitRepo::open(&repo_path).map_err(|error| error.to_string())?;
        file_context(&repo, &git, &git_path, &exclude, hunks.as_deref())
            .map_err(|error| error.to_string())
    });
    let (stages, context) = match read.await {
        Ok(Ok(Some((stages, AiContextRead::Context { context })))) => (stages, *context),
        Ok(Ok(Some((stages, AiContextRead::Ineligible { message, .. })))) => {
            return Ending::Failed(Some(stages), message);
        }
        Ok(Ok(None)) => {
            return Ending::Failed(None, "the file is no longer in conflict".to_owned());
        }
        Ok(Err(reason)) => return Ending::Failed(None, reason),
        Err(error) => return Ending::Failed(None, error.to_string()),
    };
    let _permit = tokio::select! {
        biased;
        () = cancel.requested() => return Ending::Cancelled,
        permit = requests.acquire_owned() => match permit {
            Ok(permit) => permit,
            Err(_) => return Ending::Cancelled,
        },
    };
    let result = tokio::select! {
        biased;
        () = cancel.requested() => return Ending::Cancelled,
        result = proposer.propose(context, cancel.clone()) => result,
    };
    match result {
        _ if cancel.is_requested() => Ending::Cancelled,
        Ok(proposal) => Ending::Proposal(stages, proposal),
        Err(reason) => Ending::Failed(Some(stages), reason),
    }
}

/// The job list's output for a proposal: the usage of each request, never
/// the prompt or the reply (AI-7, AI-14).
fn usage_output(usage: &[Option<AiUsage>]) -> String {
    let mut output = String::new();
    for (number, usage) in (1..).zip(usage) {
        let line = match usage {
            Some(usage) => format!(
                "Request {number}: {} input tokens, {} output tokens\n",
                usage.input_tokens, usage.output_tokens
            ),
            None => format!("Request {number}: no token usage reported\n"),
        };
        output.push_str(&line);
    }
    output
}

/// The last part of `path`, for the job's label.
fn file_name(path: &str) -> &str {
    path.rsplit('/').next().unwrap_or(path)
}

impl AiJobs {
    /// The proposal jobs of the app. Resolutions, Abort and Continue run by
    /// `jobs` drop the proposals they make stale.
    pub fn new(
        jobs: Arc<Jobs>,
        workspaces: Arc<Workspaces>,
        settings: Arc<SettingsStore>,
        events: Arc<dyn AiEvents>,
    ) -> Arc<Self> {
        let ai = Arc::new(Self {
            jobs,
            workspaces,
            settings,
            events,
            requests: Arc::new(Semaphore::new(AI_MAX_REQUESTS)),
            state: Mutex::new(State::default()),
        });
        ai.jobs.use_ai(Arc::downgrade(&ai));
        ai
    }

    /// Changes the AI features switch, the exclude list and the debug log
    /// switch. Turning AI off cancels every running AI job in every
    /// workspace and drops every proposal; nothing is kept for when it is
    /// turned on again (AI-16).
    pub fn set_options(&self, options: AiOptions) -> Result<AiSettings, SettingsError> {
        let ai = self.settings.set_ai_options(options)?;
        if !ai.enabled {
            self.drop_where(|_| true);
        }
        Ok(ai)
    }

    /// Drops the proposals and cancels the jobs of every file `matches`.
    fn drop_where(&self, matches: impl Fn(&Key) -> bool) {
        let cancelled: Vec<JobId> = {
            let mut state = lock(&self.state);
            state.proposals.retain(|key, _| !matches(key));
            let cancelled = state
                .running
                .iter()
                .filter(|(_, key)| matches(key))
                .map(|(id, _)| *id)
                .collect();
            state.running.retain(|_, key| !matches(key));
            cancelled
        };
        for id in cancelled {
            self.jobs.cancel(id);
        }
    }

    /// Drops what is kept or running for `git_path` of `repo`: the file
    /// was marked resolved or chosen (D9).
    pub(crate) fn forget_file(&self, workspace: &WorkspaceId, repo: &RepoId, git_path: &[u8]) {
        self.drop_where(|key| {
            key.workspace == *workspace && key.repo == *repo && key.git_path == git_path
        });
    }

    /// Drops what is kept or running for `repo`: its operation was aborted
    /// or continued (D9).
    pub(crate) fn forget_repo(&self, workspace: &WorkspaceId, repo: &RepoId) {
        self.drop_where(|key| key.workspace == *workspace && key.repo == *repo);
    }

    /// Resolves `git_path` of `repo` with AI after the checks of
    /// [`ai_resolve`].
    #[allow(clippy::too_many_arguments)]
    async fn resolve(
        self: &Arc<Self>,
        access: &Arc<dyn ProviderAccess>,
        git: GitRunner,
        workspace: WorkspaceId,
        repo: RepoId,
        repo_path: PathBuf,
        git_path: Vec<u8>,
        hunks: Option<Vec<u32>>,
    ) -> Result<AiResolveStart, ChangesError> {
        let ai = self.settings.ai();
        let workspace_on = !ai.enabled || self.workspaces.ai_enabled(&workspace)?;
        let prepared = prepare(
            &ai,
            workspace_on,
            access,
            repo_path.clone(),
            git_path.clone(),
        )
        .await?;
        let (proposer, path) = match prepared {
            Prepared::Ready { proposer, path } => (proposer, path),
            Prepared::AlreadyResolved => return Ok(AiResolveStart::AlreadyResolved),
            Prepared::Refused(refused) => return Ok(AiResolveStart::Refused { refused }),
        };
        let key = Key {
            workspace,
            repo,
            git_path,
        };
        let job = self.track(&key, &path)?;
        let job_id = job.job_id();
        let cancel = job.cancel_request();
        let this = self.clone();
        tokio::spawn(async move {
            let ending = propose(
                proposer,
                this.requests.clone(),
                git,
                repo_path,
                key.git_path.clone(),
                ai.exclude,
                hunks,
                cancel,
            )
            .await;
            this.end(job, key, path, ending);
        });
        Ok(AiResolveStart::Started { job_id })
    }

    /// Starts tracking a job for `key`; a job already running for the file
    /// is cancelled, as the newer request replaces it.
    fn track(&self, key: &Key, path: &str) -> Result<TrackedJob, JobError> {
        let label = format!("Resolve with AI · {}", file_name(path));
        let job = self.jobs.track_ai(&key.workspace, &key.repo, &label)?;
        let replaced: Vec<JobId> = {
            let mut state = lock(&self.state);
            let replaced = state
                .running
                .iter()
                .filter(|(_, running)| *running == key)
                .map(|(id, _)| *id)
                .collect();
            state.running.retain(|_, running| running != key);
            state.running.insert(job.job_id(), key.clone());
            replaced
        };
        for id in replaced {
            self.jobs.cancel(id);
        }
        Ok(job)
    }

    /// Keeps what job `job` for `key` ended with, unless it was cancelled
    /// meanwhile, and ends it.
    fn end(&self, job: TrackedJob, key: Key, path: String, ending: Ending) {
        let job_id = job.job_id();
        let kept = {
            let mut state = lock(&self.state);
            let live = state.running.remove(&job_id).is_some();
            match ending {
                _ if !live || job.cancel_request().is_requested() => None,
                Ending::Cancelled => None,
                Ending::Proposal(stages, proposal) => {
                    let output = usage_output(&proposal.usage);
                    let kept = Kept::Proposal(proposal);
                    Some((Some(stages), kept, output))
                }
                Ending::Failed(stages, reason) => {
                    Some((stages, Kept::Failed(reason), String::new()))
                }
            }
            .inspect(|(stages, kept, _)| {
                if let Some(stages) = stages {
                    state.proposals.insert(
                        key.clone(),
                        Stored {
                            stages: *stages,
                            kept: kept.clone(),
                        },
                    );
                }
            })
        };
        let Some((stages, kept, output)) = kept else {
            job.end(
                Outcome::Skipped {
                    reason: SkipReason::Cancelled,
                },
                String::new(),
                None,
            );
            return;
        };
        match kept {
            Kept::Proposal(_) => job.end(Outcome::Succeeded, output, None),
            Kept::Failed(reason) => job.end(
                Outcome::Failed,
                output,
                Some(format!("AI failed: {reason}")),
            ),
        }
        if stages.is_some() {
            self.events.proposal_ready(&AiProposalReady {
                workspace_id: key.workspace,
                repo_id: key.repo,
                path,
                job_id,
            });
        }
    }

    /// What is kept for `key`, whose conflict now has `stages`, or `None`
    /// once the file is resolved. A result made for other stages is
    /// dropped: the file changed since.
    fn kept(&self, key: &Key, stages: Option<ConflictStages>) -> AiProposalGet {
        let mut state = lock(&self.state);
        let Some(stages) = stages else {
            state.proposals.remove(key);
            return AiProposalGet::AlreadyResolved;
        };
        match state.proposals.get(key) {
            Some(stored) if stored.stages == stages => match &stored.kept {
                Kept::Proposal(proposal) => AiProposalGet::Proposal {
                    proposal: proposal.clone(),
                },
                Kept::Failed(reason) => AiProposalGet::Failed {
                    reason: reason.clone(),
                },
            },
            Some(_) => {
                state.proposals.remove(key);
                AiProposalGet::NoProposal
            }
            None => AiProposalGet::NoProposal,
        }
    }
}

/// Resolve with AI for unmerged file `request.file` of view
/// `request.handle` (AI-3), with the default provider. Returns at once,
/// after these checks in this order: AI features (AI-16), the workspace's
/// switch (AI-6), a default provider (AI-3), what `access` knows against
/// it (AI-2), the file's eligibility (AI-4), and the consent (AI-6).
/// Otherwise starts the job "Resolve with AI · <file name>", which keeps
/// its result for `ai_proposal_get`. `None` when a newer view of the
/// workspace replaced the request's.
pub async fn ai_resolve(
    changes: Arc<Changes>,
    ai: Arc<AiJobs>,
    access: Arc<dyn ProviderAccess>,
    git: GitRunner,
    id: WorkspaceId,
    request: AiResolveRequest,
) -> Result<Option<AiResolveStart>, ChangesError> {
    let Some((view, git_path)) = unmerged_of(&changes, &id, request.handle, request.file)? else {
        return Ok(None);
    };
    let started = ai
        .resolve(
            &access,
            git,
            id,
            view.repo,
            view.path,
            git_path,
            request.hunks,
        )
        .await?;
    Ok(Some(started))
}

/// The proposal kept for unmerged file `file` of view `handle`, the reason
/// it failed, or nothing (section 3 "Proposal"). One made for a conflict
/// the file no longer has is dropped. `None` when a newer view of the
/// workspace replaced the view.
pub async fn ai_proposal_get(
    changes: Arc<Changes>,
    ai: Arc<AiJobs>,
    id: WorkspaceId,
    handle: u32,
    file: u32,
) -> Result<Option<AiProposalGet>, ChangesError> {
    let Some((view, git_path)) = unmerged_of(&changes, &id, handle, file)? else {
        return Ok(None);
    };
    let path = view.path.clone();
    let read_path = git_path.clone();
    let stages = tokio::task::spawn_blocking(move || {
        let repo = GitRepo::open(&path)?;
        Ok::<_, ChangesError>(repo.conflict(&read_path)?.map(|conflict| conflict.stages()))
    })
    .await??;
    let key = Key {
        workspace: id,
        repo: view.repo,
        git_path,
    };
    Ok(Some(ai.kept(&key, stages)))
}

/// Resolves `git_path` of the repo at `repo_path` with AI once, outside any
/// workspace, as the job of [`ai_resolve`] does and after the same checks
/// but the workspace's switch: for the CLI. Waits for the answer.
pub async fn ai_resolve_once(
    settings: &SettingsStore,
    access: Arc<dyn ProviderAccess>,
    git: GitRunner,
    repo_path: PathBuf,
    git_path: Vec<u8>,
    hunks: Option<Vec<u32>>,
) -> Result<AiResolveOnce, ChangesError> {
    let ai = settings.ai();
    let prepared = prepare(&ai, true, &access, repo_path.clone(), git_path.clone()).await?;
    let proposer = match prepared {
        Prepared::Ready { proposer, .. } => proposer,
        Prepared::AlreadyResolved => return Ok(AiResolveOnce::AlreadyResolved),
        Prepared::Refused(refused) => return Ok(AiResolveOnce::Refused { refused }),
    };
    let requests = Arc::new(Semaphore::new(AI_MAX_REQUESTS));
    let ending = propose(
        proposer,
        requests,
        git,
        repo_path,
        git_path,
        ai.exclude,
        hunks,
        Kill::new(),
    )
    .await;
    Ok(match ending {
        Ending::Proposal(_, proposal) => AiResolveOnce::Proposal { proposal },
        Ending::Failed(_, reason) => AiResolveOnce::Failed { reason },
        Ending::Cancelled => AiResolveOnce::Failed {
            reason: "cancelled".to_owned(),
        },
    })
}
