//! Export and import of workspaces (DATA-2) with real repos and real git,
//! and cloning the missing repos of an import, and a clone into a group
//! (section 6 "Repos"), from bare repos reached over `file://`. Nothing
//! here waits for a time: each clone test waits for the clone's report.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use gitcroc_core::{
    CloneError, CloneGroup, CloneReport, CloneRequest, ConfigDir, GitRunner, JobEvents, JobId,
    JobView, Jobs, Outcome, RepoId, Summaries, SummaryCache, SummaryView, WorkspaceError,
    WorkspaceId, WorkspaceView, Workspaces, add_path, clone_start, clone_target_check,
};
use repo_gen::Repo;

/// A config dir and a folder for repos, files and remotes.
struct World {
    root: tempfile::TempDir,
}

impl ConfigDir for World {
    fn config_dir(&self) -> Option<PathBuf> {
        Some(self.root.path().join("config"))
    }
}

struct NoEvents;

impl JobEvents for NoEvents {
    fn progress(&self, _: &JobView) {}
    fn ended(&self, _: &JobView) {}
    fn summary(&self, _: SummaryView, _: bool) {}
}

impl World {
    fn new() -> Self {
        let world = Self {
            root: tempfile::tempdir().unwrap(),
        };
        fs::create_dir_all(world.path("home")).unwrap();
        world
    }

    /// `relative` under the world, with `/` between components.
    fn path(&self, relative: &str) -> PathBuf {
        let mut path = dunce::canonicalize(self.root.path()).unwrap();
        path.extend(relative.split('/'));
        path
    }

    /// git with no user or system config.
    fn git(&self) -> GitRunner {
        let home = self.path("home");
        GitRunner::new(PathBuf::from("git")).with_env([
            ("HOME", home.clone().into_os_string()),
            ("XDG_CONFIG_HOME", home.into_os_string()),
            ("GIT_CONFIG_NOSYSTEM", "1".into()),
        ])
    }

    /// A repo with one commit at `relative`, with `origin` set to `remote`.
    fn repo(&self, relative: &str, remote: &str) -> Repo {
        let repo = Repo::init(self.path(relative)).unwrap();
        repo.commit_file("a.txt", "a\n", "First").unwrap();
        repo.git(["remote", "add", "origin", remote]).unwrap();
        repo
    }

    /// A bare repo with one commit at `relative`, and its `file://` URL.
    fn bare(&self, relative: &str) -> String {
        let seed = Repo::init(self.path(&format!("seeds/{relative}"))).unwrap();
        seed.commit_file("a.txt", "a\n", "First").unwrap();
        let bare = self.path(relative);
        let mut args: Vec<std::ffi::OsString> = vec!["clone".into(), "--bare".into()];
        args.extend(["--quiet".into(), "--".into()]);
        args.extend([seed.path().into(), bare.clone().into()]);
        seed.git(args).unwrap();
        file_url(&bare)
    }

    fn workspaces(&self) -> Arc<Workspaces> {
        Arc::new(Workspaces::load(self))
    }
}

fn file_url(path: &Path) -> String {
    let path = path.to_string_lossy().replace('\\', "/");
    if path.starts_with('/') {
        format!("file://{path}")
    } else {
        format!("file:///{path}")
    }
}

async fn add(workspaces: &Arc<Workspaces>, id: &WorkspaceId, path: PathBuf, git: &GitRunner) {
    let report = add_path(
        workspaces.clone(),
        id.clone(),
        path,
        git.clone(),
        JobId(0),
        |_| {},
    )
    .await
    .unwrap();
    assert_eq!(report.added.len(), 1, "{report:?}");
}

fn active(workspaces: &Workspaces) -> WorkspaceView {
    workspaces.view(&workspaces.active().unwrap()).unwrap()
}

/// Each repo's id and path, and each group's name and members.
type Shape = (Vec<(String, String)>, Vec<(String, Vec<String>)>);

fn shape(view: &WorkspaceView) -> Shape {
    let repos = view
        .repos
        .iter()
        .map(|repo| (repo.id.0.clone(), repo.path.clone()))
        .collect();
    let groups = view
        .groups
        .iter()
        .map(|group| {
            let members = group.repos.iter().map(|repo| repo.0.clone()).collect();
            (group.name.clone(), members)
        })
        .collect();
    (repos, groups)
}

