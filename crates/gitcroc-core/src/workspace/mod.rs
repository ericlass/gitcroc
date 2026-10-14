//! Workspaces (section 3 "Workspace"): a named set of repos and groups,
//! each kept in `workspaces/<uuid>.toml` in the config dir (section 10
//! "Files") under the DATA-1 rules of [`crate::config_file`].
//!
//! Until workspace tabs exist (M3) one workspace is open: the most recently
//! modified file, or a new "My workspace". Every call still names the
//! workspace by id, so nothing assumes there is only one (ARCH-9).

mod add;

use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard, PoisonError};
use std::time::SystemTime;

use serde::{Deserialize, Serialize};

use crate::config_file::{self, ConfigWarning, Migration};
use crate::{ConfigDir, WorkspaceId};

pub use add::{AddReport, Refusal, SCAN_DEPTH, add_path};

/// The schema this build reads and writes.
pub const WORKSPACE_SCHEMA: u32 = 1;

/// The folder inside the config dir that holds the workspace files.
pub const WORKSPACES_DIR: &str = "workspaces";

/// The name of the workspace created when there is none.
pub const DEFAULT_WORKSPACE_NAME: &str = "My workspace";

/// `MIGRATIONS[n - 1]` migrates schema `n`. None exist yet.
const MIGRATIONS: &[Migration] = &[];

const _: () = assert!(WORKSPACE_SCHEMA as usize == MIGRATIONS.len() + 1);

/// Identifies a repo inside its workspace: `r1`, `r2` and so on.
#[derive(
    Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, specta::Type,
)]
pub struct RepoId(pub String);

impl RepoId {
    fn number(&self) -> Option<u64> {
        self.0.strip_prefix('r')?.parse().ok()
    }
}

impl std::fmt::Display for RepoId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

/// A repo as the UI sees it. The path is text for display only (PLAT-2).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
pub struct RepoView {
    pub id: RepoId,
    /// The repo's folder name.
    pub name: String,
    pub path: String,
    /// The URL of `origin`, or of the only remote, when it was added.
    pub remote: Option<String>,
}

/// Where the changes pane sits (UI-19).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "snake_case")]
pub enum Dock {
    Right,
    Bottom,
}

/// The workspace's pane layout (section 10, UI-19): where the changes pane
/// is docked and the pane sizes in CSS pixels. A field missing from the
/// file is `None`, and the UI uses its default; TOML leaves `None` out.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
pub struct PaneLayout {
    pub dock: Option<Dock>,
    /// The tree pane's width.
    pub tree: Option<u32>,
    /// The history pane's width while the changes pane is docked right.
    pub history: Option<u32>,
    /// The changes pane's height while it is docked at the bottom.
    pub changes: Option<u32>,
}

/// A workspace as the UI sees it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
pub struct WorkspaceView {
    pub id: WorkspaceId,
    pub name: String,
    /// In the order they were added.
    pub repos: Vec<RepoView>,
    pub layout: PaneLayout,
    /// Changes are refused; `warning` says why.
    pub read_only: bool,
    pub warning: Option<ConfigWarning>,
}

/// Errors from changing a workspace.
#[derive(Debug, thiserror::Error)]
pub enum WorkspaceError {
    #[error("the workspace is read-only, so the change was not saved")]
    ReadOnly,
    #[error("no open workspace has the id {0}")]
    UnknownWorkspace(String),
    #[error("the workspace has no repo with the id {0}")]
    UnknownRepo(RepoId),
    #[error("cannot write {}: {source}", path.display())]
    Write { path: PathBuf, source: io::Error },
    #[error("cannot encode the workspace: {0}")]
    Encode(#[from] toml::ser::Error),
    #[error("the add job did not finish: {0}")]
    Stopped(#[from] tokio::task::JoinError),
}

/// One `[[repo]]` of the file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct RepoEntry {
    id: RepoId,
    /// Canonical and absolute (GRP-1). Valid Unicode, or it is not added.
    path: PathBuf,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    remote: Option<String>,
    /// The user's answer to the offer to write a commit-graph file
    /// (PERF-2). Optional, so the schema stays 1: older builds ignore it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    commit_graph: Option<OfferAnswer>,
    /// The user's answer to the offer to turn on `core.fsmonitor` and
    /// `core.untrackedCache` (PERF-7), kept the same way.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    fsmonitor: Option<OfferAnswer>,
}

/// An answer to the commit-graph or the fsmonitor offer worth keeping.
/// Accepting keeps nothing: once the file exists or the settings are on,
/// there is nothing to offer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum OfferAnswer {
    Declined,
}

impl RepoEntry {
    fn view(&self) -> RepoView {
        RepoView {
            id: self.id.clone(),
            name: folder_name(&self.path),
            path: self.path.display().to_string(),
            remote: self.remote.clone(),
        }
    }
}

