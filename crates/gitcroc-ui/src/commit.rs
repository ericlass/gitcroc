//! The commit message box of one workspace's "Uncommitted changes"
//! (UI-15), a port of rc.1's `stores/commit.ts` for a single repo: the
//! message the user is writing, kept per repo (ARCH-9) while the app runs
//! (UI-29), and the commit or amend job it was sent with. The core composes
//! the message, checks the commit can be made and runs it (ARCH-6); this
//! keeps the unsent text and clears it when the commit succeeded, if the
//! box still holds what was sent. For a group or several repos (UI-34)
//! [`GroupDrafts`] keeps the one message and the group Commit's options
//! per target set, as rc.1 does. Nothing here is saved.

use std::collections::{HashMap, VecDeque};

use gitcroc_core::{CommitMessage, GroupId, JobId, JobView, Outcome, RepoId, TargetSet};

use crate::strings::changes as text;

/// How many ended jobs are kept for a start not answered yet.
const EARLY_KEPT: usize = 16;

/// No subject and no body: what Amend fills with the last commit's message.
pub fn is_blank(message: &CommitMessage) -> bool {
    message.subject.trim().is_empty() && message.body.trim().is_empty()
}

/// A commit or amend sent to the core.
#[derive(Debug, Clone)]
struct Pending {
    repo: RepoId,
    /// `None` until the core answers with the job's id.
    job: Option<JobId>,
    /// The message as sent; the box is cleared only if it still holds it.
    sent: CommitMessage,
}

/// What a press of Commit or Amend does.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Press {
    /// Nothing: a commit of the workspace is under way.
    Nothing,
    /// The box was empty: Amend filled it with the last commit's message.
    Filled,
    /// Send this message.
    Send(CommitMessage),
}

/// The commit box's state of one workspace.
#[derive(Debug, Default)]
pub struct Drafts {
    drafts: HashMap<RepoId, CommitMessage>,
    pending: Option<Pending>,
    /// Why the last commit of each repo was refused or failed.
    errors: HashMap<RepoId, String>,
    /// Jobs that ended before their start was answered, which the app's
    /// events may do; whether each succeeded.
    ended_early: VecDeque<(JobId, bool)>,
}

impl Drafts {
    /// The unsent message of `repo`.
    pub fn draft(&self, repo: &RepoId) -> CommitMessage {
        self.drafts.get(repo).cloned().unwrap_or_default()
    }

    /// The user changed `repo`'s message.
    pub fn edit(&mut self, repo: &RepoId, message: CommitMessage) {
        self.drafts.insert(repo.clone(), message);
    }

    /// A commit or amend of `repo` is under way.
    pub fn busy(&self, repo: &RepoId) -> bool {
        self.pending
            .as_ref()
            .is_some_and(|pending| &pending.repo == repo)
    }

    /// Why the last commit of `repo` was refused or failed.
    pub fn error(&self, repo: &RepoId) -> Option<&str> {
        self.errors.get(repo).map(String::as_str)
    }

    pub fn set_error(&mut self, repo: &RepoId, error: String) {
        self.errors.insert(repo.clone(), error);
    }

    /// Commit (`amend` false) or Amend was pressed for `repo`. Amend with an
    /// empty box only fills it with `last`, for the user to read and change
    /// first; otherwise the message is sent, unless a commit runs.
    pub fn press(&mut self, repo: &RepoId, amend: bool, last: Option<&CommitMessage>) -> Press {
        let draft = self.draft(repo);
        if amend && is_blank(&draft) {
            return match last {
                Some(last) => {
                    self.edit(repo, last.clone());
                    Press::Filled
                }
                None => Press::Nothing,
            };
        }
        if self.pending.is_some() {
            return Press::Nothing;
        }
        self.errors.remove(repo);
        self.pending = Some(Pending {
            repo: repo.clone(),
            job: None,
            sent: draft.clone(),
        });
        Press::Send(draft)
    }

