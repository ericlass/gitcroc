//! Adding repos to a workspace (section 6 "Add existing folder", GRP-1): a
//! repo with a working tree is added; any other folder is scanned for
//! repos. A job with progress (ARCH-7).

use std::fmt::Write as _;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use gitcroc_git::{GitRunner, Layout, inspect, layout};
use serde::{Deserialize, Serialize};

use super::{RepoEntry, RepoView, Workspace, WorkspaceError, Workspaces};
use crate::{JobId, JobProgress, Outcome, WorkspaceId};

/// How many folder levels below the chosen folder a scan looks at.
pub const SCAN_DEPTH: usize = 4;

/// At most one progress event per this long, apart from the last.
const PROGRESS_INTERVAL: Duration = Duration::from_millis(100);

/// What an add job did.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
pub struct AddReport {
    pub workspace_id: WorkspaceId,
    pub job_id: JobId,
    /// New in the workspace, in path order.
    pub added: Vec<RepoView>,
    /// Found, and already in the workspace (GRP-1).
    pub already_present: Vec<RepoView>,
    /// Folders that were not added or could not be read.
    pub refused: Vec<Refusal>,
}

/// A folder that was not added, and why. Git's own refusals carry git's
/// message (SEC-6). The path is text for display only (PLAT-2).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
pub struct Refusal {
    pub path: String,
    pub reason: String,
}

impl AddReport {
    /// How the add job ended (GRP-7): *succeeded* when it added a repo,
    /// *nothing to do* when every repo found was present and nothing was
    /// refused, else *failed*. The output lists each folder and what
    /// became of it.
    pub fn ending(&self) -> (Outcome, String, Option<String>) {
        // Writing to a String cannot fail.
        let mut output = String::new();
        for repo in &self.added {
            let _ = writeln!(output, "added {}", repo.path);
        }
        for repo in &self.already_present {
            let _ = writeln!(output, "already present {}", repo.path);
        }
        for refusal in &self.refused {
            let _ = writeln!(output, "refused {}: {}", refusal.path, refusal.reason);
        }
        if !self.added.is_empty() {
            (Outcome::Succeeded, output, None)
        } else if self.refused.is_empty() {
            (Outcome::NothingToDo, output, None)
        } else {
            (
                Outcome::Failed,
                output,
                Some("no repo was added".to_owned()),
            )
        }
    }
}

impl Refusal {
    fn new(path: &Path, reason: impl Into<String>) -> Self {
        Self {
            path: path.display().to_string(),
            reason: reason.into(),
        }
    }
}

/// Adds the repo at `path` to workspace `workspace`, or every repo found
/// below it to a depth of [`SCAN_DEPTH`]. The scan does not descend into a
/// repo it found, does not follow symlinks and skips folders it cannot
/// read. Reports progress through `progress` and runs on a blocking thread
/// (ARCH-7). Fails only when the workspace cannot take repos; a problem
/// with a folder is a [`Refusal`] in the report.
pub async fn add_path(
    workspaces: Arc<Workspaces>,
    workspace: WorkspaceId,
    path: PathBuf,
    git: GitRunner,
    job_id: JobId,
    progress: impl FnMut(JobProgress) + Send + 'static,
) -> Result<AddReport, WorkspaceError> {
    workspaces.with(&workspace, |open| open.check_writable())?;
    let mut progress = Progress {
        job_id,
        workspace_id: workspace.clone(),
        emit: progress,
        last: None,
    };
    tokio::task::spawn_blocking(move || {
        let (found, mut refused) = find(&path, &mut progress);
        let total = u32::try_from(found.len())
            .unwrap_or(u32::MAX)
            .saturating_add(1);
        let mut repos = Vec::new();
        for (done, candidate) in (0u32..).zip(found) {
            progress.step("Checking repos", done, total, false);
            match check(&candidate, &git) {
                Ok(repo) => repos.push(repo),
                Err(refusal) => refused.push(refusal),
            }
        }
        progress.step("Adding repos", total.saturating_sub(1), total, true);
        let result = workspaces.with(&workspace, |open| insert(open, repos));
        progress.step("Adding repos", total, total, true);
        let (added, already_present) = result?;
        Ok(AddReport {
            workspace_id: workspace,
            job_id,
            added,
            already_present,
            refused,
        })
    })
    .await?
}

struct Progress<F> {
    job_id: JobId,
    workspace_id: WorkspaceId,
    emit: F,
    last: Option<Instant>,
}

impl<F: FnMut(JobProgress)> Progress<F> {
    /// Reports `done` of `total` steps, unless one was reported very
    /// recently and `always` is false. A `total` of 0 means not known yet.
    fn step(&mut self, label: &str, done: u32, total: u32, always: bool) {
        let now = Instant::now();
        if !always
            && self
                .last
                .is_some_and(|last| now.duration_since(last) < PROGRESS_INTERVAL)
        {
            return;
        }
        self.last = Some(now);
        (self.emit)(JobProgress {
            job_id: self.job_id,
            workspace_id: self.workspace_id.clone(),
            label: label.to_owned(),
            done,
            total,
        });
    }
}

