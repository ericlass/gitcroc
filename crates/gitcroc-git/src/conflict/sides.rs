//! The commits of each side of a conflict whose messages AI-5 sends: those
//! that touched the file, reachable from the side's commit and not from the
//! merge base of the two sides, newest first. The sides are the commits the
//! conflict editor labels (OPS-4): `HEAD`, and `MERGE_HEAD`,
//! `CHERRY_PICK_HEAD`, `REVERT_HEAD`, the commit a rebase replays, or for a
//! stash apply the stash whose content the conflict holds. The walk runs in
//! `gix`, or in `git log` behind the same trait (ARCH-2). Nothing here
//! writes.

use std::ffi::OsString;

use gix::bstr::{BStr, ByteSlice};
use gix::hash::ObjectId;
use gix::index::entry::Stage;

use super::ConflictError;
use crate::status::conflict::{head_file, replayed};
use crate::status::state::repo_state;
use crate::{Engine, GitRepo, GitRunner, Operation};

/// The messages of each side of a conflicted file (AI-5), newest first and
/// whole: the caller cuts them.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SideMessages {
    /// The operation in progress, as [`crate::RepoState`] reads it.
    pub operation: Option<Operation>,
    pub ours: Vec<String>,
    pub theirs: Vec<String>,
}

/// Reads the side messages of a conflicted file (ARCH-2).
pub trait SidesRead {
    /// Up to `limit` messages per side for the file at `git_path`, as git
    /// stores it.
    fn side_messages(&self, git_path: &[u8], limit: usize) -> Result<SideMessages, ConflictError>;
}

/// Reads side messages with `gix`, or with `git log` where git must check
/// the repo.
#[derive(Debug, Clone)]
pub struct SidesReader {
    repo: GitRepo,
    git: GitRunner,
    engine: Engine,
}

impl SidesReader {
    /// A reader of `repo` that runs `git` when the repo is not fully
    /// trusted, so that git's refusal reaches the user (SEC-6) instead of
    /// a read git would not do.
    pub fn new(repo: GitRepo, git: GitRunner) -> Self {
        Self {
            repo,
            git,
            engine: Engine::Auto,
        }
    }

    /// The same reader with `engine`, so tests can compare the two.
    pub fn with_engine(self, engine: Engine) -> Self {
        Self { engine, ..self }
    }
}

/// Where a side's messages come from.
enum Side {
    /// The commits from `tip` that `hidden` cannot reach.
    Walk {
        tip: ObjectId,
        hidden: Option<ObjectId>,
    },
    /// One message: a stash's, or the patch `git am` applies.
    Message(String),
    None,
}

impl SidesRead for SidesReader {
    fn side_messages(&self, git_path: &[u8], limit: usize) -> Result<SideMessages, ConflictError> {
        let repo = self.repo.thread_local();
        let operation = repo_state(&repo)?.operation;
        let path = git_path.as_bstr();
        let use_git = match self.engine {
            Engine::Auto => repo.git_dir_trust() != gix::sec::Trust::Full,
            Engine::Gix => false,
            Engine::Git => true,
        };
        let (ours, theirs) = sides(&repo, operation, path)?;
        let read = |side: Side| -> Result<Vec<String>, ConflictError> {
            match side {
                Side::Walk { tip, hidden } if use_git => self.git_walk(tip, hidden, path, limit),
                Side::Walk { tip, hidden } => gix_walk(&repo, tip, hidden, path, limit),
                Side::Message(message) => Ok(vec![message].into_iter().take(limit).collect()),
                Side::None => Ok(Vec::new()),
            }
        };
        Ok(SideMessages {
            operation,
            ours: read(ours)?,
            theirs: read(theirs)?,
        })
    }
}

/// The two sides of the conflict of `path` during `operation`.
fn sides(
    repo: &gix::Repository,
    operation: Option<Operation>,
    path: &BStr,
) -> Result<(Side, Side), ConflictError> {
    let Some(head) = repo.head_id().ok().map(|id| id.detach()) else {
        return Ok((Side::None, Side::None));
    };
    let theirs = match operation {
        Some(Operation::Merge) => head_file(repo, "MERGE_HEAD"),
        Some(Operation::CherryPick) => head_file(repo, "CHERRY_PICK_HEAD"),
        Some(Operation::Revert) => head_file(repo, "REVERT_HEAD"),
        Some(Operation::Rebase) => replayed(repo),
        Some(Operation::ApplyMailbox) => {
            // The patch is no commit: no merge base, and its message only.
            let message = std::fs::read(repo.git_dir().join("rebase-apply").join("final-commit"))
                .ok()
                .map(|bytes| message(&bytes))
                .filter(|message| !message.is_empty());
            let ours = Side::Walk {
                tip: head,
                hidden: None,
            };
            return Ok((ours, message.map_or(Side::None, Side::Message)));
        }
        Some(Operation::StashApply) | None => {
            let Some(stash) = applied_stash(repo, path)? else {
                return Ok((
                    Side::Walk {
                        tip: head,
                        hidden: None,
                    },
                    Side::None,
                ));
            };
            // Only the stash's own message: its other parents are the
            // index and the untracked files git saved with it.
            let hidden = merge_base(repo, head, stash.base)?;
            return Ok((
                Side::Walk { tip: head, hidden },
                Side::Message(stash.message),
            ));
        }
    };
    let Some(theirs) = theirs else {
        return Ok((
            Side::Walk {
                tip: head,
                hidden: None,
            },
            Side::None,
        ));
    };
    let hidden = merge_base(repo, head, theirs)?;
    Ok((
        Side::Walk { tip: head, hidden },
        Side::Walk {
            tip: theirs,
            hidden,
        },
    ))
}

