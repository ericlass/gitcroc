//! Group checkout, create and delete of branch X (section 5 "Checkout
//! branch X", "Create branch X" and "Delete branch X" rows): X was checked
//! once against git's ref format (SEC-4) and each repo resolves it on its
//! own; repos where X points at different commits are normal (GRP-14).
//! Each repo is classed and run with the single repo's code
//! (`crate::branch`), and the run plans again from the repo as it is then,
//! as `super::sync` does. Nothing is stashed to make a checkout succeed
//! (GRP-12).

use std::path::PathBuf;

use gitcroc_git::{GitRepo, GitRunner};

use super::op::{Class, GroupOperation, RunFuture};
use crate::branch::{GroupBranchOp, group_plan, run_plan};
use crate::{JobRun, NothingToDo, OnCancel, SubJobError, SubJobSpec};

/// Checkout, create or delete of a branch on a target set.
pub(crate) struct GroupBranches {
    pub(crate) name: String,
    pub(crate) destructive: Vec<String>,
    pub(crate) op: GroupBranchOp,
}

impl GroupOperation for GroupBranches {
    fn name(&self) -> String {
        self.name.clone()
    }

    fn destructive(&self) -> Vec<String> {
        self.destructive.clone()
    }

    fn spec(&self) -> SubJobSpec {
        let (nothing_to_do, network) = match self.op {
            // A repo already on X is *nothing to do*, read from `HEAD`
            // and the refs before and after.
            GroupBranchOp::Checkout { .. } => (NothingToDo::WhenRefsUnchanged, false),
            GroupBranchOp::Create { .. } => (NothingToDo::Never, false),
            GroupBranchOp::Delete { on_remote, .. } => (NothingToDo::Never, on_remote),
        };
        SubJobSpec {
            on_cancel: OnCancel::Finish,
            may_conflict: false,
            nothing_to_do,
            network,
            conflict_allowed: false,
        }
    }

    fn classify(&self, repo: &GitRepo, _git: &GitRunner) -> Class {
        match group_plan(repo, &self.op) {
            Ok((_, None)) => Class::WillRun,
            Ok((_, Some(reason))) => Class::NeedsAttention { reason },
            Err(reason) => Class::WillSkip { reason },
        }
    }

    fn run(&self, job: JobRun, path: PathBuf) -> RunFuture {
        let op = self.op.clone();
        Box::pin(async move {
            let read = path.clone();
            let planned = tokio::task::spawn_blocking(move || {
                let repo =
                    GitRepo::open(&read).map_err(|error| SubJobError::Failed(error.to_string()))?;
                group_plan(&repo, &op).map_err(SubJobError::Skip)
            })
            .await
            .map_err(|error| SubJobError::Failed(format!("the read did not finish: {error}")))??;
            let (plan, _) = planned;
            Ok(run_plan(&job.git, &path, &plan, job.write()).await?)
        })
    }
}
