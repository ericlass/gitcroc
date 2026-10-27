//! The files of a release beside its packages (M5-13): the updater's
//! `latest.json`, which lists the packages that update themselves with their
//! signatures (PLAT-4, SEC-7), and the release notes, the version's section of
//! `CHANGELOG.md` followed by the download paragraph.

use std::fs;
use std::path::{Path, PathBuf};

use serde_json::{Value, json};

/// Where the releases are downloaded from; the updater's endpoint, in
/// 1.0.0-rc.1's `tauri.conf.json` and in the app's own update source
/// (M6-27), is this repository's latest release.
pub const RELEASES: &str = "https://github.com/ericlass/gitcroc/releases/download";

/// The page that explains how to open the unsigned builds (SEC-7).
pub const INSTALL_UNSIGNED: &str = "docs/user/install.md";

/// Errors returned by `release-manifest`.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("I/O error on {path}: {source}")]
    Io {
        path: PathBuf,
        source: std::io::Error,
    },
    #[error("{0:?} is not a version")]
    BadVersion(String),
    #[error("{0:?} is not a UTC time like 2026-10-20T12:00:00Z")]
    BadDate(String),
    #[error("CHANGELOG.md has no section for {0}")]
    NoSection(String),
    #[error("CHANGELOG.md has no section for {0}, and no other section has notes")]
    NoUnreleased(String),
    #[error("no {what} ({pattern}) among the files")]
    Missing { what: &'static str, pattern: String },
    #[error("more than one {what} among the files: {files:?}")]
    Ambiguous {
        what: &'static str,
        files: Vec<String>,
    },
    #[error("{0} has no signature next to it ({0}.sig)")]
    NoSignature(String),
}

/// The notes of a version and where they came from.
#[derive(Debug, PartialEq, Eq)]
pub struct Section {
    pub text: String,
    /// The section a dry run took because the version has none: Unreleased,
    /// or right after a release, when Unreleased is empty, the newest one.
    pub fallback: Option<String>,
}

/// The body of the section of `CHANGELOG.md` headed `## [<name>]` or
/// `## <name>`, with or without a date after it, up to the next `## `
/// heading. Link reference definitions (`[1.0.0]: https://…`) are not part
/// of it. `None` when there is no such section or it is empty.
pub fn changelog_section(changelog: &str, name: &str) -> Option<String> {
    let mut lines = changelog.lines();
    lines.find(|line| {
        heading_name(line).is_some_and(|heading| heading.eq_ignore_ascii_case(name))
    })?;
    let body: Vec<&str> = lines
        .take_while(|line| heading_name(line).is_none())
        .filter(|line| !is_link_definition(line))
        .collect();
    let text = body.join("\n").trim().to_owned();
    (!text.is_empty()).then_some(text)
}

/// The version's section. A release needs one; a dry run takes the
/// Unreleased section when the version has none, since the version of a
/// dry run may never be released, and the newest section with notes when
/// Unreleased is empty, as it is right after a release.
pub fn release_section(changelog: &str, version: &str, dry_run: bool) -> Result<Section, Error> {
    check_version(version)?;
    if let Some(text) = changelog_section(changelog, version) {
        return Ok(Section {
            text,
            fallback: None,
        });
    }
    if !dry_run {
        return Err(Error::NoSection(version.to_owned()));
    }
    let (name, text) = changelog_section(changelog, "Unreleased")
        .map(|text| ("Unreleased".to_owned(), text))
        .or_else(|| newest_section(changelog))
        .ok_or_else(|| Error::NoUnreleased(version.to_owned()))?;
    Ok(Section {
        text,
        fallback: Some(name),
    })
}

/// The first section after Unreleased that has notes, with its name.
fn newest_section(changelog: &str) -> Option<(String, String)> {
    changelog
        .lines()
        .filter_map(heading_name)
        .filter(|name| !name.eq_ignore_ascii_case("Unreleased"))
        .find_map(|name| changelog_section(changelog, name).map(|text| (name.to_owned(), text)))
}

