//! The workspace file and adding and removing repos (GRP-1, GRP-3, PLAT-2,
//! DATA-1).

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use gitcroc_core::{
    AddReport, ColumnLayout, ColumnOrder, ConfigDir, ConfigWarning, DEFAULT_WORKSPACE_NAME, Dock,
    GitRepo, GitRunner, Head, HistoryColumn, JobId, JobProgress, PaneLayout, RepoId, StatusReader,
    WORKSPACES_DIR, WorkspaceError, WorkspaceId, Workspaces, add_path, repo_summary,
};
use repo_gen::{Fixture, Repo};

/// A config dir inside a temporary dir.
struct TempConfig {
    _root: tempfile::TempDir,
    dir: PathBuf,
}

impl TempConfig {
    fn new() -> Self {
        let root = tempfile::tempdir().unwrap();
        let dir = root.path().join("gitcroc");
        Self { _root: root, dir }
    }

    fn workspaces(&self) -> PathBuf {
        self.dir.join(WORKSPACES_DIR)
    }

    /// The workspace files, by file name.
    fn files(&self) -> BTreeMap<String, String> {
        fs::read_dir(self.workspaces())
            .unwrap()
            .map(|entry| {
                let entry = entry.unwrap();
                let name = entry.file_name().to_string_lossy().into_owned();
                (name, fs::read_to_string(entry.path()).unwrap())
            })
            .collect()
    }

    fn write(&self, id: &str, contents: &str) -> PathBuf {
        fs::create_dir_all(self.workspaces()).unwrap();
        let path = self.workspaces().join(format!("{id}.toml"));
        fs::write(&path, contents).unwrap();
        path
    }
}

impl ConfigDir for TempConfig {
    fn config_dir(&self) -> Option<PathBuf> {
        Some(self.dir.clone())
    }
}

/// A fresh config dir with its workspace open.
struct Setup {
    config: TempConfig,
    store: Arc<Workspaces>,
    id: WorkspaceId,
}

impl Setup {
    fn new() -> Self {
        let config = TempConfig::new();
        let store = Arc::new(Workspaces::load(&config));
        let id = store.views()[0].id.clone();
        Self { config, store, id }
    }

    async fn add(&self, path: impl Into<PathBuf>) -> AddReport {
        add_path(
            self.store.clone(),
            self.id.clone(),
            path.into(),
            git(),
            JobId(1),
            |_| {},
        )
        .await
        .unwrap()
    }

    fn file(&self) -> String {
        fs::read_to_string(self.config.workspaces().join(format!("{}.toml", self.id.0))).unwrap()
    }

    fn repo_paths(&self) -> Vec<String> {
        let view = self.store.view(&self.id).unwrap();
        view.repos.into_iter().map(|repo| repo.path).collect()
    }
}

fn git() -> GitRunner {
    GitRunner::new(PathBuf::from("git"))
}

/// The path as the workspace stores it.
fn canonical(path: &Path) -> String {
    dunce::canonicalize(path).unwrap().display().to_string()
}

fn repo_at(path: impl Into<PathBuf>) -> Repo {
    let repo = Repo::init(path).unwrap();
    repo.commit_file("README.md", "hello\n", "First").unwrap();
    repo
}

fn bare_at(path: &Path) {
    fs::create_dir_all(path).unwrap();
    Repo::open(path).git(["init", "--quiet", "--bare"]).unwrap();
}

fn paths(report: &[gitcroc_core::RepoView]) -> Vec<String> {
    report.iter().map(|repo| repo.path.clone()).collect()
}

/// Every file under `path` with its contents.
fn snapshot(path: &Path) -> BTreeMap<PathBuf, Vec<u8>> {
    let mut files = BTreeMap::new();
    let mut pending = vec![path.to_owned()];
    while let Some(folder) = pending.pop() {
        for entry in fs::read_dir(&folder).unwrap() {
            let entry = entry.unwrap();
            let kind = entry.file_type().unwrap();
            if kind.is_dir() {
                pending.push(entry.path());
            } else {
                files.insert(entry.path(), fs::read(entry.path()).unwrap());
            }
        }
    }
    files
}

