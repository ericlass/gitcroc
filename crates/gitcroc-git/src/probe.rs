//! What a folder is, for adding it to a workspace (GRP-1, section 3
//! "Repo"): a repo with a working tree, a bare repo, or neither.

use std::ffi::OsString;
use std::path::Path;

use gix::bstr::ByteSlice;

use crate::{GitError, GitRepo, GitRunner, OpenError};

/// What a folder looks like from its entries alone, without opening it.
/// Cheap enough to run on every folder of a scan.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Layout {
    /// It has a `.git` folder, file or link.
    WorkTree,
    /// It is a git directory itself, as a bare repo is: `HEAD`, `objects`
    /// and `refs`.
    GitDir,
    /// Neither.
    Plain,
}

/// The layout of the folder at `path`.
pub fn layout(path: &Path) -> Layout {
    if path.join(".git").symlink_metadata().is_ok() {
        Layout::WorkTree
    } else if path.join("HEAD").is_file()
        && path.join("objects").is_dir()
        && path.join("refs").is_dir()
    {
        Layout::GitDir
    } else {
        Layout::Plain
    }
}

/// A repo with a working tree that git accepts.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Inspected {
    /// The working tree, as `gix` found it from the path it was given.
    pub work_dir: std::path::PathBuf,
    /// The URL of `origin`, or of the only remote; `None` otherwise. Not
    /// valid UTF-8 is decoded lossily.
    pub remote: Option<String>,
}

/// Why a folder cannot be added.
#[derive(Debug, thiserror::Error)]
pub enum InspectError {
    #[error("it is a bare repo; gitcroc needs a repo with a working tree")]
    Bare,
    /// git refused the repo, for example because of `safe.directory`. The
    /// message is git's own, for the user to see (SEC-6).
    #[error("{message}")]
    Refused { message: String },
    #[error(transparent)]
    Open(#[from] OpenError),
    #[error(transparent)]
    Git(GitError),
}

impl From<GitError> for InspectError {
    fn from(error: GitError) -> Self {
        match error {
            GitError::Failed { stderr, .. } => Self::Refused {
                message: String::from_utf8_lossy(&stderr).trim_end().to_owned(),
            },
            other @ (GitError::Spawn { .. } | GitError::Killed) => Self::Git(other),
        }
    }
}

/// Opens the repo at `path` and checks it can be added: it has a working
/// tree, and git accepts it. When `gix` trusts the repo less than fully,
/// git is asked, so its refusal reaches the user and is never bypassed
/// (SEC-6). Runs git, so call it from a thread that may block.
pub fn inspect(path: &Path, git: &GitRunner) -> Result<Inspected, InspectError> {
    let repo = GitRepo::open(path)?.thread_local();
    let Some(work_dir) = repo.workdir().map(Path::to_owned) else {
        return Err(InspectError::Bare);
    };
    if repo.git_dir_trust() != gix::sec::Trust::Full {
        let args = [OsString::from("rev-parse"), OsString::from("--git-dir")];
        git.run_blocking(&args, Some(path))?;
    }
    Ok(Inspected {
        work_dir,
        remote: remote(&repo),
    })
}

fn remote(repo: &gix::Repository) -> Option<String> {
    let names = repo.remote_names();
    let name = if names.iter().any(|name| name.as_bytes() == b"origin") {
        "origin".into()
    } else if names.len() == 1 {
        names.first()?.to_string()
    } else {
        return None;
    };
    let url = repo
        .config_snapshot()
        .string(format!("remote.{name}.url").as_str())?;
    Some(url.to_str_lossy().into_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sec_6_git_refusals_of_an_added_repo_keep_gits_message() {
        let message = "fatal: detected dubious ownership in repository at '/repo'";
        let error = InspectError::from(GitError::Failed {
            code: Some(128),
            stdout: Vec::new(),
            stderr: format!("{message}\n").into_bytes(),
        });
        assert!(matches!(&error, InspectError::Refused { message: m } if m == message));
        assert_eq!(error.to_string(), message);
    }
}