/// A workspace "Platform" with repos nested in, beside and above the
/// export folder `team/share`, two groups, a layout and an offer answer.
async fn platform(world: &World) -> (Arc<Workspaces>, WorkspaceId) {
    let git = world.git();
    world.repo("team/share/api", "https://example.com/api.git");
    world.repo("team/share/back/svc", "https://example.com/svc.git");
    world.repo("team/web", "https://example.com/web.git");
    world.repo("other/tool", "https://example.com/tool.git");
    let workspaces = world.workspaces();
    let id = workspaces.create("Platform").unwrap();
    for path in [
        "team/share/api",
        "team/share/back/svc",
        "team/web",
        "other/tool",
    ] {
        add(&workspaces, &id, world.path(path), &git).await;
    }
    let ids = |names: &[&str]| -> Vec<RepoId> {
        names
            .iter()
            .map(|name| RepoId((*name).to_owned()))
            .collect()
    };
    workspaces
        .create_group(&id, "Backend", &ids(&["r2", "r1"]))
        .unwrap();
    workspaces
        .create_group(&id, "Everything", &ids(&["r1", "r2", "r3", "r4"]))
        .unwrap();
    workspaces
        .set_layout(
            &id,
            gitcroc_core::PaneLayout {
                tree: Some(300),
                ..Default::default()
            },
        )
        .unwrap();
    workspaces
        .decline_fsmonitor(&id, &RepoId("r1".to_owned()))
        .unwrap();
    // The user's own AI switch (AI-6), which an export leaves out.
    workspaces.set_ai(&id, false).unwrap();
    (workspaces, id)
}

#[tokio::test]
async fn data_2_export_writes_paths_relative_to_the_file() {
    let world = World::new();
    let (workspaces, id) = platform(&world).await;
    let target = world.path("team/share/platform.toml");

    let report = workspaces.export(&id, &target).unwrap();
    assert_eq!(report.path, target.display().to_string());
    assert_eq!(report.absolute, Vec::<String>::new());
    let text = fs::read_to_string(&target).unwrap();
    assert_eq!(
        text,
        r#"schema = 1
name = "Platform"

[[repo]]
id = "r1"
path = "api"
remote = "https://example.com/api.git"

[[repo]]
id = "r2"
path = "back/svc"
remote = "https://example.com/svc.git"

[[repo]]
id = "r3"
path = "../web"
remote = "https://example.com/web.git"

[[repo]]
id = "r4"
path = "../../other/tool"
remote = "https://example.com/tool.git"

[[group]]
name = "Backend"
repos = ["r2", "r1"]

[[group]]
name = "Everything"
repos = ["r1", "r2", "r3", "r4"]
"#
    );
    // Written atomically: nothing else is left beside it.
    let mut names: Vec<String> = fs::read_dir(world.path("team/share"))
        .unwrap()
        .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    assert_eq!(names, ["api", "back", "platform.toml"]);
}

/// On Windows a repo on another drive stays absolute. The missing repo of
/// an import keeps any absolute path, so no second drive is needed.
#[cfg(windows)]
#[tokio::test]
async fn data_2_export_keeps_another_drive_absolute() {
    let world = World::new();
    let source = world.path("in/w.toml");
    fs::create_dir_all(source.parent().unwrap()).unwrap();
    let drive = if world.path("").starts_with("Z:") {
        "Y:"
    } else {
        "Z:"
    };
    let other = format!("{drive}/nowhere/api");
    fs::write(
        &source,
        format!("schema = 1\nname = \"W\"\n\n[[repo]]\nid = \"r1\"\npath = \"{other}\"\n"),
    )
    .unwrap();
    let workspaces = world.workspaces();
    let imported = workspaces.import(&source, &world.git()).unwrap();

    let target = world.path("out/w.toml");
    fs::create_dir_all(target.parent().unwrap()).unwrap();
    let report = workspaces.export(&imported.workspace_id, &target).unwrap();
    let absolute = format!(r"{drive}\nowhere\api");
    assert_eq!(report.absolute, std::slice::from_ref(&absolute));
    let written: toml::Table = fs::read_to_string(&target).unwrap().parse().unwrap();
    assert_eq!(written["repo"][0]["path"].as_str(), Some(absolute.as_str()));
}