    /// The core answered the commit sent for `repo`: `Ok(job)` runs it,
    /// `Err` refused it and keeps the message. `true` when the box changed,
    /// as when the job had already ended.
    pub fn answered(&mut self, repo: &RepoId, answer: Result<JobId, String>) -> bool {
        let Some(pending) = self
            .pending
            .as_mut()
            .filter(|pending| &pending.repo == repo)
        else {
            return false;
        };
        match answer {
            Ok(job) => {
                pending.job = Some(job);
                let early = self.ended_early.iter().position(|(ended, _)| *ended == job);
                match early.and_then(|at| self.ended_early.remove(at)) {
                    Some((_, succeeded)) => self.finish(succeeded),
                    None => false,
                }
            }
            Err(error) => {
                self.pending = None;
                self.errors.insert(repo.clone(), error);
                true
            }
        }
    }

    /// Job `job` ended. Success clears the box unless the user changed it
    /// meanwhile; failure, such as a hook's refusal, keeps it. `true` when
    /// the box changed.
    pub fn job_ended(&mut self, job: &JobView) -> bool {
        let succeeded = job.outcome == Some(Outcome::Succeeded);
        let waiting = self.pending.as_ref().map(|pending| pending.job);
        match waiting {
            Some(Some(id)) if id == job.job_id => self.finish(succeeded),
            // The start is still to be answered: this may be its job.
            Some(None) => {
                self.ended_early.push_back((job.job_id, succeeded));
                while self.ended_early.len() > EARLY_KEPT {
                    self.ended_early.pop_front();
                }
                false
            }
            _ => false,
        }
    }

    fn finish(&mut self, succeeded: bool) -> bool {
        let Some(pending) = self.pending.take() else {
            return false;
        };
        if !succeeded {
            self.errors
                .insert(pending.repo, text::COMMIT_FAILED.to_owned());
        } else if self.draft(&pending.repo) == pending.sent {
            self.drafts.remove(&pending.repo);
        }
        true
    }
}

/// What a group commit's message and options are kept under: its target
/// set, a multi-selection's repos in one order whatever order they were
/// selected in.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum DraftKey {
    Repo(RepoId),
    Group(GroupId),
    Repos(Vec<RepoId>),
}

impl DraftKey {
    pub fn of(target: &TargetSet) -> Self {
        match target {
            TargetSet::Repo { repo_id } => Self::Repo(repo_id.clone()),
            TargetSet::Group { group_id } => Self::Group(group_id.clone()),
            TargetSet::Repos { repo_ids } => {
                let mut repos = repo_ids.clone();
                repos.sort();
                repos.dedup();
                Self::Repos(repos)
            }
        }
    }
}

/// The options of a group commit (section 5 "Commit" row); both are off by
/// default.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct GroupOptions {
    pub stage_all: bool,
    pub push: bool,
}

/// A group commit the core is running.
#[derive(Debug, Clone)]
struct GroupPending {
    key: DraftKey,
    job: JobId,
    sent: CommitMessage,
}

/// Every repo of `job` that ran committed: none is *failed* or anything
/// but *succeeded*, leaving out the skipped ones, and at least one ran.
fn all_committed(job: &JobView) -> bool {
    let mut ran = job
        .report
        .iter()
        .flatten()
        .filter(|line| !matches!(line.outcome, Outcome::Skipped { .. }))
        .peekable();
    ran.peek().is_some() && ran.all(|line| line.outcome == Outcome::Succeeded)
}

/// The group commit box's state of one workspace.
#[derive(Debug, Default)]
pub struct GroupDrafts {
    drafts: HashMap<DraftKey, CommitMessage>,
    options: HashMap<DraftKey, GroupOptions>,
    /// Sent through the preflight, its job not started yet: the key and
    /// the message as sent.
    sent: Option<(DraftKey, CommitMessage)>,
    pending: Option<GroupPending>,
    /// Why the last group commit did not commit everywhere.
    errors: HashMap<DraftKey, String>,
    /// Group jobs that ended while a start was awaited; whether each
    /// committed everywhere.
    ended_early: VecDeque<(JobId, bool)>,
}

impl GroupDrafts {
    pub fn draft(&self, key: &DraftKey) -> CommitMessage {
        self.drafts.get(key).cloned().unwrap_or_default()
    }

    pub fn edit(&mut self, key: &DraftKey, message: CommitMessage) {
        self.drafts.insert(key.clone(), message);
    }

