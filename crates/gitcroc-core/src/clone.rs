//! Clone (section 6 "Repos", M2-14): a repo is cloned into a new folder
//! of a parent folder the user picked, as a job with progress that cancel
//! kills (GRP-10), then added to the workspace as a folder is (GRP-1). The
//! URL, the folder name and the target are checked before git runs; a
//! clone that fails or is cancelled leaves nothing behind it created.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use gitcroc_git::{GitRunner, Write, ops, parse_progress};
use serde::{Deserialize, Serialize};
use tokio::sync::oneshot;

use crate::job::git_ending;
use crate::remote::check_url;
use crate::{
    JobError, JobId, Jobs, Outcome, RepoView, WorkspaceError, WorkspaceId, Workspaces, add_path,
};

/// How often a folder that cannot be removed yet, as on Windows while a
/// process of the killed git still holds a file in it, is tried again,
/// and the pause in between.
const REMOVE_TRIES: u32 = 10;
const REMOVE_PAUSE: Duration = Duration::from_millis(100);

/// A clone to start: `url` into folder `name` of `parent`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
pub struct CloneRequest {
    pub url: String,
    pub parent: PathBuf,
    pub name: String,
}

/// How a clone ended.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
pub struct CloneReport {
    pub workspace_id: WorkspaceId,
    pub job_id: JobId,
    /// The cloned repo, now in the workspace, to select; `None` when the
    /// clone failed, was cancelled or could not be added.
    pub repo: Option<RepoView>,
}

/// A started clone: its job, and its report once it ended.
#[derive(Debug)]
pub struct CloneHandle {
    pub job_id: JobId,
    pub ended: oneshot::Receiver<CloneReport>,
}