#[tokio::test]
async fn data_2_export_then_import_round_trips_repos_groups_and_ids() {
    let world = World::new();
    let (workspaces, id) = platform(&world).await;
    workspaces
        .export(&id, &world.path("team/share/platform.toml"))
        .unwrap();
    let exported = shape(&workspaces.view(&id).unwrap());

    // Another machine: the same layout under another folder.
    let copy = World::new();
    for path in [
        "team/share/api",
        "team/share/back/svc",
        "team/web",
        "other/tool",
    ] {
        copy.repo(path, "https://example.com/unused.git");
    }
    let file = copy.path("team/share/platform.toml");
    fs::copy(world.path("team/share/platform.toml"), &file).unwrap();
    let imported = copy.workspaces();
    let report = imported.import(&file, &copy.git()).unwrap();

    assert_eq!(report.missing, Vec::<RepoId>::new());
    assert_eq!(imported.active(), Some(report.workspace_id.clone()));
    let view = active(&imported);
    assert_ne!(view.id, id);
    assert_eq!(view.name, "Platform");
    let (repos, groups) = shape(&view);
    let moved: Vec<(String, String)> = exported
        .0
        .iter()
        .map(|(repo, path)| {
            let from = world.path("").display().to_string();
            let to = copy.path("").display().to_string();
            (repo.clone(), path.replacen(&from, &to, 1))
        })
        .collect();
    assert_eq!(repos, moved);
    assert_eq!(groups, exported.1);
    // The file's remotes are kept, not the clones' own.
    assert_eq!(
        view.repos[0].remote.as_deref(),
        Some("https://example.com/api.git")
    );
    // The layout, the AI switch and the offer answers stay with the user
    // who exported.
    assert_eq!(view.layout, gitcroc_core::PaneLayout::default());
    assert!(view.ai);
    assert!(
        !imported
            .fsmonitor_declined(&view.id, &RepoId("r1".to_owned()))
            .unwrap()
    );
}