#[tokio::test]
async fn grp_1_adding_a_repo_twice_is_a_no_op() {
    let fixture = Fixture::new().unwrap();
    let repo = repo_at(fixture.path().join("app"));
    let setup = Setup::new();

    let first = setup.add(repo.path()).await;
    assert_eq!(paths(&first.added), [canonical(repo.path())]);
    assert_eq!(first.added[0].id, RepoId("r1".to_owned()));
    assert_eq!(first.added[0].name, "app");
    let file = setup.file();

    #[cfg_attr(not(unix), allow(unused_mut))]
    let mut spellings = vec![
        repo.path().to_owned(),
        // A trailing separator.
        PathBuf::from(format!(
            "{}{}",
            repo.path().display(),
            std::path::MAIN_SEPARATOR
        )),
        // A `..` segment.
        repo.path().join("..").join("app"),
    ];
    #[cfg(unix)]
    {
        let link = fixture.path().join("link-to-app");
        std::os::unix::fs::symlink(repo.path(), &link).unwrap();
        spellings.push(link);
    }
    for spelling in spellings {
        let again = setup.add(&spelling).await;
        assert_eq!(again.added, [], "{}", spelling.display());
        assert_eq!(again.already_present, first.added, "{}", spelling.display());
        assert_eq!(again.refused, [], "{}", spelling.display());
    }
    assert_eq!(setup.repo_paths(), [canonical(repo.path())]);
    assert_eq!(setup.file(), file);

    // A scan that finds it again does not add it either.
    let scan = setup.add(fixture.path()).await;
    assert_eq!(scan.added, []);
    assert_eq!(scan.already_present, first.added);
}

#[tokio::test]
async fn grp_3_removing_a_repo_leaves_the_disk_untouched() {
    let fixture = Fixture::new().unwrap();
    let repo = repo_at(fixture.path().join("app"));
    repo.write("dirty.txt", "not committed\n").unwrap();
    let other = repo_at(fixture.path().join("other"));
    let setup = Setup::new();
    setup.add(fixture.path()).await;
    let before = snapshot(fixture.path());

    let view = setup
        .store
        .remove_repo(&setup.id, &RepoId("r1".to_owned()))
        .unwrap();
    assert_eq!(snapshot(fixture.path()), before);
    assert_eq!(paths(&view.repos), [canonical(other.path())]);
    assert!(!setup.file().contains("\"r1\""), "{}", setup.file());

    // Its id is not given to the next repo.
    let again = setup.add(repo.path()).await;
    assert_eq!(again.added[0].id, RepoId("r3".to_owned()));

    let missing = setup.store.remove_repo(&setup.id, &RepoId("r1".to_owned()));
    assert!(matches!(missing, Err(WorkspaceError::UnknownRepo(_))));
}

/// Whether the file system holding `dir` ignores case in names.
fn ignores_case(dir: &Path) -> bool {
    fs::write(dir.join("CaseProbe"), "").unwrap();
    dir.join("caseprobe").exists()
}

#[tokio::test]
async fn plat_2_case_insensitive_paths_are_one_repo() {
    let fixture = Fixture::new().unwrap();
    if !ignores_case(fixture.path()) {
        eprintln!("skipped: the temporary dir's file system is case-sensitive");
        return;
    }
    let repo = repo_at(fixture.path().join("Repo"));
    let setup = Setup::new();
    let first = setup.add(repo.path()).await;
    assert_eq!(first.added.len(), 1);

    let lower = setup.add(fixture.path().join("repo")).await;
    assert_eq!(lower.added, []);
    assert_eq!(lower.already_present, first.added);
    let upper = setup.add(fixture.path().join("REPO")).await;
    assert_eq!(upper.already_present, first.added);
    assert_eq!(setup.repo_paths().len(), 1);
}

#[tokio::test]
async fn plat_2_windows_long_paths() {
    if !cfg!(windows) {
        eprintln!("skipped: long paths are a Windows limit");
        return;
    }
    let fixture = Fixture::new().unwrap();
    let mut path = fixture.path().to_owned();
    while path.as_os_str().len() <= 260 {
        path.push("a-folder-name-of-forty-characters-long-x");
    }
    // Built in a short folder and moved: git for Windows cannot change into
    // a folder this long, even with `core.longpaths`. std handles it.
    let short = repo_at(fixture.path().join("short"));
    fs::create_dir_all(&path).unwrap();
    let repo = path.join("repo");
    fs::rename(short.path(), &repo).unwrap();
    let setup = Setup::new();
    let report = setup.add(&repo).await;
    assert_eq!(report.refused, []);
    assert_eq!(report.added.len(), 1);
    assert!(report.added[0].path.len() > 260, "{}", report.added[0].path);

    let stored = setup
        .store
        .repo_path(&setup.id, &report.added[0].id)
        .unwrap();
    let reader = StatusReader::new(GitRepo::open(&stored).unwrap(), git());
    let summary = repo_summary(reader).await.unwrap();
    assert!(matches!(summary.head, Head::Branch { name, .. } if name == "main"));
}

