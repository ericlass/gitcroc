//! The working copy, the conflict state and the summary of a repo (UI-3,
//! UI-15, OPS-1, PERF-3).
//!
//! Status runs through `gix`. Where `gix` lacks something the repo needs,
//! it runs `git status` instead, behind the same trait (ARCH-2): see
//! [`Fallback`] for the cases. Both build the same [`WorkingCopy`], so
//! callers cannot tell which ran. Nothing here writes to the repo, not even
//! the index stat refresh `git status` does by default.

mod porcelain;
mod state;
mod worktree;

use std::ffi::OsString;
use std::path::Path;

use gix::bstr::{BString, ByteSlice};
use gix::hash::ObjectId;
use gix::object::tree::EntryMode;
use serde::Serialize;

pub use state::{Head, Operation, RepoState, RepoSummary, Upstream};

use crate::diff::{ChangedFile, Origin, Side};
use crate::{GitError, GitRepo, GitRunner};

/// The uncommitted state of a repo (UI-15). Each list is sorted by path and
/// numbered from 0, as a commit's changed files are.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct WorkingCopy {
    /// The index against `HEAD`.
    pub staged: Vec<ChangedFile>,
    /// The working tree against the index, untracked files included, as
    /// `status.showUntrackedFiles` lists them.
    pub unstaged: Vec<ChangedFile>,
    /// Paths with conflict stages in the index. They are in neither list
    /// above, as in `git status`.
    pub unmerged: Vec<UnmergedFile>,
}

impl WorkingCopy {
    /// No staged, unstaged, untracked or unmerged path.
    pub fn is_clean(&self) -> bool {
        self.staged.is_empty() && self.unstaged.is_empty() && self.unmerged.is_empty()
    }

    /// The number of distinct paths that are staged, unstaged, untracked or
    /// unmerged (UI-10). A path both staged and unstaged counts once.
    pub fn changed_paths(&self) -> u32 {
        let paths: std::collections::HashSet<&[u8]> = self
            .staged
            .iter()
            .chain(&self.unstaged)
            .map(ChangedFile::git_path)
            .chain(self.unmerged.iter().map(UnmergedFile::git_path))
            .collect();
        u32::try_from(paths.len()).unwrap_or(u32::MAX)
    }

    fn sort(&mut self) {
        crate::diff::sort_and_number(&mut self.staged);
        crate::diff::sort_and_number(&mut self.unstaged);
        self.unmerged.sort_by(|a, b| a.git_path.cmp(&b.git_path));
        for (index, file) in self.unmerged.iter_mut().enumerate() {
            file.id = index;
        }
    }
}

/// How the two sides of a merge left a path, as `git status` names it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "snake_case")]
pub enum Conflict {
    BothDeleted,
    AddedByUs,
    DeletedByThem,
    AddedByThem,
    DeletedByUs,
    BothAdded,
    BothModified,
}

/// One conflict stage of an unmerged path.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Stage {
    pub(crate) id: ObjectId,
    pub(crate) mode: EntryMode,
}

/// A path with conflict stages in the index.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, specta::Type)]
pub struct UnmergedFile {
    /// The index in [`WorkingCopy::unmerged`].
    #[specta(type = u32)]
    pub id: usize,
    /// The path for display, decoded lossily; never sent back (PLAT-2).
    pub path: String,
    pub conflict: Conflict,
    /// Base, ours and theirs (stages 1 to 3), each absent when that side
    /// has no file; for the conflict editor (OPS-8).
    #[serde(skip)]
    stages: [Option<Stage>; 3],
    #[serde(skip)]
    git_path: BString,
}

impl UnmergedFile {
    fn new(git_path: BString, conflict: Conflict, stages: [Option<Stage>; 3]) -> Self {
        Self {
            id: 0,
            path: String::from_utf8_lossy(&git_path).into_owned(),
            conflict,
            stages,
            git_path,
        }
    }

    /// The path as git stores it (PLAT-2).
    pub fn git_path(&self) -> &[u8] {
        &self.git_path
    }
}

/// Why a status, state or summary could not be read.
#[derive(Debug, thiserror::Error)]
pub enum StatusError {
    #[error("the repo has no working tree")]
    NoWorkTree,
    /// git refused the repo, for example because of `safe.directory`. The
    /// message is git's own, for the user to see (SEC-6).
    #[error("{message}")]
    Refused { message: String },
    #[error(transparent)]
    Git(GitError),
    #[error("cannot read git's status output: {0}")]
    Parse(String),
    #[error("cannot read the repo: {0}")]
    Read(#[source] Box<dyn std::error::Error + Send + Sync + 'static>),
}

impl From<gix::Error> for StatusError {
    fn from(error: gix::Error) -> Self {
        Self::Read(Box::new(error))
    }
}

impl From<GitError> for StatusError {
    fn from(error: GitError) -> Self {
        match error {
            GitError::Failed { stderr, .. } => Self::Refused {
                message: String::from_utf8_lossy(&stderr).trim_end().to_owned(),
            },
            spawn @ GitError::Spawn { .. } => Self::Git(spawn),
        }
    }
}

/// Status, conflict state and summary reads (ARCH-2).
pub trait StatusRead {
    /// The working copy: staged, unstaged, untracked and unmerged paths,
    /// following git's defaults and the repo's config for untracked files,
    /// ignore rules, renames and submodules.
    fn status(&self) -> Result<WorkingCopy, StatusError>;