/// One `[[group]]` of the file. Groups arrive in M3; until then they are
/// read and written back as they are, less removed repos.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct Group {
    name: String,
    #[serde(default)]
    repos: Vec<RepoId>,
}

/// The file's contents, apart from `schema`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct WorkspaceFile {
    name: String,
    #[serde(default, rename = "repo", skip_serializing_if = "Vec::is_empty")]
    repos: Vec<RepoEntry>,
    #[serde(default, rename = "group", skip_serializing_if = "Vec::is_empty")]
    groups: Vec<Group>,
    /// Optional, so the schema stays 1: older builds ignore it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    layout: Option<PaneLayout>,
}

impl Default for WorkspaceFile {
    fn default() -> Self {
        Self {
            name: DEFAULT_WORKSPACE_NAME.to_owned(),
            repos: Vec::new(),
            groups: Vec::new(),
            layout: None,
        }
    }
}

impl WorkspaceFile {
    /// The number after the highest id in use, so a removed repo's id is
    /// not given to another repo while groups or caches may still hold it.
    fn next_id(&self) -> RepoId {
        let used = self
            .repos
            .iter()
            .map(|repo| &repo.id)
            .chain(self.groups.iter().flat_map(|group| &group.repos))
            .filter_map(RepoId::number)
            .max()
            .unwrap_or(0);
        RepoId(format!("r{}", used.saturating_add(1)))
    }
}

#[derive(Debug)]
struct Workspace {
    id: WorkspaceId,
    /// `None` when there is no config dir.
    path: Option<PathBuf>,
    file: WorkspaceFile,
    /// Set when the file must not be written.
    warning: Option<ConfigWarning>,
}

impl Workspace {
    fn view(&self) -> WorkspaceView {
        WorkspaceView {
            id: self.id.clone(),
            name: self.file.name.clone(),
            repos: self.file.repos.iter().map(RepoEntry::view).collect(),
            layout: self.file.layout.clone().unwrap_or_default(),
            read_only: self.warning.is_some() || self.path.is_none(),
            warning: self.warning.clone(),
        }
    }

    /// Applies `change` to a copy, saves it, then keeps it.
    fn update<T>(
        &mut self,
        change: impl FnOnce(&mut WorkspaceFile) -> Result<T, WorkspaceError>,
    ) -> Result<T, WorkspaceError> {
        let path = match (&self.path, &self.warning) {
            (Some(path), None) => path,
            _ => return Err(WorkspaceError::ReadOnly),
        };
        let mut next = self.file.clone();
        let result = change(&mut next)?;
        save(path, &next)?;
        self.file = next;
        Ok(result)
    }

    fn check_writable(&self) -> Result<(), WorkspaceError> {
        match (&self.path, &self.warning) {
            (Some(_), None) => Ok(()),
            _ => Err(WorkspaceError::ReadOnly),
        }
    }
}

/// The open workspaces, saved on every change.
#[derive(Debug)]
pub struct Workspaces {
    /// Also held while saving, so saves happen one at a time.
    open: Mutex<Vec<Workspace>>,
}

impl Workspaces {
    /// Opens the most recently modified workspace file in `dirs`, or
    /// creates "My workspace" when there is none. Never fails: problems
    /// give a read-only workspace and a warning.
    pub fn load(dirs: &dyn ConfigDir) -> Self {
        let workspace = match dirs.config_dir() {
            Some(dir) => open_latest(&dir.join(WORKSPACES_DIR)),
            None => Workspace {
                id: new_id(),
                path: None,
                file: WorkspaceFile::default(),
                warning: Some(ConfigWarning::NoConfigDir),
            },
        };
        Self {
            open: Mutex::new(vec![workspace]),
        }
    }

    /// The open workspaces, in tab order.
    pub fn views(&self) -> Vec<WorkspaceView> {
        self.lock().iter().map(Workspace::view).collect()
    }

    /// The workspace with id `id`.
    pub fn view(&self, id: &WorkspaceId) -> Result<WorkspaceView, WorkspaceError> {
        self.with(id, |workspace| Ok(workspace.view()))
    }

    /// The path of repo `repo`, for reading it.
    pub fn repo_path(&self, id: &WorkspaceId, repo: &RepoId) -> Result<PathBuf, WorkspaceError> {
        self.with(id, |workspace| {
            Ok(entry(&workspace.file, repo)?.path.clone())
        })
    }

    /// The ids and paths of the workspace's repos, in workspace order.
    pub(crate) fn repo_paths(
        &self,
        id: &WorkspaceId,
    ) -> Result<Vec<(RepoId, PathBuf)>, WorkspaceError> {
        self.with(id, |workspace| {
            Ok(workspace
                .file
                .repos
                .iter()
                .map(|entry| (entry.id.clone(), entry.path.clone()))
                .collect())
        })
    }

