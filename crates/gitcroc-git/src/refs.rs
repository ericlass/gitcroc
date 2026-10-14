//! A repo's refs for the tree (UI-1): local branches with their upstream,
//! remote branches by remote, tags and stash entries. Names are decoded
//! lossily for display (SEC-3).

use std::collections::BTreeMap;

use gix::bstr::ByteSlice;
use gix::hash::ObjectId;
use serde::Serialize;

use crate::{GitRepo, Time};

/// Commits on one side and not the other.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct AheadBehind {
    /// Commits on the branch and not on its upstream.
    pub ahead: u32,
    /// Commits on the upstream and not on the branch.
    pub behind: u32,
}

/// The branch a local branch tracks.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct BranchUpstream {
    /// As git shortens it, for example `origin/main`.
    pub name: String,
    /// The upstream is configured but its ref does not exist.
    pub gone: bool,
    /// `None` when not asked for (see [`RefsRead::refs`]), when the upstream
    /// is gone, or when the branch has no commit.
    pub counts: Option<AheadBehind>,
}

/// A branch in `refs/heads`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct LocalBranch {
    /// The short name, for example `feature/x`.
    pub name: String,
    /// `HEAD` points to it.
    pub current: bool,
    pub upstream: Option<BranchUpstream>,
}

/// The branches of one remote, from `refs/remotes/<remote>`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RemoteBranches {
    pub remote: String,
    /// Names without the remote, for example `feature/x` for
    /// `origin/feature/x`, sorted. The symbolic `HEAD` is left out.
    pub branches: Vec<String>,
}

/// A tag in `refs/tags`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Tag {
    pub name: String,
    /// It points to a tag object rather than straight to a commit.
    pub annotated: bool,
}

/// An entry of the stash, newest first.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct StashEntry {
    /// `n` in `stash@{n}`.
    pub index: u32,
    /// The stash commit's full hash, which tells this entry from the one
    /// at the same index after the stash list changed.
    pub hash: String,
    /// As `git stash list` shows it, for example `On main: work`.
    pub message: String,
    pub time: Time,
}

/// Every ref the tree shows for one repo. Each list is sorted by name,
/// stash entries by index.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct Refs {
    pub local: Vec<LocalBranch>,
    /// Sorted by remote name.
    pub remotes: Vec<RemoteBranches>,
    pub tags: Vec<Tag>,
    pub stashes: Vec<StashEntry>,
}

/// Why the refs could not be read.
#[derive(Debug, thiserror::Error)]
#[error("cannot read the refs: {0}")]
pub struct RefsError(#[source] Box<dyn std::error::Error + Send + Sync + 'static>);

impl From<gix::Error> for RefsError {
    fn from(error: gix::Error) -> Self {
        Self(Box::new(error))
    }
}

/// Ref reads (ARCH-2).
pub trait RefsRead {
    /// The repo's refs. With `counts`, each local branch with an upstream
    /// also gets its ahead and behind counts, which walks history; without,
    /// only refs are read.
    fn refs(&self, counts: bool) -> Result<Refs, RefsError>;
}

impl RefsRead for GitRepo {
    fn refs(&self, counts: bool) -> Result<Refs, RefsError> {
        let repo = self.thread_local();
        Ok(Refs {
            local: local(&repo, counts)?,
            remotes: remotes(&repo)?,
            tags: tags(&repo)?,
            stashes: stashes(&repo),
        })
    }
}

impl GitRepo {
    /// The stash entries, newest first, without reading other refs.
    pub fn stashes(&self) -> Vec<StashEntry> {
        stashes(&self.thread_local())
    }
}

/// `HEAD` and every ref with what it points at: an object id, or `ref: `
/// and the ref a symbolic one names. Two snapshots of a repo, taken before
/// and after a write, tell whether the write changed any ref (GRP-7's
/// *nothing to do*).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RefSnapshot(BTreeMap<Vec<u8>, String>);

impl GitRepo {
    /// The repo's refs now; see [`RefSnapshot`].
    pub fn ref_snapshot(&self) -> Result<RefSnapshot, RefsError> {
        let repo = self.thread_local();
        let mut refs = BTreeMap::new();
        let target = |target: gix::refs::TargetRef<'_>| match target {
            gix::refs::TargetRef::Object(id) => id.to_string(),
            gix::refs::TargetRef::Symbolic(name) => format!("ref: {}", name.as_bstr()),
        };
        if let Some(head) = repo.try_find_reference("HEAD")? {
            refs.insert(b"HEAD".to_vec(), target(head.target()));
        }
        for reference in repo.references()?.all()? {
            let reference = reference?;
            refs.insert(
                reference.name().as_bstr().to_vec(),
                target(reference.target()),
            );
        }
        Ok(RefSnapshot(refs))
    }
}

