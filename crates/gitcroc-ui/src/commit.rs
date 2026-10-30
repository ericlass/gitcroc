//! The commit message box of one workspace's "Uncommitted changes"
//! (UI-15), a port of rc.1's `stores/commit.ts` for a single repo: the
//! message the user is writing, kept per repo (ARCH-9) while the app runs
//! (UI-29), and the commit or amend job it was sent with. The core composes
//! the message, checks the commit can be made and runs it (ARCH-6); this
//! keeps the unsent text and clears it when the commit succeeded, if the
//! box still holds what was sent. Nothing here is saved.

use std::collections::{HashMap, VecDeque};

use gitcroc_core::{CommitMessage, JobId, JobView, Outcome, RepoId};

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

#[cfg(test)]
mod tests {
    use gitcroc_core::{JobState, WorkspaceId};

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
