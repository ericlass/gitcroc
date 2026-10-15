//! The group-capable operations of section 5 and what an operation's code
//! supplies to run on a target set: a read-only classification of a repo
//! and the single-repo run. Each operation's own rules arrive with its task
//! (M3-10 to M3-13); until then it is refused as not available yet.
//! Fetch, pull and push are in `super::sync`; checkout, create and delete
//! of a branch in `super::branch`; merge and rebase in `super::integrate`;
//! create tag in `super::tag`.

use std::path::PathBuf;
use std::pin::Pin;

use gitcroc_git::{GitRepo, GitRunner, Output};
use serde::{Deserialize, Serialize};

use super::GroupError;
use super::branch::GroupBranches;
use super::integrate::GroupIntegrate;
use super::sync::GroupSync;
use super::tag::GroupTags;
use crate::branch::GroupBranchOp;
use crate::integrate::GroupIntegrateOp;
use crate::ref_name::{check_branch_name, check_tag_name};
use crate::tag::GroupTag;
use crate::{
    CommitMessage, JobRun, MergeFastForward, Prune, PullStrategy, StartPoint, SubJobError,
    SubJobSpec, SyncOp,
};

/// What a group checkout does in a repo without branch X (section 5
/// "Checkout branch X").
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "snake_case")]
pub enum MissingBranch {
    /// The repo is skipped.
    Skip,
    /// The repo switches to its default branch.
    DefaultBranch,
    /// X is created from `HEAD`.
    Create,
}

/// A group-capable operation with its options (section 5 "Group-capable
/// operations"). Names are typed or picked once and resolved in each repo
/// (GRP-14).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum GroupOp {
    /// Fetches all remotes.
    Fetch { prune: Prune },
    /// Updates the current branch from its upstream.
    Pull { strategy: PullStrategy },
    /// Pushes the current branch to its upstream, or to `origin/<branch>`
    /// with `set_upstream`. `force_with_lease` is destructive (GRP-9).
    Push {
        set_upstream: bool,
        force_with_lease: bool,
    },
    /// Switches to branch `name`.
    Checkout {
        name: String,
        missing: MissingBranch,
    },
    /// Creates branch `name` at `start`, and switches to it with `switch`.
    CreateBranch {
        name: String,
        start: StartPoint,
        switch: bool,
    },
    /// Deletes branch `name`. `force` and `on_remote` are destructive
    /// (GRP-9).
    DeleteBranch {
        name: String,
        force: bool,
        on_remote: bool,
    },
    /// Merges ref `name` into the current branch.
    Merge {
        name: String,
        fast_forward: MergeFastForward,
    },
    /// Rebases the current branch onto ref `onto`.
    Rebase { onto: String },
    /// Creates tag `name` at `HEAD`, annotated with `message` when
    /// `annotated`, and pushes it with `push`.
    CreateTag {
        name: String,
        annotated: bool,
        message: String,
        push: bool,
    },
    /// Commits the staged changes with the one shared message (GRP-15).
    Commit {
        message: CommitMessage,
        stage_all: bool,
        push: bool,
    },
}

impl GroupOp {
    /// The operation as a job's label names it, such as `Pull` or
    /// `Checkout main`.
    pub fn name(&self) -> String {
        match self {
            Self::Fetch { .. } => "Fetch".to_owned(),
            Self::Pull { .. } => "Pull".to_owned(),
            Self::Push {
                force_with_lease: true,
                ..
            } => "Force push".to_owned(),
            Self::Push { .. } => "Push".to_owned(),
            Self::Checkout { name, .. } => format!("Checkout {name}"),
            Self::CreateBranch { name, .. } => format!("Create branch {name}"),
            Self::DeleteBranch { name, .. } => format!("Delete branch {name}"),
            Self::Merge { name, .. } => format!("Merge {name}"),
            Self::Rebase { onto } => format!("Rebase onto {onto}"),
            Self::CreateTag { name, .. } => format!("Create tag {name}"),
            Self::Commit { .. } => "Commit".to_owned(),
        }
    }

    /// What cannot be undone, for the confirmation that names every
    /// affected repo (GRP-9): force push, force delete of a branch and
    /// delete of a remote branch. Empty when nothing is destructive.
    pub fn destructive(&self) -> Vec<String> {
        match self {
            Self::Push {
                force_with_lease: true,
                ..
            } => vec![
                "Force push the current branch; commits only on the remote branch may be lost"
                    .to_owned(),
            ],
            Self::DeleteBranch {
                name,
                force,
                on_remote,
            } => {
                let mut actions = Vec::new();
                if *force {
                    actions.push(format!(
                        "Force-delete branch {name} even if it is not merged; its commits may be lost"
                    ));
                }
                if *on_remote {
                    actions.push(format!("Delete branch {name} on the remote"));
                }
                actions
            }
            _ => Vec::new(),
        }
    }

