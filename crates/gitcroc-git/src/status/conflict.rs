//! What the conflict banner says about an operation stopped part way
//! (OPS-2): what it applies, onto what and, for a rebase, which step
//! stopped. Read from the files git keeps in the git directory, so an
//! operation started in a terminal reads the same (OPS-1).

use std::path::Path;

use gix::bstr::ByteSlice;
use gix::hash::ObjectId;
use serde::Serialize;

use super::StatusError;
use super::state::{Operation, repo_state};

/// Which step of a series stopped.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, specta::Type)]
pub struct Step {
    /// Counted from 1.
    pub current: u32,
    pub total: u32,
}

/// An operation in conflict state as the banner names it (OPS-2). Each part
/// git does not record is `None`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, specta::Type)]
pub struct ConflictDetails {
    pub operation: Option<Operation>,
    /// What is applied: the merged branch or commit, the branch being
    /// rebased, the commit being cherry-picked or reverted. A commit reads
    /// `1a2b3c4 Subject`.
    pub applying: Option<String>,
    /// Where: the current branch, or for a rebase the branch or commit it
    /// rebases onto.
    pub onto: Option<String>,
    /// A rebase or `git am`: the commit or patch that stopped.
    pub step: Option<Step>,
    /// Paths with conflict stages in the index; Continue waits for 0.
    pub unmerged_paths: u32,
}

/// The first line of file `name` in `dir`, trimmed; `None` when unreadable.
fn first_line(dir: &Path, name: &str) -> Option<String> {
    let text = std::fs::read_to_string(dir.join(name)).ok()?;
    Some(text.lines().next()?.trim().to_owned())
}

fn number(dir: &Path, name: &str) -> Option<u32> {
    first_line(dir, name)?.parse().ok()
}

fn step(dir: &Path, current: &str, total: &str) -> Option<Step> {
    Some(Step {
        current: number(dir, current)?,
        total: number(dir, total)?,
    })
}

fn object_id(hex: &str) -> Option<ObjectId> {
    ObjectId::from_hex(hex.as_bytes()).ok()
}

/// Commit `id` as `1a2b3c4 Subject`; the full hash when it cannot be read.
pub(crate) fn commit_text(repo: &gix::Repository, id: ObjectId) -> String {
    let Ok(commit) = repo.find_commit(id) else {
        return id.to_string();
    };
    let short = commit.id().shorten_or_id().to_string();
    match commit.message() {
        Ok(message) => format!("{short} {}", message.summary().to_str_lossy()),
        Err(_) => short,
    }
}

/// The short name of a branch, remote branch or tag at commit `id`, in
/// that order of preference; else the commit as [`commit_text`] writes it.
fn name_of(repo: &gix::Repository, id: ObjectId) -> String {
    let mut found: Vec<(u8, String)> = Vec::new();
    if let Ok(platform) = repo.references()
        && let Ok(all) = platform.all()
    {
        for mut reference in all.flatten() {
            let name = reference.name().as_bstr().to_owned();
            let rank = if name.starts_with(b"refs/heads/") {
                0
            } else if name.starts_with(b"refs/remotes/") {
                1
            } else if name.starts_with(b"refs/tags/") {
                2
            } else {
                continue;
            };
            if reference.peel_to_id().is_ok_and(|peeled| peeled == id) {
                found.push((rank, reference.name().shorten().to_string()));
            }
        }
    }
    found
        .into_iter()
        .min()
        .map_or_else(|| commit_text(repo, id), |(_, name)| name)
}

/// The current branch's short name, or the commit of a detached `HEAD`.
fn current(repo: &gix::Repository) -> Option<String> {
    if let Ok(Some(name)) = repo.head_name() {
        return Some(name.shorten().to_string());
    }
    let id = repo.head_id().ok()?.detach();
    Some(commit_text(repo, id))
}

/// A rebase's `head-name`: the branch being rebased, or the commit of a
/// detached `HEAD` from `orig-head`.
fn rebased(repo: &gix::Repository, dir: &Path) -> Option<String> {
    let head = first_line(dir, "head-name")?;
    match head.strip_prefix("refs/heads/") {
        Some(branch) => Some(branch.to_owned()),
        None => {
            let id = object_id(&first_line(dir, "orig-head")?)?;
            Some(commit_text(repo, id))
        }
    }
}

/// The commit a `*_HEAD` file names.
fn head_file(repo: &gix::Repository, name: &str) -> Option<ObjectId> {
    object_id(&first_line(repo.git_dir(), name)?)
}

pub(super) fn conflict_details(repo: &gix::Repository) -> Result<ConflictDetails, StatusError> {
    let state = repo_state(repo)?;
    let git_dir = repo.git_dir();
    let (applying, onto, step) = match state.operation {
        None => (None, None, None),
        Some(Operation::Merge) => (
            head_file(repo, "MERGE_HEAD").map(|id| name_of(repo, id)),
            current(repo),
            None,
        ),
        Some(Operation::CherryPick) => (
            head_file(repo, "CHERRY_PICK_HEAD").map(|id| commit_text(repo, id)),
            current(repo),
            None,
        ),
        Some(Operation::Revert) => (
            head_file(repo, "REVERT_HEAD").map(|id| commit_text(repo, id)),
            current(repo),
            None,
        ),
        Some(Operation::Rebase) => {
            let merge = git_dir.join("rebase-merge");
            let (dir, current_file, total_file) = if merge.is_dir() {
                (merge, "msgnum", "end")
            } else {
                (git_dir.join("rebase-apply"), "next", "last")
            };
            let onto = first_line(&dir, "onto")
                .and_then(|hex| object_id(&hex))
                .map(|id| name_of(repo, id));
            (
                rebased(repo, &dir),
                onto,
                step(&dir, current_file, total_file),
            )
        }
        Some(Operation::ApplyMailbox) => (
            None,
            current(repo),
            step(&git_dir.join("rebase-apply"), "next", "last"),
        ),
    };
    Ok(ConflictDetails {
        operation: state.operation,
        applying,
        onto,
        step,
        unmerged_paths: state.unmerged_paths,
    })
}
