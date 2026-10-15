//! The tree of one workspace (UI-1) and its filter (UI-8).
//!
//! The tree crosses IPC in two parts so that nothing unbounded is sent at
//! once (ARCH-8): [`Trees::tree`] is the skeleton, every repo with its
//! **Local**, **Remotes**, **Tags** and **Stashes** nodes not yet loaded,
//! and [`tree_refs`] loads one repo's ref nodes when one of them is first
//! expanded. Only then are ahead and behind counted for its branches.
//!
//! Until groups exist (M3) every repo sits under **Ungrouped**. Branch names
//! containing `/` fold into folders; within a node folders come first, then
//! refs, each sorted by name.
//!
//! Loaded refs are kept per workspace and repo (ARCH-9) until the repo's
//! summary is read again ([`Trees::invalidate`]).

use std::collections::{BTreeMap, HashMap};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use gitcroc_git::{GitRepo, OpenError, Refs, RefsError, RefsRead};
use serde::{Deserialize, Serialize};

use crate::{RepoId, WorkspaceError, WorkspaceId, Workspaces};

/// The id of the **Ungrouped** node.
pub const UNGROUPED_ID: &str = "ungrouped";

/// One node of the tree. Ids are unique within a workspace and stable
/// across reads, so the UI can keep expanded and selected nodes by id.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
pub struct TreeNode {
    pub id: String,
    /// What the row shows: a repo name, a ref name or one folder of one.
    pub label: String,
    pub item: TreeItem,
    /// `None` while not loaded: a repo's ref nodes before [`tree_refs`].
    /// Leaves have `Some` and no children.
    pub children: Option<Vec<TreeNode>>,
}

/// What a node stands for.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum TreeItem {
    Ungrouped,
    Repo {
        repo_id: RepoId,
    },
    Local {
        repo_id: RepoId,
    },
    Remotes {
        repo_id: RepoId,
    },
    Tags {
        repo_id: RepoId,
    },
    Stashes {
        repo_id: RepoId,
    },
    Remote {
        repo_id: RepoId,
        remote: String,
    },
    /// One `/`-separated part of branch names.
    Folder {
        repo_id: RepoId,
    },
    Branch {
        repo_id: RepoId,
        /// The full short name, for example `feature/x`.
        name: String,
        /// `HEAD` points to it.
        current: bool,
        /// The upstream, for example `origin/main`.
        upstream: Option<String>,
        /// The upstream is configured but its ref does not exist.
        gone: bool,
        /// Commits ahead of and behind the upstream; `None` when not
        /// counted or there is nothing to count against.
        ahead: Option<u32>,
        behind: Option<u32>,
    },
    RemoteBranch {
        repo_id: RepoId,
        /// With the remote, for example `origin/feature/x`.
        name: String,
        /// The remote, which may contain `/` itself, for example `origin`.
        remote: String,
        /// Without the remote, for example `feature/x`.
        branch: String,
    },
    Tag {
        repo_id: RepoId,
        name: String,
        annotated: bool,
    },
    Stash {
        repo_id: RepoId,
        /// `n` in `stash@{n}`.
        index: u32,
        /// The stash commit's full hash: what a stash operation names it by.
        hash: String,
        message: String,
        /// Seconds since the Unix epoch. A plain `number` in TypeScript,
        /// which `i64` is not by default.
        #[specta(type = i32)]
        seconds: i64,
    },
}

