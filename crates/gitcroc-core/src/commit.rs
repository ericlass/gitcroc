//! Commit and amend in "Uncommitted changes" (UI-15): the staged changes of
//! a view's repo, with the message the user wrote, as one job (ARCH-7).
//! git runs the repo's hooks and signs as its config says (SEC-6); the
//! app adds the sign-off `format.signOff` asks for (section 6). In
//! conflict state the commit is Continue's (M2-08), so it is refused here.

use std::path::PathBuf;
use std::sync::Arc;

use gitcroc_git::ops::{self, CommitMode};
use gitcroc_git::{DiffRead, GitRepo, StatusReader};
use serde::{Deserialize, Serialize};

use crate::changes::Lists;
use crate::{Changes, ChangesError, DiffReadError, GitRunner, JobId, Jobs, WorkspaceId, WriteJob};

/// A commit message as the commit box edits it.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
pub struct CommitMessage {
    /// The first line.
    pub subject: String,
    /// The rest; may be empty.
    pub body: String,
}

impl CommitMessage {
    /// `message` as the box shows it: the first line, then the rest
    /// without the blank lines between them and at the end.
    pub fn split(message: &str) -> Self {
        let (subject, body) = message.split_once('\n').unwrap_or((message, ""));
        Self {
            subject: subject.trim_end().to_owned(),
            body: body.trim_start_matches(['\r', '\n']).trim_end().to_owned(),
        }
    }

    /// The message git gets: the subject, a blank line and the body,
    /// trailing whitespace trimmed; an empty body adds nothing.
    pub fn text(&self) -> String {
        let subject = self.subject.trim_end();
        let body = self.body.trim_end();
        if body.is_empty() {
            format!("{subject}\n")
        } else {
            format!("{subject}\n\n{body}\n")
        }
    }
}

/// A commit or amend request of the changes pane.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
pub struct CommitRequest {
    /// The view of the uncommitted changes the commit is made from.
    pub handle: u32,
    pub message: CommitMessage,
    /// Replace the last commit instead of adding one.
    pub amend: bool,
}

/// The message of the commit `HEAD` points to; `None` on a branch with no
/// commit yet.
pub(crate) async fn last_message(repo: GitRepo) -> Result<Option<CommitMessage>, ChangesError> {
    let message = tokio::task::spawn_blocking(move || {
        if repo.unborn() {
            return Ok(None);
        }
        repo.commit("HEAD")
            .map(|details| Some(CommitMessage::split(&details.message)))
    })
    .await?;
    message.map_err(|error| ChangesError::Diff(DiffReadError::from(error)))
}

/// What the repo at `path` says about a commit now.
struct Head {
    unborn: bool,
    in_conflict: bool,
    sign_off: bool,
}

async fn head(path: PathBuf, git: GitRunner) -> Result<Head, ChangesError> {
    let repo = tokio::task::spawn_blocking(move || GitRepo::open(&path)).await??;
    let unborn = repo.unborn();
    let sign_off = repo.sign_off();
    let state = crate::repo_state(StatusReader::new(repo, git)).await?;
    Ok(Head {
        unborn,
        in_conflict: state.in_conflict(),
        sign_off,
    })
}

/// The job's label.
fn label(amend: bool) -> &'static str {
    if amend {
        "Amend the last commit"
    } else {
        "Commit"
    }
}

/// Commits the staged changes of view `request.handle` of workspace `id`,
/// or amends the last commit with them (UI-15), as a job. Refused without
/// a subject, in conflict state, for a commit with nothing staged and for
/// an amend with no commit to amend. `None` when a newer view of the
/// workspace replaced the request's.
pub async fn changes_commit(
    changes: Arc<Changes>,
    jobs: Arc<Jobs>,
    git: GitRunner,
    id: WorkspaceId,
    request: CommitRequest,
) -> Result<Option<JobId>, ChangesError> {
    let CommitRequest {
        handle,
        message,
        amend,
    } = request;
    let Some(view) = changes.state(&id, handle) else {
        return Ok(None);
    };
    let Lists::Uncommitted { staged, .. } = &*view.lists else {
        return Err(ChangesError::Refused(
            "only uncommitted changes are committed",
        ));
    };
    if message.subject.trim().is_empty() {
        return Err(ChangesError::Refused("the commit message has no subject"));
    }
    let head = head(view.path.clone(), git.clone()).await?;
    if head.in_conflict {
        return Err(ChangesError::Refused(
            "the repo is in conflict state; Continue makes the commit",
        ));
    }
    if amend && head.unborn {
        return Err(ChangesError::Refused("the branch has no commit to amend"));
    }
    if !amend && staged.is_empty() {
        return Err(ChangesError::Refused("nothing is staged"));
    }
    let message = message.text();
    let mode = CommitMode {
        amend,
        sign_off: head.sign_off,
    };
    let job = WriteJob::single(id, view.repo, label(amend).to_owned());
    let handle = jobs.start(git, job, move |job| async move {
        let path = view.path;
        ops::commit(&job.git, &path, message.as_bytes(), mode, job.write()).await
    })?;
    Ok(Some(handle.job_id))
}

#[cfg(test)]
mod tests {
    use super::CommitMessage;

    fn message(subject: &str, body: &str) -> CommitMessage {
        CommitMessage {
            subject: subject.to_owned(),
            body: body.to_owned(),
        }
    }

    #[test]
    fn ui_15_message_is_subject_blank_line_and_body() {
        assert_eq!(message("Fix it", "").text(), "Fix it\n");
        assert_eq!(message("Fix it  ", " \n\t\n").text(), "Fix it\n");
        assert_eq!(
            message("Fix it", "Why it broke.\n\nHow.  \n\n").text(),
            "Fix it\n\nWhy it broke.\n\nHow.\n"
        );
    }

    #[test]
    fn ui_15_amend_splits_the_last_message() {
        assert_eq!(CommitMessage::split("Fix it\n"), message("Fix it", ""));
        assert_eq!(
            CommitMessage::split("Fix it\r\n\r\nWhy.\r\n\r\nHow.\r\n"),
            message("Fix it", "Why.\r\n\r\nHow.")
        );
        let split = CommitMessage::split("Fix it\n\nWhy.\n\nHow.\n");
        assert_eq!(split, message("Fix it", "Why.\n\nHow."));
        assert_eq!(split.text(), "Fix it\n\nWhy.\n\nHow.\n");
    }
}
