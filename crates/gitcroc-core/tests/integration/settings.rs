//! Loading and saving `settings.toml` (DATA-1).

use std::fs;
use std::path::{Path, PathBuf};

use gitcroc_core::{
    Appearance, ChangesSettings, CodeTheme, ConfigDir, ConfigWarning, Context, DiffMode,
    FileListMode, GroupSettings, SETTINGS_FILE, Settings, SettingsError, SettingsStore,
    SettingsView, Theme,
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
                theme: Theme::System,
                appearance: Appearance {
                    code_theme_light: CodeTheme::GithubLight,
                    code_theme_dark: CodeTheme::GithubDark,
                    ui_font: None,
                    ui_font_size: 13,
                    code_font: None,
                    code_font_size: 13,
                },
                groups: GroupSettings {
                    group_parallelism: 8,
                    always_show_preflight: false,
                },
                changes: ChangesSettings::default(),
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
    assert!(text.starts_with("schema = 2\ntheme = \"dark\"\n"), "{text}");
    assert_eq!(SettingsStore::load(&config).view(), view);
}

#[test]
fn ui_17_diff_view_settings_are_remembered() {
    let config = TempConfig::new();
    let store = SettingsStore::load(&config);
    assert_eq!(
        store.view().settings.changes,
        ChangesSettings {
            diff_mode: DiffMode::Inline,
            file_list: FileListMode::Flat,
            ignore_whitespace: false,
            wrap_lines: false,
            context: Context::Three,
        }
    );

    let changes = ChangesSettings {
        diff_mode: DiffMode::SideBySide,
        file_list: FileListMode::Tree,
        ignore_whitespace: true,
        wrap_lines: true,
        context: Context::WholeFile,
    };
    let view = store.set_changes(changes).unwrap();
    assert_eq!(view.settings.changes, changes);
    let text = fs::read_to_string(config.file()).unwrap();
    assert!(
        text.contains("[changes]\ndiff_mode = \"side_by_side\"\n"),
        "{text}"
    );
    assert_eq!(
        SettingsStore::load(&config).view().settings.changes,
        changes
    );
}

#[test]
fn ui_17_a_file_without_diff_settings_gets_the_defaults() {
    // Written before the changes pane existed, or with only some keys.
    for (text, wrap) in [
        ("schema = 1\ntheme = \"dark\"\n", false),
        ("schema = 1\n[changes]\nwrap_lines = true\n", true),
    ] {
        let config = TempConfig::with_file(text);
        let view = SettingsStore::load(&config).view();
        assert!(!view.read_only, "{text:?}");
        assert_eq!(view.settings.changes.wrap_lines, wrap, "{text:?}");
        assert_eq!(
            view.settings.changes.diff_mode,
            DiffMode::Inline,
            "{text:?}"
        );
    }
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
            supported: 2,
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

#[test]
fn data_1_schema_1_is_migrated_keeping_the_theme() {
    let config =
        TempConfig::with_file("schema = 1\ntheme = \"dark\"\n[changes]\nwrap_lines = true\n");
    let store = SettingsStore::load(&config);
    let view = store.view();
    assert_eq!(view.warning, None);
    assert_eq!(view.settings.theme, Theme::Dark);
    assert!(view.settings.changes.wrap_lines);
    assert_eq!(view.settings.appearance, Appearance::default());

    // The next save writes schema 2 with the new keys.
    store.set_changes(view.settings.changes).unwrap();
    let text = fs::read_to_string(config.file()).unwrap();
    assert!(text.starts_with("schema = 2\ntheme = \"dark\"\n"), "{text}");
    assert!(
        text.contains("code_theme_light = \"github_light\"\ncode_theme_dark = \"github_dark\"\n"),
        "{text}"
    );
    assert!(text.contains("ui_font_size = 13\n"), "{text}");
}

#[test]
fn ui_24_code_themes_and_fonts_are_remembered() {
    let config = TempConfig::new();
    let store = SettingsStore::load(&config);
    let appearance = Appearance {
        code_theme_light: CodeTheme::SolarizedLight,
        code_theme_dark: CodeTheme::Dracula,
        ui_font: Some("Inter".to_owned()),
        ui_font_size: 14,
        code_font: Some("JetBrains Mono".to_owned()),
        code_font_size: 12,
    };
    let view = store.set_appearance(appearance.clone()).unwrap();
    assert_eq!(view.settings.appearance, appearance);
    let text = fs::read_to_string(config.file()).unwrap();
    assert!(text.contains("code_font = \"JetBrains Mono\"\n"), "{text}");
    assert_eq!(
        SettingsStore::load(&config).view().settings.appearance,
        appearance
    );
}

#[test]
fn ui_26_font_sizes_stay_in_range_and_blank_fonts_are_the_system_font() {
    let config = TempConfig::new();
    let store = SettingsStore::load(&config);
    let view = store
        .set_appearance(Appearance {
            ui_font: Some("  ".to_owned()),
            ui_font_size: 2,
            code_font: Some(" Fira Code ".to_owned()),
            code_font_size: 99,
            ..Appearance::default()
        })
        .unwrap();
    let appearance = view.settings.appearance;
    assert_eq!(appearance.ui_font, None);
    assert_eq!(appearance.ui_font_size, 8);
    assert_eq!(appearance.code_font.as_deref(), Some("Fira Code"));
    assert_eq!(appearance.code_font_size, 32);

    // A hand-edited file is held to the same limits.
    let config = TempConfig::with_file(
        "schema = 2\nui_font_size = 0\ncode_font_size = 40\ncode_font = \"\"\n",
    );
    let view = SettingsStore::load(&config).view();
    assert_eq!(view.warning, None);
    assert_eq!(view.settings.appearance.ui_font_size, 8);
    assert_eq!(view.settings.appearance.code_font_size, 32);
    assert_eq!(view.settings.appearance.code_font, None);
}

#[test]
fn group_settings_are_remembered_and_held_to_their_limits() {
    let config = TempConfig::new();
    let store = SettingsStore::load(&config);
    let groups = GroupSettings {
        group_parallelism: 3,
        always_show_preflight: true,
    };
    store.set_groups(groups).unwrap();
    let text = fs::read_to_string(config.file()).unwrap();
    assert!(text.contains("group_parallelism = 3\n"), "{text}");
    assert!(text.contains("always_show_preflight = true\n"), "{text}");
    assert_eq!(SettingsStore::load(&config).view().settings.groups, groups);

    for refused in [0, 33] {
        let result = store.set_groups(GroupSettings {
            group_parallelism: refused,
            ..groups
        });
        assert!(
            matches!(result, Err(SettingsError::Parallelism)),
            "{refused}"
        );
    }
    assert_eq!(store.view().settings.groups, groups);

    // A hand-edited file is held to the same limits.
    let config = TempConfig::with_file("schema = 2\ngroup_parallelism = 99\n");
    let view = SettingsStore::load(&config).view();
    assert_eq!(view.settings.groups.group_parallelism, 32);
}