/// Why a clone did not start.
#[derive(Debug, thiserror::Error)]
pub enum CloneError {
    #[error(transparent)]
    Workspace(#[from] WorkspaceError),
    #[error(transparent)]
    Job(#[from] JobError),
    #[error("{0}")]
    Refused(String),
    #[error("the check did not finish: {0}")]
    Stopped(#[from] tokio::task::JoinError),
}

/// The folder a clone of `url` gets by default: the last segment of its
/// path, without `.git`, as git names it; `None` when there is none to
/// take or it is no usable folder name.
pub fn clone_default_name(url: &str) -> Option<String> {
    let path = url.trim().trim_end_matches(['/', '\\']);
    let path = path.strip_suffix("/.git").unwrap_or(path);
    let last = path.rsplit(['/', '\\', ':']).next().unwrap_or(path);
    let name = last.strip_suffix(".git").unwrap_or(last);
    folder_name_error(name).is_none().then(|| name.to_owned())
}

/// Why `name` cannot be a new folder's name on any platform, or `None`.
fn folder_name_error(name: &str) -> Option<&'static str> {
    if name.is_empty() {
        Some("the folder name is empty")
    } else if name == "." || name == ".." {
        Some("the folder name cannot be . or ..")
    } else if name.contains(['/', '\\']) {
        Some("the folder name cannot contain / or \\")
    } else if name.contains([':', '*', '?', '"', '<', '>', '|']) {
        Some("the folder name cannot contain : * ? \" < > or |")
    } else if name.chars().any(char::is_control) {
        Some("the folder name cannot contain control characters")
    } else if name.trim() != name || name.ends_with('.') {
        Some("the folder name cannot start or end with a space, or end with .")
    } else {
        None
    }
}

/// The folder `name` of `parent` a clone goes to, and whether it exists
/// already, empty; or why it cannot.
fn check_target(parent: &Path, name: &str) -> Result<(PathBuf, bool), String> {
    if let Some(why) = folder_name_error(name) {
        return Err(why.to_owned());
    }
    match fs::metadata(parent) {
        Ok(metadata) if metadata.is_dir() => {}
        Ok(_) => return Err(format!("{} is not a folder", parent.display())),
        Err(error) => return Err(format!("cannot open {}: {error}", parent.display())),
    }
    let target = parent.join(name);
    // The entry itself: a symlink is never cloned into.
    match fs::symlink_metadata(&target) {
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok((target, false)),
        Err(error) => Err(format!("cannot open {}: {error}", target.display())),
        Ok(metadata) if !metadata.is_dir() => {
            Err(format!("{} exists and is not a folder", target.display()))
        }
        Ok(_) => match fs::read_dir(&target).map(|mut entries| entries.next().is_none()) {
            Ok(true) => Ok((target, true)),
            Ok(false) => Err(format!(
                "the folder {} exists and is not empty",
                target.display()
            )),
            Err(error) => Err(format!("cannot open {}: {error}", target.display())),
        },
    }
}

/// Why a clone into folder `name` of `parent` would be refused, or `None`,
/// for the dialog to show as the user types. Reads the disk.
pub async fn clone_target_check(parent: PathBuf, name: String) -> Option<String> {
    tokio::task::spawn_blocking(move || check_target(&parent, &name).err())
        .await
        .unwrap_or_else(|error| Some(format!("the check did not finish: {error}")))
}

/// Removes what a clone left in `target`: the folder, or, when it existed
/// before, empty, everything in it.
fn remove_clone(target: &Path, existed: bool) -> io::Result<()> {
    if !existed {
        return match fs::remove_dir_all(target) {
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
            result => result,
        };
    }
    for entry in fs::read_dir(target)? {
        let entry = entry?;
        if entry.file_type()?.is_dir() {
            fs::remove_dir_all(entry.path())?;
        } else {
            fs::remove_file(entry.path())?;
        }
    }
    Ok(())
}

/// [`remove_clone`], tried again while a file is still in use.
fn remove_clone_retrying(target: &Path, existed: bool) -> io::Result<()> {
    let mut tries = 1;
    loop {
        match remove_clone(target, existed) {
            Err(_) if tries < REMOVE_TRIES => {
                tries += 1;
                std::thread::sleep(REMOVE_PAUSE);
            }
            result => return result,
        }
    }
}

/// Clones `request` for workspace `id` as a job and returns at once. The
/// URL, folder name and target are checked first: a folder that exists and
/// is not empty is refused before git runs. Cancel kills git (GRP-10). On
/// success the repo is added to the workspace (GRP-1); a failed or
/// cancelled clone removes the folder it created, and only that folder, or
/// empties the one that was empty.
pub async fn clone_start(
    workspaces: Arc<Workspaces>,
    jobs: Arc<Jobs>,
    git: GitRunner,
    id: WorkspaceId,
    request: CloneRequest,
) -> Result<CloneHandle, CloneError> {
    if workspaces.view(&id)?.read_only {
        return Err(WorkspaceError::ReadOnly.into());
    }
    let url = check_url(&request.url).map_err(|why| CloneError::Refused(why.to_owned()))?;
    let CloneRequest { parent, name, .. } = request;
    let checked = {
        let (parent, name) = (parent.clone(), name.clone());
        tokio::task::spawn_blocking(move || check_target(&parent, &name)).await?
    };
    let (target, existed) = checked.map_err(CloneError::Refused)?;
    let tracked = jobs.track_network(&id, &format!("Clone {name}"))?;
    let job_id = tracked.job_id();
    let (sender, ended) = oneshot::channel();
    tokio::spawn(async move {
        let result = {
            let report = |line: &str| {
                if let Some(progress) = parse_progress(line) {
                    tracked.step(&progress.step, progress.done, progress.total);
                }
            };
            let write = Write {
                on_line: Some(&report),
                ..tracked.write()
            };
            ops::clone(&git, &parent, &url, &target, write).await
        };
        let (outcome, output, mut message) = git_ending(&result);
        let mut repo = None;
        if result.is_ok() {
            let added = add_path(workspaces, id.clone(), target, git, job_id, |_| {}).await;
            match added {
                Ok(report) => {
                    repo = report
                        .added
                        .into_iter()
                        .chain(report.already_present)
                        .next();
                    if repo.is_none() {
                        message = Some(report.refused.first().map_or_else(
                            || "the clone was not added to the workspace".to_owned(),
                            |refusal| format!("the clone was not added: {}", refusal.reason),
                        ));
                    }
                }
                Err(error) => message = Some(format!("the clone was not added: {error}")),
            }
        } else {
            let shown = target.display().to_string();
            let removed =
                tokio::task::spawn_blocking(move || remove_clone_retrying(&target, existed)).await;
            let failure = match removed {
                Ok(Ok(())) => None,
                Ok(Err(error)) => Some(error.to_string()),
                Err(error) => Some(error.to_string()),
            };
            if let Some(failure) = failure {
                let before = message
                    .map(|message| format!("{message}; "))
                    .unwrap_or_default();
                message = Some(format!("{before}{shown} could not be removed: {failure}"));
            }
        }
        let outcome = if result.is_ok() && repo.is_none() {
            Outcome::Failed
        } else {
            outcome
        };
        tracked.end(outcome, output, message);
        // The caller may not wait for the end.
        let _ = sender.send(CloneReport {
            workspace_id: id,
            job_id,
            repo,
        });
    });
    Ok(CloneHandle { job_id, ended })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_default_name_is_the_last_path_segment_without_git() {
        let name = |url| clone_default_name(url);
        assert_eq!(
            name("https://github.com/ericlass/gitcroc.git").as_deref(),
            Some("gitcroc")
        );
        assert_eq!(name("https://example.com/a/repo/").as_deref(), Some("repo"));
        assert_eq!(
            name("git@example.com:team/tool.git").as_deref(),
            Some("tool")
        );
        assert_eq!(name("host:tool.git").as_deref(), Some("tool"));
        assert_eq!(name("/srv/git/repo/.git").as_deref(), Some("repo"));
        assert_eq!(name(r"C:\repos\lib.git").as_deref(), Some("lib"));
        assert_eq!(name("file:///srv/origin.git").as_deref(), Some("origin"));
        assert_eq!(name(""), None);
        assert_eq!(name(".git"), None);
        assert_eq!(name("https://example.com/.."), None);
    }

    #[test]
    fn folder_names_that_fail_somewhere_are_refused() {
        for bad in [
            "", ".", "..", "a/b", r"a\b", "a:b", "a?", "a\tb", " a", "a ", "a.",
        ] {
            assert!(folder_name_error(bad).is_some(), "{bad:?}");
        }
        for good in ["repo", "my repo", ".hidden", "a.b", "ünï"] {
            assert_eq!(folder_name_error(good), None, "{good:?}");
        }
    }
}