    /// The operation in progress and whether paths are unmerged (OPS-1).
    fn repo_state(&self) -> Result<RepoState, StatusError>;

    /// What a repo row shows (UI-3) and the cache keeps (PERF-3).
    fn summary(&self) -> Result<RepoSummary, StatusError>;
}

/// Which implementation reads the status. Callers use [`Engine::Auto`];
/// the others exist so tests and measurements can compare the two.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Engine {
    /// `gix`, unless the repo needs git (see [`Fallback`]).
    #[default]
    Auto,
    Gix,
    Git,
}

/// The cases in which status runs `git status` instead of `gix` (ARCH-2).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Fallback {
    /// `core.fsmonitor` is set. git asks the file system monitor what
    /// changed; `gix` has no monitor support and would check every file.
    Fsmonitor,
    /// The repo is owned by someone else and not listed in
    /// `safe.directory`. git refuses it with a message the user must see
    /// (SEC-6), where `gix` would read it with reduced trust.
    Untrusted,
    /// The index is sparse (`index.sparse`). `gix` cannot compare a sparse
    /// index with a tree.
    SparseIndex,
    /// The untracked cache is on: `core.untrackedCache` is true, or unset
    /// with the cache already in the index. git skips unchanged folders
    /// with it; `gix` cannot read it and walks every folder (on
    /// `big-tree`, 150 ms against 500 ms).
    UntrackedCache,
    /// The index has conflict stages. `gix` also lists a conflicted file
    /// that only exists in stage 2 or 3 as untracked; git does not.
    Unmerged,
}

/// Reads a repo's status with `gix`, or with `git` where needed.
#[derive(Debug, Clone)]
pub struct StatusReader {
    repo: GitRepo,
    git: GitRunner,
    engine: Engine,
}

impl StatusReader {
    /// A reader of `repo` that runs `git` for the [`Fallback`] cases.
    pub fn new(repo: GitRepo, git: GitRunner) -> Self {
        Self {
            repo,
            git,
            engine: Engine::Auto,
        }
    }

    /// Forces one implementation, for tests and measurements.
    pub fn with_engine(self, engine: Engine) -> Self {
        Self { engine, ..self }
    }

    pub fn repo(&self) -> &GitRepo {
        &self.repo
    }

    /// Why `git` reads this repo's status, or `None` when `gix` does.
    pub fn fallback(&self) -> Result<Option<Fallback>, StatusError> {
        fallback(&self.repo.thread_local())
    }

    fn run_git(&self, args: &[&str]) -> Result<Vec<u8>, StatusError> {
        let work_dir = self.repo.work_dir().ok_or(StatusError::NoWorkTree)?;
        let args: Vec<OsString> = args.iter().map(OsString::from).collect();
        Ok(self.git.run_blocking(&args, Some(work_dir))?.stdout)
    }

    /// Lets git check the repo when gix trusts it less than fully, so
    /// git's refusal reaches the user instead of a read git would not do.
    fn check_trust(&self, repo: &gix::Repository) -> Result<(), StatusError> {
        if repo.git_dir_trust() != gix::sec::Trust::Full {
            self.run_git(&["rev-parse", "--git-dir"])?;
        }
        Ok(())
    }
}

impl StatusRead for StatusReader {
    fn status(&self) -> Result<WorkingCopy, StatusError> {
        let repo = self.repo.thread_local();
        let use_git = match self.engine {
            Engine::Auto => fallback(&repo)?.is_some(),
            Engine::Gix => false,
            Engine::Git => true,
        };
        let renames = renames(&repo);
        let mut working_copy = if use_git {
            let output = self.run_git(&[
                "--no-optional-locks",
                "status",
                "--porcelain=v2",
                "-z",
                if renames.is_some() {
                    "--renames"
                } else {
                    "--no-renames"
                },
            ])?;
            porcelain::parse(&output, repo.object_hash(), self.work_dir()?)?
        } else {
            worktree::status(&repo, renames)?
        };
        working_copy.sort();
        Ok(working_copy)
    }

