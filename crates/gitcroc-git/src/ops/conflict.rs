//! Resolving a conflicted file (OPS-5, OPS-6). Mark resolved writes the
//! editor's text to the file, the one write the app makes itself (ARCH-1),
//! and git stages it; choosing a side is git's alone.

use std::ffi::OsString;
use std::path::{Path, PathBuf};

use gix::bstr::ByteSlice;

use super::with_paths;
use crate::{GitError, GitRunner, Output, StageEntry, Write};

/// A side of a conflict.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Side {
    Ours,
    Theirs,
}

/// How a chosen side is put in place (OPS-6).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Choice {
    /// The side's file, link or submodule commit, with its mode, staged and
    /// checked out.
    Stage(StageEntry),
    /// The side deleted the file: deleted from the working tree and the
    /// index.
    Remove,
    /// The side deleted a submodule: taken out of the index only, its
    /// folder left alone.
    RemoveFromIndex,
}

/// `text` with the line endings of `on_disk`: each lone LF becomes CRLF
/// when the file's first line ends with CRLF.
pub fn with_line_endings(text: &[u8], on_disk: &[u8]) -> Vec<u8> {
    let crlf = on_disk
        .find_byte(b'\n')
        .is_some_and(|at| at > 0 && on_disk.get(at - 1) == Some(&b'\r'));
    if !crlf {
        return text.to_vec();
    }
    let mut out = Vec::with_capacity(text.len() + text.len() / 16);
    let mut previous = None;
    for &byte in text {
        if byte == b'\n' && previous != Some(b'\r') {
            out.push(b'\r');
        }
        out.push(byte);
        previous = Some(byte);
    }
    out
}

/// The file `git_path` names in the working tree `repo`, refused when it
/// or a folder on the way is a symbolic link, so the write cannot land
/// outside the working tree.
fn work_tree_file(repo: &Path, git_path: &[u8]) -> Result<PathBuf, GitError> {
    let relative = relative_path(repo, git_path)?;
    let mut path = repo.to_path_buf();
    for part in relative.components() {
        match part {
            std::path::Component::Normal(name) => path.push(name),
            _ => {
                let error =
                    std::io::Error::new(std::io::ErrorKind::InvalidInput, "not a path in the repo");
                return Err(refused(&path, error));
            }
        }
        if std::fs::symlink_metadata(&path).is_ok_and(|metadata| metadata.file_type().is_symlink())
        {
            let error = std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "a symbolic link is on the way",
            );
            return Err(refused(&path, error));
        }
    }
    Ok(path)
}

fn refused(path: &Path, source: std::io::Error) -> GitError {
    GitError::Write {
        path: path.to_path_buf(),
        source,
    }
}

/// Mark resolved (OPS-5): writes `text` to the file at `git_path`, with
/// the line endings the file has and its mode kept, then stages it with
/// `git add`. The caller checked the markers and that the path is still in
/// conflict.
pub async fn mark_resolved(
    git: &GitRunner,
    repo: &Path,
    git_path: &[u8],
    text: &[u8],
    write: Write<'_>,
) -> Result<Output, GitError> {
    let file = work_tree_file(repo, git_path)?;
    let on_disk = match tokio::fs::read(&file).await {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Vec::new(),
        Err(error) => return Err(refused(&file, error)),
    };
    // Writing into the file, not replacing it, keeps its mode.
    let contents = with_line_endings(text, &on_disk);
    tokio::fs::write(&file, &contents)
        .await
        .map_err(|error| refused(&file, error))?;
    with_paths(git, repo, &["add"], &[git_path], write).await
}

/// Resolves the file at `git_path` with one side (OPS-6), as `choice`
/// says. A staged side replaces every conflict stage of the path; its
/// mode is the side's on every platform, which `git checkout --ours` and
/// `git add` would not keep where the file system has no executable bit.
pub async fn choose(
    git: &GitRunner,
    repo: &Path,
    git_path: &[u8],
    choice: &Choice,
    write: Write<'_>,
) -> Result<Output, GitError> {
    match choice {
        Choice::Stage(entry) => {
            let mut args = ["update-index", "--add", "--cacheinfo"]
                .map(OsString::from)
                .to_vec();
            args.push(entry.mode_text().into());
            args.push(entry.id.to_string().into());
            args.push(os_path(repo, git_path)?);
            let staged = git.run_write(&args, repo, write).await?;
            if entry.is_submodule() {
                return Ok(staged);
            }
            let checkout = with_paths(git, repo, &["checkout"], &[git_path], write).await?;
            Ok(joined(staged, checkout))
        }
        Choice::Remove => with_paths(git, repo, &["rm", "--quiet"], &[git_path], write).await,
        Choice::RemoveFromIndex => {
            let mut args = ["update-index", "--force-remove", "--"]
                .map(OsString::from)
                .to_vec();
            args.push(os_path(repo, git_path)?);
            git.run_write(&args, repo, write).await
        }
    }
}

/// `git_path` as an OS path (PLAT-2).
fn relative_path(repo: &Path, git_path: &[u8]) -> Result<PathBuf, GitError> {
    gix::path::from_bstr(git_path.as_bstr())
        .map(std::borrow::Cow::into_owned)
        .map_err(|error| {
            let error = std::io::Error::new(std::io::ErrorKind::InvalidInput, error.to_string());
            refused(repo, error)
        })
}

/// `git_path` as an argument for git.
fn os_path(repo: &Path, git_path: &[u8]) -> Result<OsString, GitError> {
    Ok(relative_path(repo, git_path)?.into_os_string())
}

fn joined(first: Output, second: Output) -> Output {
    let mut all = first;
    all.stdout.extend(second.stdout);
    all.stderr.extend(second.stderr);
    all
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ops_5_crlf_files_get_crlf() {
        assert_eq!(with_line_endings(b"a\nb\r\nc", b"x\r\ny\n"), b"a\r\nb\r\nc");
        assert_eq!(with_line_endings(b"a\nb\n", b"x\ny\r\n"), b"a\nb\n");
        assert_eq!(with_line_endings(b"a\n", b""), b"a\n");
    }

    #[test]
    fn ops_5_the_write_stays_in_the_working_tree() {
        let dir = tempfile::tempdir().unwrap();
        assert!(work_tree_file(dir.path(), b"../outside").is_err());
        assert!(work_tree_file(dir.path(), b"/etc/passwd").is_err());
        assert_eq!(
            work_tree_file(dir.path(), b"a/b.txt").unwrap(),
            dir.path().join("a").join("b.txt")
        );
    }
}