/// The name a `## ` heading gives its section: the text in brackets, or the
/// first word.
fn heading_name(line: &str) -> Option<&str> {
    let rest = line.strip_prefix("## ")?.trim();
    match rest.strip_prefix('[') {
        Some(bracketed) => bracketed.split(']').next(),
        None => rest.split_whitespace().next(),
    }
}

fn is_link_definition(line: &str) -> bool {
    line.starts_with('[') && line.contains("]: ")
}

fn check_version(version: &str) -> Result<(), Error> {
    let starts_with_digit = version.starts_with(|c: char| c.is_ascii_digit());
    let allowed = version
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '+'));
    if starts_with_digit && allowed && version.matches('.').count() >= 2 {
        Ok(())
    } else {
        Err(Error::BadVersion(version.to_owned()))
    }
}

/// Accepts `YYYY-MM-DDTHH:MM:SSZ`, as `date -u +%Y-%m-%dT%H:%M:%SZ` prints it.
fn check_date(date: &str) -> Result<(), Error> {
    let shape = "dddd-dd-ddTdd:dd:ddZ";
    let matches = date.len() == shape.len()
        && date.chars().zip(shape.chars()).all(|(c, s)| match s {
            'd' => c.is_ascii_digit(),
            _ => c == s,
        });
    if matches {
        Ok(())
    } else {
        Err(Error::BadDate(date.to_owned()))
    }
}

/// The packages of a release, by file name, as Tauri's bundler named them
/// for 1.0.0-rc.1; the portable zip as its `bundles.yml` named it. M6-28's
/// packaging keeps these names.
#[derive(Debug, PartialEq, Eq)]
pub struct Packages {
    /// The Windows NSIS installer, which also is its own update package.
    pub installer: String,
    pub portable: String,
    pub dmg: String,
    /// The macOS app's update package, universal.
    pub app_archive: String,
    /// The AppImage, which also is its own update package.
    pub appimage: String,
    pub deb: String,
    pub rpm: String,
}

impl Packages {
    /// Finds each package among `names`, exactly one of each.
    pub fn find(names: &[String], version: &str) -> Result<Self, Error> {
        let portable = format!("gitcroc-{version}-windows-x64-portable.zip");
        let one = |what: &'static str, pattern: &str, matches: &dyn Fn(&str) -> bool| {
            let found: Vec<String> = names.iter().filter(|name| matches(name)).cloned().collect();
            match found.as_slice() {
                [name] => Ok(name.clone()),
                [] => Err(Error::Missing {
                    what,
                    pattern: pattern.to_owned(),
                }),
                _ => Err(Error::Ambiguous { what, files: found }),
            }
        };
        Ok(Self {
            installer: one("Windows installer", "*-setup.exe", &|name| {
                name.ends_with("-setup.exe")
            })?,
            portable: one("portable zip", &portable, &|name| name == portable)?,
            dmg: one("macOS disk image", "*.dmg", &|name| name.ends_with(".dmg"))?,
            app_archive: one("macOS update package", "*.app.tar.gz", &|name| {
                name.ends_with(".app.tar.gz")
            })?,
            appimage: one("AppImage", "*.AppImage", &|name| {
                name.ends_with(".AppImage")
            })?,
            deb: one("Debian package", "*.deb", &|name| name.ends_with(".deb"))?,
            rpm: one("RPM package", "*.rpm", &|name| name.ends_with(".rpm"))?,
        })
    }
}

/// The download URL of a release file.
pub fn download_url(version: &str, file: &str) -> String {
    format!("{RELEASES}/v{version}/{file}")
}

/// One platform's local CI update proof. Reads the actual bundle's signature;
/// the caller rewrites its URL to the job's loopback server (M5-15).
pub fn local_manifest(
    dir: &Path,
    version: &str,
    notes: &str,
    pub_date: &str,
    platform: &str,
    package: &str,
) -> Result<Value, Error> {
    check_version(version)?;
    check_date(pub_date)?;
    let path = dir.join(format!("{package}.sig"));
    let signature = fs::read_to_string(&path).map_err(|source| Error::Io { path, source })?;
    if signature.trim().is_empty() {
        return Err(Error::NoSignature(package.to_owned()));
    }
    Ok(
        json!({ "version": version, "notes": notes, "pub_date": pub_date,
        "platforms": { platform: { "url": download_url(version, package), "signature": signature.trim() } } }),
    )
}

