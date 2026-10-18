//! Resolve all with AI (AI-3, AI-13): every eligible unmerged file of a repo
//! sent to the default provider in one job, each file in its turn for one
//! of the [`crate::AI_MAX_REQUESTS`] requests all AI jobs share, with each file's
//! progress in the job's view. A file's result is kept as Resolve with AI
//! keeps it, and nothing is accepted for the user (D9, AI-12). Cancel stops
//! the files still waiting and drops the running requests: what finished
//! stays, and the other files keep nothing.

use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use gitcroc_git::{GitRepo, GitRunner, Kill};
use serde::{Deserialize, Serialize};

use super::ai_eligibility;
use super::proposal::{Ending, Kept, Key, propose, proposer_of, provider_checks, usage_output};
use crate::changes::ViewState;
use crate::{
    AiEligibility, AiJobs, AiRefused, Changes, ChangesError, FileProposal, JobId, Jobs, Outcome,
    ProposalConfidence, ProposalStatus, Proposer, ProviderAccess, RepoId, SkipReason, WorkspaceId,
    lock,
};

/// An unmerged file the UI has an editor buffer for (OPS-9).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
pub struct AiResolveAllFile {
    /// Its id in the view's unmerged list.
    pub file: u32,
    /// The hunks still untouched in the buffer, counted from 1: the only
    /// ones sent. None left, the file is not sent.
    pub hunks: Vec<u32>,
}

/// Resolve all with AI for the unmerged files of a repo (AI-3, AI-13).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
pub struct AiResolveAllRequest {
    pub repo: RepoId,
    /// The changes view that lists the repo's unmerged files.
    pub handle: u32,
    /// The files with an editor buffer; every other file is sent with all
    /// its hunks.
    pub files: Vec<AiResolveAllFile>,
}

/// What `ai_resolve_all` did.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, specta::Type)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum AiResolveAllStart {
    /// The job runs; its view carries each file's progress, and
    /// `ai_proposal_ready` tells when a file's result is kept.
    Started { job_id: JobId },
    /// No file is in conflict any more; read the lists again.
    AlreadyResolved,
    /// Never `ineligible`: an ineligible file is listed, not sent.
    Refused { refused: AiRefused },
}

/// Where a file of Resolve all is (AI-13).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum AiFileState {
    /// Waiting for its request to go out.
    Waiting,
    Running,
    /// Its proposal is kept: the resolved hunks by confidence, and those
    /// the model left unresolved.
    Done {
        high: u32,
        medium: u32,
        low: u32,
        unresolved: u32,
    },
    /// "AI failed", with the reason (AI-9).
    Failed {
        reason: String,
    },
    /// Not sent (AI-4).
    Ineligible {
        message: String,
    },
    /// Not sent: every hunk is edited in its buffer already.
    AllEdited,
    /// Stopped before its proposal was kept; it keeps none.
    Cancelled,
}

impl AiFileState {
    /// Whether the file is done with, sent or not.
    pub fn ended(&self) -> bool {
        !matches!(self, Self::Waiting | Self::Running)
    }
}

/// A file of Resolve all and where it is (AI-13).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
pub struct AiFileProgress {
    /// The path from the repo top, for display.
    pub path: String,
    pub state: AiFileState,
}

/// A file to send.
struct Send {
    /// Its place in the job's files.
    index: usize,
    key: Key,
    path: String,
    hunks: Option<Vec<u32>>,
    stop: Kill,
}

/// The files of a running Resolve all, which each change sends in the
/// job's view.
struct Progress {
    job: JobId,
    files: Mutex<Vec<AiFileProgress>>,
}

impl Progress {
    fn set(&self, jobs: &Jobs, index: usize, state: AiFileState) {
        let mut files = lock(&self.files);
        if let Some(file) = files.get_mut(index) {
            file.state = state;
        }
        // Sent with the files locked, so the views go out in order.
        jobs.set_ai_files(self.job, &files);
    }
}

/// What the job a file runs in needs of it.
struct Shared {
    progress: Progress,
    proposer: Arc<dyn Proposer>,
    git: GitRunner,
    repo_path: PathBuf,
    exclude: Vec<String>,
    /// The job's cancel request.
    stop: Kill,
}

/// How many hunks of `proposal` are resolved at each confidence, and how
/// many are unresolved.
fn done(proposal: &FileProposal) -> AiFileState {
    let (mut high, mut medium, mut low, mut unresolved) = (0u32, 0u32, 0u32, 0u32);
    for hunk in &proposal.hunks {
        let count = match (hunk.status, hunk.confidence) {
            (ProposalStatus::Unresolved, _) => &mut unresolved,
            (ProposalStatus::Resolved, ProposalConfidence::High) => &mut high,
            (ProposalStatus::Resolved, ProposalConfidence::Medium) => &mut medium,
            (ProposalStatus::Resolved, ProposalConfidence::Low) => &mut low,
        };
        *count = count.saturating_add(1);
    }
    AiFileState::Done {
        high,
        medium,
        low,
        unresolved,
    }
}