#[tokio::test]
async fn data_2_import_lists_missing_repos_and_clones_them_from_remote() {
    let world = World::new();
    let api = world.bare("remotes/api.git");
    let svc = world.bare("remotes/svc.git");
    world.repo("team/web", "https://example.com/web.git");
    let file = world.path("team/platform.toml");
    fs::write(
        &file,
        format!(
            r#"schema = 1
name = "Platform"

[[repo]]
id = "r1"
path = "api"
remote = "{api}"

[[repo]]
id = "r7"
path = "nested/deeper/svc"
remote = "{svc}"

[[repo]]
id = "r3"
path = "./web"

[[repo]]
id = "r4"
path = "lost"

[[group]]
name = "Backend"
repos = ["r7", "r1"]
"#
        ),
    )
    .unwrap();
    let workspaces = world.workspaces();
    let git = world.git();
    let report = workspaces.import(&file, &git).unwrap();
    let id = report.workspace_id.clone();
    let ids = |names: &[&str]| -> Vec<RepoId> {
        names
            .iter()
            .map(|name| RepoId((*name).to_owned()))
            .collect()
    };
    assert_eq!(report.missing, ids(&["r1", "r7", "r4"]));

    let missing = workspaces.missing_repos(&id).unwrap();
    let listed: Vec<(String, Option<String>, PathBuf, String)> = missing
        .iter()
        .map(|repo| {
            let parent = repo.parent.clone();
            (
                repo.repo_id.0.clone(),
                repo.remote.clone(),
                parent,
                repo.folder.clone(),
            )
        })
        .collect();
    assert_eq!(
        listed,
        [
            (
                "r1".to_owned(),
                Some(api.clone()),
                world.path("team"),
                "api".to_owned()
            ),
            (
                "r7".to_owned(),
                Some(svc.clone()),
                world.path("team/nested/deeper"),
                "svc".to_owned()
            ),
            ("r4".to_owned(), None, world.path("team"), "lost".to_owned()),
        ]
    );
    // The parent of a missing repo may not exist yet: the clone makes it.
    assert_eq!(
        clone_target_check(world.path("team/nested/deeper"), "svc".into(), true).await,
        None
    );

    let summaries = Arc::new(Summaries::load(SummaryCache::none(), &workspaces));
    let jobs = Jobs::new(workspaces.clone(), summaries, Arc::new(NoEvents));
    let clone = |repo: &str, url: &str, parent: PathBuf, name: &str| {
        clone_start(
            workspaces.clone(),
            jobs.clone(),
            git.clone(),
            id.clone(),
            CloneRequest {
                url: url.to_owned(),
                parent,
                name: name.to_owned(),
                repo: Some(RepoId(repo.to_owned())),
                group: None,
            },
        )
    };
    // Both at once; one into a folder the user picked instead.
    let first = clone("r1", &api, world.path("team"), "api").await.unwrap();
    let second = clone("r7", &svc, world.path("team/nested/deeper"), "svc-x")
        .await
        .unwrap();
    let first: CloneReport = first.ended.await.unwrap();
    let second: CloneReport = second.ended.await.unwrap();
    assert_eq!(first.message, None);
    assert_eq!(second.message, None);
    assert_eq!(first.repo.unwrap().id, RepoId("r1".to_owned()));
    assert_eq!(second.repo.unwrap().id, RepoId("r7".to_owned()));

    let view = workspaces.view(&id).unwrap();
    let (repos, groups) = shape(&view);
    let path = |relative: &str| world.path(relative).display().to_string();
    assert_eq!(
        repos,
        [
            ("r1".to_owned(), path("team/api")),
            ("r7".to_owned(), path("team/nested/deeper/svc-x")),
            ("r3".to_owned(), path("team/web")),
            ("r4".to_owned(), path("team/lost")),
        ]
    );
    assert_eq!(
        groups,
        [("Backend".to_owned(), vec!["r7".to_owned(), "r1".to_owned()])]
    );
    assert!(world.path("team/api/a.txt").is_file());
    let still: Vec<RepoId> = workspaces
        .missing_repos(&id)
        .unwrap()
        .into_iter()
        .map(|repo| repo.repo_id)
        .collect();
    assert_eq!(still, ids(&["r4"]));

    // A repo that is no longer missing is not cloned again.
    let refused = clone("r1", &api, world.path("team"), "again").await;
    assert!(
        matches!(refused, Err(CloneError::Refused(_))),
        "{refused:?}"
    );

    // A failed clone keeps the repo missing, with git's message, and
    // removes the folders it created.
    let gone = file_url(&world.path("remotes/gone.git"));
    let failed = clone("r4", &gone, world.path("team/made/for/it"), "lost")
        .await
        .unwrap();
    let failed = failed.ended.await.unwrap();
    assert_eq!(failed.repo, None);
    let message = failed.message.unwrap();
    assert!(
        message.contains("fatal:") || message.contains("gone.git"),
        "{message}"
    );
    assert!(!world.path("team/made").exists());
    assert_eq!(
        workspaces.view(&id).unwrap().repos[3].path,
        path("team/lost")
    );
}

#[tokio::test]
async fn data_1_import_of_a_newer_schema_is_refused_and_creates_nothing() {
    let world = World::new();
    let file = world.path("newer.toml");
    fs::write(&file, "schema = 99\nname = \"Future\"\n").unwrap();
    let workspaces = world.workspaces();
    let before = workspaces.list().unwrap();

    let refused = workspaces.import(&file, &world.git());
    let Err(WorkspaceError::NotImported { reason, .. }) = refused else {
        panic!("{refused:?}");
    };
    assert_eq!(
        reason,
        "it is from a newer gitcroc (schema 99; this version reads schema 1)"
    );
    assert_eq!(workspaces.list().unwrap(), before);
    assert_eq!(workspaces.views().len(), 1);
}

