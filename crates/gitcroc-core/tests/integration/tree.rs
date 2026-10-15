//! The tree of a workspace (UI-1) and its filter (UI-8).

use std::path::{Path, PathBuf};
use std::sync::Arc;

use gitcroc_core::{
    ConfigDir, GitRunner, JobId, RepoId, TreeItem, TreeNode, Trees, UNGROUPED_ID, WorkspaceId,
    Workspaces, add_path, tree_filter, tree_refs,
};
use repo_gen::{Fixture, Repo};

struct Dirs {
    root: tempfile::TempDir,
}

impl ConfigDir for Dirs {
    fn config_dir(&self) -> Option<PathBuf> {
        Some(self.root.path().join("config"))
    }
}

struct Run {
    _dirs: Dirs,
    workspaces: Arc<Workspaces>,
    trees: Arc<Trees>,
    id: WorkspaceId,
}

impl Run {
    async fn with_repos_in(path: &Path) -> Self {
        let dirs = Dirs {
            root: tempfile::tempdir().unwrap(),
        };
        let workspaces = Arc::new(Workspaces::load(&dirs));
        let id = workspaces.views()[0].id.clone();
        add_path(
            workspaces.clone(),
            id.clone(),
            path.to_owned(),
            GitRunner::new(PathBuf::from("git")),
            JobId(1),
            |_| {},
        )
        .await
        .unwrap();
        Self {
            _dirs: dirs,
            workspaces,
            trees: Arc::new(Trees::new()),
            id,
        }
    }

    fn repo(&self, name: &str) -> RepoId {
        let view = self.workspaces.view(&self.id).unwrap();
        view.repos
            .into_iter()
            .find(|repo| repo.name == name)
            .unwrap()
            .id
    }

    fn tree(&self) -> Vec<TreeNode> {
        self.trees.tree(&self.workspaces, &self.id).unwrap()
    }

    async fn refs(&self, repo: &RepoId) -> Vec<TreeNode> {
        tree_refs(
            self.trees.clone(),
            self.workspaces.clone(),
            self.id.clone(),
            repo.clone(),
        )
        .await
        .unwrap()
    }

    async fn filter(&self, text: &str) -> Vec<TreeNode> {
        tree_filter(
            self.trees.clone(),
            self.workspaces.clone(),
            self.id.clone(),
            text.to_owned(),
        )
        .await
        .unwrap()
        .unwrap()
    }
}

fn labels(nodes: &[TreeNode]) -> Vec<&str> {
    nodes.iter().map(|node| node.label.as_str()).collect()
}

fn children(node: &TreeNode) -> &[TreeNode] {
    node.children.as_deref().unwrap()
}

/// The tree as indented labels, loaded nodes only.
fn outline(nodes: &[TreeNode]) -> Vec<String> {
    fn walk(nodes: &[TreeNode], depth: usize, lines: &mut Vec<String>) {
        for node in nodes {
            lines.push(format!("{}{}", "  ".repeat(depth), node.label));
            if let Some(children) = &node.children {
                walk(children, depth + 1, lines);
            }
        }
    }
    let mut lines = Vec::new();
    walk(nodes, 0, &mut lines);
    lines
}

fn repo_at(path: PathBuf) -> Repo {
    let repo = Repo::init(path).unwrap();
    repo.commit_file("README.md", "hello\n", "First").unwrap();
    repo
}

