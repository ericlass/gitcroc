//! App settings, kept in `settings.toml` in the platform config dir (section
//! 10 "Files"), under the DATA-1 rules of [`crate::config_file`].

use std::io;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard};

use serde::{Deserialize, Serialize};

use gitcroc_git::Context;

use crate::config_file::{self, ConfigWarning, Migration};
use crate::lock;
use crate::{POOL_SIZE, WorkspaceId};

mod ai;

pub use ai::{
    AiOptions, AiSettings, AiSettingsError, DEFAULT_CONTEXT_BUDGET, MAX_CONTEXT_BUDGET,
    MIN_CONTEXT_BUDGET, ProviderDraft, ProviderEntry, ProviderId, ProviderKind,
};

/// The schema this build reads and writes.
pub const SETTINGS_SCHEMA: u32 = 2;

/// The file name inside the config dir.
pub const SETTINGS_FILE: &str = "settings.toml";

/// `MIGRATIONS[n - 1]` migrates schema `n`.
const MIGRATIONS: &[Migration] = &[from_schema_1];

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

/// How the diff shows a text file (UI-17).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "snake_case")]
pub enum DiffMode {
    #[default]
    Inline,
    SideBySide,
}

/// How the changes pane lists files (UI-14).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "snake_case")]
pub enum FileListMode {
    #[default]
    Flat,
    Tree,
}

/// The changes pane's view, remembered for every workspace (UI-14,
/// UI-17). A key missing from the file takes its default (see [`decode`]).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
pub struct ChangesSettings {
    pub diff_mode: DiffMode,
    pub file_list: FileListMode,
    pub ignore_whitespace: bool,
    pub wrap_lines: bool,
    pub context: Context,
}

/// A code theme for diffs and the conflict editor (UI-24). The UI holds
/// their colors.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "snake_case")]
pub enum CodeTheme {
    GithubLight,
    GithubDark,
    SolarizedLight,
    SolarizedDark,
    OneDark,
    Dracula,
    Nord,
    Monokai,
}

/// The smallest font size in CSS pixels (UI-26).
pub const MIN_FONT_SIZE: u32 = 8;
/// The largest font size in CSS pixels (UI-26).
pub const MAX_FONT_SIZE: u32 = 32;
/// The font size until the user picks one (UI-26).
pub const DEFAULT_FONT_SIZE: u32 = 13;

/// Code themes and fonts (UI-24, UI-26). Saved as top-level keys of the
/// file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
pub struct Appearance {
    /// Used while the app theme resolves to light.
    pub code_theme_light: CodeTheme,
    /// Used while the app theme resolves to dark.
    pub code_theme_dark: CodeTheme,
    /// A font family name; `None` is the system UI font.
    pub ui_font: Option<String>,
    /// In CSS pixels, [`MIN_FONT_SIZE`] to [`MAX_FONT_SIZE`].
    pub ui_font_size: u32,
    /// A font family name; `None` is the system monospace font.
    pub code_font: Option<String>,
    /// In CSS pixels, [`MIN_FONT_SIZE`] to [`MAX_FONT_SIZE`].
    pub code_font_size: u32,
}

impl Default for Appearance {
    fn default() -> Self {
        Self {
            code_theme_light: CodeTheme::GithubLight,
            code_theme_dark: CodeTheme::GithubDark,
            ui_font: None,
            ui_font_size: DEFAULT_FONT_SIZE,
            code_font: None,
            code_font_size: DEFAULT_FONT_SIZE,
        }
    }
}

impl Appearance {
    /// Sizes limited to [`MIN_FONT_SIZE`]..=[`MAX_FONT_SIZE`], font names
    /// trimmed, and a blank name the system font.
    fn normalized(self) -> Self {
        let font = |name: Option<String>| {
            name.map(|name| name.trim().to_owned())
                .filter(|name| !name.is_empty())
        };
        Self {
            ui_font: font(self.ui_font),
            ui_font_size: self.ui_font_size.clamp(MIN_FONT_SIZE, MAX_FONT_SIZE),
            code_font: font(self.code_font),
            code_font_size: self.code_font_size.clamp(MIN_FONT_SIZE, MAX_FONT_SIZE),
            ..self
        }
    }
}