    pub fn options(&self, key: &DraftKey) -> GroupOptions {
        self.options.get(key).copied().unwrap_or_default()
    }

    pub fn set_options(&mut self, key: &DraftKey, options: GroupOptions) {
        self.options.insert(key.clone(), options);
    }

    /// A group commit of `key`'s target set is running.
    pub fn busy(&self, key: &DraftKey) -> bool {
        self.pending
            .as_ref()
            .is_some_and(|pending| &pending.key == key)
    }

    /// A group commit of the workspace is running.
    pub fn running(&self) -> bool {
        self.pending.is_some()
    }

    pub fn error(&self, key: &DraftKey) -> Option<&str> {
        self.errors.get(key).map(String::as_str)
    }

    /// Commit was pressed for `key`: the message to send, unless a group
    /// commit runs.
    pub fn press(&mut self, key: &DraftKey) -> Option<CommitMessage> {
        if self.pending.is_some() {
            return None;
        }
        let draft = self.draft(key);
        self.errors.remove(key);
        self.sent = Some((key.clone(), draft.clone()));
        Some(draft)
    }

    /// The commit sent last started as `job`. `true` when the box changed,
    /// as when the job had already ended.
    pub fn started(&mut self, job: JobId) -> bool {
        let Some((key, sent)) = self.sent.take() else {
            return false;
        };
        self.pending = Some(GroupPending { key, job, sent });
        let early = self.ended_early.iter().position(|(ended, _)| *ended == job);
        let ended = early.and_then(|at| self.ended_early.remove(at));
        self.ended_early.clear();
        match ended {
            Some((_, committed)) => self.finish(committed),
            None => false,
        }
    }

    /// Job `job` ended. The box is cleared only when every repo that ran
    /// committed and the user did not change it meanwhile; else it keeps
    /// the message and says a repo did not commit (UI-34). `true` when the
    /// box changed.
    pub fn job_ended(&mut self, job: &JobView) -> bool {
        if job.report.is_none() {
            return false;
        }
        let committed = all_committed(job);
        if self.pending.as_ref().map(|pending| pending.job) == Some(job.job_id) {
            return self.finish(committed);
        }
        if self.sent.is_some() {
            self.ended_early.push_back((job.job_id, committed));
            while self.ended_early.len() > EARLY_KEPT {
                self.ended_early.pop_front();
            }
        }
        false
    }