/// The folders under `path` that look like repos, in path order, and the
/// folders that could not be read.
fn find<F: FnMut(JobProgress)>(
    path: &Path,
    progress: &mut Progress<F>,
) -> (Vec<PathBuf>, Vec<Refusal>) {
    let mut found = Vec::new();
    let mut refused = Vec::new();
    let root = match dunce::canonicalize(path) {
        Ok(root) if root.is_dir() => root,
        Ok(_) => return (found, vec![Refusal::new(path, "it is not a folder")]),
        Err(error) => {
            return (
                found,
                vec![Refusal::new(path, format!("cannot open it: {error}"))],
            );
        }
    };
    if layout(&root) != Layout::Plain {
        return (vec![root], refused);
    }

    let mut folders = 0u32;
    // The folders still to read, each with its depth below `root`.
    let mut pending = vec![(root.clone(), 0)];
    while let Some((folder, depth)) = pending.pop() {
        folders = folders.saturating_add(1);
        progress.step("Looking for repos", folders, 0, false);
        let entries = match fs::read_dir(&folder) {
            Ok(entries) => entries,
            Err(error) => {
                refused.push(Refusal::new(&folder, format!("cannot read it: {error}")));
                continue;
            }
        };
        let mut below = Vec::new();
        for entry in entries {
            let entry = match entry {
                Ok(entry) => entry,
                Err(error) => {
                    refused.push(Refusal::new(&folder, format!("cannot read it: {error}")));
                    break;
                }
            };
            // The entry's own type: a symlink is not a folder here, so
            // links are never followed.
            if !entry.file_type().is_ok_and(|kind| kind.is_dir()) {
                continue;
            }
            let child = entry.path();
            match layout(&child) {
                Layout::WorkTree | Layout::GitDir => found.push(child),
                Layout::Plain if depth + 1 < SCAN_DEPTH => below.push((child, depth + 1)),
                Layout::Plain => {}
            }
        }
        pending.extend(below);
    }
    if found.is_empty() && refused.is_empty() {
        refused.push(Refusal::new(
            &root,
            format!("no repo found in it or in its folders to a depth of {SCAN_DEPTH}"),
        ));
    }
    found.sort();
    refused.sort_by(|a, b| a.path.cmp(&b.path));
    (found, refused)
}

/// A repo that can be added: its canonical working tree and its remote.
struct Candidate {
    path: PathBuf,
    remote: Option<String>,
}

/// Opens the folder at `path` and checks it can be added.
fn check(path: &Path, git: &GitRunner) -> Result<Candidate, Refusal> {
    let inspected = inspect(path, git).map_err(|error| Refusal::new(path, error.to_string()))?;
    let work_dir = dunce::canonicalize(&inspected.work_dir)
        .map_err(|error| Refusal::new(path, format!("cannot open it: {error}")))?;
    if work_dir.to_str().is_none() {
        return Err(Refusal::new(
            &work_dir,
            "its path is not valid Unicode, which the workspace file cannot store",
        ));
    }
    Ok(Candidate {
        path: work_dir,
        remote: inspected.remote,
    })
}

/// Adds the candidates that are not in the workspace yet, keyed by their
/// canonical path and by folder identity, which also matches two spellings
/// of one path on a case-insensitive file system (GRP-1, PLAT-2). Saves only
/// when something was added. Returns the added repos and those already
/// present.
fn insert(
    workspace: &mut Workspace,
    candidates: Vec<Candidate>,
) -> Result<(Vec<RepoView>, Vec<RepoView>), WorkspaceError> {
    let mut known: Vec<Known> = workspace
        .file
        .repos
        .iter()
        .map(|repo| Known::new(&repo.path))
        .collect();
    let mut present = Vec::new();
    let mut new = Vec::new();
    let mut next = workspace.file.clone();
    for candidate in candidates {
        let key = Known::new(&candidate.path);
        // `known` lists the workspace's repos, then the new ones, as
        // `next.repos` does.
        let index = known.iter().position(|other| other.matches(&key));
        if let Some(repo) = index.and_then(|index| next.repos.get(index)) {
            present.push(repo.view());
            continue;
        }
        let entry = RepoEntry {
            id: next.next_id(),
            path: candidate.path,
            remote: candidate.remote,
            commit_graph: None,
        };
        new.push(entry.view());
        next.repos.push(entry);
        known.push(key);
    }
    if !new.is_empty() {
        workspace.update(|file| {
            *file = next;
            Ok(())
        })?;
    }
    Ok((new, present))
}

/// A repo's path and folder identity, when the folder exists.
struct Known {
    path: PathBuf,
    identity: Option<Identity>,
}

impl Known {
    fn new(path: &Path) -> Self {
        Self {
            path: path.to_owned(),
            identity: identity(path),
        }
    }

    fn matches(&self, other: &Self) -> bool {
        self.path == other.path || (self.identity.is_some() && self.identity == other.identity)
    }
}

#[cfg(unix)]
type Identity = (u64, u64);

/// The device and inode, without keeping the folder open.
#[cfg(unix)]
fn identity(path: &Path) -> Option<Identity> {
    use std::os::unix::fs::MetadataExt;

    fs::metadata(path)
        .ok()
        .map(|metadata| (metadata.dev(), metadata.ino()))
}

#[cfg(windows)]
type Identity = same_file::Handle;

/// The volume and file index, read through an open handle.
#[cfg(windows)]
fn identity(path: &Path) -> Option<Identity> {
    same_file::Handle::from_path(path).ok()
}

#[cfg(not(any(unix, windows)))]
type Identity = ();

#[cfg(not(any(unix, windows)))]
fn identity(_: &Path) -> Option<Identity> {
    None
}
