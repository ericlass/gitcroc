//! Group merge of X into the current branch and rebase of it onto X
//! (section 5 "Merge X into current" and "Rebase current onto X" rows):
//! each repo resolves X on its own (GRP-14) and is classed and run with
//! the single repo's code (`crate::integrate`), the run planning again from
//! the repo as it is then, as `super::sync` does. The preflight predicts
//! conflicts with `git merge-tree` (GRP-13); the prediction is advisory and
//! the run decides. A merge or rebase git stops is *conflict* (OPS-1), and
//! the repo shows its banner (OPS-2). Nothing is stashed (GRP-12).

use std::path::PathBuf;

use gitcroc_git::{GitRepo, GitRunner, MergePrediction, predict_merge};

use super::op::{Class, GroupOperation, RunFuture};
use crate::integrate::{GroupIntegrateOp, group_plan, run_plan};
use crate::{JobRun, NothingToDo, OnCancel, SubJobError, SubJobSpec};

/// The most file names a predicted conflict's reason lists.
const MAX_NAMED_FILES: usize = 10;

/// Merge or rebase on a target set.
pub(crate) struct GroupIntegrate {
    pub(crate) name: String,
    pub(crate) op: GroupIntegrateOp,
}

impl GroupOperation for GroupIntegrate {
    fn name(&self) -> String {
        self.name.clone()
    }

    fn destructive(&self) -> Vec<String> {
        Vec::new()
    }

    fn spec(&self) -> SubJobSpec {
        SubJobSpec {
            on_cancel: OnCancel::Finish,
            may_conflict: true,
            // Up to date changes no ref.
            nothing_to_do: NothingToDo::WhenRefsUnchanged,
            network: false,
            conflict_allowed: false,
        }
    }

    fn classify(&self, repo: &GitRepo, git: &GitRunner) -> Class {
        match group_plan(repo, &self.op) {
            Ok((_, theirs)) => match predict(repo, git, &theirs) {
                Some(files) => Class::NeedsAttention {
                    reason: conflict_reason(
                        &files,
                        matches!(self.op, GroupIntegrateOp::Rebase { .. }),
                    ),
                },
                None => Class::WillRun,
            },
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

/// The files a merge of commit `theirs` into `HEAD` would conflict in, or
/// `None` when it would not, or cannot be told. A fast-forward or a merge
/// that is up to date is not merged in memory. For a rebase the same merge
/// stands in for replaying each commit.
fn predict(repo: &GitRepo, git: &GitRunner, theirs: &str) -> Option<Vec<String>> {
    let head = repo.resolve_commit("HEAD")?;
    let ancestor = |one: &str, two: &str| repo.is_ancestor(one, two).unwrap_or(true);
    if ancestor(theirs, &head) || ancestor(&head, theirs) {
        return None;
    }
    match predict_merge(git, repo.work_dir()?, &head, theirs) {
        Ok(MergePrediction::Conflicts(files)) => Some(files),
        // Advisory: a merge git cannot predict is left to the run.
        Ok(MergePrediction::Clean) | Err(_) => None,
    }
}

/// Why a repo needs attention: `conflicts predicted in 2 files: a, b`, or
/// for a rebase `may conflict in …`, naming at most [`MAX_NAMED_FILES`].
fn conflict_reason(files: &[String], rebase: bool) -> String {
    let lead = if rebase {
        "may conflict"
    } else {
        "conflicts predicted"
    };
    let count = match files.len() {
        0 => return lead.to_owned(),
        1 => "1 file".to_owned(),
        n => format!("{n} files"),
    };
    let mut named = files
        .iter()
        .take(MAX_NAMED_FILES)
        .cloned()
        .collect::<Vec<_>>()
        .join(", ");
    if files.len() > MAX_NAMED_FILES {
        named.push_str(&format!(" and {} more", files.len() - MAX_NAMED_FILES));
    }
    format!("{lead} in {count}: {named}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn grp_13_the_reason_names_the_files() {
        let files = |n: usize| (1..=n).map(|i| format!("f{i}")).collect::<Vec<_>>();
        assert_eq!(
            conflict_reason(&files(1), false),
            "conflicts predicted in 1 file: f1"
        );
        assert_eq!(
            conflict_reason(&files(2), true),
            "may conflict in 2 files: f1, f2"
        );
        assert_eq!(conflict_reason(&[], false), "conflicts predicted");
        assert_eq!(
            conflict_reason(&files(12), false),
            "conflicts predicted in 12 files: f1, f2, f3, f4, f5, f6, f7, f8, f9, f10 and 2 more"
        );
    }
}