    /// Whether the user declined the offer to write a commit-graph file for
    /// `repo` (PERF-2).
    pub fn commit_graph_declined(
        &self,
        id: &WorkspaceId,
        repo: &RepoId,
    ) -> Result<bool, WorkspaceError> {
        self.with(id, |workspace| {
            Ok(entry(&workspace.file, repo)?.commit_graph == Some(OfferAnswer::Declined))
        })
    }

    /// Keeps "don't ask again" for the commit-graph offer of `repo`.
    pub fn decline_commit_graph(
        &self,
        id: &WorkspaceId,
        repo: &RepoId,
    ) -> Result<(), WorkspaceError> {
        self.with(id, |workspace| {
            workspace.update(|file| {
                let found = file
                    .repos
                    .iter_mut()
                    .find(|entry| entry.id == *repo)
                    .ok_or_else(|| WorkspaceError::UnknownRepo(repo.clone()))?;
                found.commit_graph = Some(OfferAnswer::Declined);
                Ok(())
            })
        })
    }

    /// Whether the user declined the offer to turn on `core.fsmonitor` and
    /// `core.untrackedCache` for `repo` (PERF-7).
    pub fn fsmonitor_declined(
        &self,
        id: &WorkspaceId,
        repo: &RepoId,
    ) -> Result<bool, WorkspaceError> {
        self.with(id, |workspace| {
            Ok(entry(&workspace.file, repo)?.fsmonitor == Some(OfferAnswer::Declined))
        })
    }

    /// Keeps "don't ask again" for the fsmonitor offer of `repo`.
    pub fn decline_fsmonitor(&self, id: &WorkspaceId, repo: &RepoId) -> Result<(), WorkspaceError> {
        self.with(id, |workspace| {
            workspace.update(|file| {
                let found = file
                    .repos
                    .iter_mut()
                    .find(|entry| entry.id == *repo)
                    .ok_or_else(|| WorkspaceError::UnknownRepo(repo.clone()))?;
                found.fsmonitor = Some(OfferAnswer::Declined);
                Ok(())
            })
        })
    }

    /// Keeps the workspace's pane layout (UI-19).
    pub fn set_layout(&self, id: &WorkspaceId, layout: PaneLayout) -> Result<(), WorkspaceError> {
        self.with(id, |workspace| {
            workspace.update(|file| {
                file.layout = Some(layout);
                Ok(())
            })
        })
    }

    /// Removes a repo from the workspace and from its groups. Only the
    /// workspace file changes, never anything on disk (GRP-3).
    pub fn remove_repo(
        &self,
        id: &WorkspaceId,
        repo: &RepoId,
    ) -> Result<WorkspaceView, WorkspaceError> {
        self.with(id, |workspace| {
            workspace.update(|file| {
                let before = file.repos.len();
                file.repos.retain(|entry| entry.id != *repo);
                if file.repos.len() == before {
                    return Err(WorkspaceError::UnknownRepo(repo.clone()));
                }
                for group in &mut file.groups {
                    group.repos.retain(|member| member != repo);
                }
                Ok(())
            })?;
            Ok(workspace.view())
        })
    }

    fn with<T>(
        &self,
        id: &WorkspaceId,
        action: impl FnOnce(&mut Workspace) -> Result<T, WorkspaceError>,
    ) -> Result<T, WorkspaceError> {
        let mut open = self.lock();
        let workspace = open
            .iter_mut()
            .find(|workspace| workspace.id == *id)
            .ok_or_else(|| WorkspaceError::UnknownWorkspace(id.0.clone()))?;
        action(workspace)
    }

    fn lock(&self) -> MutexGuard<'_, Vec<Workspace>> {
        // A panic while holding the lock cannot leave a workspace
        // half-changed: its file is replaced only after a successful save.
        self.open.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

fn entry<'file>(
    file: &'file WorkspaceFile,
    repo: &RepoId,
) -> Result<&'file RepoEntry, WorkspaceError> {
    file.repos
        .iter()
        .find(|entry| entry.id == *repo)
        .ok_or_else(|| WorkspaceError::UnknownRepo(repo.clone()))
}

fn new_id() -> WorkspaceId {
    WorkspaceId(uuid::Uuid::new_v4().to_string())
}

/// The last component of `path`, for display.
fn folder_name(path: &Path) -> String {
    path.file_name()
        .unwrap_or(path.as_os_str())
        .to_string_lossy()
        .into_owned()
}

