//! Several open workspaces (UI-27), their tabs restored on restart (UI-33),
//! and creating, renaming and deleting workspace files (M3-01).

// Integration test crate: helpers outside #[test] fns are test code too (QA-4).
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use gitcroc_core::{
    ConfigDir, ConfigWarning, DEFAULT_WORKSPACE_NAME, GitRunner, JobId, SETTINGS_FILE,
    SettingsStore, Theme, WORKSPACES_DIR, WorkspaceError, WorkspaceId, Workspaces, add_path,
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

    fn file(&self, id: &WorkspaceId) -> PathBuf {
        self.workspaces().join(format!("{}.toml", id.0))
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

    /// The workspaces as a new start would load them.
    fn load(&self) -> Workspaces {
        Workspaces::load(self)
    }
}

impl ConfigDir for TempConfig {
    fn config_dir(&self) -> Option<PathBuf> {
        Some(self.dir.clone())
    }
}

/// The names of the open workspaces, in tab order.
fn tab_names(store: &Workspaces) -> Vec<String> {
    store.views().into_iter().map(|view| view.name).collect()
}

fn active_name(store: &Workspaces) -> Option<String> {
    let tabs = store.tabs();
    let active = tabs.active?;
    tabs.open
        .into_iter()
        .find(|view| view.id == active)
        .map(|view| view.name)
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

#[test]
fn ui_27_opening_an_open_workspace_switches_to_it() {
    let config = TempConfig::new();
    let store = config.load();
    let java = store.create("Java").unwrap();
    store.create("C#").unwrap();
    assert_eq!(active_name(&store).as_deref(), Some("C#"));

    let opened = store.open(&java).unwrap();
    assert!(!opened, "an open workspace is not opened again");
    assert_eq!(tab_names(&store), [DEFAULT_WORKSPACE_NAME, "Java", "C#"]);
    assert_eq!(store.tabs().active, Some(java));
}

#[test]
fn ui_27_a_closed_workspace_opens_as_the_last_tab() {
    let config = TempConfig::new();
    let store = config.load();
    let java = store.create("Java").unwrap();
    store.create("C#").unwrap();
    store.close(&java).unwrap();
    assert_eq!(tab_names(&store), [DEFAULT_WORKSPACE_NAME, "C#"]);

    assert!(store.open(&java).unwrap());
    assert_eq!(tab_names(&store), [DEFAULT_WORKSPACE_NAME, "C#", "Java"]);
    assert_eq!(store.tabs().active, Some(java));

    let unknown = store.open(&WorkspaceId("no-such-file".to_owned()));
    assert!(matches!(unknown, Err(WorkspaceError::NoSuchWorkspace(_))));
    let path = store.open(&WorkspaceId("../settings".to_owned()));
    assert!(matches!(path, Err(WorkspaceError::NoSuchWorkspace(_))));
}

#[test]
fn ui_33_open_tabs_order_and_active_tab_survive_a_restart() {
    let config = TempConfig::new();
    {
        let store = config.load();
        let first = store.tabs().active.unwrap();
        let one = store.create("One").unwrap();
        let two = store.create("Two").unwrap();
        let three = store.create("Three").unwrap();
        store.close(&first).unwrap();
        store.reorder(&[three, one.clone(), two]).unwrap();
        store.activate(&one).unwrap();
        assert_eq!(tab_names(&store), ["Three", "One", "Two"]);
    }
    let store = config.load();
    assert_eq!(tab_names(&store), ["Three", "One", "Two"]);
    assert_eq!(active_name(&store).as_deref(), Some("One"));
    assert!(store.tabs().missing.is_empty());
}

#[test]
fn ui_33_a_missing_workspace_file_is_left_out_with_a_warning() {
    let config = TempConfig::new();
    let gone = {
        let store = config.load();
        let gone = store.create("Gone").unwrap();
        store.create("Kept").unwrap();
        gone
    };
    fs::remove_file(config.file(&gone)).unwrap();

    let store = config.load();
    assert_eq!(tab_names(&store), [DEFAULT_WORKSPACE_NAME, "Kept"]);
    assert_eq!(active_name(&store).as_deref(), Some("Kept"));
    assert_eq!(
        store.tabs().missing,
        [config.file(&gone).display().to_string()]
    );
}

#[test]
fn ui_33_closing_every_tab_restores_none() {
    let config = TempConfig::new();
    {
        let store = config.load();
        let only = store.tabs().active.unwrap();
        store.close(&only).unwrap();
        let tabs = store.tabs();
        assert!(tabs.open.is_empty());
        assert_eq!(tabs.active, None);
    }
    let store = config.load();
    assert!(store.views().is_empty());
    assert_eq!(config.files().len(), 1, "closing never deletes a file");
}

#[test]
fn ui_33_the_session_keeps_the_other_settings() {
    let config = TempConfig::new();
    let settings = Arc::new(SettingsStore::load(&config));
    settings.set_theme(Theme::Dark).unwrap();
    let store = Workspaces::load_with(&config, settings.clone());
    store.create("Java").unwrap();
    // A later settings change keeps the session.
    settings.set_theme(Theme::Light).unwrap();

    let text = fs::read_to_string(config.dir.join(SETTINGS_FILE)).unwrap();
    assert!(text.contains("theme = \"light\""), "{text}");
    assert!(text.contains("[session]"), "{text}");
    let store = config.load();
    assert_eq!(tab_names(&store), [DEFAULT_WORKSPACE_NAME, "Java"]);
    assert_eq!(
        SettingsStore::load(&config).view().settings.theme,
        Theme::Light
    );
}

#[test]
fn ui_27_closing_the_active_tab_activates_its_neighbour() {
    let config = TempConfig::new();
    let store = config.load();
    let first = store.tabs().active.unwrap();
    let two = store.create("Two").unwrap();
    let three = store.create("Three").unwrap();

    store.activate(&two).unwrap();
    store.close(&two).unwrap();
    assert_eq!(store.tabs().active, Some(three.clone()));
    store.close(&three).unwrap();
    assert_eq!(store.tabs().active, Some(first));

    let closed = store.activate(&two);
    assert!(matches!(closed, Err(WorkspaceError::UnknownWorkspace(_))));
    let bad = store.reorder(&[three]);
    assert!(matches!(bad, Err(WorkspaceError::BadOrder)));
}

#[test]
fn ui_27_workspace_names_are_trimmed_unique_and_at_most_64_characters() {
    let config = TempConfig::new();
    let store = config.load();
    let before = config.files();

    let blank = store.create("   ");
    assert!(matches!(blank, Err(WorkspaceError::BlankName)));
    let long = store.create(&"é".repeat(65));
    assert!(matches!(long, Err(WorkspaceError::NameTooLong)));
    let taken = store.create(" MY WORKSPACE ");
    assert!(matches!(taken, Err(WorkspaceError::NameTaken(_))));
    assert_eq!(
        config.files(),
        before,
        "nothing is written for a refused name"
    );

    let longest = store.create(&format!("  {}  ", "é".repeat(64))).unwrap();
    assert_eq!(store.view(&longest).unwrap().name, "é".repeat(64));

    // Another case of its own name is allowed; another's name is not.
    let first = store.views()[0].id.clone();
    store.rename(&first, "my Workspace").unwrap();
    assert_eq!(store.view(&first).unwrap().name, "my Workspace");
    let clash = store.rename(&longest, "MY WORKSPACE");
    assert!(matches!(clash, Err(WorkspaceError::NameTaken(_))));

    // A closed workspace counts and can be renamed.
    store.close(&longest).unwrap();
    let closed_name = store.create(&"É".repeat(64));
    assert!(matches!(closed_name, Err(WorkspaceError::NameTaken(_))));
    store.rename(&longest, "Archive").unwrap();
    let names: Vec<String> = store
        .list()
        .unwrap()
        .into_iter()
        .map(|item| item.name)
        .collect();
    assert_eq!(names, ["Archive", "my Workspace"]);
}

#[test]
fn ui_27_list_shows_every_file_sorted_by_name_with_its_warning() {
    let config = TempConfig::new();
    let store = config.load();
    let beta = store.create("beta").unwrap();
    store.create("Alpha").unwrap();
    store.close(&beta).unwrap();
    fs::write(
        config.workspaces().join("broken.toml"),
        "schema = 1\n[[repo]\n",
    )
    .unwrap();
    fs::write(
        config.workspaces().join("not a workspace.toml"),
        "schema = 1\n",
    )
    .unwrap();

    let list = store.list().unwrap();
    let rows: Vec<(&str, bool)> = list
        .iter()
        .map(|item| (item.name.as_str(), item.open))
        .collect();
    assert_eq!(
        rows,
        [
            ("Alpha", true),
            ("beta", false),
            ("broken", false),
            (DEFAULT_WORKSPACE_NAME, true)
        ]
    );
    assert!(matches!(
        list[2].warning,
        Some(ConfigWarning::Unreadable { .. })
    ));

    // It opens read-only (DATA-1).
    let broken = list[2].id.clone();
    store.open(&broken).unwrap();
    assert!(store.view(&broken).unwrap().read_only);
    let rename = store.rename(&broken, "Fixed");
    assert!(matches!(rename, Err(WorkspaceError::ReadOnly)));
}

#[tokio::test]
async fn grp_3_deleting_a_workspace_removes_only_its_file() {
    let fixture = Fixture::new().unwrap();
    let repo = Repo::init(fixture.path().join("app")).unwrap();
    repo.commit_file("README.md", "hello\n", "First").unwrap();
    repo.write("dirty.txt", "not committed\n").unwrap();
    let config = TempConfig::new();
    let store = Arc::new(config.load());
    let doomed = store.create("Doomed").unwrap();
    add_path(
        store.clone(),
        doomed.clone(),
        fixture.path().to_owned(),
        GitRunner::new(PathBuf::from("git")),
        JobId(1),
        |_| {},
    )
    .await
    .unwrap();
    let before = snapshot(fixture.path());

    let open = store.delete(&doomed, true);
    assert!(matches!(open, Err(WorkspaceError::IsOpen)));
    store.close(&doomed).unwrap();
    let unconfirmed = store.delete(&doomed, false);
    assert!(matches!(unconfirmed, Err(WorkspaceError::NotConfirmed)));
    assert!(config.file(&doomed).is_file());

    store.delete(&doomed, true).unwrap();
    assert!(!config.file(&doomed).exists());
    assert_eq!(config.files().len(), 1);
    assert_eq!(snapshot(fixture.path()), before);
    let again = store.delete(&doomed, true);
    assert!(matches!(again, Err(WorkspaceError::NoSuchWorkspace(_))));
}