    fn repo_state(&self) -> Result<RepoState, StatusError> {
        let repo = self.repo.thread_local();
        self.check_trust(&repo)?;
        state::repo_state(&repo)
    }

    fn summary(&self) -> Result<RepoSummary, StatusError> {
        let changes = self.status()?.changed_paths();
        let repo = self.repo.thread_local();
        state::summary(&repo, changes)
    }
}

impl StatusReader {
    fn work_dir(&self) -> Result<&Path, StatusError> {
        self.repo.work_dir().ok_or(StatusError::NoWorkTree)
    }
}

/// Rename detection between `HEAD` and the index as `status.renames`, then
/// `diff.renames`, configures it, on when neither is set. Copies are
/// treated as renames, as in a commit's file list: the two engines pair
/// copies differently, and `--renames` makes git look for renames only.
/// gix 0.89 does not read `status.renames = false` itself.
fn renames(repo: &gix::Repository) -> Option<gix::diff::Rewrites> {
    let config = repo.config_snapshot();
    let setting = config
        .string("status.renames")
        .or_else(|| config.string("diff.renames"));
    let on = setting.is_none_or(|value| {
        value.as_bstr() == "copies"
            || value.as_bstr() == "copy"
            || gix::config::Boolean::try_from(value.as_bstr()).map_or(true, |b| b.0)
    });
    let limit = config
        .integer("status.renameLimit")
        .or_else(|| config.integer("diff.renameLimit"))
        .and_then(|limit| usize::try_from(limit).ok());
    let default = gix::diff::Rewrites::default();
    on.then(|| gix::diff::Rewrites {
        copies: None,
        limit: limit.unwrap_or(default.limit),
        ..default
    })
}

fn fallback(repo: &gix::Repository) -> Result<Option<Fallback>, StatusError> {
    if repo.git_dir_trust() != gix::sec::Trust::Full {
        return Ok(Some(Fallback::Untrusted));
    }
    let config = repo.config_snapshot();
    if let Some(value) = config.string("core.fsmonitor") {
        let off = gix::config::Boolean::try_from(value.as_bstr()).is_ok_and(|b| !b.0);
        if !off {
            return Ok(Some(Fallback::Fsmonitor));
        }
    }
    let index = repo.index_or_empty()?;
    if index.is_sparse() {
        return Ok(Some(Fallback::SparseIndex));
    }
    let untracked_cache = config.string("core.untrackedCache").map(|value| {
        gix::config::Boolean::try_from(value.as_bstr())
            .ok()
            .map(|b| b.0)
    });
    let untracked_cache = match untracked_cache {
        Some(Some(on)) => on,
        // Unset or `keep`: git uses the cache the index has.
        None | Some(None) => index.untracked().is_some(),
    };
    if untracked_cache {
        return Ok(Some(Fallback::UntrackedCache));
    }
    let unmerged = index
        .entries()
        .iter()
        .any(|entry| entry.stage() != gix::index::entry::Stage::Unconflicted);
    if unmerged {
        return Ok(Some(Fallback::Unmerged));
    }
    Ok(None)
}

/// An unstaged entry: the index side, if any, against the working tree.
fn unstaged(
    status: crate::FileStatus,
    index: Option<Side>,
    work_tree_mode: Option<EntryMode>,
    path: &BString,
    hash: gix::hash::Kind,
) -> Option<ChangedFile> {
    let work_tree = work_tree_mode.map(|mode| Side {
        id: ObjectId::null(hash),
        mode,
        path: path.clone(),
    });
    ChangedFile::new(status, index, work_tree, Origin::Unstaged)
}

/// An untracked path; a folder git lists as one entry gets a trailing `/`.
fn untracked(work_dir: &Path, path: &[u8], hash: gix::hash::Kind) -> Option<ChangedFile> {
    let path = BString::from(path);
    let mode = if path.ends_with(b"/") {
        EntryMode::from(gix::object::tree::EntryKind::Tree)
    } else {
        let file = gix::path::from_bstr(path.as_bstr()).ok()?;
        let link = std::fs::symlink_metadata(work_dir.join(file))
            .is_ok_and(|metadata| metadata.file_type().is_symlink());
        if link {
            EntryMode::from(gix::object::tree::EntryKind::Link)
        } else {
            EntryMode::from(gix::object::tree::EntryKind::Blob)
        }
    };
    unstaged(crate::FileStatus::Untracked, None, Some(mode), &path, hash)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sec_6_git_refusals_reach_the_caller_with_gits_message() {
        let message = "fatal: detected dubious ownership in repository at '/repo'";
        let error = StatusError::from(GitError::Failed {
            code: Some(128),
            stderr: format!("{message}\n").into_bytes(),
        });
        assert!(matches!(&error, StatusError::Refused { message: m } if m == message));
        assert_eq!(error.to_string(), message);
    }
}