/// The fewest sub-jobs of a group operation that run at once.
pub const MIN_GROUP_PARALLELISM: u32 = 1;
/// The most sub-jobs of a group operation that run at once.
pub const MAX_GROUP_PARALLELISM: u32 = 32;

/// How group operations run (section 5 "Lifecycle of a group operation").
/// Saved as top-level keys of the file.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
pub struct GroupSettings {
    /// How many sub-jobs run at once, [`MIN_GROUP_PARALLELISM`] to
    /// [`MAX_GROUP_PARALLELISM`]. The pool that jobs and the refresh share
    /// has as many places (PERF-4).
    pub group_parallelism: u32,
    /// The preflight dialog shows before every group operation, also when
    /// every repo will run.
    pub always_show_preflight: bool,
}

impl Default for GroupSettings {
    fn default() -> Self {
        Self {
            group_parallelism: POOL_SIZE as u32,
            always_show_preflight: false,
        }
    }
}

const _: () = assert!(POOL_SIZE as u32 >= MIN_GROUP_PARALLELISM);
const _: () = assert!(POOL_SIZE as u32 <= MAX_GROUP_PARALLELISM);

impl GroupSettings {
    /// The parallelism moved into its limits.
    fn normalized(self) -> Self {
        Self {
            group_parallelism: self
                .group_parallelism
                .clamp(MIN_GROUP_PARALLELISM, MAX_GROUP_PARALLELISM),
            ..self
        }
    }
}

/// Every setting. A key missing from the file takes its default (see
/// [`decode`]).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
pub struct Settings {
    pub theme: Theme,
    #[serde(flatten)]
    pub appearance: Appearance,
    #[serde(flatten)]
    pub groups: GroupSettings,
    pub changes: ChangesSettings,
}

/// The open workspace tabs, kept for the next start (UI-33). Saved as the
/// optional `[session]` table, so the schema stays the same: older builds
/// ignore it. The UI never sees it; the core restores the tabs.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Session {
    /// The open workspaces, in tab order.
    #[serde(default)]
    pub tabs: Vec<WorkspaceId>,
    /// The active tab.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub active: Option<WorkspaceId>,
}

/// The file's contents apart from `schema`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
struct SettingsFile {
    #[serde(flatten)]
    settings: Settings,
    #[serde(skip_serializing_if = "Option::is_none")]
    session: Option<Session>,
    /// The optional `[ai]` table, so the schema stays the same: older builds
    /// ignore it. Written once an AI setting changes.
    #[serde(skip_serializing_if = "Option::is_none")]
    ai: Option<AiSettings>,
}

/// The settings as the UI sees them.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
pub struct SettingsView {
    pub settings: Settings,
    /// Changes are refused; `warning` says why.
    pub read_only: bool,
    pub warning: Option<ConfigWarning>,
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
    #[error(
        "group operations run {MIN_GROUP_PARALLELISM} to {MAX_GROUP_PARALLELISM} repos at once"
    )]
    Parallelism,
    #[error("{0}")]
    Ai(#[from] AiSettingsError),
}

