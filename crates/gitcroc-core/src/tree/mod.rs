//! The tree of one workspace (UI-1) and its filter (UI-8).
//!
//! The tree crosses IPC in parts so that nothing unbounded is sent at once
//! (ARCH-8): [`Trees::tree`] is the skeleton, and [`tree_refs`] loads one
//! repo's ref nodes when one of them is first expanded, counting ahead and
//! behind only then. [`group_branches`] loads a group's **Branches** node
//! the same way.
//!
//! The skeleton has one node per group in the user's order, then
//! **Ungrouped** with the repos in no group. A group holds its **Branches**
//! node, then its members in order; each repo holds its **Local**,
//! **Remotes**, **Tags** and **Stashes** nodes, not loaded. A repo in
//! several groups appears under each (UI-2): its nodes' ids carry the
//! place, `group:<id>:` before the repo's own, so they are unique, and its
//! items carry the place's group. Branch names containing `/` fold into
//! folders; within a node folders come first, then refs, each sorted by
//! name.
//!
//! Loaded refs are kept per workspace and repo (ARCH-9) until the repo's
//! summary is read again ([`Trees::invalidate`]).

use std::collections::{BTreeMap, HashMap, HashSet};
use std::sync::{Arc, Mutex};

use gitcroc_git::{GitRepo, OpenError, Refs, RefsError};
use serde::{Deserialize, Serialize};

