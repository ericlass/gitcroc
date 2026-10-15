//! Group fetch, pull and push (GRP-12, section 5 "Fetch", "Pull" and
//! "Push" rows): each repo is classed and run with the single repo's code
//! (`crate::sync`). The run plans again from the repo as it is then: a
//! repo that no longer fits, as when `HEAD` was detached since the
//! preflight, is *skipped* with the preflight's reason. Each repo pulls or
//! pushes its own current branch, whatever branch that is. Nothing is
//! stashed to make a pull succeed (GRP-12).

use std::path::PathBuf;

use gitcroc_git::{GitRepo, GitRunner};

use super::op::{Class, GroupOperation, RunFuture};
use crate::sync::{group_plan, run_plan};
use crate::{JobRun, NothingToDo, OnCancel, SubJobError, SubJobSpec, SyncOp};

/// Fetch, pull or push on a target set.
pub(crate) struct GroupSync {
    pub(crate) name: String,
    pub(crate) destructive: Vec<String>,
    pub(crate) op: SyncOp,
}

impl GroupOperation for GroupSync {
    fn name(&self) -> String {
        self.name.clone()
    }

    fn destructive(&self) -> Vec<String> {
        self.destructive.clone()
    }

    fn spec(&self) -> SubJobSpec {
        let (on_cancel, may_conflict, conflict_allowed) = match self.op {
            // GRP-10: a fetch is killed, and a pull while it fetches; a push
            // runs to its end. Fetch may run in conflict state (GRP-8).
            SyncOp::Fetch { .. } => (OnCancel::Kill, false, true),
            SyncOp::Pull { .. } => (OnCancel::Kill, true, false),
            SyncOp::Push { .. } => (OnCancel::Finish, false, false),
        };
        SubJobSpec {
            on_cancel,
            may_conflict,
            // A push that sets the upstream adds the remote-tracking
            // branch, so it changed a ref.
            nothing_to_do: NothingToDo::WhenRefsUnchanged,
            network: true,
            conflict_allowed,
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
            Ok(run_plan(&job, &path, &plan).await?)
        })
    }
}
