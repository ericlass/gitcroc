//! App settings, kept in `settings.toml` in the platform config dir (section
//! 10 "Files"). Written atomically; older schemas are migrated on load, and a
//! newer schema or an unreadable file is used read-only and left untouched
//! (DATA-1).

use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard, PoisonError};

use serde::{Deserialize, Serialize};

/// The schema this build reads and writes.
pub const SETTINGS_SCHEMA: u32 = 1;

/// The file name inside the config dir.
pub const SETTINGS_FILE: &str = "settings.toml";

/// Turns a file of schema `n` into one of schema `n + 1`, in place.
type Migration = fn(&mut toml::Table);

/// `MIGRATIONS[n - 1]` migrates schema `n`. None exist yet.
const MIGRATIONS: &[Migration] = &[];

const _: () = assert!(SETTINGS_SCHEMA as usize == MIGRATIONS.len() + 1);

/// Where the app keeps its config files. Tests point it at a temporary dir.
pub trait ConfigDir: Send + Sync {
    /// The app's own config dir, or `None` when the platform has none.
    fn config_dir(&self) -> Option<PathBuf>;
}

/// The platform config dir with `gitcroc/` appended, for example
/// `~/.config/gitcroc` on Linux.
#[derive(Debug, Clone, Copy, Default)]
pub struct PlatformConfigDir;

impl ConfigDir for PlatformConfigDir {
    fn config_dir(&self) -> Option<PathBuf> {
        directories::BaseDirs::new().map(|dirs| dirs.config_dir().join("gitcroc"))
    }
}

/// The app theme (UI-23).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "snake_case")]
pub enum Theme {
    /// Follow the OS light or dark mode, live.
    #[default]
    System,
    Light,
    Dark,
}

/// Every setting. A key missing from the file takes its default (see
/// [`decode`]).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
pub struct Settings {
    pub theme: Theme,
}

/// Why the settings are read-only, shown to the user as a banner. Paths are
/// text for display only (PLAT-2).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum SettingsWarning {
    /// The file at `path` has schema `found`, newer than the `supported` one.
    /// Its known settings are used; it is not written.
    NewerSchema {
        path: String,
        found: u32,
        supported: u32,
    },
    /// The file at `path` could not be read or parsed, for `reason`. The
    /// defaults are used; the file is not written.
    Unreadable { path: String, reason: String },
    /// The platform has no config dir, so nothing can be saved.
    NoConfigDir,
}

/// The settings as the UI sees them.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
pub struct SettingsView {
    pub settings: Settings,
    /// Changes are refused; `warning` says why.
    pub read_only: bool,
    pub warning: Option<SettingsWarning>,
}

/// Errors from changing a setting.
#[derive(Debug, thiserror::Error)]
pub enum SettingsError {
    #[error("the settings are read-only, so the change was not saved")]
    ReadOnly,
    #[error("cannot write {}: {source}", path.display())]
    Write { path: PathBuf, source: io::Error },
    #[error("cannot encode the settings: {0}")]
    Encode(#[from] toml::ser::Error),
}

#[derive(Debug, Default)]
struct Loaded {
    settings: Settings,
    /// Set when the file must not be written.
    warning: Option<SettingsWarning>,
}

/// The loaded settings for the whole app, saved on every change.
#[derive(Debug)]
pub struct SettingsStore {
    /// `None` when there is no config dir.
    path: Option<PathBuf>,
    /// Also held while saving, so saves happen one at a time.
    state: Mutex<Loaded>,
}

impl SettingsStore {
    /// Loads `settings.toml` from `dirs`. Never fails: problems give the
    /// defaults and a warning.
    pub fn load(dirs: &dyn ConfigDir) -> Self {
        let Some(dir) = dirs.config_dir() else {
            return Self {
                path: None,
                state: Mutex::new(Loaded {
                    settings: Settings::default(),
                    warning: Some(SettingsWarning::NoConfigDir),
                }),
            };
        };
        let path = dir.join(SETTINGS_FILE);
        let loaded = match fs::read_to_string(&path) {
            Ok(text) => parse(&path, &text, MIGRATIONS),
            Err(error) if error.kind() == io::ErrorKind::NotFound => Loaded::default(),
            Err(error) => unreadable(&path, error.to_string()),
        };
        Self {
            path: Some(path),
            state: Mutex::new(loaded),
        }
    }

    /// The path of the settings file, if there is a config dir.
    pub fn path(&self) -> Option<&Path> {
        self.path.as_deref()
    }

    pub fn view(&self) -> SettingsView {
        view(&self.lock())
    }

    /// Changes the theme and saves the file.
    pub fn set_theme(&self, theme: Theme) -> Result<SettingsView, SettingsError> {
        self.update(|settings| settings.theme = theme)
    }

    fn update(&self, change: impl FnOnce(&mut Settings)) -> Result<SettingsView, SettingsError> {
        let mut state = self.lock();
        let path = match (&self.path, &state.warning) {
            (Some(path), None) => path,
            _ => return Err(SettingsError::ReadOnly),
        };
        let mut next = state.settings.clone();
        change(&mut next);
        let text = encode(&next)?;
        write_atomic(path, text.as_bytes(), || Ok(())).map_err(|source| SettingsError::Write {
            path: path.clone(),
            source,
        })?;
        state.settings = next;
        Ok(view(&state))
    }

