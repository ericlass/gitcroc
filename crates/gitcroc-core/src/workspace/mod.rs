//! Workspaces (section 3 "Workspace"): a named set of repos and groups,
//! each kept in `workspaces/<uuid>.toml` in the config dir (section 10
//! "Files") under the DATA-1 rules of [`crate::config_file`].
//!
//! Several workspaces are open at once, one per tab (UI-27). The open ones,
//! their order and the active one are kept in the `[session]` table of
//! `settings.toml` and restored on the next start (UI-33). Every call names
//! the workspace by id (ARCH-9).

mod add;

use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::SystemTime;

use serde::{Deserialize, Serialize};

use crate::config_file::{self, ConfigWarning, Migration};
use crate::settings::{Session, SettingsStore};
use crate::{ConfigDir, WorkspaceId};

pub use add::{AddReport, Refusal, SCAN_DEPTH, add_path};

/// The schema this build reads and writes.
pub const WORKSPACE_SCHEMA: u32 = 1;

/// The folder inside the config dir that holds the workspace files.
pub const WORKSPACES_DIR: &str = "workspaces";

/// The name of the workspace created when there is none.
pub const DEFAULT_WORKSPACE_NAME: &str = "My workspace";

/// The longest workspace name, in characters.
pub const MAX_WORKSPACE_NAME: usize = 64;

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

/// The open workspaces as the UI sees them (UI-27, UI-33).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
pub struct WorkspaceTabs {
    /// In tab order. Empty when every tab was closed.
    pub open: Vec<WorkspaceView>,
    /// `None` only when no workspace is open.
    pub active: Option<WorkspaceId>,
    /// The workspace files the saved tabs named that are gone, left out at
    /// start. Paths are text for display only (PLAT-2).
    pub missing: Vec<String>,
}

/// A workspace file, for the list of every workspace (UI-28).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
pub struct WorkspaceListItem {
    pub id: WorkspaceId,
    pub name: String,
    /// It is open in a tab.
    pub open: bool,
    /// The file opens read-only; this says why (DATA-1).
    pub warning: Option<ConfigWarning>,
}

/// Errors from changing a workspace.
#[derive(Debug, thiserror::Error)]
pub enum WorkspaceError {
    #[error("the workspace is read-only, so the change was not saved")]
    ReadOnly,
    #[error("no open workspace has the id {0}")]
    UnknownWorkspace(String),
    #[error("no workspace file has the id {0}")]
    NoSuchWorkspace(String),
    #[error("a workspace name cannot be blank")]
    BlankName,
    #[error("a workspace name has at most {MAX_WORKSPACE_NAME} characters")]
    NameTooLong,
    #[error("a workspace named \"{0}\" exists already")]
    NameTaken(String),
    #[error("the workspace is open; close its tab first")]
    IsOpen,
    #[error("deleting a workspace needs confirmation")]
    NotConfirmed,
    #[error("the tab order must name each open workspace once")]
    BadOrder,
    #[error("cannot read {}: {source}", path.display())]
    Read { path: PathBuf, source: io::Error },
    #[error("cannot delete {}: {source}", path.display())]
    Delete { path: PathBuf, source: io::Error },
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

/// The open workspaces and the active one.
#[derive(Debug, Default)]
struct Tabs {
    /// In tab order.
    open: Vec<Workspace>,
    /// `None` only when `open` is empty.
    active: Option<WorkspaceId>,
    /// See [`WorkspaceTabs::missing`].
    missing: Vec<String>,
}

impl Tabs {
    fn ids(&self) -> Vec<WorkspaceId> {
        self.open
            .iter()
            .map(|workspace| workspace.id.clone())
            .collect()
    }

    fn is_open(&self, id: &WorkspaceId) -> bool {
        self.open.iter().any(|workspace| workspace.id == *id)
    }
}

/// The open workspaces, saved on every change, and the session in the
/// settings (UI-33).
#[derive(Debug)]
pub struct Workspaces {
    /// `workspaces/` in the config dir; `None` when there is none.
    dir: Option<PathBuf>,
    settings: Arc<SettingsStore>,
    /// Also held while saving, so saves happen one at a time.
    tabs: Mutex<Tabs>,
}

impl Workspaces {
    /// [`Self::load_with`] with the settings of `dirs`, for tools and tests
    /// that keep no settings store of their own.
    pub fn load(dirs: &dyn ConfigDir) -> Self {
        Self::load_with(dirs, Arc::new(SettingsStore::load(dirs)))
    }