/// The job's own outcome, which the status bar shows (UI-21): failed when
/// AI failed for any file, else succeeded when any file's proposal is
/// kept, else cancelled; nothing to do when no file was sent.
fn overall(files: &[AiFileProgress], sent: usize) -> (Outcome, Option<String>) {
    if sent == 0 {
        return (Outcome::NothingToDo, None);
    }
    let count = |kind: fn(&AiFileState) -> bool| files.iter().filter(|f| kind(&f.state)).count();
    let failed = count(|state| matches!(state, AiFileState::Failed { .. }));
    if failed > 0 {
        let of = if sent == 1 { "file" } else { "files" };
        return (
            Outcome::Failed,
            Some(format!("AI failed for {failed} of {sent} {of}")),
        );
    }
    if count(|state| matches!(state, AiFileState::Done { .. })) > 0 {
        return (Outcome::Succeeded, None);
    }
    (
        Outcome::Skipped {
            reason: SkipReason::Cancelled,
        },
        Some("cancelled".to_owned()),
    )
}

impl AiJobs {
    /// Resolve all with AI for the unmerged files of `view`, after the
    /// checks of [`ai_resolve_all`]; `buffers` are the files with an editor
    /// buffer.
    async fn resolve_all(
        self: &Arc<Self>,
        access: &Arc<dyn ProviderAccess>,
        git: GitRunner,
        workspace: WorkspaceId,
        view: ViewState,
        buffers: Vec<AiResolveAllFile>,
    ) -> Result<AiResolveAllStart, ChangesError> {
        let ai = self.settings.ai();
        let workspace_on = !ai.enabled || self.workspaces.ai_enabled(&workspace)?;
        let entry = match provider_checks(&ai, workspace_on, access) {
            Ok(entry) => entry,
            Err(refused) => return Ok(AiResolveAllStart::Refused { refused }),
        };
        let unmerged: Vec<(u32, Vec<u8>)> = view
            .lists
            .unmerged_files()
            .iter()
            .map(|file| {
                let id = u32::try_from(file.id).unwrap_or(u32::MAX);
                (id, file.git_path().to_vec())
            })
            .collect();
        let (repo_path, exclude) = (view.path.clone(), ai.exclude.clone());
        let read = tokio::task::spawn_blocking(move || {
            let repo = GitRepo::open(&repo_path)?;
            let mut files = Vec::new();
            for (id, git_path) in unmerged {
                // One resolved meanwhile is left out.
                if let Some(conflict) = repo.conflict(&git_path)? {
                    let eligibility = ai_eligibility(&git_path, &conflict.content, &exclude);
                    files.push((id, git_path, conflict.path, eligibility));
                }
            }
            Ok::<_, ChangesError>(files)
        })
        .await??;
        if read.is_empty() {
            return Ok(AiResolveAllStart::AlreadyResolved);
        }
        let mut files = Vec::new();
        let mut sends = Vec::new();
        for (id, git_path, path, eligibility) in read {
            let hunks = buffers
                .iter()
                .find(|buffer| buffer.file == id)
                .map(|buffer| buffer.hunks.clone());
            let state = match (eligibility, &hunks) {
                (AiEligibility::Ineligible { message, .. }, _) => {
                    AiFileState::Ineligible { message }
                }
                (AiEligibility::Eligible, Some(hunks)) if hunks.is_empty() => {
                    AiFileState::AllEdited
                }
                (AiEligibility::Eligible, _) => {
                    sends.push(Send {
                        index: files.len(),
                        key: Key {
                            workspace: workspace.clone(),
                            repo: view.repo.clone(),
                            git_path,
                        },
                        path: path.clone(),
                        hunks,
                        stop: Kill::new(),
                    });
                    AiFileState::Waiting
                }
            };
            files.push(AiFileProgress { path, state });
        }
        let proposer = if sends.is_empty() {
            None
        } else {
            if let Some(consent) = access.consent_needed(&entry) {
                let refused = AiRefused::ConsentNeeded { consent };
                return Ok(AiResolveAllStart::Refused { refused });
            }
            match proposer_of(access, entry, ai.debug_log).await? {
                Ok(proposer) => Some(proposer),
                Err(refused) => return Ok(AiResolveAllStart::Refused { refused }),
            }
        };
        let name = self
            .workspaces
            .view(&workspace)?
            .repos
            .into_iter()
            .find(|repo| repo.id == view.repo)
            .map_or_else(|| view.repo.0.clone(), |repo| repo.name);
        let count = sends.len();
        let noun = if count == 1 { "file" } else { "files" };
        let label = format!("Resolve all with AI · {name} ({count} {noun})");
        let job = self.jobs.track_ai(&workspace, &view.repo, &label)?;
        let job_id = job.job_id();
        for send in &sends {
            self.run_for(send.key.clone(), job_id, send.stop.clone());
        }
        self.jobs.set_ai_files(job_id, &files);
        let Some(proposer) = proposer else {
            let (outcome, message) = overall(&files, 0);
            job.end(outcome, String::new(), message);
            return Ok(AiResolveAllStart::Started { job_id });
        };
        let shared = Arc::new(Shared {
            progress: Progress {
                job: job_id,
                files: Mutex::new(files),
            },
            proposer,
            git,
            repo_path: view.path,
            exclude: ai.exclude,
            stop: job.cancel_request(),
        });
        let tasks: Vec<_> = sends
            .into_iter()
            .map(|send| tokio::spawn(self.clone().propose_file(shared.clone(), send)))
            .collect();
        tokio::spawn(async move {
            let mut output = String::new();
            for task in tasks {
                if let Ok(lines) = task.await {
                    output.push_str(&lines);
                }
            }
            let (outcome, message) = overall(&lock(&shared.progress.files), count);
            job.end(outcome, output, message);
        });
        Ok(AiResolveAllStart::Started { job_id })
    }

