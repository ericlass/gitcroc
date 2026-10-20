//! The command palette's contents (UI-22): the catalog of operations and
//! the repos, groups and branches of a workspace that match a query.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use gitcroc_core::{
    AiOptions, ConfigDir, GitRunner, JobId, MAX_PALETTE_ITEMS, PaletteItem, PaletteOp,
    PaletteResult, PaletteSubject, Palettes, RepoId, SettingsStore, Trees, WorkspaceId, Workspaces,
    add_path, palette_catalog, palette_query, tree_refs,
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
    settings: Arc<SettingsStore>,
    trees: Arc<Trees>,
    palettes: Arc<Palettes>,
    id: WorkspaceId,
}

async fn add(workspaces: &Arc<Workspaces>, id: &WorkspaceId, path: &Path) {
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
}

impl Run {
    async fn with_repos_in(path: &Path) -> Self {
        let dirs = Dirs {
            root: tempfile::tempdir().unwrap(),
        };
        let workspaces = Arc::new(Workspaces::load(&dirs));
        let id = workspaces.views()[0].id.clone();
        add(&workspaces, &id, path).await;
        Self {
            settings: Arc::new(SettingsStore::load(&dirs)),
            _dirs: dirs,
            workspaces,
            trees: Arc::new(Trees::new()),
            palettes: Arc::new(Palettes::new()),
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

    async fn query_in(&self, id: &WorkspaceId, text: &str) -> PaletteResult {
        palette_query(
            self.palettes.clone(),
            self.trees.clone(),
            self.workspaces.clone(),
            self.settings.clone(),
            id.clone(),
            text.to_owned(),
        )
        .await
        .unwrap()
        .unwrap()
    }

    async fn query(&self, text: &str) -> PaletteResult {
        self.query_in(&self.id, text).await
    }

    fn set_ai(&self, enabled: bool) {
        self.settings
            .set_ai_options(AiOptions {
                enabled,
                exclude: Vec::new(),
                debug_log: false,
            })
            .unwrap();
    }
}

fn repo_at(path: PathBuf) -> Repo {
    let repo = Repo::init(path).unwrap();
    repo.commit_file("README.md", "hello\n", "First").unwrap();
    repo
}

/// Each item as one line: its kind and its text.
fn lines(result: &PaletteResult) -> Vec<String> {
    result
        .items
        .iter()
        .map(|item| match item {
            PaletteItem::Operation { label, .. } => format!("operation {label}"),
            PaletteItem::Repo { name, .. } => format!("repo {name}"),
            PaletteItem::Group { name, .. } => format!("group {name}"),
            PaletteItem::Branch {
                repo_name, name, ..
            } => format!("branch {repo_name} {name}"),
        })
        .collect()
}

fn ops(result: &PaletteResult) -> Vec<PaletteOp> {
    result
        .items
        .iter()
        .filter_map(|item| match item {
            PaletteItem::Operation { op, .. } => Some(*op),
            _ => None,
        })
        .collect()
}

/// Every operation of section 6's table and section 5's group forms, by
/// the name the spec gives it, with the entry that reaches it.
const SECTIONS: &[(&str, &str, PaletteOp)] = &[
    ("Repos", "Add existing folder", PaletteOp::AddFolder),
    ("Repos", "Clone", PaletteOp::Clone),
    ("Repos", "Remove from workspace", PaletteOp::RemoveRepo),
    (
        "Repos",
        "Open in file manager",
        PaletteOp::OpenInFileManager,
    ),
    ("Repos", "Open in terminal", PaletteOp::OpenInTerminal),
    ("Working copy", "Stage by file", PaletteOp::StageFile),
    ("Working copy", "Unstage by file", PaletteOp::UnstageFile),
    ("Working copy", "Discard by file", PaletteOp::DiscardFile),
    ("Working copy", "Stage all", PaletteOp::StageAll),
    ("Working copy", "Unstage all", PaletteOp::UnstageAll),
    ("Working copy", "Commit", PaletteOp::Commit),
    ("Working copy", "Amend last commit", PaletteOp::Amend),
    ("Branches", "Checkout", PaletteOp::Checkout),
    ("Branches", "Create", PaletteOp::CreateBranch),
    ("Branches", "Rename", PaletteOp::RenameBranch),
    ("Branches", "Delete", PaletteOp::DeleteBranch),
    ("Branches", "Set upstream", PaletteOp::SetUpstream),
    (
        "Branches",
        "Checkout of a remote branch",
        PaletteOp::CheckoutRemoteBranch,
    ),
    (
        "Branches",
        "Checkout of a commit (detached)",
        PaletteOp::CheckoutCommit,
    ),
    ("Sync", "Fetch", PaletteOp::Fetch),
    ("Sync", "Fetch with options", PaletteOp::FetchWithOptions),
    ("Sync", "Pull", PaletteOp::Pull),
    ("Sync", "Pull with options", PaletteOp::PullWithOptions),
    ("Sync", "Push", PaletteOp::Push),
    ("Sync", "Push with options", PaletteOp::PushWithOptions),
    ("Integrate", "Merge", PaletteOp::Merge),
    ("Integrate", "Rebase (non-interactive)", PaletteOp::Rebase),
    ("Integrate", "Cherry-pick", PaletteOp::CherryPick),
    ("Integrate", "Revert a commit", PaletteOp::Revert),
    ("Integrate", "Abort from conflict state", PaletteOp::Abort),
    (
        "Integrate",
        "Continue from conflict state",
        PaletteOp::Continue,
    ),
    (
        "Conflict state",
        "Mark resolved (OPS-5)",
        PaletteOp::MarkResolved,
    ),
    ("Conflict state", "Take ours (OPS-3)", PaletteOp::TakeOurs),
    (
        "Conflict state",
        "Take theirs (OPS-3)",
        PaletteOp::TakeTheirs,
    ),
    ("Conflict state", "Take both (OPS-3)", PaletteOp::TakeBoth),
    ("Conflict state", "Keep ours (OPS-6)", PaletteOp::KeepOurs),
    (
        "Conflict state",
        "Keep theirs (OPS-6)",
        PaletteOp::KeepTheirs,
    ),
    ("Stash", "Stash", PaletteOp::Stash),
    ("Stash", "Apply", PaletteOp::StashApply),
    ("Stash", "Pop", PaletteOp::StashPop),
    ("Stash", "Drop", PaletteOp::StashDrop),
    ("Tags", "Create", PaletteOp::CreateTag),
    ("Tags", "Delete", PaletteOp::DeleteTag),
    ("Tags", "Push", PaletteOp::PushTag),
    ("Reset", "Soft", PaletteOp::ResetSoft),
    ("Reset", "Mixed", PaletteOp::ResetMixed),
    ("Reset", "Hard", PaletteOp::ResetHard),
    ("Remotes", "Add", PaletteOp::AddRemote),
    ("Remotes", "Rename", PaletteOp::RenameRemote),
    ("Remotes", "Remove", PaletteOp::RemoveRemote),
    ("Remotes", "Edit URL", PaletteOp::EditRemoteUrl),
    ("Section 5", "Fetch", PaletteOp::Fetch),
    ("Section 5", "Pull", PaletteOp::Pull),
    ("Section 5", "Push", PaletteOp::Push),
    ("Section 5", "Checkout branch X", PaletteOp::Checkout),
    ("Section 5", "Create branch X", PaletteOp::CreateBranch),
    ("Section 5", "Delete branch X", PaletteOp::DeleteBranch),
    ("Section 5", "Merge X into current", PaletteOp::Merge),
    ("Section 5", "Rebase current onto X", PaletteOp::Rebase),
    ("Section 5", "Create tag X", PaletteOp::CreateTag),
    ("Section 5", "Commit", PaletteOp::Commit),
];

/// The entries around the operations: AI (AI-3, AI-11) and the app, with
/// the update check (PLAT-4).
const AROUND: &[(&str, PaletteOp)] = &[
    ("Resolve with AI", PaletteOp::ResolveWithAi),
    ("Resolve all with AI", PaletteOp::ResolveAllWithAi),
    (
        "Accept all high-confidence",
        PaletteOp::AcceptHighConfidence,
    ),
    ("Settings", PaletteOp::Settings),
    ("New workspace", PaletteOp::NewWorkspace),
    ("Import workspace", PaletteOp::ImportWorkspace),
    ("Export workspace", PaletteOp::ExportWorkspace),
    ("Job list", PaletteOp::JobList),
    ("Close the tab", PaletteOp::CloseTab),
    ("Next tab", PaletteOp::NextTab),
    ("Previous tab", PaletteOp::PreviousTab),
    ("New group", PaletteOp::NewGroup),
    ("Check for updates (M5-12)", PaletteOp::CheckForUpdates),
];

#[tokio::test]
async fn ui_22_palette_lists_every_operation_of_sections_5_and_6() {
    let fixture = Fixture::new().unwrap();
    repo_at(fixture.path().join("api"));
    let run = Run::with_repos_in(fixture.path()).await;

    // The empty query lists the operations only, in catalog order.
    let listed = run.query("").await;
    let catalog: Vec<PaletteOp> = palette_catalog().iter().map(|entry| entry.op).collect();
    assert_eq!(ops(&listed), catalog);
    assert_eq!(listed.items.len(), catalog.len());
    assert_eq!(listed.total as usize, catalog.len());

    // One entry per operation, each with a label.
    let unique: HashSet<PaletteOp> = catalog.iter().copied().collect();
    assert_eq!(unique.len(), catalog.len());
    assert!(
        palette_catalog()
            .iter()
            .all(|entry| !entry.label.is_empty())
    );

    for (area, name, op) in SECTIONS {
        assert!(unique.contains(op), "{area}: {name} has no entry");
    }
    // Nothing else is listed.
    let named: HashSet<PaletteOp> = SECTIONS
        .iter()
        .map(|(_, _, op)| *op)
        .chain(AROUND.iter().map(|(_, op)| *op))
        .collect();
    assert_eq!(named, unique);

    // Section 5's group forms act on the target set, as the toolbar does.
    for (area, name, op) in SECTIONS.iter().filter(|(area, ..)| *area == "Section 5") {
        let entry = palette_catalog()
            .iter()
            .find(|entry| entry.op == *op)
            .unwrap();
        assert_eq!(entry.subject, PaletteSubject::TargetSet, "{area}: {name}");
    }
    let subject = |op| {
        palette_catalog()
            .iter()
            .find(|entry| entry.op == op)
            .unwrap()
            .subject
    };
    assert_eq!(subject(PaletteOp::RenameBranch), PaletteSubject::Branch);
    assert_eq!(subject(PaletteOp::DeleteTag), PaletteSubject::Tag);
    assert_eq!(subject(PaletteOp::StashPop), PaletteSubject::Stash);
    assert_eq!(subject(PaletteOp::EditRemoteUrl), PaletteSubject::Remote);
    assert_eq!(subject(PaletteOp::ResetHard), PaletteSubject::Commit);
    assert_eq!(subject(PaletteOp::StageFile), PaletteSubject::File);
    assert_eq!(subject(PaletteOp::Continue), PaletteSubject::Conflict);
    assert_eq!(subject(PaletteOp::Settings), PaletteSubject::App);

    // An item carries its entry's label and subject.
    let PaletteItem::Operation { op, label, subject } = &listed.items[0] else {
        panic!("not an operation");
    };
    let first = &palette_catalog()[0];
    assert_eq!(
        (*op, label.as_str(), *subject),
        (first.op, first.label, first.subject)
    );
}

#[tokio::test]
async fn ui_22_palette_finds_repos_groups_and_branches() {
    let fixture = Fixture::new().unwrap();
    let api = repo_at(fixture.path().join("api"));
    api.branch("feature/login").unwrap();
    let web = repo_at(fixture.path().join("web"));
    web.branch("feature/login-page").unwrap();
    let clone = web.clone_to(fixture.path().join("web-clone")).unwrap();
    clone.switch("feature/login-page").unwrap();
    let run = Run::with_repos_in(fixture.path()).await;
    let (api_id, web_id) = (run.repo("api"), run.repo("web"));
    let group = run
        .workspaces
        .create_group(&run.id, "Login team", &[api_id.clone(), web_id.clone()])
        .unwrap();
    // `api`'s refs are loaded; the others' are not.
    tree_refs(
        run.trees.clone(),
        run.workspaces.clone(),
        run.id.clone(),
        api_id.clone(),
        None,
    )
    .await
    .unwrap();

    let found = run.query("login").await;
    assert_eq!(
        lines(&found),
        [
            "group Login team",
            "branch api feature/login",
            "branch web feature/login-page",
            "branch web-clone feature/login-page",
            "branch web-clone origin/feature/login-page",
        ]
    );
    assert_eq!(found.total, 5);
    assert_eq!(
        found.items[0],
        PaletteItem::Group {
            group_id: group,
            name: "Login team".to_owned(),
            members: 2,
        }
    );
    assert_eq!(
        found.items[1],
        PaletteItem::Branch {
            repo_id: api_id.clone(),
            repo_name: "api".to_owned(),
            name: "feature/login".to_owned(),
            full_name: "refs/heads/feature/login".to_owned(),
            remote: None,
        }
    );
    assert_eq!(
        found.items[4],
        PaletteItem::Branch {
            repo_id: run.repo("web-clone"),
            repo_name: "web-clone".to_owned(),
            name: "origin/feature/login-page".to_owned(),
            full_name: "refs/remotes/origin/feature/login-page".to_owned(),
            remote: Some("origin".to_owned()),
        }
    );

    // A repo with its current branch; tags and stashes are not listed.
    clone.tag("web-release").unwrap();
    let repos = run.query("web").await;
    assert_eq!(
        lines(&repos)[..2],
        ["repo web".to_owned(), "repo web-clone".to_owned()]
    );
    assert_eq!(
        repos.items[1],
        PaletteItem::Repo {
            repo_id: run.repo("web-clone"),
            name: "web-clone".to_owned(),
            branch: Some("feature/login-page".to_owned()),
        }
    );
    assert!(lines(&repos).iter().all(|line| !line.contains("release")));
}

#[tokio::test]
async fn ui_22_palette_matches_every_word_in_any_order() {
    let fixture = Fixture::new().unwrap();
    let api = repo_at(fixture.path().join("api"));
    for name in ["feature/web", "main-push", "push", "subweb"] {
        api.branch(name).unwrap();
    }
    repo_at(fixture.path().join("web"));
    repo_at(fixture.path().join("webapp"));
    let run = Run::with_repos_in(fixture.path()).await;
    run.workspaces
        .create_group(&run.id, "My web", &[run.repo("webapp")])
        .unwrap();

    // Whole name, name prefix, word prefix, anywhere; ties by kind, then
    // in tree order: the group's members first, then the ungrouped repos.
    assert_eq!(
        lines(&run.query("web").await),
        [
            "repo web",
            "repo webapp",
            "group My web",
            "branch api feature/web",
            "branch webapp main",
            "branch api subweb",
            "branch web main",
        ]
    );
    // Operations come before branches of the same rank, in catalog order.
    assert_eq!(
        lines(&run.query("push").await),
        [
            "operation Push",
            "branch api push",
            "operation Push with options…",
            "operation Push tag",
            "branch api main-push",
        ]
    );

    // Every word must occur, in any order and any case; a branch's text
    // also has its repo's name.
    let words = ["branch webapp main", "branch web main"];
    assert_eq!(lines(&run.query("main web").await), words);
    assert_eq!(lines(&run.query("WEB  Main").await), words);
    assert_eq!(
        lines(&run.query("api FEATURE").await),
        ["branch api feature/web"]
    );
    assert!(run.query("web nothing").await.items.is_empty());
    // An operation matches by its keywords too.
    assert!(ops(&run.query("switch").await).contains(&PaletteOp::Checkout));

    // The empty query, or one of spaces only, lists the operations only.
    for empty in ["", "   "] {
        let listed = run.query(empty).await;
        assert!(
            listed
                .items
                .iter()
                .all(|item| matches!(item, PaletteItem::Operation { .. }))
        );
        assert_eq!(listed.items.len(), palette_catalog().len());
    }
}

#[tokio::test]
async fn ui_22_palette_returns_at_most_100_items_with_the_total() {
    let fixture = Fixture::new().unwrap();
    let api = repo_at(fixture.path().join("api"));
    for index in 0..150 {
        api.branch(&format!("topic-{index:03}")).unwrap();
    }
    let run = Run::with_repos_in(fixture.path()).await;

    let found = run.query("topic").await;
    assert_eq!(MAX_PALETTE_ITEMS, 100);
    assert_eq!(found.items.len(), MAX_PALETTE_ITEMS);
    assert_eq!(found.total, 150);
    // The first 100 in order.
    assert_eq!(lines(&found)[0], "branch api topic-000");
    assert_eq!(lines(&found)[99], "branch api topic-099");
}

#[tokio::test]
async fn ui_22_palette_newer_query_supersedes() {
    let fixture = Fixture::new().unwrap();
    repo_at(fixture.path().join("api"));
    let run = Run::with_repos_in(fixture.path()).await;
    let other = run.workspaces.create("Other").unwrap();

    let answer = |request, id: &WorkspaceId| {
        run.palettes
            .answer(
                &run.trees,
                &run.workspaces,
                &run.settings,
                id,
                "api",
                request,
            )
            .unwrap()
    };
    let older = run.palettes.begin(&run.id);
    let newer = run.palettes.begin(&run.id);
    // A query of another workspace supersedes nothing here (ARCH-9).
    let elsewhere = run.palettes.begin(&other);
    assert!(answer(older, &run.id).is_none());
    let api = ["repo api", "branch api main"];
    assert_eq!(lines(&answer(newer, &run.id).unwrap()), api);
    assert!(answer(elsewhere, &other).unwrap().items.is_empty());

    // Queries made one after another each get their answer.
    assert_eq!(lines(&run.query("api").await), api);
    assert_eq!(lines(&run.query("api").await), api);
}

#[tokio::test]
async fn ai_16_palette_has_no_ai_entries_while_ai_is_off() {
    let fixture = Fixture::new().unwrap();
    repo_at(fixture.path().join("api"));
    let run = Run::with_repos_in(fixture.path()).await;
    let ai = [
        PaletteOp::ResolveWithAi,
        PaletteOp::ResolveAllWithAi,
        PaletteOp::AcceptHighConfidence,
    ];
    let has_ai = |found: &PaletteResult| ops(found).iter().any(|op| ai.contains(op));

    assert!(has_ai(&run.query("").await));
    assert!(has_ai(&run.query("resolve").await));
    assert!(
        palette_catalog()
            .iter()
            .filter(|entry| entry.ai)
            .map(|entry| entry.op)
            .eq(ai)
    );

    run.set_ai(false);
    let listed = run.query("").await;
    assert!(!has_ai(&listed));
    assert_eq!(listed.total as usize, palette_catalog().len() - ai.len());
    assert!(!has_ai(&run.query("resolve").await));
    assert!(!has_ai(&run.query("high-confidence").await));
    // Everything else stays listed, whether or not it can apply now.
    assert!(ops(&run.query("").await).contains(&PaletteOp::MarkResolved));

    run.set_ai(true);
    assert!(has_ai(&run.query("resolve").await));
}

#[tokio::test]
async fn arch_9_palette_is_per_workspace() {
    let fixture = Fixture::new().unwrap();
    let api = repo_at(fixture.path().join("one").join("api"));
    api.branch("only-in-one").unwrap();
    repo_at(fixture.path().join("two").join("web"));
    let run = Run::with_repos_in(&fixture.path().join("one")).await;
    let other = run.workspaces.create("Other").unwrap();
    add(&run.workspaces, &other, &fixture.path().join("two")).await;
    let web = run.workspaces.view(&other).unwrap().repos[0].id.clone();
    run.workspaces
        .create_group(&other, "Two's group", &[web])
        .unwrap();

    let api = ["repo api", "branch api main", "branch api only-in-one"];
    assert_eq!(lines(&run.query("api").await), api);
    assert!(run.query_in(&other, "api").await.items.is_empty());
    assert!(run.query_in(&other, "only-in").await.items.is_empty());
    assert_eq!(
        lines(&run.query_in(&other, "web").await),
        ["repo web", "branch web main"]
    );
    assert_eq!(
        lines(&run.query_in(&other, "two's").await),
        ["group Two's group"]
    );
    assert!(run.query("two's").await.items.is_empty());

    // A workspace that is not open has no palette.
    let unknown = palette_query(
        run.palettes.clone(),
        run.trees.clone(),
        run.workspaces.clone(),
        run.settings.clone(),
        WorkspaceId("nowhere".to_owned()),
        "api".to_owned(),
    )
    .await;
    assert!(unknown.is_err());
}