    /// The code that runs it on a target set; refused until the
    /// operation's task brings its preflight rule and run. A branch or tag
    /// name, the ref to merge or rebase onto and a tag's message are
    /// checked here, once for every repo (SEC-4, GRP-14).
    pub fn operation(&self) -> Result<std::sync::Arc<dyn GroupOperation>, GroupError> {
        let (name, destructive) = (self.name(), self.destructive());
        let op = match self.clone() {
            Self::Fetch { prune } => SyncOp::Fetch { prune },
            Self::Pull { strategy } => SyncOp::Pull { strategy },
            Self::Push {
                set_upstream,
                force_with_lease,
            } => SyncOp::Push {
                set_upstream,
                remote: None,
                force_with_lease,
            },
            Self::Checkout { name: x, missing } => {
                check_branch_name(&x)?;
                let op = GroupBranchOp::Checkout { name: x, missing };
                return Ok(branches(name, destructive, op));
            }
            Self::CreateBranch {
                name: x,
                start,
                switch,
            } => {
                check_branch_name(&x)?;
                let op = GroupBranchOp::Create {
                    name: x,
                    start,
                    switch,
                };
                return Ok(branches(name, destructive, op));
            }
            Self::DeleteBranch {
                name: x,
                force,
                on_remote,
            } => {
                check_branch_name(&x)?;
                let op = GroupBranchOp::Delete {
                    name: x,
                    force,
                    on_remote,
                };
                return Ok(branches(name, destructive, op));
            }
            Self::Merge {
                name: x,
                fast_forward,
            } => {
                check_revision(&x)?;
                let op = GroupIntegrateOp::Merge {
                    name: x,
                    fast_forward,
                };
                return Ok(std::sync::Arc::new(GroupIntegrate { name, op }));
            }
            Self::Rebase { onto } => {
                check_revision(&onto)?;
                let op = GroupIntegrateOp::Rebase { onto };
                return Ok(std::sync::Arc::new(GroupIntegrate { name, op }));
            }
            Self::CreateTag {
                name: x,
                annotated,
                message,
                push,
            } => {
                check_tag_name(&x)?;
                let message = if annotated {
                    if message.trim().is_empty() {
                        return Err(GroupError::Refused(
                            "an annotated tag needs a message".to_owned(),
                        ));
                    }
                    if message.contains('\0') {
                        return Err(GroupError::Refused(
                            "the message cannot contain a NUL character".to_owned(),
                        ));
                    }
                    Some(message)
                } else {
                    None
                };
                let tag = GroupTag {
                    name: x,
                    message,
                    push,
                };
                return Ok(std::sync::Arc::new(GroupTags { name, tag }));
            }
            Self::Commit { .. } => return Err(GroupError::NotAvailable(name)),
        };
        Ok(std::sync::Arc::new(GroupSync {
            name,
            destructive,
            op,
        }))
    }
}

/// `revision` can name a ref or commit: git would not read it as an
/// option.
fn check_revision(revision: &str) -> Result<(), GroupError> {
    if revision.trim().is_empty() || revision.starts_with('-') {
        return Err(GroupError::Refused(format!(
            "'{revision}' names no branch, tag or commit"
        )));
    }
    Ok(())
}

fn branches(
    name: String,
    destructive: Vec<String>,
    op: GroupBranchOp,
) -> std::sync::Arc<dyn GroupOperation> {
    std::sync::Arc::new(GroupBranches {
        name,
        destructive,
        op,
    })
}

/// A repo's class in a preflight (section 5, step 1).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Class {
    WillRun,
    WillSkip {
        reason: String,
    },
    /// GRP-8: missing, in conflict state, or busy with another write.
    Blocked {
        reason: String,
    },
    /// It runs, but the user should look first, as at a predicted conflict
    /// or a missing upstream.
    NeedsAttention {
        reason: String,
    },
}

/// What [`GroupOperation::run`] returns.
pub type RunFuture = Pin<Box<dyn Future<Output = Result<Output, SubJobError>> + Send>>;

/// An operation that runs on a target set: what its code supplies to the
/// engine of [`super::GroupOps`].
pub trait GroupOperation: Send + Sync + 'static {
    /// The operation as a job's label names it, such as `Pull`.
    fn name(&self) -> String;

    /// What cannot be undone (GRP-9); empty when nothing is. When it is
    /// not empty, the preflight always asks.
    fn destructive(&self) -> Vec<String>;

    /// How each sub-job runs.
    fn spec(&self) -> SubJobSpec;

    /// The class of `repo` for this operation, once GRP-8 let it pass.
    /// Read-only: it reads with the read API or `git`'s reads, never
    /// writes. Runs on a thread that may block.
    fn classify(&self, repo: &GitRepo, git: &GitRunner) -> Class;

    /// Runs the operation in the repo at `path`, through
    /// [`gitcroc_git::ops`] with [`JobRun::write`].
    fn run(&self, job: JobRun, path: PathBuf) -> RunFuture;
}
