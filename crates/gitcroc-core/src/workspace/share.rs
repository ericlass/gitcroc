//! Sharing a workspace (DATA-2): export writes the section 10 format with
//! each repo path relative to the exported file's folder; import reads such
//! a file into a new workspace, resolving the paths against its folder. A
//! repo whose folder is missing is kept, with its id and groups, so it can
//! be cloned from `remote` into place later (`crate::clone`). The pane
//! layout, the AI switch (AI-6) and the offer answers are the user's own
//! and are not exported.
//! Everything in an imported file is untrusted (SEC-3): it is checked as
//! the workspace's own rules check a change, and refused as a whole when
//! it breaks one.

use std::fs;
use std::io;
use std::path::{Component, Path, PathBuf};

use gitcroc_git::{GitRunner, Layout, layout};
use serde::{Deserialize, Serialize};

use super::add::{Known, check};
use super::group::check_group_name;
use super::{
    Group, MAX_WORKSPACE_NAME, MIGRATIONS, RepoEntry, RepoId, RepoView, WorkspaceError,
    WorkspaceFile, WorkspaceListItem, Workspaces, check_name, decode, entry, file_path,
    folder_name, list, load_file, new_id, save,
};
use crate::WorkspaceId;
use crate::config_file::{self, ConfigWarning};

/// What an export wrote. Paths are text for display only (PLAT-2).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
pub struct ExportReport {
    /// The file written.
    pub path: String,
    /// The repo paths no relative path reaches, such as on another drive,
    /// written absolute.
    pub absolute: Vec<String>,
}

/// What an import created.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
pub struct ImportReport {
    /// The new workspace, opened as the active tab.
    pub workspace_id: WorkspaceId,
    /// The repos whose folder is missing or is no repo, in file order.
    pub missing: Vec<RepoId>,
}

/// A repo of the workspace whose folder is missing or is no repo, and
/// where a clone of it goes by default (DATA-2).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
pub struct MissingRepo {
    pub repo_id: RepoId,
    pub name: String,
    /// The URL to clone from; `None` when the file named none.
    pub remote: Option<String>,
    /// The folder the repo's path names, and its last component.
    pub parent: PathBuf,
    pub folder: String,
}

impl Workspaces {
    /// Writes workspace `id` to `target` in the section 10 format,
    /// atomically (DATA-1), with every repo path relative to the folder of
    /// `target` and `/` between components. A path no relative path
    /// reaches stays absolute and is reported. A workspace that is not open
    /// is read from its file. Reads the disk: call it off the async workers
    /// (ARCH-7).
    pub fn export(&self, id: &WorkspaceId, target: &Path) -> Result<ExportReport, WorkspaceError> {
        let file = match self.with(id, |workspace| Ok(workspace.file.clone())) {
            Err(WorkspaceError::UnknownWorkspace(_)) => {
                load_file(id.clone(), self.existing_file(id)?).file
            }
            file => file?,
        };
        let failed = |source| WorkspaceError::Write {
            path: target.to_owned(),
            source,
        };
        let name = target
            .file_name()
            .ok_or_else(|| failed(io::Error::other("the path names no file")))?;
        let folder = target
            .parent()
            .filter(|folder| !folder.as_os_str().is_empty())
            .unwrap_or(Path::new("."));
        let base = dunce::canonicalize(folder).map_err(failed)?;
        let mut absolute = Vec::new();
        let repos = file
            .repos
            .into_iter()
            .map(|repo| {
                let path = relative(&base, &repo.path).map_or_else(
                    || {
                        absolute.push(repo.path.display().to_string());
                        repo.path.clone()
                    },
                    PathBuf::from,
                );
                RepoEntry {
                    id: repo.id,
                    path,
                    remote: repo.remote,
                    commit_graph: None,
                    fsmonitor: None,
                }
            })
            .collect();
        let shared = WorkspaceFile {
            name: file.name,
            ai: None,
            repos,
            groups: file.groups,
            layout: None,
        };
        let target = base.join(name);
        save(&target, &shared)?;
        Ok(ExportReport {
            path: target.display().to_string(),
            absolute,
        })
    }