/// Why a repo's refs could not be loaded.
#[derive(Debug, thiserror::Error)]
pub enum TreeError {
    #[error(transparent)]
    Workspace(#[from] WorkspaceError),
    #[error(transparent)]
    Open(#[from] OpenError),
    #[error(transparent)]
    Refs(#[from] RefsError),
    #[error("the read stopped: {0}")]
    Stopped(#[from] tokio::task::JoinError),
}

/// A repo's refs as last read.
#[derive(Debug, Clone)]
struct Loaded {
    refs: Refs,
    /// Ahead and behind were counted.
    counts: bool,
}

/// The loaded refs and filter requests of every open workspace.
#[derive(Debug, Default)]
pub struct Trees {
    refs: Mutex<HashMap<WorkspaceId, HashMap<RepoId, Loaded>>>,
    /// The latest filter request of each workspace.
    filters: Mutex<HashMap<WorkspaceId, u64>>,
}

impl Trees {
    pub fn new() -> Self {
        Self::default()
    }

    /// The skeleton of workspace `id`: **Ungrouped** with every repo in
    /// workspace order, each with its four ref nodes not loaded.
    pub fn tree(
        &self,
        workspaces: &Workspaces,
        id: &WorkspaceId,
    ) -> Result<Vec<TreeNode>, WorkspaceError> {
        let view = workspaces.view(id)?;
        let repos = view
            .repos
            .into_iter()
            .map(|repo| repo_node(&repo.id, repo.name))
            .collect();
        Ok(vec![ungrouped(repos)])
    }

    /// Drops everything kept for workspace `id`, whose tab closed (ARCH-9).
    pub fn forget(&self, id: &WorkspaceId) {
        lock(&self.refs).remove(id);
        lock(&self.filters).remove(id);
    }

    /// Forgets the loaded refs of `repo`, for example after its summary was
    /// read again. Returns whether any were loaded.
    pub fn invalidate(&self, id: &WorkspaceId, repo: &RepoId) -> bool {
        lock(&self.refs)
            .get_mut(id)
            .is_some_and(|repos| repos.remove(repo).is_some())
    }

    fn loaded(&self, id: &WorkspaceId, repo: &RepoId, counts: bool) -> Option<Refs> {
        lock(&self.refs)
            .get(id)
            .and_then(|repos| repos.get(repo))
            .filter(|loaded| loaded.counts || !counts)
            .map(|loaded| loaded.refs.clone())
    }

    /// Reads the refs of `repo`, or takes the loaded ones.
    fn read(
        &self,
        workspaces: &Workspaces,
        id: &WorkspaceId,
        repo: &RepoId,
        counts: bool,
    ) -> Result<Refs, TreeError> {
        if let Some(refs) = self.loaded(id, repo, counts) {
            return Ok(refs);
        }
        let path = workspaces.repo_path(id, repo)?;
        let refs = GitRepo::open(&path)?.refs(counts)?;
        lock(&self.refs).entry(id.clone()).or_default().insert(
            repo.clone(),
            Loaded {
                refs: refs.clone(),
                counts,
            },
        );
        Ok(refs)
    }

    /// Makes a new filter request the latest of workspace `id`.
    fn begin_filter(&self, id: &WorkspaceId) -> u64 {
        let mut filters = lock(&self.filters);
        let latest = filters.entry(id.clone()).or_default();
        *latest = latest.wrapping_add(1);
        *latest
    }

    fn is_latest(&self, id: &WorkspaceId, request: u64) -> bool {
        lock(&self.filters).get(id) == Some(&request)
    }

    /// The filtered tree, or `None` when a newer request came in.
    fn filter(
        &self,
        workspaces: &Workspaces,
        id: &WorkspaceId,
        text: &str,
        request: u64,
    ) -> Result<Option<Vec<TreeNode>>, WorkspaceError> {
        let text = text.to_lowercase();
        if text.is_empty() {
            return self.tree(workspaces, id).map(Some);
        }
        let view = workspaces.view(id)?;
        let mut repos = Vec::new();
        for repo in view.repos {
            if !self.is_latest(id, request) {
                return Ok(None);
            }
            let named = repo.name.to_lowercase().contains(&text);
            // A repo whose refs cannot be read is matched by its name only.
            let refs = self.read(workspaces, id, &repo.id, false).ok();
            let kept: Vec<TreeNode> = refs
                .map(|refs| ref_nodes(&repo.id, &refs))
                .unwrap_or_default()
                .into_iter()
                .filter_map(|node| prune(node, &text))
                .collect();
            if !kept.is_empty() {
                let mut node = repo_node(&repo.id, repo.name);
                node.children = Some(kept);
                repos.push(node);
            } else if named {
                repos.push(repo_node(&repo.id, repo.name));
            }
        }
        Ok(self.is_latest(id, request).then(|| vec![ungrouped(repos)]))
    }
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    // Every change is a single insert or removal, so a panic leaves no
    // half-changed state.
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

/// The four ref nodes of `repo`, loaded, with ahead and behind counted.
/// Reads the repo off the async workers (ARCH-7).
pub async fn tree_refs(
    trees: Arc<Trees>,
    workspaces: Arc<Workspaces>,
    id: WorkspaceId,
    repo: RepoId,
) -> Result<Vec<TreeNode>, TreeError> {
    tokio::task::spawn_blocking(move || {
        let refs = trees.read(&workspaces, &id, &repo, true)?;
        Ok(ref_nodes(&repo, &refs))
    })
    .await?
}

/// The tree of workspace `id` filtered by `text` (UI-8): nodes whose repo
/// name or ref name contains it, ignoring case, with their ancestors. A
/// repo matched by name alone keeps its ref nodes, not loaded. Empty text
/// gives the whole skeleton. Refs not loaded yet are read without ahead
/// and behind.
///
/// Each call supersedes the previous one for the same workspace: an older
/// call still running returns `None` (`Ok(None)`), so results never arrive
/// out of order.
pub async fn tree_filter(
    trees: Arc<Trees>,
    workspaces: Arc<Workspaces>,
    id: WorkspaceId,
    text: String,
) -> Result<Option<Vec<TreeNode>>, WorkspaceError> {
    let request = trees.begin_filter(&id);
    tokio::task::spawn_blocking(move || trees.filter(&workspaces, &id, &text, request))
        .await
        .map_err(WorkspaceError::Stopped)?
}

fn ungrouped(repos: Vec<TreeNode>) -> TreeNode {
    TreeNode {
        id: UNGROUPED_ID.to_owned(),
        label: "Ungrouped".to_owned(),
        item: TreeItem::Ungrouped,
        children: Some(repos),
    }
}

fn repo_id(repo: &RepoId) -> String {
    format!("repo:{}", repo.0)
}

/// A repo node with its four ref nodes, not loaded.
fn repo_node(repo: &RepoId, name: String) -> TreeNode {
    let children = categories(repo)
        .into_iter()
        .map(|(id, label, item)| TreeNode {
            id,
            label: label.to_owned(),
            item,
            children: None,
        })
        .collect();
    TreeNode {
        id: repo_id(repo),
        label: name,
        item: TreeItem::Repo {
            repo_id: repo.clone(),
        },
        children: Some(children),
    }
}

/// Id, label and item of the four ref nodes, in tree order.
fn categories(repo: &RepoId) -> [(String, &'static str, TreeItem); 4] {
    let base = repo_id(repo);
    let repo_id = || repo.clone();
    [
        (
            format!("{base}:local"),
            "Local",
            TreeItem::Local { repo_id: repo_id() },
        ),
        (
            format!("{base}:remotes"),
            "Remotes",
            TreeItem::Remotes { repo_id: repo_id() },
        ),
        (
            format!("{base}:tags"),
            "Tags",
            TreeItem::Tags { repo_id: repo_id() },
        ),
        (
            format!("{base}:stashes"),
            "Stashes",
            TreeItem::Stashes { repo_id: repo_id() },
        ),
    ]
}

/// The four ref nodes of `repo`, loaded from `refs`.
fn ref_nodes(repo: &RepoId, refs: &Refs) -> Vec<TreeNode> {
    let [local, remotes, tags, stashes] = categories(repo);
    let node = |(id, label, item): (String, &str, TreeItem), children| TreeNode {
        id,
        label: label.to_owned(),
        item,
        children: Some(children),
    };

    let branches = refs
        .local
        .iter()
        .map(|branch| {
            let upstream = branch.upstream.as_ref();
            let counts = upstream.and_then(|upstream| upstream.counts);
            (
                branch.name.clone(),
                TreeItem::Branch {
                    repo_id: repo.clone(),
                    name: branch.name.clone(),
                    current: branch.current,
                    upstream: upstream.map(|upstream| upstream.name.clone()),
                    gone: upstream.is_some_and(|upstream| upstream.gone),
                    ahead: counts.map(|counts| counts.ahead),
                    behind: counts.map(|counts| counts.behind),
                },
            )
        })
        .collect();
    let local_children = fold(&local.0, repo, branches);

    let remote_children = refs
        .remotes
        .iter()
        .map(|remote| {
            let id = format!("{}:{}", remotes.0, remote.remote);
            let branches = remote
                .branches
                .iter()
                .map(|branch| {
                    (
                        branch.clone(),
                        TreeItem::RemoteBranch {
                            repo_id: repo.clone(),
                            name: format!("{}/{branch}", remote.remote),
                            remote: remote.remote.clone(),
                            branch: branch.clone(),
                        },
                    )
                })
                .collect();
            TreeNode {
                children: Some(fold(&id, repo, branches)),
                id,
                label: remote.remote.clone(),
                item: TreeItem::Remote {
                    repo_id: repo.clone(),
                    remote: remote.remote.clone(),
                },
            }
        })
        .collect();

    let tag_children = refs
        .tags
        .iter()
        .map(|tag| TreeNode {
            id: format!("{}:ref:{}", tags.0, tag.name),
            label: tag.name.clone(),
            item: TreeItem::Tag {
                repo_id: repo.clone(),
                name: tag.name.clone(),
                annotated: tag.annotated,
            },
            children: Some(Vec::new()),
        })
        .collect();

    let stash_children = refs
        .stashes
        .iter()
        .map(|stash| TreeNode {
            id: format!("{}:ref:{}", stashes.0, stash.index),
            label: stash.message.clone(),
            item: TreeItem::Stash {
                repo_id: repo.clone(),
                index: stash.index,
                hash: stash.hash.clone(),
                message: stash.message.clone(),
                seconds: stash.time.seconds,
            },
            children: Some(Vec::new()),
        })
        .collect();

    vec![
        node(local, local_children),
        node(remotes, remote_children),
        node(tags, tag_children),
        node(stashes, stash_children),
    ]
}

/// Folds `/`-separated names into folders below the node `parent`: folders
/// first, then refs, each sorted by name.
fn fold(parent: &str, repo: &RepoId, refs: Vec<(String, TreeItem)>) -> Vec<TreeNode> {
    #[derive(Default)]
    struct Folder {
        folders: BTreeMap<String, Folder>,
        refs: BTreeMap<String, TreeItem>,
    }

    fn build(folder: Folder, parent: &str, path: &str, repo: &RepoId) -> Vec<TreeNode> {
        let mut nodes = Vec::new();
        for (name, sub) in folder.folders {
            let path = format!("{path}{name}/");
            nodes.push(TreeNode {
                id: format!("{parent}:dir:{path}"),
                label: name,
                item: TreeItem::Folder {
                    repo_id: repo.clone(),
                },
                children: Some(build(sub, parent, &path, repo)),
            });
        }
        for (name, item) in folder.refs {
            nodes.push(TreeNode {
                id: format!("{parent}:ref:{path}{name}"),
                label: name,
                item,
                children: Some(Vec::new()),
            });
        }
        nodes
    }

    let mut root = Folder::default();
    for (name, item) in refs {
        let mut parts: Vec<&str> = name.split('/').collect();
        let leaf = parts.pop().unwrap_or_default().to_owned();
        let mut folder = &mut root;
        for part in parts {
            folder = folder.folders.entry(part.to_owned()).or_default();
        }
        folder.refs.insert(leaf, item);
    }
    build(root, parent, "", repo)
}

/// The text a node is matched by: a ref's full name. Other nodes are kept
/// only as ancestors of a match.
fn match_key(item: &TreeItem) -> Option<&str> {
    match item {
        TreeItem::Branch { name, .. }
        | TreeItem::RemoteBranch { name, .. }
        | TreeItem::Tag { name, .. } => Some(name),
        _ => None,
    }
}

/// `node` with only the matching descendants, or `None` when neither it
/// nor any descendant matches. `text` is lower case.
fn prune(node: TreeNode, text: &str) -> Option<TreeNode> {
    if match_key(&node.item).is_some_and(|key| key.to_lowercase().contains(text)) {
        return Some(node);
    }
    let kept: Vec<TreeNode> = node
        .children?
        .into_iter()
        .filter_map(|child| prune(child, text))
        .collect();
    (!kept.is_empty()).then_some(TreeNode {
        children: Some(kept),
        ..node
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use gitcroc_git::{LocalBranch, RemoteBranches};

    struct NoConfigDir;

    impl crate::ConfigDir for NoConfigDir {
        fn config_dir(&self) -> Option<std::path::PathBuf> {
            None
        }
    }

    #[test]
    fn ui_8_a_newer_filter_supersedes_an_older_one() {
        let workspaces = Workspaces::load(&NoConfigDir);
        let id = workspaces.views()[0].id.clone();
        let other = WorkspaceId("other".to_owned());
        let trees = Trees::new();
        let older = trees.begin_filter(&id);
        let elsewhere = trees.begin_filter(&other);
        let newer = trees.begin_filter(&id);
        let filter = |request| trees.filter(&workspaces, &id, "x", request).unwrap();
        assert_eq!(filter(older), None);
        assert!(filter(newer).is_some());
        // ARCH-9: requests of another workspace supersede nothing here.
        assert!(trees.is_latest(&other, elsewhere));
    }

    fn labels(nodes: &[TreeNode]) -> Vec<&str> {
        nodes.iter().map(|node| node.label.as_str()).collect()
    }

    #[test]
    fn ui_1_folders_come_before_refs_and_sort_by_name() {
        let repo = RepoId("r1".to_owned());
        let branch = |name: &str| LocalBranch {
            name: name.to_owned(),
            current: name == "main",
            upstream: None,
        };
        let refs = Refs {
            local: ["zeta", "feature/a/b", "main", "bugfix/x", "feature/c"]
                .map(branch)
                .to_vec(),
            remotes: vec![RemoteBranches {
                remote: "origin".to_owned(),
                branches: vec!["main".to_owned(), "feature/x".to_owned()],
            }],
            ..Refs::default()
        };
        let nodes = ref_nodes(&repo, &refs);
        assert_eq!(labels(&nodes), ["Local", "Remotes", "Tags", "Stashes"]);
        let local = nodes[0].children.as_deref().unwrap_or_default();
        assert_eq!(labels(local), ["bugfix", "feature", "main", "zeta"]);
        let feature = local[1].children.as_deref().unwrap_or_default();
        assert_eq!(labels(feature), ["a", "c"]);
        assert_eq!(feature[0].id, "repo:r1:local:dir:feature/a/");
        let b = &feature[0].children.as_deref().unwrap_or_default()[0];
        assert_eq!(b.id, "repo:r1:local:ref:feature/a/b");
        let remotes = nodes[1].children.as_deref().unwrap_or_default();
        let origin = remotes[0].children.as_deref().unwrap_or_default();
        assert_eq!(labels(origin), ["feature", "main"]);
    }
}