#[tokio::test]
async fn data_2_import_refuses_a_file_that_breaks_the_workspace_rules() {
    let world = World::new();
    let workspaces = world.workspaces();
    let before = workspaces.list().unwrap();
    let cases = [
        (
            "[[repo]]\nid = \"r1\"\npath = \"a\"\n\n[[repo]]\nid = \"r1\"\npath = \"b\"\n",
            "the repo id r1 is listed twice".to_owned(),
        ),
        (
            "[[repo]]\nid = \"r1\"\npath = \"a\"\n\n[[repo]]\nid = \"r2\"\npath = \"x/../a\"\n",
            format!(
                "the repos r1 and r2 have the same folder {}",
                world.path("a").display()
            ),
        ),
        (
            "[[group]]\nname = \"G\"\n\n[[group]]\nname = \"g\"\n",
            "a group named \"G\" exists already".to_owned(),
        ),
        (
            "[[group]]\nname = \"Ungrouped\"\n",
            "\"Ungrouped\" is the name of the repos in no group".to_owned(),
        ),
        (
            "[[group]]\nname = \"G\"\nrepos = [\"r9\"]\n",
            "the group \"G\" names the repo id r9, which the file does not list".to_owned(),
        ),
    ];
    let file = world.path("bad.toml");
    for (body, why) in cases {
        fs::write(&file, format!("schema = 1\nname = \"Bad\"\n\n{body}")).unwrap();
        let refused = workspaces.import(&file, &world.git());
        match refused {
            Err(WorkspaceError::NotImported { reason, .. }) => assert_eq!(reason, why),
            other => panic!("{body}: {other:?}"),
        }
    }
    assert_eq!(workspaces.list().unwrap(), before);
}

#[tokio::test]
async fn data_2_an_imported_name_that_is_taken_gets_2_appended() {
    let world = World::new();
    let workspaces = world.workspaces();
    workspaces.create("Platform").unwrap();
    let file = world.path("platform.toml");
    fs::write(&file, "schema = 1\nname = \"platform\"\n").unwrap();

    workspaces.import(&file, &world.git()).unwrap();
    assert_eq!(active(&workspaces).name, "platform (2)");
    workspaces.import(&file, &world.git()).unwrap();
    assert_eq!(active(&workspaces).name, "platform (3)");
}

#[tokio::test]
async fn grp_2_a_clone_joins_an_existing_or_a_new_group() {
    let world = World::new();
    let url = world.bare("remotes/lib.git");
    let workspaces = world.workspaces();
    let id = workspaces.views()[0].id.clone();
    let backend = workspaces.create_group(&id, "Backend", &[]).unwrap();
    let summaries = Arc::new(Summaries::load(SummaryCache::none(), &workspaces));
    let jobs = Jobs::new(workspaces.clone(), summaries, Arc::new(NoEvents));
    fs::create_dir_all(world.path("clones")).unwrap();
    let clone = |name: &str, group: CloneGroup| {
        clone_start(
            workspaces.clone(),
            jobs.clone(),
            world.git(),
            id.clone(),
            CloneRequest {
                url: url.clone(),
                parent: world.path("clones"),
                name: name.to_owned(),
                repo: None,
                group: Some(group),
            },
        )
    };

    let existing = clone("one", CloneGroup::Existing { group_id: backend })
        .await
        .unwrap();
    let existing = existing.ended.await.unwrap();
    assert_eq!(existing.message, None);
    let fresh = clone(
        "two",
        CloneGroup::New {
            name: " Libs ".to_owned(),
        },
    )
    .await
    .unwrap();
    let fresh = fresh.ended.await.unwrap();
    assert_eq!(fresh.message, None);
    let (_, groups) = shape(&workspaces.view(&id).unwrap());
    assert_eq!(
        groups,
        [
            ("Backend".to_owned(), vec!["r1".to_owned()]),
            ("Libs".to_owned(), vec!["r2".to_owned()]),
        ]
    );

    // A new name another group has, ignoring case, is refused before git
    // runs (GRP-2).
    let refused = clone(
        "three",
        CloneGroup::New {
            name: "backend".to_owned(),
        },
    )
    .await;
    let Err(CloneError::Workspace(WorkspaceError::GroupNameTaken(name))) = refused else {
        panic!("{refused:?}");
    };
    assert_eq!(name, "Backend");
    assert!(!world.path("clones/three").exists());
    let ended: Vec<Outcome> = jobs
        .all_views()
        .into_iter()
        .filter_map(|job| job.outcome)
        .collect();
    assert_eq!(ended, [Outcome::Succeeded, Outcome::Succeeded]);
}