    /// Reads the workspace file at `source` with the DATA-1 rules into a
    /// new workspace, opened as the last tab and made active. Its name is
    /// the file's, with " (2)" and so on when that is taken. Repos and
    /// groups keep their ids, order and members. Relative paths resolve
    /// against the file's folder. A repo found there is added as a folder
    /// is (GRP-1); one whose folder is missing or is no repo keeps its
    /// path, resolved and normalized, and is reported missing. A file from
    /// a newer gitcroc, or one that breaks a rule of the workspace, such as
    /// a repo id or folder listed twice or a group name GRP-2 refuses, is
    /// refused and nothing is created. Reads the disk and runs git: call
    /// it off the async workers (ARCH-7).
    pub fn import(&self, source: &Path, git: &GitRunner) -> Result<ImportReport, WorkspaceError> {
        let dir = self.dir.clone().ok_or(WorkspaceError::ReadOnly)?;
        let refused = |reason: String| WorkspaceError::NotImported {
            path: source.to_owned(),
            reason,
        };
        let unread = |error| WorkspaceError::Read {
            path: source.to_owned(),
            source: error,
        };
        let text = fs::read_to_string(source).map_err(unread)?;
        let loaded = config_file::parse(source, &text, MIGRATIONS, decode);
        if let Some(warning) = loaded.warning {
            return Err(refused(warning_reason(&warning)));
        }
        let file = loaded.value;
        let name = check_name(&file.name).map_err(|error| refused(error.to_string()))?;
        let folder = source
            .parent()
            .filter(|folder| !folder.as_os_str().is_empty())
            .unwrap_or(Path::new("."));
        let base = dunce::canonicalize(folder).map_err(unread)?;

        let mut repos: Vec<RepoEntry> = Vec::with_capacity(file.repos.len());
        let mut known: Vec<Known> = Vec::with_capacity(file.repos.len());
        let mut missing = Vec::new();
        for repo in file.repos {
            if repo.id.0.is_empty() {
                return Err(refused("a repo has an empty id".to_owned()));
            }
            if repos.iter().any(|other| other.id == repo.id) {
                return Err(refused(format!("the repo id {} is listed twice", repo.id)));
            }
            let resolved = normalize(&base.join(&repo.path));
            let mut remote = repo.remote;
            let path = if layout(&resolved) == Layout::Plain {
                missing.push(repo.id.clone());
                resolved
            } else {
                // A repo git refuses is kept where it is; its summary
                // shows git's reason.
                match check(&resolved, git) {
                    Ok(found) => {
                        remote = remote.or(found.remote);
                        found.path
                    }
                    Err(_) => resolved,
                }
            };
            let key = Known::new(&path);
            if let Some(index) = known.iter().position(|other| other.matches(&key)) {
                let other = repos.get(index).map(|other| other.id.0.as_str());
                return Err(refused(format!(
                    "the repos {} and {} have the same folder {}",
                    other.unwrap_or_default(),
                    repo.id,
                    path.display()
                )));
            }
            known.push(key);
            repos.push(RepoEntry {
                id: repo.id,
                path,
                remote,
                commit_graph: None,
                fsmonitor: None,
            });
        }

        let mut groups: Vec<Group> = Vec::with_capacity(file.groups.len());
        for group in file.groups {
            let name = check_group_name(&groups, &group.name, None)
                .map_err(|error| refused(error.to_string()))?;
            if let Some(unknown) = group
                .repos
                .iter()
                .find(|member| !repos.iter().any(|repo| repo.id == **member))
            {
                return Err(refused(format!(
                    "the group \"{name}\" names the repo id {unknown}, which the file does not list"
                )));
            }
            let mut members: Vec<RepoId> = Vec::with_capacity(group.repos.len());
            for member in group.repos {
                if !members.contains(&member) {
                    members.push(member);
                }
            }
            groups.push(Group {
                name,
                repos: members,
            });
        }

        let mut tabs = self.lock();
        let name = free_name(&list(Some(&dir), &tabs)?, &name);
        let id = new_id();
        let path = file_path(&dir, &id);
        let file = WorkspaceFile {
            name,
            ai: None,
            repos,
            groups,
            layout: None,
        };
        save(&path, &file)?;
        tabs.open
            .push(super::Workspace::new(id.clone(), Some(path), file, None));
        tabs.active = Some(id.clone());
        self.save_session(&tabs);
        Ok(ImportReport {
            workspace_id: id,
            missing,
        })
    }