use crate::group::{GroupSummary, group_summaries};
use crate::lock;
use crate::{GroupId, RepoId, Summaries, WorkspaceError, WorkspaceId, Workspaces};

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
    /// A group (UI-4), with what its row shows (GRP-4).
    Group {
        group_id: GroupId,
        summary: GroupSummary,
    },
    /// A group's **Branches** node (UI-5); `group_branches` loads it.
    GroupBranches {
        group_id: GroupId,
    },
    /// One `/`-separated part of the branch names of a group's
    /// **Branches** node.
    GroupFolder {
        group_id: GroupId,
    },
    /// A local branch name that exists in `present` of the group's
    /// `members` (UI-5).
    GroupBranch {
        group_id: GroupId,
        /// The full short name, for example `feature/x`.
        name: String,
        present: u32,
        members: u32,
    },
    Ungrouped,
    /// A repo, under the group `group_id` or under **Ungrouped**.
    Repo {
        repo_id: RepoId,
        group_id: Option<GroupId>,
    },
    /// A repo's ref nodes carry the repo's place, for `refs_get`.
    Local {
        repo_id: RepoId,
        group_id: Option<GroupId>,
    },
    Remotes {
        repo_id: RepoId,
        group_id: Option<GroupId>,
    },
    Tags {
        repo_id: RepoId,
        group_id: Option<GroupId>,
    },
    Stashes {
        repo_id: RepoId,
        group_id: Option<GroupId>,
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
    #[error("the workspace has no group with the id {0}")]
    UnknownGroup(GroupId),
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

    /// The skeleton of workspace `id` (UI-1): each group in the user's
    /// order with its **Branches** node and its members, then
    /// **Ungrouped** with the repos in no group, in workspace order. Each
    /// repo has its four ref nodes, not loaded. The group nodes carry what
    /// `summaries` give their rows (GRP-4, UI-4).
    pub fn tree(
        &self,
        workspaces: &Workspaces,
        summaries: &Summaries,
        id: &WorkspaceId,
    ) -> Result<Vec<TreeNode>, WorkspaceError> {
        let layout = Layout::read(workspaces, summaries, id)?;
        let mut nodes: Vec<TreeNode> = layout
            .groups
            .iter()
            .map(|group| {
                let members = group
                    .members
                    .iter()
                    .map(|repo| layout.repo_node(repo, Some(&group.summary.group_id)))
                    .collect();
                group_node(group, members)
            })
            .collect();
        let repos = layout
            .ungrouped()
            .map(|repo| layout.repo_node(repo, None))
            .collect();
        nodes.push(ungrouped(repos));
        Ok(nodes)
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
    pub(crate) fn read(
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
        summaries: &Summaries,
        id: &WorkspaceId,
        text: &str,
        request: u64,
    ) -> Result<Option<Vec<TreeNode>>, WorkspaceError> {
        let text = text.to_lowercase();
        if text.is_empty() {
            return self.tree(workspaces, summaries, id).map(Some);
        }
        let layout = Layout::read(workspaces, summaries, id)?;
        // Each repo's refs are read once, wherever it appears.
        let mut read: HashMap<RepoId, Option<Refs>> = HashMap::new();
        let mut filtered = |repo: &RepoId, place: Option<&GroupId>| -> Option<TreeNode> {
            let refs = read
                .entry(repo.clone())
                // A repo whose refs cannot be read is matched by its name only.
                .or_insert_with(|| self.read(workspaces, id, repo, false).ok());
            let kept: Vec<TreeNode> = refs
                .as_ref()
                .map(|refs| ref_nodes(repo, place, refs))
                .unwrap_or_default()
                .into_iter()
                .filter_map(|node| prune(node, &text))
                .collect();
            let mut node = layout.repo_node(repo, place);
            if !kept.is_empty() {
                node.children = Some(kept);
                return Some(node);
            }
            node.label.to_lowercase().contains(&text).then_some(node)
        };
        let mut nodes = Vec::new();
        for group in &layout.groups {
            if !self.is_latest(id, request) {
                return Ok(None);
            }
            let place = Some(&group.summary.group_id);
            // A group matched by name shows all its members, not loaded.
            let members: Vec<TreeNode> = if group.name.to_lowercase().contains(&text) {
                let members = group.members.iter();
                members.map(|repo| layout.repo_node(repo, place)).collect()
            } else {
                let members = group.members.iter();
                members.filter_map(|repo| filtered(repo, place)).collect()
            };
            if !members.is_empty() || group.name.to_lowercase().contains(&text) {
                nodes.push(group_node(group, members));
            }
        }
        let mut repos = Vec::new();
        for repo in layout.ungrouped() {
            if !self.is_latest(id, request) {
                return Ok(None);
            }
            repos.extend(filtered(repo, None));
        }
        nodes.push(ungrouped(repos));
        Ok(self.is_latest(id, request).then_some(nodes))
    }
}

/// The four ref nodes of `repo` where it appears under group `place`, or
/// under **Ungrouped** for `None`, loaded, with ahead and behind counted.
/// Reads the repo off the async workers (ARCH-7).
pub async fn tree_refs(
    trees: Arc<Trees>,
    workspaces: Arc<Workspaces>,
    id: WorkspaceId,
    repo: RepoId,
    place: Option<GroupId>,
) -> Result<Vec<TreeNode>, TreeError> {
    tokio::task::spawn_blocking(move || {
        let refs = trees.read(&workspaces, &id, &repo, true)?;
        Ok(ref_nodes(&repo, place.as_ref(), &refs))
    })
    .await?
}

/// The children of group `group`'s **Branches** node (UI-5): every local
/// branch name of at least one member, with how many of the members have
/// it, folded into folders. Members whose refs cannot be read have none.
/// Reads the members off the async workers (ARCH-7), without ahead and
/// behind.
pub async fn group_branches(
    trees: Arc<Trees>,
    workspaces: Arc<Workspaces>,
    id: WorkspaceId,
    group: GroupId,
) -> Result<Vec<TreeNode>, TreeError> {
    tokio::task::spawn_blocking(move || {
        let members = workspaces
            .groups(&id)?
            .into_iter()
            .find(|known| known.id == group)
            .ok_or_else(|| TreeError::UnknownGroup(group.clone()))?
            .repos;
        let total = u32::try_from(members.len()).unwrap_or(u32::MAX);
        let mut present: BTreeMap<String, u32> = BTreeMap::new();
        for repo in &members {
            let Ok(refs) = trees.read(&workspaces, &id, repo, false) else {
                continue;
            };
            let names: HashSet<&String> = refs.local.iter().map(|branch| &branch.name).collect();
            for name in names {
                *present.entry(name.clone()).or_default() += 1;
            }
        }
        let branches = present
            .into_iter()
            .map(|(name, present)| {
                let item = TreeItem::GroupBranch {
                    group_id: group.clone(),
                    name: name.clone(),
                    present,
                    members: total,
                };
                (name, item)
            })
            .collect();
        let folder = || TreeItem::GroupFolder {
            group_id: group.clone(),
        };
        Ok(fold(&branches_id(&group), &folder, branches))
    })
    .await?
}

/// The tree of workspace `id` filtered by `text` (UI-8): nodes whose repo
/// name or ref name contains it, ignoring case, with their ancestors. A
/// repo matched by name alone keeps its ref nodes, not loaded. Empty text
/// gives the whole skeleton. Refs not loaded yet are read without ahead
/// and behind. A group matched by name shows all its members, not loaded;
/// another group shows the members that match, and only when one does.
///
/// Each call supersedes the previous one for the same workspace: an older
/// call still running returns `None` (`Ok(None)`), so results never arrive
/// out of order.
pub async fn tree_filter(
    trees: Arc<Trees>,
    workspaces: Arc<Workspaces>,
    summaries: Arc<Summaries>,
    id: WorkspaceId,
    text: String,
) -> Result<Option<Vec<TreeNode>>, WorkspaceError> {
    let request = trees.begin_filter(&id);
    tokio::task::spawn_blocking(move || trees.filter(&workspaces, &summaries, &id, &text, request))
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

/// What the skeleton is built from: the groups with their rows' summaries,
/// and the repos' names.
struct Layout {
    groups: Vec<GroupLayout>,
    /// Every repo's name, in workspace order.
    repos: Vec<(RepoId, String)>,
}

struct GroupLayout {
    name: String,
    members: Vec<RepoId>,
    summary: GroupSummary,
}

impl Layout {
    fn read(
        workspaces: &Workspaces,
        summaries: &Summaries,
        id: &WorkspaceId,
    ) -> Result<Self, WorkspaceError> {
        let view = workspaces.view(id)?;
        let mut rows = group_summaries(workspaces, summaries, id)?.into_iter();
        let groups = view
            .groups
            .into_iter()
            .filter_map(|group| {
                let summary = rows.find(|row| row.group_id == group.id)?;
                Some(GroupLayout {
                    name: group.name,
                    members: group.repos,
                    summary,
                })
            })
            .collect();
        let repos = view
            .repos
            .into_iter()
            .map(|repo| (repo.id, repo.name))
            .collect();
        Ok(Self { groups, repos })
    }

    /// The repos in no group, in workspace order.
    fn ungrouped(&self) -> impl Iterator<Item = &RepoId> {
        self.repos
            .iter()
            .map(|(repo, _)| repo)
            .filter(|repo| !self.groups.iter().any(|group| group.members.contains(repo)))
    }

    /// The node of `repo` under group `place`, or under **Ungrouped**.
    fn repo_node(&self, repo: &RepoId, place: Option<&GroupId>) -> TreeNode {
        let name = self
            .repos
            .iter()
            .find(|(known, _)| known == repo)
            .map_or_else(|| repo.0.clone(), |(_, name)| name.clone());
        repo_node(repo, place, name)
    }
}

fn group_id(group: &GroupId) -> String {
    format!("group:{}", group.0)
}

fn branches_id(group: &GroupId) -> String {
    format!("{}:branches", group_id(group))
}

/// A group node with its **Branches** node, not loaded, and `members`.
fn group_node(group: &GroupLayout, members: Vec<TreeNode>) -> TreeNode {
    let group_id = &group.summary.group_id;
    let branches = TreeNode {
        id: branches_id(group_id),
        label: "Branches".to_owned(),
        item: TreeItem::GroupBranches {
            group_id: group_id.clone(),
        },
        children: None,
    };
    TreeNode {
        id: self::group_id(group_id),
        label: group.name.clone(),
        item: TreeItem::Group {
            group_id: group_id.clone(),
            summary: group.summary.clone(),
        },
        children: Some(std::iter::once(branches).chain(members).collect()),
    }
}

/// The id of `repo`'s node under group `place`, or under **Ungrouped**.
fn repo_id(repo: &RepoId, place: Option<&GroupId>) -> String {
    match place {
        Some(group) => format!("{}:repo:{}", group_id(group), repo.0),
        None => format!("repo:{}", repo.0),
    }
}

/// A repo node with its four ref nodes, not loaded.
fn repo_node(repo: &RepoId, place: Option<&GroupId>, name: String) -> TreeNode {
    let children = categories(repo, place)
        .into_iter()
        .map(|(id, label, item)| TreeNode {
            id,
            label: label.to_owned(),
            item,
            children: None,
        })
        .collect();
    TreeNode {
        id: repo_id(repo, place),
        label: name,
        item: TreeItem::Repo {
            repo_id: repo.clone(),
            group_id: place.cloned(),
        },
        children: Some(children),
    }
}

/// Id, label and item of the four ref nodes, in tree order.
fn categories(repo: &RepoId, place: Option<&GroupId>) -> [(String, &'static str, TreeItem); 4] {
    let base = repo_id(repo, place);
    let repo_id = || repo.clone();
    let group_id = || place.cloned();
    [
        (
            format!("{base}:local"),
            "Local",
            TreeItem::Local {
                repo_id: repo_id(),
                group_id: group_id(),
            },
        ),
        (
            format!("{base}:remotes"),
            "Remotes",
            TreeItem::Remotes {
                repo_id: repo_id(),
                group_id: group_id(),
            },
        ),
        (
            format!("{base}:tags"),
            "Tags",
            TreeItem::Tags {
                repo_id: repo_id(),
                group_id: group_id(),
            },
        ),
        (
            format!("{base}:stashes"),
            "Stashes",
            TreeItem::Stashes {
                repo_id: repo_id(),
                group_id: group_id(),
            },
        ),
    ]
}

/// The four ref nodes of `repo` under group `place`, loaded from `refs`.
fn ref_nodes(repo: &RepoId, place: Option<&GroupId>, refs: &Refs) -> Vec<TreeNode> {
    let [local, remotes, tags, stashes] = categories(repo, place);
    let folder = || TreeItem::Folder {
        repo_id: repo.clone(),
    };
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
    let local_children = fold(&local.0, &folder, branches);

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
                children: Some(fold(&id, &folder, branches)),
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
/// first, then refs, each sorted by name. `folder` gives a folder's item.
fn fold(
    parent: &str,
    folder: &dyn Fn() -> TreeItem,
    refs: Vec<(String, TreeItem)>,
) -> Vec<TreeNode> {
    #[derive(Default)]
    struct Folder {
        folders: BTreeMap<String, Folder>,
        refs: BTreeMap<String, TreeItem>,
    }

    fn build(
        folder: Folder,
        parent: &str,
        path: &str,
        item: &dyn Fn() -> TreeItem,
    ) -> Vec<TreeNode> {
        let mut nodes = Vec::new();
        for (name, sub) in folder.folders {
            let path = format!("{path}{name}/");
            nodes.push(TreeNode {
                id: format!("{parent}:dir:{path}"),
                label: name,
                item: item(),
                children: Some(build(sub, parent, &path, item)),
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
    build(root, parent, "", folder)
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
        let summaries = Summaries::load(crate::SummaryCache::none(), &workspaces);
        let id = workspaces.views()[0].id.clone();
        let other = WorkspaceId("other".to_owned());
        let trees = Trees::new();
        let older = trees.begin_filter(&id);
        let elsewhere = trees.begin_filter(&other);
        let newer = trees.begin_filter(&id);
        let filter = |request| {
            trees
                .filter(&workspaces, &summaries, &id, "x", request)
                .unwrap()
        };
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
        let nodes = ref_nodes(&repo, None, &refs);
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