    /// Proposes for one file of a Resolve all, keeps the result, and
    /// returns its lines of the job's output: the usage of each request
    /// (AI-7, AI-14).
    async fn propose_file(self: Arc<Self>, shared: Arc<Shared>, send: Send) -> String {
        let jobs = self.jobs.clone();
        let running = || {
            shared.progress.set(&jobs, send.index, AiFileState::Running);
        };
        let ending = tokio::select! {
            biased;
            () = shared.stop.requested() => Ending::Cancelled,
            ending = propose(
                shared.proposer.clone(),
                self.requests.clone(),
                shared.git.clone(),
                shared.repo_path.clone(),
                send.key.git_path.clone(),
                shared.exclude.clone(),
                send.hunks.clone(),
                send.stop.clone(),
                running,
            ) => ending,
        };
        let job = shared.progress.job;
        let stopped = send.stop.is_requested() || shared.stop.is_requested();
        let kept = self.keep(job, &send.key, stopped, ending);
        let (state, output) = match &kept {
            None => (AiFileState::Cancelled, String::new()),
            Some((_, Kept::Proposal(proposal))) => {
                let lines = usage_output(&proposal.usage)
                    .lines()
                    .map(|line| format!("{}: {line}\n", send.path))
                    .collect();
                (done(proposal), lines)
            }
            Some((_, Kept::Failed(reason))) => (
                AiFileState::Failed {
                    reason: reason.clone(),
                },
                String::new(),
            ),
        };
        shared.progress.set(&self.jobs, send.index, state);
        if let Some((Some(_), _)) = kept {
            self.ready(&send.key, send.path, job);
        }
        output
    }
}

/// Resolve all with AI for the unmerged files of `request.repo` that view
/// `request.handle` lists (AI-3, AI-13), with the default provider. Returns
/// at once, after the checks of `ai_resolve` that are not a file's: AI
/// features (AI-16), the workspace's switch (AI-6), a default provider
/// (AI-3) and what `access` knows against it (AI-2); then, when a file is
/// to be sent, the consent (AI-6). Ineligible files (AI-4) and files whose
/// buffer has every hunk edited are listed and not sent. Otherwise starts
/// the job "Resolve all with AI · <repo> (n files)", which proposes for at
/// most [`crate::AI_MAX_REQUESTS`] files at a time and keeps each result for
/// `ai_proposal_get`. `None` when a newer view of the workspace replaced
/// the request's, or it is another repo's.
pub async fn ai_resolve_all(
    changes: Arc<Changes>,
    ai: Arc<AiJobs>,
    access: Arc<dyn ProviderAccess>,
    git: GitRunner,
    id: WorkspaceId,
    request: AiResolveAllRequest,
) -> Result<Option<AiResolveAllStart>, ChangesError> {
    let Some(view) = changes.state(&id, request.handle) else {
        return Ok(None);
    };
    if view.repo != request.repo {
        return Ok(None);
    }
    let started = ai
        .resolve_all(&access, git, id, view, request.files)
        .await?;
    Ok(Some(started))
}