    /// The repos of workspace `id` whose folder is missing or is no repo,
    /// in workspace order (DATA-2). Reads the disk: call it off the async
    /// workers (ARCH-7).
    pub fn missing_repos(&self, id: &WorkspaceId) -> Result<Vec<MissingRepo>, WorkspaceError> {
        let repos = self.with(id, |workspace| Ok(workspace.file.repos.clone()))?;
        Ok(repos
            .into_iter()
            .filter(|repo| layout(&repo.path) == Layout::Plain)
            .map(|repo| MissingRepo {
                name: folder_name(&repo.path),
                parent: repo.path.parent().map(Path::to_owned).unwrap_or_default(),
                folder: folder_name(&repo.path),
                repo_id: repo.id,
                remote: repo.remote,
            })
            .collect())
    }

    /// Whether the folder of repo `repo` is missing or is no repo.
    pub(crate) fn is_missing(
        &self,
        id: &WorkspaceId,
        repo: &RepoId,
    ) -> Result<bool, WorkspaceError> {
        let path = self.repo_path(id, repo)?;
        Ok(layout(&path) == Layout::Plain)
    }

    /// Points repo `repo` at the clone in `path`, so it keeps its id and
    /// groups (DATA-2). The folder is checked as an added one is, and is
    /// refused when another repo of the workspace has it (GRP-1). Reads the
    /// disk and runs git: call it off the async workers (ARCH-7).
    pub(crate) fn fill_repo(
        &self,
        id: &WorkspaceId,
        repo: &RepoId,
        path: &Path,
        git: &GitRunner,
    ) -> Result<RepoView, String> {
        let found = check(path, git).map_err(|refusal| refusal.reason)?;
        let key = Known::new(&found.path);
        self.with(id, |workspace| {
            entry(&workspace.file, repo)?;
            let other = workspace
                .file
                .repos
                .iter()
                .find(|other| other.id != *repo && key.matches(&Known::new(&other.path)));
            if let Some(other) = other {
                return Err(WorkspaceError::AlreadyPresent(other.id.clone()));
            }
            workspace.update(|file| {
                let filled = file
                    .repos
                    .iter_mut()
                    .find(|other| other.id == *repo)
                    .ok_or_else(|| WorkspaceError::UnknownRepo(repo.clone()))?;
                filled.path = found.path;
                filled.remote = filled.remote.take().or(found.remote);
                Ok(filled.view())
            })
        })
        .map_err(|error| error.to_string())
    }
}

/// Why a file with `warning` is not imported (DATA-1).
fn warning_reason(warning: &ConfigWarning) -> String {
    match warning {
        ConfigWarning::NewerSchema {
            found, supported, ..
        } => format!(
            "it is from a newer gitcroc (schema {found}; this version reads schema {supported})"
        ),
        ConfigWarning::Unreadable { reason, .. } => reason.clone(),
        ConfigWarning::NoConfigDir => "there is no config directory".to_owned(),
    }
}

/// `name`, or with " (2)", " (3)" and so on when a workspace has it,
/// ignoring case; shortened to fit [`MAX_WORKSPACE_NAME`].
fn free_name(items: &[WorkspaceListItem], name: &str) -> String {
    let taken = |candidate: &str| {
        let lower = candidate.to_lowercase();
        items.iter().any(|item| item.name.to_lowercase() == lower)
    };
    if !taken(name) {
        return name.to_owned();
    }
    // One of these is free: there are fewer names than numbers.
    (2..items.len().saturating_add(3))
        .map(|number| {
            let suffix = format!(" ({number})");
            let room = MAX_WORKSPACE_NAME.saturating_sub(suffix.chars().count());
            let base: String = name.chars().take(room).collect();
            format!("{}{suffix}", base.trim_end())
        })
        .find(|candidate| !taken(candidate))
        .unwrap_or_else(|| name.to_owned())
}

/// `path` with `.` dropped and each `..` taking off the component before
/// it, without reading the disk: a missing folder cannot be canonicalized.
fn normalize(path: &Path) -> PathBuf {
    let mut normal = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                if matches!(normal.components().next_back(), Some(Component::Normal(_))) {
                    normal.pop();
                } else if !normal.has_root() {
                    normal.push(component);
                }
            }
            other => normal.push(other),
        }
    }
    normal
}