/// Opens the most recently modified `*.toml` in `dir`, or creates a new
/// workspace file there.
fn open_latest(dir: &Path) -> Workspace {
    match latest_file(dir) {
        Ok(Some((id, path))) => {
            let loaded = config_file::load(&path, MIGRATIONS, decode);
            Workspace {
                id,
                path: Some(path),
                file: loaded.value,
                warning: loaded.warning,
            }
        }
        Ok(None) => create(dir),
        // Not knowing which workspace is the user's, creating another one
        // would hide theirs; open none for writing instead.
        Err(error) => Workspace {
            id: new_id(),
            path: None,
            file: WorkspaceFile::default(),
            warning: Some(ConfigWarning::Unreadable {
                path: dir.display().to_string(),
                reason: error.to_string(),
            }),
        },
    }
}

/// The id and path of the most recently modified workspace file in `dir`;
/// `None` when the folder is missing or has none.
fn latest_file(dir: &Path) -> io::Result<Option<(WorkspaceId, PathBuf)>> {
    let entries = match fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error),
    };
    let mut latest: Option<(SystemTime, WorkspaceId, PathBuf)> = None;
    for entry in entries {
        let entry = entry?;
        let path = entry.path();
        if path.extension().is_none_or(|extension| extension != "toml") {
            continue;
        }
        // The file name is the id, so a name that is not text cannot be one.
        let Some(id) = path.file_stem().and_then(|stem| stem.to_str()) else {
            continue;
        };
        let metadata = entry.metadata()?;
        if !metadata.is_file() {
            continue;
        }
        let modified = metadata.modified()?;
        let newer = latest
            .as_ref()
            .is_none_or(|(time, latest_id, _)| (modified, id) > (*time, latest_id.0.as_str()));
        if newer {
            latest = Some((modified, WorkspaceId(id.to_owned()), path));
        }
    }
    Ok(latest.map(|(_, id, path)| (id, path)))
}

/// A new "My workspace", saved at once so its id stays the same across
/// starts.
fn create(dir: &Path) -> Workspace {
    let id = new_id();
    let path = dir.join(format!("{}.toml", id.0));
    let file = WorkspaceFile::default();
    let warning = save(&path, &file)
        .err()
        .map(|error| ConfigWarning::Unreadable {
            path: path.display().to_string(),
            reason: error.to_string(),
        });
    Workspace {
        id,
        path: Some(path),
        file,
        warning,
    }
}

fn decode(table: toml::Table) -> Result<WorkspaceFile, toml::de::Error> {
    toml::Value::Table(table).try_into()
}

fn save(path: &Path, file: &WorkspaceFile) -> Result<(), WorkspaceError> {
    let text = config_file::encode(WORKSPACE_SCHEMA, file)?;
    config_file::write_atomic(path, text.as_bytes(), || Ok(())).map_err(|source| {
        WorkspaceError::Write {
            path: path.to_owned(),
            source,
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn data_1_workspace_file_matches_section_10() {
        let file = WorkspaceFile {
            name: "Platform".to_owned(),
            repos: vec![
                RepoEntry {
                    id: RepoId("r1".to_owned()),
                    path: PathBuf::from("/home/eric/src/platform/api"),
                    remote: Some("git@example.com:platform/api.git".to_owned()),
                    commit_graph: None,
                    fsmonitor: Some(OfferAnswer::Declined),
                },
                RepoEntry {
                    id: RepoId("r2".to_owned()),
                    path: PathBuf::from("/home/eric/src/platform/web"),
                    remote: None,
                    commit_graph: None,
                    fsmonitor: None,
                },
            ],
            groups: vec![Group {
                name: "Backend".to_owned(),
                repos: vec![RepoId("r1".to_owned())],
            }],
            layout: None,
        };
        let text = config_file::encode(WORKSPACE_SCHEMA, &file).unwrap();
        assert_eq!(
            text,
            r#"schema = 1
name = "Platform"

[[repo]]
id = "r1"
path = "/home/eric/src/platform/api"
remote = "git@example.com:platform/api.git"
fsmonitor = "declined"

[[repo]]
id = "r2"
path = "/home/eric/src/platform/web"

[[group]]
name = "Backend"
repos = ["r1"]
"#
        );
        let loaded = config_file::parse(Path::new("w.toml"), &text, MIGRATIONS, decode);
        assert_eq!(loaded.value, file);
        assert_eq!(loaded.warning, None);
    }

    #[test]
    fn grp_1_repo_ids_are_the_next_unused_number() {
        let mut file = WorkspaceFile::default();
        assert_eq!(file.next_id(), RepoId("r1".to_owned()));
        file.repos.push(RepoEntry {
            id: RepoId("r2".to_owned()),
            path: PathBuf::from("/a"),
            remote: None,
            commit_graph: None,
            fsmonitor: None,
        });
        file.groups.push(Group {
            name: "G".to_owned(),
            repos: vec![RepoId("r7".to_owned()), RepoId("odd".to_owned())],
        });
        assert_eq!(file.next_id(), RepoId("r8".to_owned()));
    }
}