#[tokio::test]
async fn plat_2_non_ascii_paths() {
    let fixture = Fixture::new().unwrap();
    let repo = repo_at(fixture.path().join("Ünïcødé folder").join("répo ü"));
    let setup = Setup::new();
    let report = setup.add(fixture.path()).await;
    assert_eq!(report.refused, []);
    assert_eq!(paths(&report.added), [canonical(repo.path())]);
    assert_eq!(report.added[0].name, "répo ü");

    // The path survives the file: a fresh load finds the same repo.
    let reloaded = Workspaces::load(&setup.config);
    let view = reloaded.view(&setup.id).unwrap();
    assert_eq!(view.repos, report.added);
    let stored = reloaded.repo_path(&setup.id, &view.repos[0].id).unwrap();
    let reader = StatusReader::new(GitRepo::open(&stored).unwrap(), git());
    let summary = repo_summary(reader).await.unwrap();
    assert!(!summary.dirty);
}

/// Linux only: macOS and Windows file systems refuse such names.
#[cfg(target_os = "linux")]
#[tokio::test]
async fn plat_2_a_path_that_is_not_unicode_is_refused() {
    use std::ffi::OsStr;
    use std::os::unix::ffi::OsStrExt;

    let fixture = Fixture::new().unwrap();
    let bad = fixture.path().join(OsStr::from_bytes(b"caf\xe9"));
    repo_at(&bad);
    let good = repo_at(fixture.path().join("good"));
    let setup = Setup::new();
    let report = setup.add(fixture.path()).await;
    assert_eq!(paths(&report.added), [canonical(good.path())]);
    assert_eq!(report.refused.len(), 1, "{report:?}");
    assert!(
        report.refused[0].reason.contains("not valid Unicode"),
        "{report:?}"
    );
}

#[tokio::test]
async fn grp_1_scan_finds_repos_at_depths_1_to_4() {
    let fixture = Fixture::new().unwrap();
    let root = fixture.path();
    let one = repo_at(root.join("one"));
    let two = repo_at(root.join("a").join("two"));
    let three = repo_at(root.join("a").join("b").join("three"));
    let four = repo_at(root.join("a").join("b").join("c").join("four"));
    repo_at(root.join("a").join("b").join("c").join("d").join("five"));
    // Inside a found repo: not looked for.
    repo_at(one.path().join("vendor").join("nested"));
    fs::create_dir_all(root.join("empty").join("folder")).unwrap();

    let setup = Setup::new();
    let report = setup.add(root).await;
    let mut expected = vec![
        canonical(one.path()),
        canonical(two.path()),
        canonical(three.path()),
        canonical(four.path()),
    ];
    expected.sort();
    assert_eq!(paths(&report.added), expected);
    assert_eq!(report.already_present, []);
    assert_eq!(report.refused, []);
    let ids: Vec<&str> = report.added.iter().map(|repo| repo.id.0.as_str()).collect();
    assert_eq!(ids, ["r1", "r2", "r3", "r4"]);
}

#[tokio::test]
async fn grp_1_scan_refuses_bare_repos_and_skips_symlinks() {
    let fixture = Fixture::new().unwrap();
    let root = fixture.path().join("root");
    let repo = repo_at(root.join("app"));
    bare_at(&root.join("server.git"));
    // Inside the bare repo's folder: not looked for.
    repo_at(root.join("server.git").join("hidden"));
    #[cfg(unix)]
    {
        let outside = repo_at(fixture.path().join("outside"));
        std::os::unix::fs::symlink(outside.path(), root.join("link")).unwrap();
    }

    let setup = Setup::new();
    let report = setup.add(&root).await;
    assert_eq!(paths(&report.added), [canonical(repo.path())]);
    assert_eq!(report.refused.len(), 1, "{report:?}");
    assert_eq!(report.refused[0].path, canonical(&root.join("server.git")));
    assert!(report.refused[0].reason.contains("bare"), "{report:?}");

    // Chosen directly, a bare repo is refused too.
    let direct = setup.add(root.join("server.git")).await;
    assert_eq!(direct.added, []);
    assert!(direct.refused[0].reason.contains("bare"), "{direct:?}");
}

