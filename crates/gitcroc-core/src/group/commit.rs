//! Group commit (GRP-15, section 5 "Commit" row): one commit per repo with
//! staged changes, each with the same message, through `git commit -F -`
//! as a single repo's commit (M2-05). Hooks and signing follow git config
//! (SEC-6). The commits are not linked: one rejected, as by a hook, is
//! *failed* while the others still commit (GRP-6). With "Stage all tracked
//! changes first" a repo with tracked changes counts as staged and runs
//! `git add -u` before it commits; with "Push after commit" the same
//! sub-job then pushes the current branch to its upstream, as a group push
//! does (M3-10). The run reads the repo again, as `super::sync` does.

use std::collections::HashSet;
use std::path::PathBuf;

use gitcroc_git::ops::{self, CommitMode};
use gitcroc_git::{
    ChangedFile, FileStatus, GitError, GitRepo, GitRunner, Output, StatusRead, StatusReader,
    WorkingCopy,
};

use super::op::{Class, GroupOperation, RunFuture};
use crate::sync::{group_plan, run_plan};
use crate::{JobRun, NothingToDo, OnCancel, SubJobError, SubJobSpec, SyncOp};

/// Why a repo is left out.
const NOTHING_STAGED: &str = "nothing staged";

/// Where a push after the commit goes: the current branch's upstream,
/// never one set on the way.
const PUSH: SyncOp = SyncOp::Push {
    set_upstream: false,
    remote: None,
    force_with_lease: false,
};

/// Commit on a target set.
pub(crate) struct GroupCommit {
    /// The message as git gets it.
    pub(crate) message: String,
    pub(crate) stage_all: bool,
    pub(crate) push: bool,
}

/// The number of files a commit takes from `working`: the staged ones, and
/// with `stage_all` every changed tracked one, each path once.
fn committed(working: &WorkingCopy, stage_all: bool) -> u32 {
    let tracked = working
        .unstaged
        .iter()
        .filter(|file| stage_all && file.status != FileStatus::Untracked);
    let paths: HashSet<&[u8]> = working
        .staged
        .iter()
        .chain(tracked)
        .map(ChangedFile::git_path)
        .collect();
    u32::try_from(paths.len()).unwrap_or(u32::MAX)
}

/// The number of files a commit takes from `repo`, as [`committed`].
fn count(repo: &GitRepo, git: &GitRunner, stage_all: bool) -> Result<u32, String> {
    let working = StatusReader::new(repo.clone(), git.clone())
        .status()
        .map_err(|error| format!("cannot read the status: {error}"))?;
    Ok(committed(&working, stage_all))
}

impl GroupOperation for GroupCommit {
    fn name(&self) -> String {
        "Commit".to_owned()
    }

    fn destructive(&self) -> Vec<String> {
        Vec::new()
    }

    /// The confirmation lists what each repo commits before anything is
    /// committed (GRP-15).
    fn always_confirm(&self) -> bool {
        true
    }

    fn spec(&self) -> SubJobSpec {
        SubJobSpec {
            on_cancel: OnCancel::Finish,
            may_conflict: false,
            nothing_to_do: NothingToDo::Never,
            network: self.push,
            conflict_allowed: false,
        }
    }

    fn classify(&self, repo: &GitRepo, git: &GitRunner) -> Class {
        self.classify_counted(repo, git).0
    }

    fn classify_counted(&self, repo: &GitRepo, git: &GitRunner) -> (Class, Option<u32>) {
        let files = match count(repo, git, self.stage_all) {
            Ok(0) => {
                let reason = NOTHING_STAGED.to_owned();
                return (Class::WillSkip { reason }, None);
            }
            Ok(files) => files,
            Err(reason) => return (Class::Blocked { reason }, None),
        };
        let class = match self.push.then(|| group_plan(repo, &PUSH)) {
            Some(Err(reason)) => Class::NeedsAttention {
                reason: format!("{reason}; the commit will not be pushed"),
            },
            _ => Class::WillRun,
        };
        (class, Some(files))
    }

    fn run(&self, job: JobRun, path: PathBuf) -> RunFuture {
        let (message, stage_all, push) = (self.message.clone(), self.stage_all, self.push);
        Box::pin(async move {
            let (read, git) = (path.clone(), job.git.clone());
            let sign_off = tokio::task::spawn_blocking(move || {
                let repo =
                    GitRepo::open(&read).map_err(|error| SubJobError::Failed(error.to_string()))?;
                match count(&repo, &git, stage_all) {
                    Ok(0) => Err(SubJobError::Skip(NOTHING_STAGED.to_owned())),
                    Ok(_) => Ok(repo.sign_off()),
                    Err(reason) => Err(SubJobError::Failed(reason)),
                }
            })
            .await
            .map_err(|error| SubJobError::Failed(format!("the read did not finish: {error}")))??;
            let mut output = Output {
                stdout: Vec::new(),
                stderr: Vec::new(),
            };
            if stage_all {
                output.append(ops::stage_tracked(&job.git, &path, job.write()).await?);
            }
            let mode = CommitMode {
                amend: false,
                sign_off,
            };
            let commit = ops::commit(&job.git, &path, message.as_bytes(), mode, job.write());
            output.append(commit.await?);
            if !push {
                return Ok(output);
            }
            let read = path.clone();
            let plan = tokio::task::spawn_blocking(move || {
                let repo = GitRepo::open(&read).map_err(|error| error.to_string())?;
                group_plan(&repo, &PUSH).map(|(plan, _)| plan)
            })
            .await
            .unwrap_or_else(|error| Err(format!("the read did not finish: {error}")));
            let failed = |output, why: String| SubJobError::Partial {
                output,
                message: format!("the commit was made; the push failed: {why}"),
            };
            let plan = match plan {
                Ok(plan) => plan,
                Err(reason) => return Err(failed(output, reason)),
            };
            match run_plan(&job, &path, &plan).await {
                Ok(pushed) => {
                    output.append(pushed);
                    Ok(output)
                }
                Err(error) => {
                    if let GitError::Failed { stdout, stderr, .. } = &error {
                        output.append(Output {
                            stdout: stdout.clone(),
                            stderr: stderr.clone(),
                        });
                    }
                    Err(failed(output, error.to_string()))
                }
            }
        })
    }
}