    /// Opens the workspaces of the session saved in `settings` (UI-33),
    /// leaving out those whose file is gone. Without a saved session, opens
    /// the most recently modified workspace file in `dirs`, or creates "My
    /// workspace" when there is none. Never fails: problems give read-only
    /// workspaces and warnings.
    pub fn load_with(dirs: &dyn ConfigDir, settings: Arc<SettingsStore>) -> Self {
        let Some(dir) = dirs.config_dir().map(|dir| dir.join(WORKSPACES_DIR)) else {
            let workspace = Workspace {
                id: new_id(),
                path: None,
                file: WorkspaceFile::default(),
                warning: Some(ConfigWarning::NoConfigDir),
            };
            return Self {
                dir: None,
                settings,
                tabs: Mutex::new(Tabs {
                    active: Some(workspace.id.clone()),
                    open: vec![workspace],
                    missing: Vec::new(),
                }),
            };
        };
        let tabs = match settings.session() {
            Some(session) => restore(&dir, &session),
            None => {
                let workspace = open_latest(&dir);
                Tabs {
                    active: Some(workspace.id.clone()),
                    open: vec![workspace],
                    missing: Vec::new(),
                }
            }
        };
        Self {
            dir: Some(dir),
            settings,
            tabs: Mutex::new(tabs),
        }
    }

    /// The open workspaces, in tab order.
    pub fn views(&self) -> Vec<WorkspaceView> {
        self.lock().open.iter().map(Workspace::view).collect()
    }

    /// The open workspaces, the active one and the files left out at start.
    pub fn tabs(&self) -> WorkspaceTabs {
        let tabs = self.lock();
        WorkspaceTabs {
            open: tabs.open.iter().map(Workspace::view).collect(),
            active: tabs.active.clone(),
            missing: tabs.missing.clone(),
        }
    }

    /// The active workspace, if one is open.
    pub fn active(&self) -> Option<WorkspaceId> {
        self.lock().active.clone()
    }

    /// Every workspace file, open or not, sorted by name, ignoring case. A
    /// file that cannot be read is listed with its warning (DATA-1). Reads
    /// every workspace file: call it off the async workers (ARCH-7).
    pub fn list(&self) -> Result<Vec<WorkspaceListItem>, WorkspaceError> {
        list(self.dir.as_deref(), &self.lock())
    }

    /// Creates a workspace named `name`, trimmed, opens it as the last tab
    /// and makes it active. The name must be unique among the workspace
    /// files, ignoring case.
    pub fn create(&self, name: &str) -> Result<WorkspaceId, WorkspaceError> {
        let name = check_name(name)?;
        let dir = self.dir.as_deref().ok_or(WorkspaceError::ReadOnly)?;
        let mut tabs = self.lock();
        check_unique(&list(Some(dir), &tabs)?, &name, None)?;
        let id = new_id();
        let path = file_path(dir, &id);
        let file = WorkspaceFile {
            name,
            ..WorkspaceFile::default()
        };
        save(&path, &file)?;
        tabs.open.push(Workspace {
            id: id.clone(),
            path: Some(path),
            file,
            warning: None,
        });
        tabs.active = Some(id.clone());
        self.save_session(&tabs);
        Ok(id)
    }

    /// Opens workspace `id` as the last tab and makes it active. A
    /// workspace already open is not opened again; it is made active
    /// (UI-27). Returns whether it was opened now. Reads its file: call it
    /// off the async workers (ARCH-7).
    pub fn open(&self, id: &WorkspaceId) -> Result<bool, WorkspaceError> {
        let mut tabs = self.lock();
        if tabs.is_open(id) {
            tabs.active = Some(id.clone());
            self.save_session(&tabs);
            return Ok(false);
        }
        let path = self.existing_file(id)?;
        tabs.open.push(load_file(id.clone(), path));
        tabs.active = Some(id.clone());
        self.save_session(&tabs);
        Ok(true)
    }

    /// Closes the tab of workspace `id`. Its file and repos stay as they
    /// are (UI-28). When it was active, the tab that takes its place
    /// becomes active, or the one before it when it was the last.
    pub fn close(&self, id: &WorkspaceId) -> Result<(), WorkspaceError> {
        let mut tabs = self.lock();
        let index = tabs
            .open
            .iter()
            .position(|workspace| workspace.id == *id)
            .ok_or_else(|| WorkspaceError::UnknownWorkspace(id.0.clone()))?;
        tabs.open.remove(index);
        if tabs.active.as_ref() == Some(id) {
            let next = tabs.open.get(index).or_else(|| tabs.open.last());
            tabs.active = next.map(|workspace| workspace.id.clone());
        }
        self.save_session(&tabs);
        Ok(())
    }