/// The file's contents, and a warning when it must not be written.
type Loaded = config_file::Loaded<SettingsFile>;

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
                    value: SettingsFile::default(),
                    warning: Some(ConfigWarning::NoConfigDir),
                }),
            };
        };
        let path = dir.join(SETTINGS_FILE);
        let loaded = config_file::load(&path, MIGRATIONS, decode);
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

    /// The tabs saved by the last run, if it saved them (UI-33).
    pub fn session(&self) -> Option<Session> {
        self.lock().value.session.clone()
    }

    /// Keeps the open tabs and saves the file (UI-33).
    pub fn set_session(&self, session: Session) -> Result<(), SettingsError> {
        self.update_file(|file| file.session = Some(session))
            .map(drop)
    }

    /// Changes the theme and saves the file.
    pub fn set_theme(&self, theme: Theme) -> Result<SettingsView, SettingsError> {
        self.update(|settings| settings.theme = theme)
    }

    /// Changes the changes pane's view and saves the file.
    pub fn set_changes(&self, changes: ChangesSettings) -> Result<SettingsView, SettingsError> {
        self.update(|settings| settings.changes = changes)
    }

    /// Changes the code themes and fonts and saves the file. Sizes outside
    /// the limits are moved to the nearest one.
    pub fn set_appearance(&self, appearance: Appearance) -> Result<SettingsView, SettingsError> {
        self.update(|settings| settings.appearance = appearance.normalized())
    }

    /// Changes how group operations run and saves the file. A parallelism
    /// outside its limits is refused.
    pub fn set_groups(&self, groups: GroupSettings) -> Result<SettingsView, SettingsError> {
        if !(MIN_GROUP_PARALLELISM..=MAX_GROUP_PARALLELISM).contains(&groups.group_parallelism) {
            return Err(SettingsError::Parallelism);
        }
        self.update(|settings| settings.groups = groups)
    }

    /// The AI settings and providers (AI-1, AI-16).
    pub fn ai(&self) -> AiSettings {
        self.lock().value.ai.clone().unwrap_or_default()
    }

    /// Checks `draft` and saves it as a new provider, or over the one with
    /// its id. `keys` runs first, with the entry and whether it is new, and
    /// may refuse; the file is written only after it succeeds, so a key the
    /// keychain refused never leaves an entry behind (D11).
    pub fn save_provider<E: From<SettingsError>>(
        &self,
        draft: ProviderDraft,
        keys: impl FnOnce(&ProviderEntry, bool) -> Result<(), E>,
    ) -> Result<ProviderEntry, E> {
        self.change_ai(|ai| {
            let (id, consent) = match &draft.id {
                None => (ai.next_id(), None),
                Some(id) => match ai.provider(id) {
                    Some(entry) => (id.clone(), entry.consent.clone()),
                    None => {
                        return Err(SettingsError::from(AiSettingsError::UnknownProvider(
                            id.clone(),
                        ))
                        .into());
                    }
                },
            };
            let new = draft.id.is_none();
            let entry = draft
                .into_entry(id, consent)
                .map_err(|error| E::from(error.into()))?;
            keys(&entry, new)?;
            match ai.providers.iter_mut().find(|old| old.id == entry.id) {
                Some(old) => *old = entry.clone(),
                None => ai.providers.push(entry.clone()),
            }
            Ok(entry)
        })
    }

    /// Removes the provider with `id`, and the default if it was the
    /// default; returns the removed entry.
    pub fn delete_provider(&self, id: &ProviderId) -> Result<ProviderEntry, SettingsError> {
        self.change_ai(|ai| {
            let at = ai
                .providers
                .iter()
                .position(|entry| &entry.id == id)
                .ok_or_else(|| AiSettingsError::UnknownProvider(id.clone()))?;
            if ai.default.as_ref() == Some(id) {
                ai.default = None;
            }
            Ok(ai.providers.remove(at))
        })
    }

    /// Makes the provider with `id` the default (AI-1).
    pub fn set_default_provider(&self, id: &ProviderId) -> Result<AiSettings, SettingsError> {
        self.change_ai(|ai| {
            if ai.provider(id).is_none() {
                return Err(AiSettingsError::UnknownProvider(id.clone()).into());
            }
            ai.default = Some(id.clone());
            Ok(ai.clone())
        })
    }

    /// Changes the AI features switch, the exclude list and the debug log
    /// switch; blank patterns are dropped.
    pub fn set_ai_options(&self, options: AiOptions) -> Result<AiSettings, SettingsError> {
        self.change_ai(|ai| {
            ai.set_options(options);
            Ok(ai.clone())
        })
    }

    fn change_ai<T, E: From<SettingsError>>(
        &self,
        change: impl FnOnce(&mut AiSettings) -> Result<T, E>,
    ) -> Result<T, E> {
        self.modify(|file| change(file.ai.get_or_insert_default()))
            .map(|(value, _)| value)
    }

    fn update(&self, change: impl FnOnce(&mut Settings)) -> Result<SettingsView, SettingsError> {
        self.update_file(|file| change(&mut file.settings))
    }

    fn update_file(
        &self,
        change: impl FnOnce(&mut SettingsFile),
    ) -> Result<SettingsView, SettingsError> {
        self.modify(|file| {
            change(file);
            Ok::<(), SettingsError>(())
        })
        .map(|((), view)| view)
    }

    /// Changes a copy of the file's contents, writes it, and only then
    /// keeps it. Nothing is kept when `change` fails.
    fn modify<T, E: From<SettingsError>>(
        &self,
        change: impl FnOnce(&mut SettingsFile) -> Result<T, E>,
    ) -> Result<(T, SettingsView), E> {
        let mut state = self.lock();
        let path = match (&self.path, &state.warning) {
            (Some(path), None) => path,
            _ => return Err(SettingsError::ReadOnly.into()),
        };
        let mut next = state.value.clone();
        let value = change(&mut next)?;
        let text = config_file::encode(SETTINGS_SCHEMA, &next).map_err(SettingsError::from)?;
        config_file::write_atomic(path, text.as_bytes(), || Ok(())).map_err(|source| {
            SettingsError::Write {
                path: path.clone(),
                source,
            }
        })?;
        state.value = next;
        Ok((value, view(&state)))
    }

    fn lock(&self) -> MutexGuard<'_, Loaded> {
        // A panic while holding the lock cannot leave `Loaded` half-changed:
        // it is replaced only after a successful save.
        lock(&self.state)
    }
}