#[cfg(unix)]
#[tokio::test]
async fn grp_1_scan_survives_an_unreadable_folder() {
    use std::os::unix::fs::PermissionsExt;

    let fixture = Fixture::new().unwrap();
    let root = fixture.path().join("root");
    let repo = repo_at(root.join("app"));
    let locked = root.join("locked");
    repo_at(locked.join("inside"));
    fs::set_permissions(&locked, fs::Permissions::from_mode(0o000)).unwrap();
    if fs::read_dir(&locked).is_ok() {
        fs::set_permissions(&locked, fs::Permissions::from_mode(0o755)).unwrap();
        eprintln!("skipped: running as a user that can read any folder");
        return;
    }

    let setup = Setup::new();
    let report = setup.add(&root).await;
    fs::set_permissions(&locked, fs::Permissions::from_mode(0o755)).unwrap();
    assert_eq!(paths(&report.added), [canonical(repo.path())]);
    assert_eq!(report.refused.len(), 1, "{report:?}");
    assert_eq!(report.refused[0].path, canonical(&locked));
    assert!(
        report.refused[0].reason.contains("cannot read"),
        "{report:?}"
    );
}

#[tokio::test]
async fn grp_1_a_folder_without_repos_or_missing_is_reported() {
    let fixture = Fixture::new().unwrap();
    let setup = Setup::new();
    let empty = setup.add(fixture.path()).await;
    assert_eq!(empty.added, []);
    assert!(
        empty.refused[0].reason.contains("no repo found"),
        "{empty:?}"
    );

    let missing = setup.add(fixture.path().join("nope")).await;
    assert!(
        missing.refused[0].reason.contains("cannot open"),
        "{missing:?}"
    );
    assert!(!setup.file().contains("[[repo]]"));
}

#[tokio::test]
async fn grp_1_remote_is_origin_or_the_only_remote() {
    let fixture = Fixture::new().unwrap();
    let origin = repo_at(fixture.path().join("origin"));
    origin
        .git(["remote", "add", "upstream", "https://example.com/u.git"])
        .unwrap();
    origin
        .git(["remote", "add", "origin", "git@example.com:o.git"])
        .unwrap();
    let only = repo_at(fixture.path().join("only"));
    only.git(["remote", "add", "fork", "https://example.com/f.git"])
        .unwrap();
    let two = repo_at(fixture.path().join("two"));
    two.git(["remote", "add", "a", "https://example.com/a.git"])
        .unwrap();
    two.git(["remote", "add", "b", "https://example.com/b.git"])
        .unwrap();
    repo_at(fixture.path().join("none"));

    let setup = Setup::new();
    let report = setup.add(fixture.path()).await;
    let remotes: BTreeMap<String, Option<String>> = report
        .added
        .iter()
        .map(|repo| (repo.name.clone(), repo.remote.clone()))
        .collect();
    assert_eq!(remotes["origin"].as_deref(), Some("git@example.com:o.git"));
    assert_eq!(
        remotes["only"].as_deref(),
        Some("https://example.com/f.git")
    );
    assert_eq!(remotes["two"], None);
    assert_eq!(remotes["none"], None);
    assert!(setup.file().contains("remote = \"git@example.com:o.git\""));
}

#[tokio::test]
async fn arch_7_adding_reports_progress() {
    let fixture = Fixture::new().unwrap();
    repo_at(fixture.path().join("one"));
    repo_at(fixture.path().join("two"));
    let setup = Setup::new();
    let events: Arc<Mutex<Vec<JobProgress>>> = Arc::default();
    let sink = events.clone();
    let report = add_path(
        setup.store.clone(),
        setup.id.clone(),
        fixture.path().to_owned(),
        git(),
        JobId(7),
        move |progress| sink.lock().unwrap().push(progress),
    )
    .await
    .unwrap();
    assert_eq!(report.job_id, JobId(7));
    assert_eq!(report.workspace_id, setup.id);
    let events = events.lock().unwrap();
    assert!(events.len() >= 2, "{events:?}");
    let last = events.last().unwrap();
    assert_eq!((last.job_id, last.done, last.total), (JobId(7), 3, 3));
    assert!(events.iter().all(|event| event.workspace_id == setup.id));
}