    /// Renames workspace `id`, open or not, with the name rules of
    /// [`Self::create`]. A change of case only is allowed.
    pub fn rename(&self, id: &WorkspaceId, name: &str) -> Result<(), WorkspaceError> {
        let name = check_name(name)?;
        let mut tabs = self.lock();
        check_unique(&list(self.dir.as_deref(), &tabs)?, &name, Some(id))?;
        if let Some(workspace) = tabs.open.iter_mut().find(|workspace| workspace.id == *id) {
            return workspace.update(|file| {
                file.name = name;
                Ok(())
            });
        }
        let mut closed = load_file(id.clone(), self.existing_file(id)?);
        closed.update(|file| {
            file.name = name;
            Ok(())
        })
    }

    /// Deletes the file of workspace `id`, and nothing else: the repos stay
    /// on disk (GRP-3). Refused while it is open, and without `confirmed`.
    pub fn delete(&self, id: &WorkspaceId, confirmed: bool) -> Result<(), WorkspaceError> {
        if !confirmed {
            return Err(WorkspaceError::NotConfirmed);
        }
        let tabs = self.lock();
        if tabs.is_open(id) {
            return Err(WorkspaceError::IsOpen);
        }
        let path = self.existing_file(id)?;
        fs::remove_file(&path).map_err(|source| WorkspaceError::Delete { path, source })
    }

    /// Puts the tabs in the order of `ids`, which names each open
    /// workspace once.
    pub fn reorder(&self, ids: &[WorkspaceId]) -> Result<(), WorkspaceError> {
        let mut tabs = self.lock();
        let mut sorted = ids.to_vec();
        sorted.sort_by(|a, b| a.0.cmp(&b.0));
        sorted.dedup();
        let mut open = tabs.ids();
        open.sort_by(|a, b| a.0.cmp(&b.0));
        if sorted.len() != ids.len() || sorted != open {
            return Err(WorkspaceError::BadOrder);
        }
        let mut workspaces = std::mem::take(&mut tabs.open);
        for id in ids {
            if let Some(index) = workspaces.iter().position(|workspace| workspace.id == *id) {
                tabs.open.push(workspaces.swap_remove(index));
            }
        }
        self.save_session(&tabs);
        Ok(())
    }

    /// Makes the open workspace `id` the active tab.
    pub fn activate(&self, id: &WorkspaceId) -> Result<(), WorkspaceError> {
        let mut tabs = self.lock();
        if !tabs.is_open(id) {
            return Err(WorkspaceError::UnknownWorkspace(id.0.clone()));
        }
        tabs.active = Some(id.clone());
        self.save_session(&tabs);
        Ok(())
    }

    /// The file of workspace `id`, which must exist.
    fn existing_file(&self, id: &WorkspaceId) -> Result<PathBuf, WorkspaceError> {
        let missing = || WorkspaceError::NoSuchWorkspace(id.0.clone());
        let dir = self.dir.as_deref().ok_or_else(missing)?;
        if !valid_id(&id.0) {
            return Err(missing());
        }
        let path = file_path(dir, id);
        if path.is_file() {
            Ok(path)
        } else {
            Err(missing())
        }
    }

    /// Keeps the open tabs in the settings for the next start (UI-33). A
    /// failure is not the change's: the settings banner already says when
    /// they are read-only, and a failed write leaves the previous session,
    /// which at worst restores older tabs.
    fn save_session(&self, tabs: &Tabs) {
        let _ = self.settings.set_session(Session {
            tabs: tabs.ids(),
            active: tabs.active.clone(),
        });
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
        let mut tabs = self.lock();
        let workspace = tabs
            .open
            .iter_mut()
            .find(|workspace| workspace.id == *id)
            .ok_or_else(|| WorkspaceError::UnknownWorkspace(id.0.clone()))?;
        action(workspace)
    }