fn view(state: &Loaded) -> SettingsView {
    SettingsView {
        settings: state.value.settings.clone(),
        read_only: state.warning.is_some(),
        warning: state.warning.clone(),
    }
}

/// Schema 2 added the code themes and fonts; their defaults fill in.
fn from_schema_1(_table: &mut toml::Table) {}

/// Reads the file's table. A `[session]` that cannot be read is dropped:
/// the start then opens a workspace as when there is none (UI-33). An
/// `[ai]` table that cannot be read or holds an invalid entry makes the
/// whole file unreadable, so it is used read-only and the user's providers
/// are never overwritten (DATA-1).
fn decode(mut table: toml::Table) -> Result<SettingsFile, toml::de::Error> {
    let session = table
        .remove("session")
        .and_then(|session| session.try_into().ok());
    let ai = match table.remove("ai") {
        None => None,
        Some(ai) => {
            let ai: AiSettings = ai.try_into()?;
            ai.check().map_err(|error| {
                <toml::de::Error as serde::de::Error>::custom(format!("in [ai]: {error}"))
            })?;
            Some(ai)
        }
    };
    Ok(SettingsFile {
        settings: decode_settings(table)?,
        session,
        ai,
    })
}

/// Reads the settings from a file's table, taking the default for every
/// missing key, inside tables too. Unknown keys, such as `schema`, are
/// ignored. Font sizes and the group parallelism outside their limits are
/// moved to the nearest one.
fn decode_settings(table: toml::Table) -> Result<Settings, toml::de::Error> {
    let mut full = toml::Table::try_from(Settings::default())
        .map_err(<toml::de::Error as serde::de::Error>::custom)?;
    merge(&mut full, table);
    let mut settings: Settings = toml::Value::Table(full).try_into()?;
    settings.appearance = std::mem::take(&mut settings.appearance).normalized();
    settings.groups = settings.groups.normalized();
    Ok(settings)
}

/// Puts every key of `from` into `into`, merging tables both have.
fn merge(into: &mut toml::Table, from: toml::Table) {
    for (key, value) in from {
        match (into.get_mut(&key), value) {
            (Some(toml::Value::Table(inner)), toml::Value::Table(value)) => merge(inner, value),
            (_, value) => {
                into.insert(key, value);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn data_1_saved_file_has_schema_and_theme() {
        let settings = Settings {
            theme: Theme::Dark,
            ..Settings::default()
        };
        let text = config_file::encode(SETTINGS_SCHEMA, &settings).unwrap();
        assert!(text.starts_with("schema = 2\ntheme = \"dark\"\n"), "{text}");
    }
}