fn text(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}

fn local(repo: &gix::Repository, counts: bool) -> Result<Vec<LocalBranch>, RefsError> {
    let head = repo.head_name()?;
    let mut branches = Vec::new();
    for reference in repo.references()?.local_branches()? {
        let mut reference = reference?;
        let name = reference.name().to_owned();
        let id = reference.peel_to_id().ok().map(|id| id.detach());
        let upstream = match repo
            .branch_remote_tracking_ref_name(name.as_ref(), gix::remote::Direction::Fetch)
        {
            None => None,
            Some(tracking) => Some(upstream(repo, tracking?.as_ref(), id, counts)?),
        };
        branches.push(LocalBranch {
            name: text(name.shorten()),
            current: head.as_ref() == Some(&name),
            upstream,
        });
    }
    branches.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(branches)
}

fn upstream(
    repo: &gix::Repository,
    name: &gix::refs::FullNameRef,
    branch: Option<ObjectId>,
    counts: bool,
) -> Result<BranchUpstream, RefsError> {
    let target = match repo.try_find_reference(name)? {
        Some(mut reference) => reference.peel_to_id().ok().map(|id| id.detach()),
        None => None,
    };
    let counts = match (counts, branch, target) {
        (true, Some(branch), Some(target)) => Some(AheadBehind {
            ahead: count_only_in(repo, branch, target)?,
            behind: count_only_in(repo, target, branch)?,
        }),
        _ => None,
    };
    Ok(BranchUpstream {
        name: text(name.shorten()),
        gone: target.is_none(),
        counts,
    })
}

/// Commits reachable from `tip` and not from `hidden`.
fn count_only_in(
    repo: &gix::Repository,
    tip: ObjectId,
    hidden: ObjectId,
) -> Result<u32, RefsError> {
    let mut count = 0u32;
    for info in repo.rev_walk([tip]).with_hidden([hidden]).all()? {
        info.map_err(|error| RefsError(Box::new(error)))?;
        count = count.saturating_add(1);
    }
    Ok(count)
}

/// Remote branches grouped by the configured remote whose name is the
/// longest prefix, as remote names may contain `/`. A ref that matches no
/// configured remote is grouped by its first component.
fn remotes(repo: &gix::Repository) -> Result<Vec<RemoteBranches>, RefsError> {
    let configured: Vec<String> = repo
        .remote_names()
        .into_iter()
        .map(|name| text(name.as_ref()))
        .collect();
    let mut grouped: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for name in &configured {
        grouped.entry(name.clone()).or_default();
    }
    for reference in repo.references()?.remote_branches()? {
        let reference = reference?;
        if reference.target().try_id().is_none() {
            continue; // symbolic, like refs/remotes/origin/HEAD
        }
        let short = text(reference.name().shorten());
        let remote = configured
            .iter()
            .filter(|remote| {
                short
                    .strip_prefix(remote.as_str())
                    .is_some_and(|rest| rest.starts_with('/'))
            })
            .max_by_key(|remote| remote.len())
            .cloned()
            .or_else(|| short.split_once('/').map(|(remote, _)| remote.to_owned()));
        let Some(remote) = remote else {
            continue;
        };
        let branch = short
            .get(remote.len().saturating_add(1)..)
            .unwrap_or_default()
            .to_owned();
        grouped.entry(remote).or_default().push(branch);
    }
    Ok(grouped
        .into_iter()
        .map(|(remote, mut branches)| {
            branches.sort();
            RemoteBranches { remote, branches }
        })
        .collect())
}

fn tags(repo: &gix::Repository) -> Result<Vec<Tag>, RefsError> {
    let mut tags = Vec::new();
    for reference in repo.references()?.tags()? {
        let reference = reference?;
        let annotated = reference.target().try_id().is_some_and(|id| {
            repo.find_header(id)
                .is_ok_and(|header| header.kind() == gix::object::Kind::Tag)
        });
        tags.push(Tag {
            name: text(reference.name().shorten()),
            annotated,
        });
    }
    tags.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(tags)
}

/// The stash reflog, newest first. A repo without a stash, or with an
/// unreadable stash log, has no entries.
fn stashes(repo: &gix::Repository) -> Vec<StashEntry> {
    let Ok(Some(reference)) = repo.try_find_reference("refs/stash") else {
        return Vec::new();
    };
    let mut log = reference.log_iter();
    let Ok(Some(lines)) = log.rev() else {
        return Vec::new();
    };
    lines
        .map_while(Result::ok)
        .zip(0u32..)
        .map(|(line, index)| StashEntry {
            index,
            hash: line.new_oid.to_string(),
            message: text(line.message.trim()),
            time: Time {
                seconds: line.signature.time.seconds,
                offset: line.signature.time.offset,
            },
        })
        .collect()
}