fn merge_base(
    repo: &gix::Repository,
    one: ObjectId,
    two: ObjectId,
) -> Result<Option<ObjectId>, ConflictError> {
    Ok(repo
        .merge_base(one, two)
        .map_err(ConflictError::read)?
        .map(|id| id.detach()))
}

/// A commit message as sent: trailing whitespace removed, decoded lossily.
fn message(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).trim_end().to_owned()
}

/// A stash entry a conflict came from.
struct Stash {
    /// The commit the stash was made on.
    base: ObjectId,
    message: String,
}

/// The newest stash whose content at `path` is the conflict's theirs stage,
/// and whose base commit holds its base stage: git records no stash for an
/// apply. `None` when none matches.
fn applied_stash(repo: &gix::Repository, path: &BStr) -> Result<Option<Stash>, ConflictError> {
    let index = repo.index_or_empty().map_err(ConflictError::read)?;
    let stage = |stage| {
        index
            .entry_by_path_and_stage(path, stage)
            .map(|entry| entry.id)
    };
    let (base, theirs) = (stage(Stage::Base), stage(Stage::Theirs));
    let Ok(Some(reference)) = repo.try_find_reference("refs/stash") else {
        return Ok(None);
    };
    let mut log = reference.log_iter();
    let Ok(Some(lines)) = log.rev() else {
        return Ok(None);
    };
    for line in lines.map_while(Result::ok) {
        let Ok(commit) = repo.find_commit(line.new_oid) else {
            continue;
        };
        let Some(parent) = commit.parent_ids().next().map(|id| id.detach()) else {
            continue;
        };
        if entry_at(repo, line.new_oid, path)?.map(|(id, _)| id) == theirs
            && entry_at(repo, parent, path)?.map(|(id, _)| id) == base
        {
            let text = commit.message_raw().map_err(ConflictError::read)?;
            return Ok(Some(Stash {
                base: parent,
                message: message(text),
            }));
        }
    }
    Ok(None)
}

/// The blob and mode at `path` in `commit`'s tree, or `None`.
fn entry_at(
    repo: &gix::Repository,
    commit: ObjectId,
    path: &BStr,
) -> Result<Option<(ObjectId, gix::object::tree::EntryMode)>, ConflictError> {
    let tree = repo
        .find_commit(commit)
        .map_err(ConflictError::read)?
        .tree()
        .map_err(ConflictError::read)?;
    let entry = tree
        .lookup_entry(path.split(|&byte| byte == b'/'))
        .map_err(ConflictError::read)?;
    Ok(entry.map(|entry| (entry.object_id(), entry.mode())))
}

/// Up to `limit` messages of the commits from `tip`, without those `hidden`
/// reaches, that changed `path`, newest first. Merges are left out: a
/// commit counts when it differs at `path` from its parent, as `git log
/// --full-history --no-merges` counts it.
fn gix_walk(
    repo: &gix::Repository,
    tip: ObjectId,
    hidden: Option<ObjectId>,
    path: &BStr,
    limit: usize,
) -> Result<Vec<String>, ConflictError> {
    use gix::revision::walk::Sorting;
    use gix::traverse::commit::simple::CommitTimeOrder;

    let mut messages = Vec::new();
    if limit == 0 {
        return Ok(messages);
    }
    let walk = repo
        .rev_walk([tip])
        .with_hidden(hidden)
        .sorting(Sorting::ByCommitTime(CommitTimeOrder::NewestFirst))
        .all()
        .map_err(ConflictError::read)?;
    for info in walk {
        let info = info.map_err(ConflictError::read)?;
        let parents: Vec<ObjectId> = info.parent_ids.iter().copied().collect();
        if parents.len() > 1 {
            continue;
        }
        let here = entry_at(repo, info.id, path)?;
        let before = match parents.first() {
            Some(&parent) => entry_at(repo, parent, path)?,
            None => None,
        };
        if here == before {
            continue;
        }
        let commit = repo.find_commit(info.id).map_err(ConflictError::read)?;
        let text = message(commit.message_raw().map_err(ConflictError::read)?);
        // As `git log -z` shows it, an empty message is no entry.
        if !text.is_empty() {
            messages.push(text);
        }
        if messages.len() == limit {
            break;
        }
    }
    Ok(messages)
}

impl SidesReader {
    /// [`gix_walk`] with `git log`.
    fn git_walk(
        &self,
        tip: ObjectId,
        hidden: Option<ObjectId>,
        path: &BStr,
        limit: usize,
    ) -> Result<Vec<String>, ConflictError> {
        if limit == 0 {
            return Ok(Vec::new());
        }
        let work_dir = self.repo.work_dir().ok_or(ConflictError::NoWorkTree)?;
        let path = gix::path::from_bstr(path).map_err(ConflictError::read)?;
        let revisions = [
            Some(tip.to_string()),
            hidden.map(|hidden| format!("^{hidden}")),
        ];
        let args: Vec<OsString> = [
            "--literal-pathspecs",
            "log",
            "--no-show-signature",
            "-z",
            "--no-merges",
            "--full-history",
            "--format=%B",
        ]
        .map(OsString::from)
        .into_iter()
        .chain([format!("--max-count={limit}").into()])
        .chain(revisions.into_iter().flatten().map(OsString::from))
        .chain(["--".into(), path.as_os_str().to_owned()])
        .collect();
        let output = self
            .git
            .run_blocking(&args, Some(work_dir))
            .map_err(ConflictError::read)?;
        Ok(output
            .stdout
            .split(|&byte| byte == 0)
            .map(message)
            .filter(|message| !message.is_empty())
            .collect())
    }
}