#[test]
fn data_1_first_start_creates_my_workspace() {
    let config = TempConfig::new();
    let store = Workspaces::load(&config);
    let views = store.views();
    assert_eq!(views.len(), 1);
    let view = &views[0];
    assert_eq!(view.name, DEFAULT_WORKSPACE_NAME);
    assert!(!view.read_only);
    assert!(uuid::Uuid::parse_str(&view.id.0).is_ok(), "{}", view.id.0);
    let files = config.files();
    assert_eq!(
        files
            .get(&format!("{}.toml", view.id.0))
            .map(String::as_str),
        Some("schema = 1\nname = \"My workspace\"\n")
    );

    // The next start opens the same one.
    assert_eq!(Workspaces::load(&config).views(), views);
}

#[test]
fn data_1_the_most_recently_modified_workspace_opens() {
    let config = TempConfig::new();
    config.write("older", "schema = 1\nname = \"Older\"\n");
    std::thread::sleep(std::time::Duration::from_millis(50));
    let newer = config.write("newer", "schema = 1\nname = \"Newer\"\n");
    let old_time = fs::metadata(config.workspaces().join("older.toml"))
        .unwrap()
        .modified()
        .unwrap();
    assert!(fs::metadata(&newer).unwrap().modified().unwrap() > old_time);

    let views = Workspaces::load(&config).views();
    assert_eq!(views.len(), 1);
    assert_eq!(views[0].id, WorkspaceId("newer".to_owned()));
    assert_eq!(views[0].name, "Newer");
}

#[tokio::test]
async fn data_1_workspace_round_trip_keeps_groups() {
    let fixture = Fixture::new().unwrap();
    let repo = repo_at(fixture.path().join("app"));
    let config = TempConfig::new();
    let text = r#"schema = 1
name = "Platform"

[[repo]]
id = "r1"
path = "/somewhere/api"
remote = "git@example.com:platform/api.git"

[[repo]]
id = "r2"
path = "/somewhere/web"

[[group]]
name = "Backend"
repos = ["r1"]

[[group]]
name = "Everything"
repos = ["r1", "r2"]
"#;
    let path = config.write("ws", text);
    let store = Arc::new(Workspaces::load(&config));
    let id = WorkspaceId("ws".to_owned());
    let view = store.view(&id).unwrap();
    assert_eq!(view.name, "Platform");
    assert!(!view.read_only);
    let names: Vec<&str> = view.repos.iter().map(|repo| repo.name.as_str()).collect();
    assert_eq!(names, ["api", "web"]);

    let report = add_path(
        store.clone(),
        id.clone(),
        repo.path().to_owned(),
        git(),
        JobId(1),
        |_| {},
    )
    .await
    .unwrap();
    assert_eq!(report.added[0].id, RepoId("r3".to_owned()));
    let saved = fs::read_to_string(&path).unwrap();
    let expected = text.replace(
        "\n[[group]]\nname = \"Backend\"",
        &format!(
            "\n[[repo]]\nid = \"r3\"\npath = {}\n\n[[group]]\nname = \"Backend\"",
            toml_string(&canonical(repo.path()))
        ),
    );
    assert_eq!(saved, expected);

    // Removing a repo also takes it out of its groups.
    store.remove_repo(&id, &RepoId("r1".to_owned())).unwrap();
    let saved = fs::read_to_string(&path).unwrap();
    assert!(
        saved.contains("[[group]]\nname = \"Backend\"\nrepos = []\n"),
        "{saved}"
    );
    assert!(
        saved.contains("name = \"Everything\"\nrepos = [\"r2\"]\n"),
        "{saved}"
    );
}

/// `text` as the TOML writer quotes it.
fn toml_string(text: &str) -> String {
    toml::Value::String(text.to_owned()).to_string()
}