    fn lock(&self) -> MutexGuard<'_, Loaded> {
        // A panic while holding the lock cannot leave `Loaded` half-changed:
        // it is replaced only after a successful save.
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

fn view(state: &Loaded) -> SettingsView {
    SettingsView {
        settings: state.settings.clone(),
        read_only: state.warning.is_some(),
        warning: state.warning.clone(),
    }
}

fn unreadable(path: &Path, reason: String) -> Loaded {
    Loaded {
        settings: Settings::default(),
        warning: Some(SettingsWarning::Unreadable {
            path: path.display().to_string(),
            reason,
        }),
    }
}

/// Parses a settings file, migrating it forward with `migrations`; the
/// current schema is `migrations.len() + 1`.
fn parse(path: &Path, text: &str, migrations: &[Migration]) -> Loaded {
    let current = migrations.len() + 1;
    let mut table: toml::Table = match text.parse() {
        Ok(table) => table,
        Err(error) => return unreadable(path, error.to_string()),
    };
    let schema = table.get("schema").and_then(toml::Value::as_integer);
    let schema = match schema.and_then(|s| usize::try_from(s).ok()) {
        Some(schema) if schema >= 1 => schema,
        _ => return unreadable(path, "it has no valid `schema` number".to_owned()),
    };

    if schema > current {
        // Read what this build understands; keep the defaults otherwise.
        let settings = decode(table).unwrap_or_default();
        return Loaded {
            settings,
            warning: Some(SettingsWarning::NewerSchema {
                path: path.display().to_string(),
                found: u32::try_from(schema).unwrap_or(u32::MAX),
                supported: u32::try_from(current).unwrap_or(u32::MAX),
            }),
        };
    }

    for migrate in &migrations[schema - 1..] {
        migrate(&mut table);
    }
    match decode(table) {
        Ok(settings) => Loaded {
            settings,
            warning: None,
        },
        Err(error) => unreadable(path, error.to_string()),
    }
}

/// Reads the settings from a file's table, taking the default for every
/// missing key. Unknown keys, such as `schema`, are ignored.
fn decode(table: toml::Table) -> Result<Settings, toml::de::Error> {
    let mut full = toml::Table::try_from(Settings::default())
        .map_err(<toml::de::Error as serde::de::Error>::custom)?;
    full.extend(table);
    toml::Value::Table(full).try_into()
}

fn encode(settings: &Settings) -> Result<String, toml::ser::Error> {
    #[derive(Serialize)]
    struct OnDisk<'a> {
        schema: u32,
        #[serde(flatten)]
        settings: &'a Settings,
    }
    toml::to_string(&OnDisk {
        schema: SETTINGS_SCHEMA,
        settings,
    })
}

/// Writes `contents` to a temporary file next to `target`, flushes it to
/// disk, then renames it over `target`, so `target` is always either the old
/// or the new file. `before_rename` runs just before the rename; tests use it
/// to fail there.
fn write_atomic(
    target: &Path,
    contents: &[u8],
    before_rename: impl FnOnce() -> io::Result<()>,
) -> io::Result<()> {
    let dir = target
        .parent()
        .ok_or_else(|| io::Error::other("the settings path has no parent dir"))?;
    fs::create_dir_all(dir)?;
    let mut temp = tempfile::Builder::new()
        .prefix(".settings.")
        .suffix(".tmp")
        .tempfile_in(dir)?;
    temp.write_all(contents)?;
    temp.as_file().sync_all()?;
    // On error the temporary file is deleted when `temp` drops.
    before_rename()?;
    temp.persist(target).map_err(|error| error.error)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names_in(dir: &Path) -> Vec<String> {
        let mut names: Vec<String> = fs::read_dir(dir)
            .unwrap()
            .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        names.sort();
        names
    }

    #[test]
    fn data_1_write_is_atomic() {
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("nested").join(SETTINGS_FILE);

        write_atomic(&target, b"schema = 1\ntheme = \"light\"\n", || Ok(())).unwrap();
        assert_eq!(names_in(target.parent().unwrap()), [SETTINGS_FILE]);
        let parsed = parse(&target, &fs::read_to_string(&target).unwrap(), MIGRATIONS);
        assert_eq!(parsed.settings.theme, Theme::Light);
        assert_eq!(parsed.warning, None);

        // A failure after the temporary file is written but before the
        // rename leaves the old file as it was and no temporary file behind.
        let failed = write_atomic(&target, b"schema = 1\ntheme = \"dark\"\n", || {
            Err(io::Error::other("simulated crash"))
        });
        assert!(failed.is_err());
        assert_eq!(names_in(target.parent().unwrap()), [SETTINGS_FILE]);
        assert_eq!(
            fs::read_to_string(&target).unwrap(),
            "schema = 1\ntheme = \"light\"\n"
        );
    }

    #[test]
    fn data_1_older_schema_is_migrated() {
        // A pretend schema 1 that called the theme `mode`.
        let rename_mode: Migration = |table| {
            if let Some(mode) = table.remove("mode") {
                table.insert("theme".to_owned(), mode);
            }
        };
        let path = Path::new("settings.toml");
        let loaded = parse(path, "schema = 1\nmode = \"dark\"\n", &[rename_mode]);
        assert_eq!(loaded.settings.theme, Theme::Dark);
        assert_eq!(loaded.warning, None);

        let current = parse(path, "schema = 2\ntheme = \"light\"\n", &[rename_mode]);
        assert_eq!(current.settings.theme, Theme::Light);
    }

    #[test]
    fn data_1_saved_file_has_schema_and_theme() {
        let text = encode(&Settings { theme: Theme::Dark }).unwrap();
        assert_eq!(text, "schema = 1\ntheme = \"dark\"\n");
    }
}