    fn lock(&self) -> MutexGuard<'_, Tabs> {
        // A panic while holding the lock cannot leave a workspace
        // half-changed: its file is replaced only after a successful save.
        self.tabs.lock().unwrap_or_else(PoisonError::into_inner)
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

/// Whether `id` can name a workspace file: letters, digits, `-` and `_`,
/// as in the uuids the app gives. Anything else, such as a path, is never
/// joined to the workspaces folder.
fn valid_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 64
        && id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
}

fn file_path(dir: &Path, id: &WorkspaceId) -> PathBuf {
    dir.join(format!("{}.toml", id.0))
}

/// `name` trimmed, when it is 1 to [`MAX_WORKSPACE_NAME`] characters long.
fn check_name(name: &str) -> Result<String, WorkspaceError> {
    let name = name.trim();
    if name.is_empty() {
        return Err(WorkspaceError::BlankName);
    }
    if name.chars().count() > MAX_WORKSPACE_NAME {
        return Err(WorkspaceError::NameTooLong);
    }
    Ok(name.to_owned())
}

/// Refuses `name` when a workspace other than `except` has it, ignoring
/// case.
fn check_unique(
    items: &[WorkspaceListItem],
    name: &str,
    except: Option<&WorkspaceId>,
) -> Result<(), WorkspaceError> {
    let lower = name.to_lowercase();
    match items
        .iter()
        .find(|item| Some(&item.id) != except && item.name.to_lowercase() == lower)
    {
        Some(item) => Err(WorkspaceError::NameTaken(item.name.clone())),
        None => Ok(()),
    }
}

/// Every workspace file in `dir` and every open workspace, sorted by name,
/// ignoring case, then by id.
fn list(dir: Option<&Path>, tabs: &Tabs) -> Result<Vec<WorkspaceListItem>, WorkspaceError> {
    let mut items: Vec<WorkspaceListItem> = tabs
        .open
        .iter()
        .map(|workspace| WorkspaceListItem {
            id: workspace.id.clone(),
            name: workspace.file.name.clone(),
            open: true,
            warning: workspace.warning.clone(),
        })
        .collect();
    if let Some(dir) = dir {
        for (id, path) in workspace_files(dir).map_err(|source| WorkspaceError::Read {
            path: dir.to_owned(),
            source,
        })? {
            if tabs.is_open(&id) {
                continue;
            }
            let loaded = config_file::load(&path, MIGRATIONS, decode);
            // An unreadable file has no name; its id stands in.
            let name = match loaded.warning {
                Some(ConfigWarning::Unreadable { .. }) => id.0.clone(),
                _ => loaded.value.name,
            };
            items.push(WorkspaceListItem {
                id,
                name,
                open: false,
                warning: loaded.warning,
            });
        }
    }
    items.sort_by(|a, b| (a.name.to_lowercase(), &a.id.0).cmp(&(b.name.to_lowercase(), &b.id.0)));
    Ok(items)
}

/// The workspaces of `session` whose file exists, in its order, and the
/// files that are gone.
fn restore(dir: &Path, session: &Session) -> Tabs {
    let mut tabs = Tabs::default();
    for id in &session.tabs {
        if tabs.is_open(id) {
            continue;
        }
        let path = file_path(dir, id);
        if valid_id(&id.0) && path.is_file() {
            tabs.open.push(load_file(id.clone(), path));
        } else {
            tabs.missing.push(path.display().to_string());
        }
    }
    tabs.active = session
        .active
        .clone()
        .filter(|active| tabs.is_open(active))
        .or_else(|| tabs.open.first().map(|workspace| workspace.id.clone()));
    tabs
}

/// Workspace `id` from its file at `path`, read-only when the file cannot
/// be used (DATA-1).
fn load_file(id: WorkspaceId, path: PathBuf) -> Workspace {
    let loaded = config_file::load(&path, MIGRATIONS, decode);
    Workspace {
        id,
        path: Some(path),
        file: loaded.value,
        warning: loaded.warning,
    }
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
        Ok(Some((id, path))) => load_file(id, path),
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

/// The id and path of every workspace file in `dir`: a `*.toml` file whose
/// name is a valid id. Empty when the folder is missing.
fn workspace_files(dir: &Path) -> io::Result<Vec<(WorkspaceId, PathBuf)>> {
    let entries = match fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(error),
    };
    let mut files = Vec::new();
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
        if !valid_id(id) || !entry.metadata()?.is_file() {
            continue;
        }
        files.push((WorkspaceId(id.to_owned()), path));
    }
    Ok(files)
}

/// The id and path of the most recently modified workspace file in `dir`;
/// `None` when the folder is missing or has none.
fn latest_file(dir: &Path) -> io::Result<Option<(WorkspaceId, PathBuf)>> {
    let mut latest: Option<(SystemTime, WorkspaceId, PathBuf)> = None;
    for (id, path) in workspace_files(dir)? {
        let modified = fs::metadata(&path)?.modified()?;
        let newer = latest
            .as_ref()
            .is_none_or(|(time, latest_id, _)| (modified, &id.0) > (*time, &latest_id.0));
        if newer {
            latest = Some((modified, id, path));
        }
    }
    Ok(latest.map(|(_, id, path)| (id, path)))
}

/// A new "My workspace", saved at once so its id stays the same across
/// starts.
fn create(dir: &Path) -> Workspace {
    let id = new_id();
    let path = file_path(dir, &id);
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