#[tokio::test]
async fn data_1_newer_workspace_schema_is_read_only() {
    let fixture = Fixture::new().unwrap();
    let repo = repo_at(fixture.path().join("app"));
    let config = TempConfig::new();
    let text = "schema = 2\nname = \"Future\"\nfuture = \"new\"\n\n[[repo]]\nid = \"r1\"\npath = \"/a/b\"\n";
    let path = config.write("future", text);
    let store = Arc::new(Workspaces::load(&config));
    let id = WorkspaceId("future".to_owned());
    let view = store.view(&id).unwrap();
    assert!(view.read_only);
    assert_eq!(view.name, "Future");
    assert_eq!(view.repos.len(), 1);
    assert_eq!(
        view.warning,
        Some(ConfigWarning::NewerSchema {
            path: path.display().to_string(),
            found: 2,
            supported: 1,
        })
    );

    let added = add_path(
        store.clone(),
        id.clone(),
        repo.path().to_owned(),
        git(),
        JobId(1),
        |_| {},
    )
    .await;
    assert!(matches!(added, Err(WorkspaceError::ReadOnly)));
    let removed = store.remove_repo(&id, &RepoId("r1".to_owned()));
    assert!(matches!(removed, Err(WorkspaceError::ReadOnly)));
    assert_eq!(fs::read_to_string(&path).unwrap(), text);
}

#[test]
fn data_1_corrupt_workspace_is_never_overwritten() {
    let config = TempConfig::new();
    let text = "schema = 1\nname = \"Broken\"\n[[repo]\n";
    let path = config.write("broken", text);
    let store = Workspaces::load(&config);
    let view = store.view(&WorkspaceId("broken".to_owned())).unwrap();
    assert!(view.read_only);
    assert!(matches!(
        view.warning,
        Some(ConfigWarning::Unreadable { .. })
    ));
    let removed = store.remove_repo(&view.id, &RepoId("r1".to_owned()));
    assert!(matches!(removed, Err(WorkspaceError::ReadOnly)));
    assert_eq!(fs::read_to_string(&path).unwrap(), text);
    assert_eq!(config.files().len(), 1);
}

#[test]
fn arch_9_calls_name_their_workspace() {
    let config = TempConfig::new();
    let store = Workspaces::load(&config);
    let other = WorkspaceId("not-open".to_owned());
    assert!(matches!(
        store.view(&other),
        Err(WorkspaceError::UnknownWorkspace(_))
    ));
    assert!(matches!(
        store.remove_repo(&other, &RepoId("r1".to_owned())),
        Err(WorkspaceError::UnknownWorkspace(_))
    ));
}

#[test]
fn ui_19_pane_layout_round_trips_in_workspace_file() {
    let config = TempConfig::new();
    let text =
        "schema = 1\nname = \"Platform\"\n\n[[repo]]\nid = \"r1\"\npath = \"/somewhere/api\"\n";
    let path = config.write("ws", text);
    let store = Workspaces::load(&config);
    let id = WorkspaceId("ws".to_owned());
    // No layout yet: every field is left to the UI's defaults.
    assert_eq!(store.view(&id).unwrap().layout, PaneLayout::default());

    let layout = PaneLayout {
        dock: Some(Dock::Bottom),
        tree: Some(300),
        history: Some(520),
        changes: Some(240),
        columns: None,
    };
    store.set_layout(&id, layout.clone()).unwrap();
    assert_eq!(store.view(&id).unwrap().layout, layout);
    let saved = fs::read_to_string(&path).unwrap();
    assert!(saved.starts_with(text), "{saved}");
    assert!(
        saved
            .ends_with("\n[layout]\ndock = \"bottom\"\ntree = 300\nhistory = 520\nchanges = 240\n"),
        "{saved}"
    );
    assert_eq!(Workspaces::load(&config).view(&id).unwrap().layout, layout);

    // A partial table, as a person might write it, keeps the rest unset.
    config.write("ws", &format!("{text}\n[layout]\ndock = \"right\"\n"));
    let partial = Workspaces::load(&config).view(&id).unwrap();
    assert!(!partial.read_only);
    assert_eq!(
        partial.layout,
        PaneLayout {
            dock: Some(Dock::Right),
            ..PaneLayout::default()
        }
    );
}