/// Whether two components name the same folder: ignoring case on Windows,
/// whose file systems do.
fn same(a: &Component<'_>, b: &Component<'_>) -> bool {
    if cfg!(windows) {
        a.as_os_str().to_string_lossy().to_lowercase()
            == b.as_os_str().to_string_lossy().to_lowercase()
    } else {
        a == b
    }
}

/// `path` relative to the folder `base`, both absolute, with `/` between
/// components and `..` where needed; `None` when no relative path reaches
/// it, as on another drive, or it is not text.
fn relative(base: &Path, path: &Path) -> Option<String> {
    if !base.is_absolute() || !path.is_absolute() {
        return None;
    }
    let base: Vec<Component<'_>> = base.components().collect();
    let path: Vec<Component<'_>> = path.components().collect();
    // The prefix and root are the drive or share: they must match.
    let roots = |components: &[Component<'_>]| {
        components
            .iter()
            .take_while(|component| matches!(component, Component::Prefix(_) | Component::RootDir))
            .count()
    };
    let (base_root, path_root) = (roots(&base), roots(&path));
    if base_root != path_root
        || !base
            .iter()
            .zip(&path)
            .take(base_root)
            .all(|(a, b)| same(a, b))
    {
        return None;
    }
    let common = base
        .iter()
        .zip(&path)
        .take_while(|(a, b)| same(a, b))
        .count();
    let mut parts: Vec<&str> = vec![".."; base.len() - common];
    for component in &path[common..] {
        match component {
            Component::Normal(part) => parts.push(part.to_str()?),
            _ => return None,
        }
    }
    if parts.is_empty() {
        Some(".".to_owned())
    } else {
        Some(parts.join("/"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(unix)]
    #[test]
    fn data_2_relative_paths_reach_nested_sibling_and_parent_folders() {
        let base = Path::new("/home/eric/share");
        let rel = |path: &str| relative(base, Path::new(path));
        assert_eq!(rel("/home/eric/share/api").as_deref(), Some("api"));
        assert_eq!(
            rel("/home/eric/share/back/api").as_deref(),
            Some("back/api")
        );
        assert_eq!(rel("/home/eric/web").as_deref(), Some("../web"));
        assert_eq!(rel("/srv/repo").as_deref(), Some("../../../srv/repo"));
        assert_eq!(rel("/home/eric/share").as_deref(), Some("."));
        assert_eq!(rel("relative"), None);
    }

    #[cfg(windows)]
    #[test]
    fn data_2_relative_paths_on_windows() {
        let base = Path::new(r"C:\Users\eric\share");
        let rel = |path: &str| relative(base, Path::new(path));
        assert_eq!(
            rel(r"C:\Users\eric\share\back\api").as_deref(),
            Some("back/api")
        );
        assert_eq!(rel(r"c:\users\eric\web").as_deref(), Some("../web"));
        // Another drive: no relative path reaches it.
        assert_eq!(rel(r"D:\repos\api"), None);
        assert_eq!(rel(r"\\server\share\api"), None);
    }

    #[test]
    fn data_2_normalize_drops_dot_segments_without_the_disk() {
        let root = if cfg!(windows) { r"C:\" } else { "/" };
        let path = Path::new(root)
            .join("a")
            .join(".")
            .join("b")
            .join("..")
            .join("c");
        assert_eq!(normalize(&path), Path::new(root).join("a").join("c"));
        let up = Path::new(root).join("..").join("a");
        assert_eq!(normalize(&up), Path::new(root).join("a"));
    }

    #[test]
    fn data_2_a_taken_name_gets_the_next_number() {
        let item = |name: &str| WorkspaceListItem {
            id: WorkspaceId(name.to_owned()),
            name: name.to_owned(),
            open: false,
            warning: None,
        };
        let items = [item("Platform"), item("platform (2)")];
        assert_eq!(free_name(&items, "Web"), "Web");
        assert_eq!(free_name(&items, "PLATFORM"), "PLATFORM (3)");
        let long = "x".repeat(MAX_WORKSPACE_NAME);
        let named = free_name(&[item(&long)], &long);
        assert_eq!(named.chars().count(), MAX_WORKSPACE_NAME);
        assert!(named.ends_with(" (2)"));
    }
}
