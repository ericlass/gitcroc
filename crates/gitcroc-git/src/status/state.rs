//! The conflict state (OPS-1) and the summary (UI-3, PERF-3), read from
//! the repo itself: its files in the git directory, its index and its refs.

use gix::hash::ObjectId;
use serde::{Deserialize, Serialize};

use super::StatusError;

/// An operation that stopped part way and waits to be continued or
/// aborted.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "snake_case")]
pub enum Operation {
    /// `MERGE_HEAD`.
    Merge,
    /// `rebase-merge`, or `rebase-apply` from `git rebase`.
    Rebase,
    /// `rebase-apply` from `git am`.
    ApplyMailbox,
    /// `CHERRY_PICK_HEAD`.
    CherryPick,
    /// `REVERT_HEAD`.
    Revert,
    /// Unmerged paths and none of the files above: a stash applied or
    /// popped with conflicts, as git keeps no state file for it. It ends
    /// when the last path is staged.
    StashApply,
}

/// Whether a repo is in conflict state (OPS-1).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
pub struct RepoState {
    /// The operation in progress, if any.
    pub operation: Option<Operation>,
    /// How many paths have conflict stages in the index.
    pub unmerged_paths: u32,
}

impl RepoState {
    /// An operation in progress or unmerged paths.
    pub fn in_conflict(&self) -> bool {
        self.operation.is_some() || self.unmerged_paths > 0
    }
}

/// Where `HEAD` is.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Head {
    /// On a branch; `hash` is `None` while it has no commit yet.
    Branch {
        name: String,
        hash: Option<String>,
    },
    Detached {
        hash: String,
    },
}

/// The branch the current branch tracks.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
pub struct Upstream {
    /// As git shortens it, for example `origin/main`.
    pub name: String,
    /// Commits on the branch and not on the upstream.
    pub ahead: u32,
    /// Commits on the upstream and not on the branch.
    pub behind: u32,
    /// The upstream is configured but its ref does not exist, for example
    /// after the remote branch was deleted and pruned. Counts are then 0.
    pub gone: bool,
}

/// What a repo row shows (UI-3) and the cache keeps (PERF-3).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
pub struct RepoSummary {
    pub head: Head,
    /// `None` when the branch has no upstream or `HEAD` is detached.
    pub upstream: Option<Upstream>,
    /// Anything staged, unstaged, untracked or unmerged.
    pub dirty: bool,
    /// How many distinct paths are staged, unstaged, untracked or unmerged
    /// (UI-10); 0 exactly when not `dirty`.
    pub changes: u32,
    pub state: RepoState,
}

pub(super) fn repo_state(repo: &gix::Repository) -> Result<RepoState, StatusError> {
    let git_dir = repo.git_dir();
    let operation = if git_dir.join("rebase-merge").is_dir() {
        Some(Operation::Rebase)
    } else if git_dir.join("rebase-apply").is_dir() {
        if git_dir.join("rebase-apply").join("applying").exists() {
            Some(Operation::ApplyMailbox)
        } else {
            Some(Operation::Rebase)
        }
    } else if git_dir.join("MERGE_HEAD").is_file() {
        Some(Operation::Merge)
    } else if git_dir.join("CHERRY_PICK_HEAD").is_file() {
        Some(Operation::CherryPick)
    } else if git_dir.join("REVERT_HEAD").is_file() {
        Some(Operation::Revert)
    } else {
        None
    };
    let index = repo.index_or_empty()?;
    let mut unmerged_paths = 0u32;
    let mut last = None;
    for entry in index.entries() {
        if entry.stage() != gix::index::entry::Stage::Unconflicted {
            let path = entry.path(&index);
            if last != Some(path) {
                unmerged_paths = unmerged_paths.saturating_add(1);
                last = Some(path);
            }
        }
    }
    let operation = match operation {
        None if unmerged_paths > 0 => Some(Operation::StashApply),
        operation => operation,
    };
    Ok(RepoState {
        operation,
        unmerged_paths,
    })
}

pub(super) fn summary(repo: &gix::Repository, changes: u32) -> Result<RepoSummary, StatusError> {
    let state = repo_state(repo)?;
    let dirty = changes > 0;
    let id = repo.head_id().ok().map(|id| id.detach());
    let Some(branch) = repo.head_name()? else {
        let id = repo.head_id()?.detach();
        return Ok(RepoSummary {
            head: Head::Detached {
                hash: id.to_string(),
            },
            upstream: None,
            dirty,
            changes,
            state,
        });
    };
    let upstream = match repo
        .branch_remote_tracking_ref_name(branch.as_ref(), gix::remote::Direction::Fetch)
    {
        None => None,
        Some(name) => Some(upstream(repo, &name?, id)?),
    };
    Ok(RepoSummary {
        head: Head::Branch {
            name: branch.shorten().to_string(),
            hash: id.map(|id| id.to_string()),
        },
        upstream,
        dirty,
        changes,
        state,
    })
}

fn upstream(
    repo: &gix::Repository,
    name: &gix::refs::FullName,
    branch: Option<ObjectId>,
) -> Result<Upstream, StatusError> {
    let display = name.shorten().to_string();
    let target = match repo.try_find_reference(name.as_ref())? {
        Some(mut reference) => Some(reference.peel_to_id()?.detach()),
        None => None,
    };
    let (Some(target), Some(branch)) = (target, branch) else {
        return Ok(Upstream {
            name: display,
            ahead: 0,
            behind: 0,
            gone: target.is_none(),
        });
    };
    Ok(Upstream {
        name: display,
        ahead: count_only_in(repo, branch, target)?,
        behind: count_only_in(repo, target, branch)?,
        gone: false,
    })
}

/// Commits reachable from `tip` and not from `hidden`.
fn count_only_in(
    repo: &gix::Repository,
    tip: ObjectId,
    hidden: ObjectId,
) -> Result<u32, StatusError> {
    let mut count = 0u32;
    for info in repo.rev_walk([tip]).with_hidden([hidden]).all()? {
        info?;
        count = count.saturating_add(1);
    }
    Ok(count)
}