#[test]
fn ui_36_history_columns_round_trip_in_workspace_file() {
    use HistoryColumn::{Author, Date, Graph, Hash, Subject};
    let config = TempConfig::new();
    let text =
        "schema = 1\nname = \"Platform\"\n\n[[repo]]\nid = \"r1\"\npath = \"/somewhere/api\"\n";
    let path = config.write("ws", text);
    let store = Workspaces::load(&config);
    let id = WorkspaceId("ws".to_owned());

    let layout = PaneLayout {
        dock: Some(Dock::Right),
        columns: Some(ColumnLayout {
            order: Some(ColumnOrder(vec![Subject, Graph, Hash, Author, Date])),
            graph: Some(96),
            author: None,
            date: Some(150),
            hash: Some(80),
        }),
        ..PaneLayout::default()
    };
    store.set_layout(&id, layout.clone()).unwrap();
    assert_eq!(store.view(&id).unwrap().layout, layout);
    let saved = fs::read_to_string(&path).unwrap();
    assert!(saved.starts_with(text), "{saved}");
    assert!(
        saved.ends_with(
            "\n[layout]\ndock = \"right\"\n\n[layout.columns]\n\
             order = [\"subject\", \"graph\", \"hash\", \"author\", \"date\"]\n\
             graph = 96\ndate = 150\nhash = 80\n"
        ),
        "{saved}"
    );
    assert_eq!(Workspaces::load(&config).view(&id).unwrap().layout, layout);

    // A newer build's column, a repeat and a missing column: the order keeps
    // the known columns once, and the missing ones follow in their default
    // order. The workspace stays writable (DATA-1).
    config.write(
        "ws",
        &format!(
            "{text}\n[layout.columns]\norder = [\"hash\", \"committer\", \"subject\", \"hash\"]\n"
        ),
    );
    let lenient = Workspaces::load(&config).view(&id).unwrap();
    assert!(!lenient.read_only);
    assert_eq!(
        lenient.layout.columns,
        Some(ColumnLayout {
            order: Some(ColumnOrder(vec![Hash, Subject, Graph, Author, Date])),
            ..ColumnLayout::default()
        })
    );
}

#[test]
fn ai_6_workspace_switch_round_trips_in_workspace_file() {
    let config = TempConfig::new();
    let text =
        "schema = 1\nname = \"Platform\"\n\n[[repo]]\nid = \"r1\"\npath = \"/somewhere/api\"\n";
    let path = config.write("ws", text);
    let store = Workspaces::load(&config);
    let id = WorkspaceId("ws".to_owned());
    // Absent, AI is on.
    assert!(store.view(&id).unwrap().ai);

    let view = store.set_ai(&id, false).unwrap();
    assert!(!view.ai);
    let saved = fs::read_to_string(&path).unwrap();
    // A key of its own, so the schema stays 1 and older builds ignore it.
    assert!(
        saved.starts_with("schema = 1\nname = \"Platform\"\nai = false\n"),
        "{saved}"
    );
    assert!(!Workspaces::load(&config).view(&id).unwrap().ai);

    // On again leaves the file as it was.
    assert!(store.set_ai(&id, true).unwrap().ai);
    assert_eq!(fs::read_to_string(&path).unwrap(), text);

    // A read-only workspace keeps its file.
    let future = config.write("future", "schema = 99\nname = \"Future\"\n");
    let store = Workspaces::load(&config);
    store.open(&WorkspaceId("future".to_owned())).unwrap();
    assert!(matches!(
        store.set_ai(&WorkspaceId("future".to_owned()), false),
        Err(WorkspaceError::ReadOnly)
    ));
    assert_eq!(
        fs::read_to_string(&future).unwrap(),
        "schema = 99\nname = \"Future\"\n"
    );
}

#[test]
fn ui_19_a_read_only_workspace_keeps_its_file() {
    let config = TempConfig::new();
    let text = "schema = 99\nname = \"Future\"\n";
    let path = config.write("future", text);
    let store = Workspaces::load(&config);
    let id = WorkspaceId("future".to_owned());
    let layout = PaneLayout {
        dock: Some(Dock::Bottom),
        ..PaneLayout::default()
    };
    assert!(matches!(
        store.set_layout(&id, layout),
        Err(WorkspaceError::ReadOnly)
    ));
    assert_eq!(fs::read_to_string(&path).unwrap(), text);
}
