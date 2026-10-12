//! Loading and saving `settings.toml` (DATA-1).

// Integration test crate: helpers outside #[test] fns are test code too (QA-4).
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::fs;
use std::path::{Path, PathBuf};

use gitcroc_core::{
    ConfigDir, ConfigWarning, SETTINGS_FILE, Settings, SettingsError, SettingsStore, SettingsView,
    Theme,
};

/// A config dir inside a temporary dir; it does not exist until a save.
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

    fn with_file(contents: &str) -> Self {
        let config = Self::new();
        fs::create_dir_all(&config.dir).unwrap();
        fs::write(config.file(), contents).unwrap();
        config
    }

    fn file(&self) -> PathBuf {
        self.dir.join(SETTINGS_FILE)
    }
}

impl ConfigDir for TempConfig {
    fn config_dir(&self) -> Option<PathBuf> {
        Some(self.dir.clone())
    }
}

struct NoConfigDir;

impl ConfigDir for NoConfigDir {
    fn config_dir(&self) -> Option<PathBuf> {
        None
    }
}

fn display(path: &Path) -> String {
    path.display().to_string()
}

#[test]
fn data_1_missing_file_gives_defaults() {
    let config = TempConfig::new();
    let store = SettingsStore::load(&config);
    assert_eq!(
        store.view(),
        SettingsView {
            settings: Settings {
                theme: Theme::System
            },
            read_only: false,
            warning: None,
        }
    );
    assert!(!config.file().exists(), "loading must not create the file");
}

#[test]
fn data_1_saved_settings_load_again() {
    let config = TempConfig::new();
    let store = SettingsStore::load(&config);
    let view = store.set_theme(Theme::Dark).unwrap();
    assert_eq!(view.settings.theme, Theme::Dark);
    assert_eq!(store.view(), view);

    let text = fs::read_to_string(config.file()).unwrap();
    assert_eq!(text, "schema = 1\ntheme = \"dark\"\n");
    assert_eq!(SettingsStore::load(&config).view(), view);
}

#[test]
fn data_1_newer_schema_is_read_only() {
    let original = "schema = 99\ntheme = \"light\"\nfuture = true\n";
    let config = TempConfig::with_file(original);
    let store = SettingsStore::load(&config);

    let view = store.view();
    assert_eq!(view.settings.theme, Theme::Light, "known settings are used");
    assert!(view.read_only);
    assert_eq!(
        view.warning,
        Some(ConfigWarning::NewerSchema {
            path: display(&config.file()),
            found: 99,
            supported: 1,
        })
    );

    assert!(matches!(
        store.set_theme(Theme::Dark),
        Err(SettingsError::ReadOnly)
    ));
    assert_eq!(store.view(), view);
    assert_eq!(fs::read_to_string(config.file()).unwrap(), original);
}

#[test]
fn data_1_corrupt_file_is_not_overwritten() {
    for original in [
        "schema = 1\ntheme = \"light\n",
        "schema = 1\ntheme = \"purple\"\n",
        "theme = \"dark\"\n",
        "schema = 0\n",
    ] {
        let config = TempConfig::with_file(original);
        let store = SettingsStore::load(&config);

        let view = store.view();
        assert_eq!(view.settings, Settings::default(), "{original:?}");
        assert!(view.read_only, "{original:?}");
        match &view.warning {
            Some(ConfigWarning::Unreadable { path, reason }) => {
                assert_eq!(path, &display(&config.file()));
                assert!(!reason.is_empty());
            }
            other => panic!("{original:?}: expected Unreadable, got {other:?}"),
        }

        assert!(matches!(
            store.set_theme(Theme::Dark),
            Err(SettingsError::ReadOnly)
        ));
        assert_eq!(fs::read_to_string(config.file()).unwrap(), original);
    }
}

#[test]
fn data_1_no_config_dir_is_read_only() {
    let store = SettingsStore::load(&NoConfigDir);
    assert_eq!(store.path(), None);
    let view = store.view();
    assert_eq!(view.settings, Settings::default());
    assert!(view.read_only);
    assert_eq!(view.warning, Some(ConfigWarning::NoConfigDir));
    assert!(matches!(
        store.set_theme(Theme::Light),
        Err(SettingsError::ReadOnly)
    ));
}

#[test]
fn data_1_missing_keys_take_defaults() {
    let config = TempConfig::with_file("schema = 1\n");
    let view = SettingsStore::load(&config).view();
    assert_eq!(view.settings, Settings::default());
    assert_eq!(view.warning, None);
}
