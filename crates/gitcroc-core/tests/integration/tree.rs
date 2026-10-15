//! The tree of a workspace (UI-1) and its filter (UI-8).

use std::path::{Path, PathBuf};
use std::sync::Arc;

use gitcroc_core::{
    ConfigDir, GitRunner, GroupBranch, GroupId, JobId, RepoId, Summaries, SummaryCache, TreeItem,
    TreeNode, Trees, UNGROUPED_ID, WorkspaceId, Workspaces, add_path, group_branches,
    refresh_workspace, tree_filter, tree_refs,
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
    summaries: Arc<Summaries>,
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
        let summaries = Arc::new(Summaries::load(SummaryCache::none(), &workspaces));
        Self {
            _dirs: dirs,
            workspaces,
            summaries,
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
        self.trees
            .tree(&self.workspaces, &self.summaries, &self.id)
            .unwrap()
    }

    async fn refs(&self, repo: &RepoId) -> Vec<TreeNode> {
        self.refs_in(repo, None).await
    }

    /// The ref nodes of `repo` where it appears under group `place`.
    async fn refs_in(&self, repo: &RepoId, place: Option<&GroupId>) -> Vec<TreeNode> {
        tree_refs(
            self.trees.clone(),
            self.workspaces.clone(),
            self.id.clone(),
            repo.clone(),
            place.cloned(),
        )
        .await
        .unwrap()
    }

    async fn filter(&self, text: &str) -> Vec<TreeNode> {
        tree_filter(
            self.trees.clone(),
            self.workspaces.clone(),
            self.summaries.clone(),
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
    let tree = run.trees.tree(&run.workspaces, &run.summaries, &elsewhere);
    assert!(tree.is_err());
    let refs = tree_refs(
        run.trees.clone(),
        run.workspaces.clone(),
        elsewhere.clone(),
        run.repo("api"),
        None,
    )
    .await;
    assert!(refs.is_err());
    let filtered = tree_filter(
        run.trees.clone(),
        run.workspaces.clone(),
        run.summaries.clone(),
        elsewhere,
        "api".to_owned(),
    )
    .await;
    assert!(filtered.is_err());
}

/// Every node id in `nodes` and below, loaded nodes only.
fn ids(nodes: &[TreeNode]) -> Vec<String> {
    let mut all = Vec::new();
    for node in nodes {
        all.push(node.id.clone());
        all.extend(ids(node.children.as_deref().unwrap_or_default()));
    }
    all
}

fn find<'a>(nodes: &'a [TreeNode], id: &str) -> &'a TreeNode {
    nodes
        .iter()
        .find(|node| node.id == id)
        .unwrap_or_else(|| panic!("no node {id} in {:?}", labels(nodes)))
}

#[tokio::test]
async fn ui_1_tree_lists_groups_in_order_then_ungrouped() {
    let fixture = Fixture::new().unwrap();
    for name in ["api", "docs", "lib", "web"] {
        repo_at(fixture.path().join(name));
    }
    let run = Run::with_repos_in(fixture.path()).await;
    let (api, lib, web) = (run.repo("api"), run.repo("lib"), run.repo("web"));
    let backend = run
        .workspaces
        .create_group(&run.id, "Backend", &[lib.clone(), api.clone()])
        .unwrap();
    let frontend = run
        .workspaces
        .create_group(&run.id, "Frontend", &[web])
        .unwrap();
    run.workspaces
        .reorder_groups(&run.id, &[frontend, backend.clone()])
        .unwrap();

    let tree = run.tree();
    assert_eq!(labels(&tree), ["Frontend", "Backend", "Ungrouped"]);
    assert_eq!(tree[2].id, UNGROUPED_ID);
    // A group holds its Branches node, not loaded, then its members in order.
    let group = &tree[1];
    assert_eq!(group.id, format!("group:{backend}"));
    assert_eq!(labels(children(group)), ["Branches", "lib", "api"]);
    assert_eq!(children(group)[0].children, None);
    let TreeItem::Group { group_id, summary } = &group.item else {
        panic!("not a group: {:?}", group.item);
    };
    assert_eq!(group_id, &backend);
    assert_eq!(summary.members, 2);
    // Members have their ref nodes, not loaded, as under Ungrouped.
    let member = &children(group)[2];
    assert_eq!(
        member.item,
        TreeItem::Repo {
            repo_id: api,
            group_id: Some(backend.clone()),
        }
    );
    assert_eq!(
        labels(children(member)),
        ["Local", "Remotes", "Tags", "Stashes"]
    );
    // Ungrouped holds the repos in no group.
    assert_eq!(labels(children(&tree[2])), ["docs"]);
}

