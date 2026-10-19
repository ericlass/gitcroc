//! Groups in the core (M3-04): names and members (GRP-2), changes that
//! never touch the disk (GRP-3) and the derived branch (GRP-4).

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use gitcroc_core::{
    ConfigDir, GitRunner, GroupBranch, GroupId, JobId, RepoId, Summaries, SummaryCache, TargetSet,
    WorkspaceError, WorkspaceId, Workspaces, add_path, group_summaries, refresh_summaries,
    target_repos,
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

/// A workspace of the repos below a fixture's folder.
struct Run {
    dirs: Dirs,
    fixture: Fixture,
    workspaces: Arc<Workspaces>,
    id: WorkspaceId,
}

impl Run {
    /// Repos named `names`, each with one commit on `main`.
    async fn with(names: &[&str]) -> Self {
        let fixture = Fixture::new().unwrap();
        for name in names {
            let repo = Repo::init(fixture.path().join(name)).unwrap();
            repo.commit_file("README.md", "hello\n", "First").unwrap();
        }
        let dirs = Dirs {
            root: tempfile::tempdir().unwrap(),
        };
        let workspaces = Arc::new(Workspaces::load(&dirs));
        let id = workspaces.views()[0].id.clone();
        add_path(
            workspaces.clone(),
            id.clone(),
            fixture.path().to_owned(),
            git(),
            JobId(1),
            |_| {},
        )
        .await
        .unwrap();
        Self {
            dirs,
            fixture,
            workspaces,
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

    fn repos(&self, names: &[&str]) -> Vec<RepoId> {
        names.iter().map(|name| self.repo(name)).collect()
    }

    fn members(&self, group: &GroupId) -> Vec<RepoId> {
        self.workspaces
            .groups(&self.id)
            .unwrap()
            .into_iter()
            .find(|known| known.id == *group)
            .unwrap()
            .repos
    }

    fn names(&self) -> Vec<String> {
        let groups = self.workspaces.groups(&self.id).unwrap();
        groups.into_iter().map(|group| group.name).collect()
    }
}

fn git() -> GitRunner {
    GitRunner::new(PathBuf::from("git"))
}

/// Every file under `path` with its contents.
fn snapshot(path: &Path) -> BTreeMap<PathBuf, Vec<u8>> {
    let mut files = BTreeMap::new();
    let mut pending = vec![path.to_owned()];
    while let Some(folder) = pending.pop() {
        for entry in fs::read_dir(&folder).unwrap() {
            let entry = entry.unwrap();
            if entry.file_type().unwrap().is_dir() {
                pending.push(entry.path());
            } else {
                files.insert(entry.path(), fs::read(entry.path()).unwrap());
            }
        }
    }
    files
}

#[tokio::test]
async fn grp_2_group_names_are_unique_ignoring_case_and_1_to_64_characters() {
    let run = Run::with(&["api"]).await;
    let store = &run.workspaces;
    let backend = store.create_group(&run.id, "  Backend  ", &[]).unwrap();
    assert_eq!(run.names(), ["Backend"]);

    let refused = |name: &str| store.create_group(&run.id, name, &[]);
    assert!(matches!(
        refused("   "),
        Err(WorkspaceError::BlankGroupName)
    ));
    assert!(matches!(
        refused(&"é".repeat(65)),
        Err(WorkspaceError::GroupNameTooLong)
    ));
    assert!(matches!(
        refused(" BACKEND "),
        Err(WorkspaceError::GroupNameTaken(name)) if name == "Backend"
    ));
    for reserved in ["Ungrouped", " ungrouped ", "UNGROUPED"] {
        assert!(matches!(
            refused(reserved),
            Err(WorkspaceError::GroupNameReserved)
        ));
    }
    let unknown = store.create_group(&run.id, "Other", &[RepoId("r99".to_owned())]);
    assert!(matches!(unknown, Err(WorkspaceError::UnknownRepo(_))));
    // Nothing refused was kept.
    assert_eq!(run.names(), ["Backend"]);

    let longest = store.create_group(&run.id, &"é".repeat(64), &[]).unwrap();
    let rename = store.rename_group(&run.id, &longest, "backend");
    assert!(matches!(rename, Err(WorkspaceError::GroupNameTaken(_))));
    // A change of case of its own name is allowed, and the id stays.
    store.rename_group(&run.id, &backend, "BACKEND").unwrap();
    let groups = store.groups(&run.id).unwrap();
    assert_eq!(
        (groups[0].id.clone(), groups[0].name.as_str()),
        (backend, "BACKEND")
    );
}

#[tokio::test]
async fn grp_2_members_keep_their_order() {
    let run = Run::with(&["a", "b", "c", "d", "e"]).await;
    let store = &run.workspaces;
    let group = store
        .create_group(&run.id, "All", &run.repos(&["c", "a"]))
        .unwrap();
    assert_eq!(run.members(&group), run.repos(&["c", "a"]));

    // Added at a position; a repo already in it is there once.
    store
        .add_to_group(&run.id, &group, &run.repos(&["e", "b", "a"]), Some(1))
        .unwrap();
    assert_eq!(run.members(&group), run.repos(&["c", "e", "b", "a"]));
    store
        .add_to_group(&run.id, &group, &run.repos(&["d"]), None)
        .unwrap();
    assert_eq!(run.members(&group), run.repos(&["c", "e", "b", "a", "d"]));

    // Moved within the group, keeping their own order.
    store
        .move_in_group(&run.id, &group, &run.repos(&["a", "c"]), 0)
        .unwrap();
    assert_eq!(run.members(&group), run.repos(&["a", "c", "e", "b", "d"]));
    let stranger = run.repos(&["a"]);
    store.remove_from_group(&run.id, &group, &stranger).unwrap();
    let refused = store.move_in_group(&run.id, &group, &stranger, 0);
    assert!(matches!(refused, Err(WorkspaceError::UnknownRepo(_))));

    // Moved to another group, in one save.
    let other = store
        .create_group(&run.id, "Other", &run.repos(&["b"]))
        .unwrap();
    store
        .move_between_groups(&run.id, &run.repo("e"), &group, &other, Some(0))
        .unwrap();
    assert_eq!(run.members(&group), run.repos(&["c", "b", "d"]));
    assert_eq!(run.members(&other), run.repos(&["e", "b"]));

    // Groups reorder, and everything survives a reload in file order.
    store
        .reorder_groups(&run.id, &[other.clone(), group.clone()])
        .unwrap();
    let bad = store.reorder_groups(&run.id, std::slice::from_ref(&other));
    assert!(matches!(bad, Err(WorkspaceError::BadGroupOrder)));
    let reloaded = Workspaces::load(&run.dirs);
    let groups = reloaded.groups(&run.id).unwrap();
    let names: Vec<&str> = groups.iter().map(|group| group.name.as_str()).collect();
    assert_eq!(names, ["Other", "All"]);
    // Ids are given in file order on load.
    assert_eq!(groups[0].id, GroupId("g1".to_owned()));
    assert_eq!(groups[0].repos, run.repos(&["e", "b"]));
    assert_eq!(groups[1].repos, run.repos(&["c", "b", "d"]));
}

#[tokio::test]
async fn grp_5_target_set_is_a_repo_a_group_or_a_multi_selection() {
    let run = Run::with(&["a", "b", "c", "d", "e"]).await;
    let store = &run.workspaces;
    let resolve = |target: TargetSet| target_repos(store, &run.id, &target);
    let first = store
        .create_group(&run.id, "First", &run.repos(&["d", "b"]))
        .unwrap();
    let second = store
        .create_group(&run.id, "Second", &run.repos(&["b", "c"]))
        .unwrap();

    let one = resolve(TargetSet::Repo {
        repo_id: run.repo("e"),
    });
    assert_eq!(one.unwrap(), run.repos(&["e"]));
    // A group gives its members in group order.
    let group = resolve(TargetSet::Group {
        group_id: second.clone(),
    });
    assert_eq!(group.unwrap(), run.repos(&["b", "c"]));
    // A multi-selection comes back in tree order: the groups' members, then
    // Ungrouped in workspace order. A repo in two groups counts once.
    let repos = resolve(TargetSet::Repos {
        repo_ids: run.repos(&["e", "c", "b", "a", "b", "d"]),
    });
    assert_eq!(repos.unwrap(), run.repos(&["d", "b", "c", "a", "e"]));

    // What no longer exists is an error.
    store.delete_group(&run.id, &first).unwrap();
    let gone = resolve(TargetSet::Group { group_id: first });
    assert!(matches!(gone, Err(WorkspaceError::UnknownGroup(_))));
    let stranger = RepoId("r99".to_owned());
    let unknown = resolve(TargetSet::Repos {
        repo_ids: vec![run.repo("a"), stranger.clone()],
    });
    assert!(matches!(unknown, Err(WorkspaceError::UnknownRepo(repo)) if repo == stranger));
    let unknown = resolve(TargetSet::Repo { repo_id: stranger });
    assert!(matches!(unknown, Err(WorkspaceError::UnknownRepo(_))));
}

#[tokio::test]
async fn grp_3_group_changes_never_touch_the_disk() {
    let run = Run::with(&["a", "b", "c"]).await;
    Repo::open(run.fixture.path().join("a"))
        .write("dirty.txt", "not committed\n")
        .unwrap();
    let before = snapshot(run.fixture.path());
    let store = &run.workspaces;
    let (a, b, c) = (run.repo("a"), run.repo("b"), run.repo("c"));

    let one = store
        .create_group(&run.id, "One", &[a.clone(), b.clone()])
        .unwrap();
    let two = store.create_group(&run.id, "Two", &[]).unwrap();
    store.rename_group(&run.id, &one, "First").unwrap();
    store
        .add_to_group(&run.id, &two, &[c.clone(), a.clone()], Some(0))
        .unwrap();
    store
        .move_in_group(&run.id, &two, std::slice::from_ref(&a), 0)
        .unwrap();
    store
        .move_between_groups(&run.id, &b, &one, &two, None)
        .unwrap();
    store
        .remove_from_group(&run.id, &two, std::slice::from_ref(&c))
        .unwrap();
    store
        .reorder_groups(&run.id, &[two.clone(), one.clone()])
        .unwrap();
    store.delete_group(&run.id, &one).unwrap();
    // Removing a repo from the workspace removes it from its groups too.
    store.remove_repo(&run.id, &a).unwrap();
    assert_eq!(run.members(&two), [b]);

    assert_eq!(snapshot(run.fixture.path()), before);
}

#[tokio::test]
async fn grp_4_group_branch_is_the_common_branch_or_mixed_with_a_count() {
    let run = Run::with(&[
        "main1",
        "main2",
        "feature",
        "detached1",
        "detached2",
        "gone",
        "unread",
    ])
    .await;
    let repo = |name: &str| Repo::open(run.fixture.path().join(name));
    repo("feature")
        .git(["switch", "--quiet", "-c", "feature"])
        .unwrap();
    for name in ["detached1", "detached2"] {
        repo(name).git(["switch", "--quiet", "--detach"]).unwrap();
    }
    fs::remove_dir_all(run.fixture.path().join("gone")).unwrap();
    let summaries = Arc::new(Summaries::load(SummaryCache::none(), &run.workspaces));
    // Every repo but `unread` is read; `gone` reads as missing.
    let read = run.repos(&[
        "main1",
        "main2",
        "feature",
        "detached1",
        "detached2",
        "gone",
    ]);
    refresh_summaries(
        run.workspaces.clone(),
        summaries.clone(),
        run.id.clone(),
        read,
        git(),
        |_| {},
    )
    .await;

    let cases: [(&[&str], GroupBranch); 7] = [
        (&["main1", "main2"], branch("main")),
        // A member with no summary or a missing path does not count.
        (&["main1", "unread", "gone"], branch("main")),
        (&["main1", "feature"], GroupBranch::Mixed { count: 2 }),
        // Detached counts as one value, however many are.
        (&["detached1", "detached2"], GroupBranch::Detached),
        (
            &["main1", "detached1", "detached2"],
            GroupBranch::Mixed { count: 2 },
        ),
        (
            &["main1", "feature", "detached1", "detached2"],
            GroupBranch::Mixed { count: 3 },
        ),
        (&["unread", "gone"], GroupBranch::None),
    ];
    for (index, (members, _)) in cases.iter().enumerate() {
        run.workspaces
            .create_group(&run.id, &format!("Case {index}"), &run.repos(members))
            .unwrap();
    }
    let shown = group_summaries(&run.workspaces, &summaries, &run.id).unwrap();
    for ((members, expected), summary) in cases.iter().zip(&shown) {
        assert_eq!(&summary.branch, expected, "{members:?}");
        assert_eq!(summary.members as usize, members.len());
    }
}

fn branch(name: &str) -> GroupBranch {
    GroupBranch::Branch {
        name: name.to_owned(),
    }
}

/// A workspace file as M2 wrote it, in the section 10 format.
const M2_FILE: &str = r#"schema = 1
name = "Platform"

[[repo]]
id = "r1"
path = "/home/eric/src/platform/api"
remote = "git@example.com:platform/api.git"

[[repo]]
id = "r2"
path = "/home/eric/src/platform/web"
remote = "git@example.com:platform/web.git"

[[group]]
name = "Backend"
repos = ["r1"]

[[group]]
name = "Everything"
repos = ["r1", "r2"]
"#;

#[test]
fn data_1_a_workspace_file_with_groups_keeps_the_section_10_format() {
    let dirs = Dirs {
        root: tempfile::tempdir().unwrap(),
    };
    let folder = dirs.root.path().join("config").join("workspaces");
    fs::create_dir_all(&folder).unwrap();
    let file = folder.join("6f1c1d1e-0000-4000-8000-000000000001.toml");
    fs::write(&file, M2_FILE).unwrap();

    let store = Workspaces::load(&dirs);
    let id = store.active().unwrap();
    let groups = store.groups(&id).unwrap();
    let shown: Vec<(&str, &str, Vec<&str>)> = groups
        .iter()
        .map(|group| {
            let repos = group.repos.iter().map(|repo| repo.0.as_str()).collect();
            (group.id.0.as_str(), group.name.as_str(), repos)
        })
        .collect();
    assert_eq!(
        shown,
        [
            ("g1", "Backend", vec!["r1"]),
            ("g2", "Everything", vec!["r1", "r2"]),
        ]
    );
    // A change that leaves the groups as they were writes the same text.
    store
        .rename_group(&id, &GroupId("g1".to_owned()), "Backend")
        .unwrap();
    assert_eq!(fs::read_to_string(&file).unwrap(), M2_FILE);
    // A real change keeps the format: one `[[group]]` per group.
    store
        .create_group(&id, "Frontend", &[RepoId("r2".to_owned())])
        .unwrap();
    let text = fs::read_to_string(&file).unwrap();
    assert_eq!(
        text,
        format!("{M2_FILE}\n[[group]]\nname = \"Frontend\"\nrepos = [\"r2\"]\n")
    );
}

/// A workspace file whose groups name groups as members: a group's id, a
/// group's name and the built-in "Ungrouped", beside one real repo.
const NESTED_FILE: &str = r#"schema = 1
name = "Platform"

[[repo]]
id = "r1"
path = "/home/eric/src/platform/api"

[[repo]]
id = "r2"
path = "/home/eric/src/platform/web"

[[group]]
name = "Backend"
repos = ["r1"]

[[group]]
name = "Outer"
repos = ["g1", "Backend", "r2", "Ungrouped"]
"#;

#[tokio::test]
async fn d7_a_repo_can_be_in_no_one_or_several_groups_and_groups_do_not_nest() {
    let run = Run::with(&["api", "web", "cli"]).await;
    let store = &run.workspaces;
    let [api, web, cli] = [run.repo("api"), run.repo("web"), run.repo("cli")];
    let backend = store
        .create_group(&run.id, "Backend", std::slice::from_ref(&api))
        .unwrap();
    let front = store
        .create_group(&run.id, "Front", std::slice::from_ref(&web))
        .unwrap();
    let all = store
        .create_group(&run.id, "All", &[web.clone(), api.clone()])
        .unwrap();
    store
        .add_to_group(&run.id, &front, std::slice::from_ref(&api), None)
        .unwrap();

    let groups_of = |repo: &RepoId| -> Vec<String> {
        let groups = store.groups(&run.id).unwrap();
        let holding = groups
            .into_iter()
            .filter(|group| group.repos.contains(repo));
        holding.map(|group| group.name).collect()
    };
    assert_eq!(groups_of(&cli), Vec::<String>::new());
    assert_eq!(groups_of(&web), ["Front", "All"]);
    assert_eq!(groups_of(&api), ["Backend", "Front", "All"]);
    let resolve = |group: &GroupId| {
        let target = TargetSet::Group {
            group_id: group.clone(),
        };
        target_repos(store, &run.id, &target).unwrap()
    };
    assert_eq!(resolve(&backend), std::slice::from_ref(&api));
    assert_eq!(resolve(&all), [web.clone(), api.clone()]);

    // A group holds repos only: a group as a member is refused.
    let as_repo = RepoId(backend.0.clone());
    let refused = store.create_group(&run.id, "Outer", std::slice::from_ref(&as_repo));
    assert!(matches!(refused, Err(WorkspaceError::UnknownRepo(repo)) if repo == as_repo));
    let refused = store.add_to_group(&run.id, &all, std::slice::from_ref(&as_repo), None);
    assert!(matches!(refused, Err(WorkspaceError::UnknownRepo(repo)) if repo == as_repo));

    // A file that names a group as a member loads with the group ignored.
    let dirs = Dirs {
        root: tempfile::tempdir().unwrap(),
    };
    let folder = dirs.root.path().join("config").join("workspaces");
    fs::create_dir_all(&folder).unwrap();
    let file = folder.join("6f1c1d1e-0000-4000-8000-000000000002.toml");
    fs::write(&file, NESTED_FILE).unwrap();
    let store = Workspaces::load(&dirs);
    let id = store.active().unwrap();
    let view = store.view(&id).unwrap();
    assert_eq!(view.warning, None);
    assert!(!view.read_only);
    let members: Vec<(&str, Vec<&str>)> = view
        .groups
        .iter()
        .map(|group| {
            let repos = group.repos.iter().map(|repo| repo.0.as_str()).collect();
            (group.name.as_str(), repos)
        })
        .collect();
    assert_eq!(members, [("Backend", vec!["r1"]), ("Outer", vec!["r2"])]);
    let outer = TargetSet::Group {
        group_id: GroupId("g2".to_owned()),
    };
    assert_eq!(
        target_repos(&store, &id, &outer).unwrap(),
        [RepoId("r2".to_owned())]
    );
}
