//! The rules every config file follows (DATA-1), shared by `settings.toml`
//! and the workspace files: written atomically; older schemas are migrated
//! on load; a newer schema or an unreadable file is used read-only and left
//! untouched.

use std::fs;
use std::io::{self, Write};
use std::path::Path;

use serde::{Deserialize, Serialize};

/// Turns a file of schema `n` into one of schema `n + 1`, in place.
pub(crate) type Migration = fn(&mut toml::Table);

/// Why a config file is read-only, shown to the user as a banner. Paths are
/// text for display only (PLAT-2).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ConfigWarning {
    /// The file at `path` has schema `found`, newer than the `supported` one.
    /// What this build understands of it is used; it is not written.
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

/// A file's contents, and why it must not be written if it must not.
#[derive(Debug, Default)]
pub(crate) struct Loaded<T> {
    pub value: T,
    pub warning: Option<ConfigWarning>,
}

/// Reads the file at `path`, migrating it forward with `migrations` (the
/// current schema is `migrations.len() + 1`) and turning it into a value
/// with `decode`. Never fails: a missing file gives the default, any other
/// problem the default and a warning.
pub(crate) fn load<T: Default>(
    path: &Path,
    migrations: &[Migration],
    decode: impl Fn(toml::Table) -> Result<T, toml::de::Error>,
) -> Loaded<T> {
    match fs::read_to_string(path) {
        Ok(text) => parse(path, &text, migrations, decode),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Loaded::default(),
        Err(error) => unreadable(path, error.to_string()),
    }
}

/// Like [`load`], for text already read from `path`.
pub(crate) fn parse<T: Default>(
    path: &Path,
    text: &str,
    migrations: &[Migration],
    decode: impl Fn(toml::Table) -> Result<T, toml::de::Error>,
) -> Loaded<T> {
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
        let value = decode(table).unwrap_or_default();
        return Loaded {
            value,
            warning: Some(ConfigWarning::NewerSchema {
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
        Ok(value) => Loaded {
            value,
            warning: None,
        },
        Err(error) => unreadable(path, error.to_string()),
    }
}

pub(crate) fn unreadable<T: Default>(path: &Path, reason: String) -> Loaded<T> {
    Loaded {
        value: T::default(),
        warning: Some(ConfigWarning::Unreadable {
            path: path.display().to_string(),
            reason,
        }),
    }
}

/// `value` as a file of schema `schema`, which comes first.
pub(crate) fn encode<T: Serialize>(schema: u32, value: &T) -> Result<String, toml::ser::Error> {
    #[derive(Serialize)]
    struct OnDisk<'a, T> {
        schema: u32,
        #[serde(flatten)]
        value: &'a T,
    }
    toml::to_string(&OnDisk { schema, value })
}

/// Writes `contents` to a temporary file next to `target`, flushes it to
/// disk, then renames it over `target`, so `target` is always either the old
/// or the new file. `before_rename` runs just before the rename; tests use it
/// to fail there.
pub(crate) fn write_atomic(
    target: &Path,
    contents: &[u8],
    before_rename: impl FnOnce() -> io::Result<()>,
) -> io::Result<()> {
    let dir = target
        .parent()
        .ok_or_else(|| io::Error::other("the config file path has no parent dir"))?;
    let name = target
        .file_name()
        .ok_or_else(|| io::Error::other("the config file path has no file name"))?;
    fs::create_dir_all(dir)?;
    let mut prefix = std::ffi::OsString::from(".");
    prefix.push(name);
    prefix.push(".");
    let mut temp = tempfile::Builder::new()
        .prefix(&prefix)
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

    fn decode(table: toml::Table) -> Result<toml::Table, toml::de::Error> {
        Ok(table)
    }

    #[test]
    fn data_1_write_is_atomic() {
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("nested").join("file.toml");

        write_atomic(&target, b"schema = 1\nkey = \"old\"\n", || Ok(())).unwrap();
        assert_eq!(names_in(target.parent().unwrap()), ["file.toml"]);

        // A failure after the temporary file is written but before the
        // rename leaves the old file as it was and no temporary file behind.
        let failed = write_atomic(&target, b"schema = 1\nkey = \"new\"\n", || {
            Err(io::Error::other("simulated crash"))
        });
        assert!(failed.is_err());
        assert_eq!(names_in(target.parent().unwrap()), ["file.toml"]);
        assert_eq!(
            fs::read_to_string(&target).unwrap(),
            "schema = 1\nkey = \"old\"\n"
        );
    }

    #[test]
    fn data_1_older_schema_is_migrated() {
        // A pretend schema 1 that called the key `mode`.
        let rename_mode: Migration = |table| {
            if let Some(mode) = table.remove("mode") {
                table.insert("theme".to_owned(), mode);
            }
        };
        let path = Path::new("file.toml");
        let loaded = parse(
            path,
            "schema = 1\nmode = \"dark\"\n",
            &[rename_mode],
            decode,
        );
        assert_eq!(loaded.value["theme"].as_str(), Some("dark"));
        assert_eq!(loaded.warning, None);

        let current = parse(
            path,
            "schema = 2\ntheme = \"light\"\n",
            &[rename_mode],
            decode,
        );
        assert_eq!(current.value["theme"].as_str(), Some("light"));
        assert_eq!(current.warning, None);

        let newer = parse(
            path,
            "schema = 3\ntheme = \"light\"\n",
            &[rename_mode],
            decode,
        );
        assert_eq!(newer.value["theme"].as_str(), Some("light"));
        assert_eq!(
            newer.warning,
            Some(ConfigWarning::NewerSchema {
                path: "file.toml".to_owned(),
                found: 3,
                supported: 2
            })
        );
    }
}