    fn finish(&mut self, committed: bool) -> bool {
        let Some(pending) = self.pending.take() else {
            return false;
        };
        if !committed {
            self.errors
                .insert(pending.key, text::GROUP_COMMIT_FAILED.to_owned());
        } else if self.draft(&pending.key) == pending.sent {
            self.drafts.remove(&pending.key);
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use gitcroc_core::{JobState, RepoResult, SkipReason, WorkspaceId};

    use super::*;

    fn message(subject: &str) -> CommitMessage {
        CommitMessage {
            subject: subject.to_owned(),
            body: String::new(),
        }
    }

    fn ended(job: u32, outcome: Outcome) -> JobView {
        JobView {
            job_id: JobId(job),
            workspace_id: WorkspaceId("w".into()),
            workspace_name: "w".into(),
            label: String::new(),
            repos: Vec::new(),
            repo_names: Vec::new(),
            state: JobState::Ended,
            done: 0,
            total: 0,
            step: None,
            started_at: None,
            duration_ms: None,
            outcome: Some(outcome),
            output: String::new(),
            message: None,
            sub_jobs: Vec::new(),
            report: None,
            ai_files: Vec::new(),
        }
    }

    fn group_ended(job: u32, outcomes: &[Outcome]) -> JobView {
        let mut view = ended(job, Outcome::Succeeded);
        view.report = Some(
            outcomes
                .iter()
                .enumerate()
                .map(|(n, outcome)| RepoResult {
                    repo_id: RepoId(format!("r{n}")),
                    repo_name: format!("r{n}"),
                    outcome: outcome.clone(),
                    output: String::new(),
                    message: None,
                })
                .collect(),
        );
        view
    }

    #[test]
    fn ui_34_the_draft_is_kept_per_target_set() {
        let (a, b) = (RepoId("a".into()), RepoId("b".into()));
        let mut drafts = GroupDrafts::default();
        let one = DraftKey::of(&TargetSet::Repos {
            repo_ids: vec![b.clone(), a.clone()],
        });
        drafts.edit(&one, message("Both"));
        drafts.set_options(
            &one,
            GroupOptions {
                stage_all: true,
                push: false,
            },
        );
        // The same repos in another order are the same target set.
        let same = DraftKey::of(&TargetSet::Repos {
            repo_ids: vec![a.clone(), b],
        });
        assert_eq!(drafts.draft(&same), message("Both"));
        assert!(drafts.options(&same).stage_all);
        let other = DraftKey::of(&TargetSet::Repos { repo_ids: vec![a] });
        assert_eq!(drafts.draft(&other), CommitMessage::default());
        assert_eq!(drafts.options(&other), GroupOptions::default());
    }

    #[test]
    fn ui_34_cleared_only_when_every_repo_that_ran_committed() {
        let key = DraftKey::Group(GroupId("g".into()));
        let mut drafts = GroupDrafts::default();
        drafts.edit(&key, message("One"));
        assert_eq!(drafts.press(&key), Some(message("One")));
        assert!(!drafts.started(JobId(1)));
        assert!(drafts.busy(&key));
        assert_eq!(drafts.press(&key), None);
        let skipped = Outcome::Skipped {
            reason: SkipReason::Cancelled,
        };
        assert!(drafts.job_ended(&group_ended(1, &[Outcome::Succeeded, skipped])));
        assert_eq!(drafts.draft(&key), CommitMessage::default());

        // A hook refused one: the message stays and the box says so.
        drafts.edit(&key, message("Two"));
        drafts.press(&key);
        // The job ended before its start was answered.
        assert!(!drafts.job_ended(&group_ended(2, &[Outcome::Succeeded, Outcome::Failed])));
        assert!(drafts.started(JobId(2)));
        assert_eq!(drafts.draft(&key), message("Two"));
        assert_eq!(drafts.error(&key), Some(text::GROUP_COMMIT_FAILED));
        assert!(!drafts.busy(&key));
        // Sent again, the error goes.
        drafts.press(&key);
        assert_eq!(drafts.error(&key), None);
    }

    #[test]
    fn ui_15_amend_fills_an_empty_box_then_sends() {
        let repo = RepoId("api".into());
        let mut drafts = Drafts::default();
        let last = message("Last");
        assert_eq!(drafts.press(&repo, true, Some(&last)), Press::Filled);
        assert_eq!(drafts.draft(&repo), last);
        assert_eq!(drafts.press(&repo, true, Some(&last)), Press::Send(last));
        assert!(drafts.busy(&repo));
        assert_eq!(drafts.press(&repo, false, None), Press::Nothing);
    }

    #[test]
    fn ui_15_success_clears_only_the_message_sent() {
        let repo = RepoId("api".into());
        let mut drafts = Drafts::default();
        drafts.edit(&repo, message("One"));
        drafts.press(&repo, false, None);
        assert!(!drafts.answered(&repo, Ok(JobId(1))));
        assert!(drafts.job_ended(&ended(1, Outcome::Succeeded)));
        assert_eq!(drafts.draft(&repo), CommitMessage::default());

        // Changed while it ran: kept.
        drafts.edit(&repo, message("Two"));
        drafts.press(&repo, false, None);
        drafts.answered(&repo, Ok(JobId(2)));
        drafts.edit(&repo, message("Two, changed"));
        drafts.job_ended(&ended(2, Outcome::Succeeded));
        assert_eq!(drafts.draft(&repo), message("Two, changed"));
    }

    #[test]
    fn ui_15_a_failure_keeps_the_message_and_says_so() {
        let repo = RepoId("api".into());
        let mut drafts = Drafts::default();
        drafts.edit(&repo, message("One"));
        drafts.press(&repo, false, None);
        // The job ended before its start was answered.
        assert!(!drafts.job_ended(&ended(7, Outcome::Failed)));
        assert!(drafts.answered(&repo, Ok(JobId(7))));
        assert_eq!(drafts.draft(&repo), message("One"));
        assert_eq!(drafts.error(&repo), Some(text::COMMIT_FAILED));
        assert!(!drafts.busy(&repo));
    }
}