#[tokio::test]
async fn ui_1_tree_structure() {
    let fixture = Fixture::new().unwrap();
    let api = repo_at(fixture.path().join("api"));
    api.branch("feature/a/b").unwrap();
    api.branch("feature/c").unwrap();
    api.tag("v1").unwrap();
    api.write("README.md", "work\n").unwrap();
    api.stash("work").unwrap();
    let web = repo_at(fixture.path().join("web"));
    let clone = web.clone_to(fixture.path().join("web-clone")).unwrap();
    clone.branch("topic").unwrap();
    let run = Run::with_repos_in(fixture.path()).await;

    // The skeleton: every repo under Ungrouped, each with its four ref
    // nodes, not loaded.
    let tree = run.tree();
    assert_eq!(labels(&tree), ["Ungrouped"]);
    assert_eq!(tree[0].id, UNGROUPED_ID);
    assert_eq!(tree[0].item, TreeItem::Ungrouped);
    let repos = children(&tree[0]);
    assert_eq!(labels(repos), ["api", "web", "web-clone"]);
    for repo in repos {
        assert_eq!(
            labels(children(repo)),
            ["Local", "Remotes", "Tags", "Stashes"]
        );
        assert!(children(repo).iter().all(|node| node.children.is_none()));
    }

    // One repo's refs: `feature/a/b` folds into `feature` and `a`.
    let api_refs = run.refs(&run.repo("api")).await;
    assert_eq!(
        outline(&api_refs),
        [
            "Local",
            "  feature",
            "    a",
            "      b",
            "    c",
            "  main",
            "Remotes",
            "Tags",
            "  v1",
            "Stashes",
            "  On main: work",
        ]
    );
    let main = &children(&api_refs[0])[1];
    assert!(matches!(
        &main.item,
        TreeItem::Branch { name, current: true, upstream: None, .. } if name == "main"
    ));
    let b = &children(&children(&children(&api_refs[0])[0])[0])[0];
    assert!(
        matches!(&b.item, TreeItem::Branch { name, current: false, .. } if name == "feature/a/b")
    );

    // Remote branches sit under their remote; a local branch with an
    // upstream has ahead and behind.
    let clone_refs = run.refs(&run.repo("web-clone")).await;
    assert_eq!(
        outline(&clone_refs[..2]),
        [
            "Local", "  main", "  topic", "Remotes", "  origin", "    main"
        ]
    );
    assert!(matches!(
        &children(&clone_refs[0])[0].item,
        TreeItem::Branch { upstream: Some(upstream), ahead: Some(0), behind: Some(0), .. }
            if upstream == "origin/main"
    ));

    // Loaded ref nodes keep the ids of the skeleton's, so they take their
    // place; below them every id is unique across the workspace.
    let api_skeleton: Vec<&str> = children(&repos[0]).iter().map(|n| n.id.as_str()).collect();
    let api_loaded: Vec<&str> = api_refs.iter().map(|n| n.id.as_str()).collect();
    assert_eq!(api_skeleton, api_loaded);
    fn collect<'a>(nodes: &'a [TreeNode], ids: &mut Vec<&'a str>) {
        for node in nodes {
            ids.push(node.id.as_str());
            collect(node.children.as_deref().unwrap_or_default(), ids);
        }
    }
    let mut ids = Vec::new();
    collect(&tree, &mut ids);
    for refs in [&api_refs, &clone_refs] {
        for category in refs.iter() {
            collect(children(category), &mut ids);
        }
    }
    let count = ids.len();
    ids.sort_unstable();
    ids.dedup();
    assert_eq!(ids.len(), count);
}

#[tokio::test]
async fn ui_8_filter_matches_repos_and_branches() {
    let fixture = Fixture::new().unwrap();
    let api = repo_at(fixture.path().join("api"));
    api.branch("feature/Login").unwrap();
    api.branch("bugfix/crash").unwrap();
    api.tag("login-v1").unwrap();
    let web = repo_at(fixture.path().join("web"));
    web.branch("release").unwrap();
    repo_at(fixture.path().join("login-service"));
    let run = Run::with_repos_in(fixture.path()).await;

    // A match keeps the node and its ancestors and drops the rest; repo
    // names, branch names and tag names match, ignoring case. Refs never
    // loaded are read for it.
    let filtered = run.filter("LOGIN").await;
    assert_eq!(
        outline(&filtered),
        [
            "Ungrouped",
            "  api",
            "    Local",
            "      feature",
            "        Login",
            "    Tags",
            "      login-v1",
            "  login-service",
            "    Local",
            "    Remotes",
            "    Tags",
            "    Stashes",
        ]
    );
    // A repo matched by its name alone keeps its ref nodes, not loaded.
    let service = &children(&filtered[0])[1];
    assert!(children(service).iter().all(|node| node.children.is_none()));

    // A folder part of a name matches too: `feature/` keeps the folder and
    // the branch inside it.
    assert_eq!(
        outline(&run.filter("ture/l").await),
        [
            "Ungrouped",
            "  api",
            "    Local",
            "      feature",
            "        Login"
        ]
    );

    // Nothing matches: Ungrouped stays, empty.
    assert_eq!(outline(&run.filter("nothing").await), ["Ungrouped"]);

    // Clearing gives back the whole skeleton.
    assert_eq!(run.filter("").await, run.tree());
}

#[tokio::test]
async fn ui_1_loaded_refs_are_read_again_after_invalidation() {
    let fixture = Fixture::new().unwrap();
    let api = repo_at(fixture.path().join("api"));
    let run = Run::with_repos_in(fixture.path()).await;
    let repo = run.repo("api");
    assert_eq!(labels(children(&run.refs(&repo).await[0])), ["main"]);

    api.branch("new").unwrap();
    // Until invalidated the loaded refs stand.
    assert_eq!(labels(children(&run.refs(&repo).await[0])), ["main"]);
    assert!(run.trees.invalidate(&run.id, &repo));
    assert!(!run.trees.invalidate(&run.id, &repo));
    assert_eq!(labels(children(&run.refs(&repo).await[0])), ["main", "new"]);
}

#[tokio::test]
async fn arch_9_trees_are_kept_per_workspace() {
    let fixture = Fixture::new().unwrap();
    repo_at(fixture.path().join("api"));
    let run = Run::with_repos_in(fixture.path()).await;
    let elsewhere = WorkspaceId("elsewhere".to_owned());
    assert!(run.trees.tree(&run.workspaces, &elsewhere).is_err());
    let refs = tree_refs(
        run.trees.clone(),
        run.workspaces.clone(),
        elsewhere.clone(),
        run.repo("api"),
    )
    .await;
    assert!(refs.is_err());
    let filtered = tree_filter(
        run.trees.clone(),
        run.workspaces.clone(),
        elsewhere,
        "api".to_owned(),
    )
    .await;
    assert!(filtered.is_err());
}