#[tokio::test]
async fn ui_2_a_repo_in_two_groups_appears_under_each() {
    let fixture = Fixture::new().unwrap();
    repo_at(fixture.path().join("api"))
        .branch("feature/x")
        .unwrap();
    repo_at(fixture.path().join("web"));
    let run = Run::with_repos_in(fixture.path()).await;
    let api = run.repo("api");
    let one = run
        .workspaces
        .create_group(&run.id, "One", std::slice::from_ref(&api))
        .unwrap();
    let two = run
        .workspaces
        .create_group(&run.id, "Two", std::slice::from_ref(&api))
        .unwrap();

    let tree = run.tree();
    let under = |group: &GroupId| {
        let node = find(&tree, &format!("group:{group}"));
        find(children(node), &format!("group:{group}:repo:{api}")).clone()
    };
    let (first, second) = (under(&one), under(&two));
    assert_ne!(first.id, second.id);
    for (node, place) in [(&first, &one), (&second, &two)] {
        assert_eq!(
            node.item,
            TreeItem::Repo {
                repo_id: api.clone(),
                group_id: Some(place.clone()),
            }
        );
    }
    // The same repo: Ungrouped does not list it.
    assert_eq!(labels(children(find(&tree, UNGROUPED_ID))), ["web"]);

    // Its refs, loaded in each place, have ids of that place.
    let refs_one = run.refs_in(&api, Some(&one)).await;
    let refs_two = run.refs_in(&api, Some(&two)).await;
    assert_eq!(refs_one[0].id, format!("group:{one}:repo:{api}:local"));
    assert_eq!(
        refs_one[0].item,
        TreeItem::Local {
            repo_id: api.clone(),
            group_id: Some(one.clone()),
        }
    );
    assert_eq!(outline(&refs_one), outline(&refs_two));
    // Loaded ref nodes take the place of the skeleton's nodes of the same
    // id; within the skeleton, and among both places' refs, none repeats.
    let unique = |all: Vec<String>| {
        let count = all.len();
        let mut sorted = all;
        sorted.sort();
        sorted.dedup();
        assert_eq!(sorted.len(), count, "node ids repeat");
    };
    unique(ids(&tree));
    unique([ids(&refs_one), ids(&refs_two)].concat());
}

#[tokio::test]
async fn ui_5_group_branches_node_counts_members_with_each_branch() {
    let fixture = Fixture::new().unwrap();
    for (name, branches) in [
        ("a", &["feature/x"][..]),
        ("b", &["feature/x"][..]),
        ("c", &["fix"][..]),
    ] {
        let repo = repo_at(fixture.path().join(name));
        for branch in branches {
            repo.branch(branch).unwrap();
        }
    }
    let run = Run::with_repos_in(fixture.path()).await;
    let members = ["a", "b", "c"].map(|name| run.repo(name));
    let group = run
        .workspaces
        .create_group(&run.id, "All", &members)
        .unwrap();

    let nodes = group_branches(
        run.trees.clone(),
        run.workspaces.clone(),
        run.id.clone(),
        group.clone(),
    )
    .await
    .unwrap();
    // Folders first, then names, each sorted.
    assert_eq!(labels(&nodes), ["feature", "fix", "main"]);
    assert_eq!(nodes[0].id, format!("group:{group}:branches:dir:feature/"));
    let count = |node: &TreeNode| match &node.item {
        TreeItem::GroupBranch {
            name,
            present,
            members,
            ..
        } => (name.clone(), *present, *members),
        other => panic!("not a group branch: {other:?}"),
    };
    let x = &children(&nodes[0])[0];
    assert_eq!(x.id, format!("group:{group}:branches:ref:feature/x"));
    assert_eq!(count(x), ("feature/x".to_owned(), 2, 3));
    assert_eq!(count(&nodes[1]), ("fix".to_owned(), 1, 3));
    assert_eq!(count(&nodes[2]), ("main".to_owned(), 3, 3));
}

#[tokio::test]
async fn ui_8_filter_matches_group_names() {
    let fixture = Fixture::new().unwrap();
    for name in ["api", "lib", "web"] {
        repo_at(fixture.path().join(name));
    }
    let run = Run::with_repos_in(fixture.path()).await;
    let (api, lib) = (run.repo("api"), run.repo("lib"));
    run.workspaces
        .create_group(&run.id, "Backend", &[api, lib])
        .unwrap();

    // A group matched by name shows all its members, not loaded.
    let tree = run.filter("BACK").await;
    assert_eq!(labels(&tree), ["Backend", "Ungrouped"]);
    assert_eq!(labels(children(&tree[0])), ["Branches", "api", "lib"]);
    assert_eq!(children(&children(&tree[0])[1])[0].children, None);
    assert!(children(&tree[1]).is_empty());

    // Otherwise a group shows the members that match, and only then.
    let tree = run.filter("lib").await;
    assert_eq!(labels(&tree), ["Backend", "Ungrouped"]);
    assert_eq!(labels(children(&tree[0])), ["Branches", "lib"]);
    let tree = run.filter("web").await;
    assert_eq!(labels(&tree), ["Ungrouped"]);
    assert_eq!(labels(children(&tree[0])), ["web"]);
}

#[tokio::test]
async fn grp_4_the_tree_sends_each_groups_branch_and_totals() {
    let fixture = Fixture::new().unwrap();
    repo_at(fixture.path().join("api"));
    let web = repo_at(fixture.path().join("web"));
    web.write("edited.txt", "x\n").unwrap();
    let run = Run::with_repos_in(fixture.path()).await;
    let members = [run.repo("api"), run.repo("web")];
    run.workspaces
        .create_group(&run.id, "Both", &members)
        .unwrap();
    let summary = |tree: &[TreeNode]| match &tree[0].item {
        TreeItem::Group { summary, .. } => summary.clone(),
        other => panic!("not a group: {other:?}"),
    };
    // No summary yet: nothing counts.
    let before = summary(&run.tree());
    assert_eq!((before.branch, before.dirty), (GroupBranch::None, 0));

    refresh_workspace(
        run.workspaces.clone(),
        run.summaries.clone(),
        run.id.clone(),
        GitRunner::new(PathBuf::from("git")),
        |_| {},
    )
    .await
    .unwrap();
    let after = summary(&run.tree());
    assert_eq!(
        after.branch,
        GroupBranch::Branch {
            name: "main".to_owned()
        }
    );
    assert_eq!((after.members, after.dirty, after.conflicts), (2, 1, 0));
}