/// Tauri's `latest.json`: the packages that update themselves, each with the
/// content of its `.sig` file (PLAT-4). Both macOS keys name the universal
/// app. The `.deb`, the `.rpm` and the portable zip only notify, so they are
/// not in it. `signature` reads the `.sig` of a file.
pub fn manifest(
    version: &str,
    notes: &str,
    pub_date: &str,
    packages: &Packages,
    signature: impl Fn(&str) -> Result<String, Error>,
) -> Result<Value, Error> {
    check_version(version)?;
    check_date(pub_date)?;
    let entry = |file: &str| -> Result<Value, Error> {
        Ok(json!({ "signature": signature(file)?, "url": download_url(version, file) }))
    };
    let app = entry(&packages.app_archive)?;
    Ok(json!({
        "version": version,
        "notes": notes,
        "pub_date": pub_date,
        "platforms": {
            "windows-x86_64": entry(&packages.installer)?,
            "darwin-x86_64": app.clone(),
            "darwin-aarch64": app,
            "linux-x86_64": entry(&packages.appimage)?,
        },
    }))
}

/// The release notes: the changelog section, then a fixed paragraph that
/// links the page on the unsigned builds (SEC-7) and names the file for each
/// platform.
pub fn release_notes(version: &str, section: &str, packages: &Packages) -> String {
    let Packages {
        installer,
        portable,
        dmg,
        appimage,
        deb,
        rpm,
        ..
    } = packages;
    format!(
        "{section}

## Download

- **Windows 10 and 11, x64:** `{installer}`, or `{portable}` to run it without installing.
- **macOS 12 and newer, Apple silicon and Intel:** `{dmg}`.
- **Linux x86_64:** `{appimage}`, `{deb}` or `{rpm}`.

The Windows and macOS builds are not code-signed or notarized, so the first time you open one the system warns \
about an unidentified developer. [Installing the unsigned builds](https://github.com/ericlass/gitcroc/blob/v{version}/{INSTALL_UNSIGNED}) \
explains how to get past the warning. `SHA256SUMS` has the checksum of every file.
"
    )
}

/// What `assemble` made.
#[derive(Debug)]
pub struct Assembled {
    pub notes: String,
    pub fallback: Option<String>,
}

/// Reads the packages in `dir`, writes `dir/latest.json` and returns the
/// release notes.
pub fn assemble(
    dir: &Path,
    changelog: &str,
    version: &str,
    pub_date: &str,
    dry_run: bool,
) -> Result<Assembled, Error> {
    let section = release_section(changelog, version, dry_run)?;
    let entries = fs::read_dir(dir).map_err(|source| Error::Io {
        path: dir.to_owned(),
        source,
    })?;
    let mut names = Vec::new();
    for entry in entries {
        let entry = entry.map_err(|source| Error::Io {
            path: dir.to_owned(),
            source,
        })?;
        names.push(entry.file_name().to_string_lossy().into_owned());
    }
    names.sort();
    let packages = Packages::find(&names, version)?;
    let signature = |file: &str| {
        let path = dir.join(format!("{file}.sig"));
        match fs::read_to_string(&path) {
            Ok(text) if !text.trim().is_empty() => Ok(text.trim().to_owned()),
            Ok(_) => Err(Error::NoSignature(file.to_owned())),
            Err(source) if source.kind() == std::io::ErrorKind::NotFound => {
                Err(Error::NoSignature(file.to_owned()))
            }
            Err(source) => Err(Error::Io { path, source }),
        }
    };
    let manifest = manifest(version, &section.text, pub_date, &packages, signature)?;
    let path = dir.join("latest.json");
    let mut text = serde_json::to_string_pretty(&manifest).unwrap_or_default();
    text.push('\n');
    fs::write(&path, text).map_err(|source| Error::Io { path, source })?;
    Ok(Assembled {
        notes: release_notes(version, &section.text, &packages),
        fallback: section.fallback,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const CHANGELOG: &str = "\
# Changelog

All notable changes to gitcroc are documented in this file.

## [Unreleased]

### Added
- Something new.

## [1.0.0-rc.1] - 2026-11-02

### Added
- Workspaces and groups.
- AI conflict resolution.

## [0.9.0]

## 0.8.0 - 2026-01-01

- An old line.

[Unreleased]: https://github.com/ericlass/gitcroc/compare/v1.0.0-rc.1...HEAD
[1.0.0-rc.1]: https://github.com/ericlass/gitcroc/releases/tag/v1.0.0-rc.1
";

    const VERSION: &str = "1.0.0";

    #[test]
    fn plat_4_sec_7_local_manifest_keeps_the_real_package_signature() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("update.AppImage.sig"), "signed-package\n").unwrap();
        let value = local_manifest(
            dir.path(),
            VERSION,
            "notes",
            "2026-10-20T12:00:00Z",
            "linux-x86_64",
            "update.AppImage",
        )
        .unwrap();
        assert_eq!(value["platforms"].as_object().unwrap().len(), 1);
        assert_eq!(
            value["platforms"]["linux-x86_64"]["signature"],
            "signed-package"
        );
        assert_eq!(
            value["platforms"]["linux-x86_64"]["url"],
            download_url(VERSION, "update.AppImage")
        );
        fs::remove_file(dir.path().join("update.AppImage.sig")).unwrap();
        assert!(
            local_manifest(
                dir.path(),
                VERSION,
                "",
                "2026-10-20T12:00:00Z",
                "linux-x86_64",
                "update.AppImage"
            )
            .is_err()
        );
    }

    fn names() -> Vec<String> {
        [
            "gitcroc_1.0.0_x64-setup.exe",
            "gitcroc_1.0.0_x64-setup.exe.sig",
            "gitcroc-1.0.0-windows-x64-portable.zip",
            "gitcroc_1.0.0_universal.dmg",
            "gitcroc.app.tar.gz",
            "gitcroc.app.tar.gz.sig",
            "gitcroc_1.0.0_amd64.AppImage",
            "gitcroc_1.0.0_amd64.AppImage.sig",
            "gitcroc_1.0.0_amd64.deb",
            "gitcroc-1.0.0-1.x86_64.rpm",
        ]
        .map(String::from)
        .to_vec()
    }

    fn packages() -> Packages {
        Packages::find(&names(), VERSION).unwrap()
    }

    fn signature(file: &str) -> Result<String, Error> {
        Ok(format!("sig of {file}"))
    }

    /// A directory with every package and the `.sig` of each update package.
    fn release_dir() -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        for name in names() {
            let content = if name.ends_with(".sig") {
                format!("sig of {}\n", name.trim_end_matches(".sig"))
            } else {
                "package".to_owned()
            };
            fs::write(dir.path().join(name), content).unwrap();
        }
        dir
    }

    #[test]
    fn plat_4_manifest_lists_installer_app_and_appimage_only() {
        let manifest = manifest(
            VERSION,
            "notes",
            "2026-11-02T10:00:00Z",
            &packages(),
            signature,
        )
        .unwrap();
        let platforms: Vec<&String> = manifest["platforms"].as_object().unwrap().keys().collect();
        assert_eq!(
            platforms,
            [
                "darwin-aarch64",
                "darwin-x86_64",
                "linux-x86_64",
                "windows-x86_64"
            ]
        );
        let text = manifest.to_string();
        for only_notifies in [".deb", ".rpm", "portable.zip", ".dmg"] {
            assert!(
                !text.contains(only_notifies),
                "{only_notifies} is in {text}"
            );
        }
    }

    #[test]
    fn plat_4_manifest_urls_and_platform_keys() {
        let manifest = manifest(
            VERSION,
            "the notes",
            "2026-11-02T10:00:00Z",
            &packages(),
            signature,
        )
        .unwrap();
        let base = "https://github.com/ericlass/gitcroc/releases/download/v1.0.0";
        assert_eq!(
            manifest,
            json!({
                "version": "1.0.0",
                "notes": "the notes",
                "pub_date": "2026-11-02T10:00:00Z",
                "platforms": {
                    "windows-x86_64": {
                        "signature": "sig of gitcroc_1.0.0_x64-setup.exe",
                        "url": format!("{base}/gitcroc_1.0.0_x64-setup.exe"),
                    },
                    "darwin-x86_64": {
                        "signature": "sig of gitcroc.app.tar.gz",
                        "url": format!("{base}/gitcroc.app.tar.gz"),
                    },
                    "darwin-aarch64": {
                        "signature": "sig of gitcroc.app.tar.gz",
                        "url": format!("{base}/gitcroc.app.tar.gz"),
                    },
                    "linux-x86_64": {
                        "signature": "sig of gitcroc_1.0.0_amd64.AppImage",
                        "url": format!("{base}/gitcroc_1.0.0_amd64.AppImage"),
                    },
                },
            })
        );
    }

    #[test]
    fn sec_7_a_missing_sig_is_an_error() {
        let dir = release_dir();
        fs::remove_file(dir.path().join("gitcroc.app.tar.gz.sig")).unwrap();
        let err =
            assemble(dir.path(), CHANGELOG, VERSION, "2026-11-02T10:00:00Z", true).unwrap_err();
        assert!(
            matches!(err, Error::NoSignature(ref file) if file == "gitcroc.app.tar.gz"),
            "{err}"
        );
        assert!(!dir.path().join("latest.json").exists());

        fs::write(dir.path().join("gitcroc.app.tar.gz.sig"), " \n").unwrap();
        let err =
            assemble(dir.path(), CHANGELOG, VERSION, "2026-11-02T10:00:00Z", true).unwrap_err();
        assert!(matches!(err, Error::NoSignature(_)), "{err}");
    }

    #[test]
    fn the_notes_come_from_the_changelog_section() {
        let dir = release_dir();
        // The package names say 1.0.0 except the portable zip, which names
        // the version it is built for.
        fs::rename(
            dir.path().join("gitcroc-1.0.0-windows-x64-portable.zip"),
            dir.path()
                .join("gitcroc-1.0.0-rc.1-windows-x64-portable.zip"),
        )
        .unwrap();
        let assembled = assemble(
            dir.path(),
            CHANGELOG,
            "1.0.0-rc.1",
            "2026-11-02T10:00:00Z",
            false,
        )
        .unwrap();
        assert_eq!(assembled.fallback, None);
        let section = "### Added\n- Workspaces and groups.\n- AI conflict resolution.";
        assert!(
            assembled
                .notes
                .starts_with(&format!("{section}\n\n## Download\n")),
            "{}",
            assembled.notes
        );

        let written: Value =
            serde_json::from_str(&fs::read_to_string(dir.path().join("latest.json")).unwrap())
                .unwrap();
        assert_eq!(written["notes"], section);
        assert_eq!(written["version"], "1.0.0-rc.1");
        assert_eq!(
            written["platforms"]["linux-x86_64"]["signature"],
            "sig of gitcroc_1.0.0_amd64.AppImage"
        );
    }

    #[test]
    fn sec_7_release_notes_link_the_unsigned_install_page_and_name_each_file() {
        let notes = release_notes(VERSION, "### Fixed\n- A bug.", &packages());
        assert!(
            notes.contains("https://github.com/ericlass/gitcroc/blob/v1.0.0/docs/user/install.md"),
            "{notes}"
        );
        for file in [
            "gitcroc_1.0.0_x64-setup.exe",
            "gitcroc-1.0.0-windows-x64-portable.zip",
            "gitcroc_1.0.0_universal.dmg",
            "gitcroc_1.0.0_amd64.AppImage",
            "gitcroc_1.0.0_amd64.deb",
            "gitcroc-1.0.0-1.x86_64.rpm",
        ] {
            assert!(
                notes.contains(&format!("`{file}`")),
                "{file} is not named in {notes}"
            );
        }
        assert!(
            !notes.contains("app.tar.gz"),
            "the update package is for the updater only"
        );
    }

    #[test]
    fn the_changelog_section_is_found_for_a_version_and_missing_for_another() {
        assert_eq!(
            changelog_section(CHANGELOG, "1.0.0-rc.1").as_deref(),
            Some("### Added\n- Workspaces and groups.\n- AI conflict resolution.")
        );
        assert_eq!(
            changelog_section(CHANGELOG, "Unreleased").as_deref(),
            Some("### Added\n- Something new.")
        );
        // A heading without brackets, and the link definitions are not notes.
        assert_eq!(
            changelog_section(CHANGELOG, "0.8.0").as_deref(),
            Some("- An old line.")
        );
        assert_eq!(changelog_section(CHANGELOG, "1.0.0"), None);
        assert_eq!(changelog_section(CHANGELOG, "1.0"), None);
        // A section with no notes counts as missing.
        assert_eq!(changelog_section(CHANGELOG, "0.9.0"), None);
    }

    #[test]
    fn a_release_needs_its_section_and_a_dry_run_falls_back_to_unreleased_or_the_newest() {
        let err = release_section(CHANGELOG, "1.0.0", false).unwrap_err();
        assert!(
            matches!(err, Error::NoSection(ref version) if version == "1.0.0"),
            "{err}"
        );
        assert_eq!(
            release_section(CHANGELOG, "1.0.0", true).unwrap(),
            Section {
                text: "### Added\n- Something new.".to_owned(),
                fallback: Some("Unreleased".to_owned())
            }
        );
        // A dry run of a version with a section uses it.
        assert_eq!(
            release_section(CHANGELOG, "1.0.0-rc.1", true)
                .unwrap()
                .fallback,
            None
        );
        // Right after a release Unreleased is empty; a dry run takes the
        // newest section with notes.
        let released = CHANGELOG.replace("### Added\n- Something new.\n", "");
        assert_eq!(
            release_section(&released, "1.0.1", true).unwrap(),
            Section {
                text: "### Added\n- Workspaces and groups.\n- AI conflict resolution.".to_owned(),
                fallback: Some("1.0.0-rc.1".to_owned())
            }
        );
        let err = release_section("# Changelog\n", "1.0.0", true).unwrap_err();
        assert!(matches!(err, Error::NoUnreleased(_)), "{err}");
    }

    #[test]
    fn every_package_is_needed_once() {
        let mut names = names();
        names.retain(|name| !name.ends_with(".rpm"));
        let err = Packages::find(&names, VERSION).unwrap_err();
        assert!(
            matches!(
                err,
                Error::Missing {
                    what: "RPM package",
                    ..
                }
            ),
            "{err}"
        );

        let mut names = self::names();
        names.push("gitcroc_0.9.0_universal.dmg".to_owned());
        let err = Packages::find(&names, VERSION).unwrap_err();
        assert!(
            matches!(
                err,
                Error::Ambiguous {
                    what: "macOS disk image",
                    ..
                }
            ),
            "{err}"
        );

        // The portable zip must be of this version.
        let err = Packages::find(&self::names(), "1.0.1").unwrap_err();
        assert!(
            matches!(
                err,
                Error::Missing {
                    what: "portable zip",
                    ..
                }
            ),
            "{err}"
        );
    }

    #[test]
    fn version_and_date_are_checked() {
        for version in ["", "v1.0.0", "1.0", "1.0.0/../x", "1.0.0 rc"] {
            assert!(
                matches!(
                    release_section(CHANGELOG, version, true),
                    Err(Error::BadVersion(_))
                ),
                "{version:?}"
            );
        }
        for date in [
            "2026-11-02",
            "2026-11-02T10:00:00+01:00",
            "2026-11-02 10:00:00Z",
        ] {
            assert!(
                matches!(
                    manifest(VERSION, "", date, &packages(), signature),
                    Err(Error::BadDate(_))
                ),
                "{date:?}"
            );
        }
    }

    #[test]
    fn the_project_changelog_has_an_unreleased_section() {
        let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../CHANGELOG.md");
        let changelog = fs::read_to_string(&path).unwrap();
        assert!(changelog.starts_with("# Changelog\n"));
        assert!(
            changelog.lines().any(|line| line == "## [Unreleased]"),
            "CHANGELOG.md has no Unreleased section"
        );
    }
}
