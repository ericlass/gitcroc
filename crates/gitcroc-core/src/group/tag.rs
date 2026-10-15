//! Group create of tag X (section 5 "Create tag X" row): X was checked
//! once against git's ref format (SEC-4) and is created at each repo's own
//! `HEAD` (GRP-14), with the single repo's code (`crate::tag`). With push,
//! the same sub-job then pushes it where a single repo's push goes. The
//! run plans again from the repo as it is then, as `super::sync` does.

use std::path::PathBuf;

use gitcroc_git::{GitRepo, GitRunner};

use super::op::{Class, GroupOperation, RunFuture};
use crate::tag::{GroupTag, group_plan, run_plan};
use crate::{JobRun, NothingToDo, OnCancel, SubJobError, SubJobSpec};

/// Create tag on a target set.
pub(crate) struct GroupTags {
    pub(crate) name: String,
    pub(crate) tag: GroupTag,
}

impl GroupOperation for GroupTags {
    fn name(&self) -> String {
        self.name.clone()
    }

    fn destructive(&self) -> Vec<String> {
        Vec::new()
    }

    fn spec(&self) -> SubJobSpec {
        SubJobSpec {
            on_cancel: OnCancel::Finish,
            may_conflict: false,
            nothing_to_do: NothingToDo::Never,
            network: self.tag.push,
            conflict_allowed: false,
        }
    }

    fn classify(&self, repo: &GitRepo, _git: &GitRunner) -> Class {
        match group_plan(repo, &self.tag) {
            Ok(_) => Class::WillRun,
            Err(reason) => Class::WillSkip { reason },
        }
    }

    fn run(&self, job: JobRun, path: PathBuf) -> RunFuture {
        let tag = self.tag.clone();
        Box::pin(async move {
            let read = path.clone();
            let plan = tokio::task::spawn_blocking(move || {
                let repo =
                    GitRepo::open(&read).map_err(|error| SubJobError::Failed(error.to_string()))?;
                group_plan(&repo, &tag).map_err(SubJobError::Skip)
            })
            .await
            .map_err(|error| SubJobError::Failed(format!("the read did not finish: {error}")))??;
            Ok(run_plan(&job, &path, &plan).await?)
        })
    }
}
