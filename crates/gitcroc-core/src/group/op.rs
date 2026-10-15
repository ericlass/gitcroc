//! The group-capable operations of section 5 and what an operation's code
//! supplies to run on a target set: a read-only classification of a repo
//! and the single-repo run. Each operation's own rules arrive with its task
//! (M3-10 to M3-13); until then it is refused as not available yet.

use std::path::PathBuf;
use std::pin::Pin;

use gitcroc_git::{GitError, GitRepo, GitRunner, Output};
use serde::{Deserialize, Serialize};

use super::GroupError;
use crate::{CommitMessage, JobRun, MergeFastForward, Prune, PullStrategy, StartPoint, SubJobSpec};

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

    /// Whether it is destructive (GRP-9): force push, force delete of a
    /// branch and delete of a remote branch.
    pub fn destructive(&self) -> bool {
        matches!(
            self,
            Self::Push {
                force_with_lease: true,
                ..
            } | Self::DeleteBranch { force: true, .. }
                | Self::DeleteBranch {
                    on_remote: true,
                    ..
                }
        )
    }

    /// The code that runs it on a target set; refused until the
    /// operation's task brings its preflight rule and run.
    pub fn operation(&self) -> Result<std::sync::Arc<dyn GroupOperation>, GroupError> {
        Err(GroupError::NotAvailable(self.name()))
    }
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
pub type RunFuture = Pin<Box<dyn Future<Output = Result<Output, GitError>> + Send>>;

/// An operation that runs on a target set: what its code supplies to the
/// engine of [`super::GroupOps`].
pub trait GroupOperation: Send + Sync + 'static {
    /// The operation as a job's label names it, such as `Pull`.
    fn name(&self) -> String;

    /// Whether it is destructive (GRP-9): the preflight then always asks.
    fn destructive(&self) -> bool;

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
