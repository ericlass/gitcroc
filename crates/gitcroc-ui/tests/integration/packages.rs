//! The packages of section 11 (M6-28): the packaging metadata in this
//! crate's `Cargo.toml`, the NSIS template and the workflows that build and
//! install them. cargo-packager 0.11.8 names the files it makes; the tests
//! follow its rules, which are quoted where they are used.

use std::fs;
use std::path::{Path, PathBuf};

use toml::{Table, Value};

/// The cargo-packager version the workflows install and the NSIS template
/// is based on (M6-03).
const CARGO_PACKAGER: &str = "0.11.8";

fn crate_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn repo_dir() -> PathBuf {
    crate_dir().join("../..")
}

fn read(path: &Path) -> String {
    fs::read_to_string(path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()))
}

fn manifest() -> Table {
    let path = crate_dir().join("Cargo.toml");
    toml::from_str(&read(&path)).unwrap_or_else(|e| panic!("parse {}: {e}", path.display()))
}

/// `[package.metadata.<name>]`.
fn metadata(name: &str) -> Table {
    manifest()["package"]["metadata"][name]
        .as_table()
        .unwrap_or_else(|| panic!("[package.metadata.{name}]"))
        .clone()
}

fn packager() -> Table {
    metadata("packager")
}

fn text<'a>(table: &'a Table, key: &str) -> &'a str {
    table
        .get(key)
        .and_then(Value::as_str)
        .unwrap_or_else(|| panic!("{key} is text"))
}

fn workspace_version() -> String {
    let path = repo_dir().join("Cargo.toml");
    let manifest: Table =
        toml::from_str(&read(&path)).unwrap_or_else(|e| panic!("parse {}: {e}", path.display()));
    manifest["workspace"]["package"]["version"]
        .as_str()
        .expect("workspace.package.version")
        .to_owned()
}

/// The main binary's name as cargo-packager takes it: the file stem of the
/// `binaries` entry marked `main`.
fn main_binary(packager: &Table) -> String {
    let binaries = packager["binaries"].as_array().expect("binaries");
    let main: Vec<&str> = binaries
        .iter()
        .filter(|binary| binary.get("main").and_then(Value::as_bool) == Some(true))
        .map(|binary| binary["path"].as_str().expect("a binary's path"))
        .collect();
    assert_eq!(main.len(), 1, "one main binary: {main:?}");
    Path::new(main[0])
        .file_stem()
        .expect("a file name")
        .to_string_lossy()
        .into_owned()
}

/// Every file that builds, signs or installs a package: the workflows, the
/// scripts they run and the justfile.
fn build_files() -> Vec<(String, String)> {
    let mut files = Vec::new();
    for dir in [".github/workflows", ".github/scripts"] {
        let dir = repo_dir().join(dir);
        let entries = fs::read_dir(&dir).unwrap_or_else(|e| panic!("read {}: {e}", dir.display()));
        for entry in entries {
            let path = entry.expect("a directory entry").path();
            if path.is_file() {
                let name = path
                    .strip_prefix(repo_dir())
                    .unwrap_or(&path)
                    .to_string_lossy()
                    .into_owned();
                files.push((name, read(&path)));
            }
        }
    }
    files.push(("justfile".to_owned(), read(&repo_dir().join("justfile"))));
    files.sort();
    files
}

fn workflow(name: &str) -> String {
    read(&repo_dir().join(".github/workflows").join(name))
}

fn nsis_template() -> String {
    let nsis = packager()["nsis"].as_table().expect("[nsis]").clone();
    let path = crate_dir().join(text(&nsis, "template"));
    read(&path)
}

/// SEC-7: no package is code-signed or notarized in v1, and the macOS app
/// is ad hoc signed. The packaging config sets no Windows certificate or
/// sign command and no macOS identity but `-`; nothing that builds the
/// packages names signing or notarization credentials, and `codesign`
/// signs only with `-`. Update packages are signed by M6-28b's own step,
/// never by cargo-packager's or cargo-generate-rpm's signer.
#[test]
fn sec_7_packages_are_unsigned_and_macos_is_ad_hoc() {
    let packager = packager();
    let macos = packager["macos"].as_table().expect("[macos]");
    assert_eq!(
        text(macos, "signing-identity"),
        "-",
        "macOS is ad hoc signed"
    );
    for key in ["provider-short-name", "provider_short_name", "entitlements"] {
        assert!(!macos.contains_key(key), "[macos] sets {key}");
    }
    if let Some(windows) = packager.get("windows").and_then(Value::as_table) {
        for key in [
            "certificate-thumbprint",
            "certificate_thumbprint",
            "sign-command",
            "sign_command",
            "timestamp-url",
            "timestamp_url",
            "tsp",
            "digest-algorithm",
            "digest_algorithm",
        ] {
            assert!(!windows.contains_key(key), "[windows] sets {key}");
        }
    }
    let rpm = metadata("generate-rpm");
    assert!(
        !rpm.keys().any(|key| key.contains("sign")),
        "the .rpm is not signed"
    );

    let credentials = [
        "APPLE_CERTIFICATE",
        "APPLE_SIGNING_IDENTITY",
        "APPLE_ID",
        "APPLE_PASSWORD",
        "APPLE_TEAM_ID",
        "APPLE_API_ISSUER",
        "APPLE_API_KEY",
        "notarytool",
        "stapler",
        "signtool",
        "certificate-thumbprint",
        "certificateThumbprint",
        "sign-command",
        "signCommand",
        "CARGO_PACKAGER_SIGN_PRIVATE_KEY",
        "CARGO_PACKAGER_SIGN_PRIVATE_KEY_PASSWORD",
        "--private-key",
        "--signing-key",
    ];
    let mut codesigns = 0;
    for (name, text) in build_files() {
        for credential in credentials {
            assert!(!text.contains(credential), "{name} mentions {credential}");
        }
        for line in text.lines().filter(|line| line.contains("codesign")) {
            let words: Vec<&str> = line.split_whitespace().collect();
            for (index, word) in words.iter().enumerate() {
                if *word == "-s" || *word == "--sign" {
                    codesigns += 1;
                    assert_eq!(
                        words.get(index + 1).copied(),
                        Some("-"),
                        "{name} signs with an identity: {line}"
                    );
                }
            }
        }
    }
    assert!(
        codesigns > 0,
        "the .dmg is signed ad hoc with codesign -s -"
    );
}

/// PLAT-4: the packages keep 1.0.0-rc.1's file names, which
/// `tools/release-manifest` finds and the download page links to.
/// cargo-packager names the installer `<main binary>_<v>_x64-setup.exe`,
/// the disk image `<product name>_<v>_universal.dmg`, the `.deb`
/// `<main binary>_<v>_amd64.deb` and the AppImage
/// `<main binary>_<v>_x86_64.AppImage`, which the workflow renames to
/// rc.1's `amd64`. cargo-generate-rpm names the `.rpm`
/// `<name>-<rpm v>-<release>.x86_64.rpm`, release 1 by default, where the
/// RPM version has `~` for `-`; the workflow renames it to rc.1's name. The
/// workflow makes the portable zip and the macOS update archive.
#[test]
fn plat_4_package_names_match_release_manifest() {
    let version = workspace_version();
    let packager = packager();
    let product = text(&packager, "product-name");
    let binary = main_binary(&packager);
    let rpm = metadata("generate-rpm");
    let rpm_name = text(&rpm, "name");
    let rpm_release = rpm.get("release").map_or("1".to_owned(), |release| {
        release.to_string().replace('"', "")
    });
    assert!(
        !packager.contains_key("version"),
        "the packages take the workspace version"
    );
    assert!(
        !rpm.contains_key("version"),
        "the .rpm takes the workspace version"
    );

    let packages = workflow("packages.yml");
    // The workflow's own names, with `${VERSION}` as it writes it.
    for made in [
        "gitcroc-${VERSION}-windows-x64-portable.zip",
        "gitcroc.app.tar.gz",
        "mv \"dist/gitcroc_${VERSION}_x86_64.AppImage\" \"dist/gitcroc_${VERSION}_amd64.AppImage\"",
        "--set-metadata \"version = \\\"${VERSION//-/\\~}\\\"\"",
        "mv \"dist/gitcroc-${VERSION//-/\\~}-1.x86_64.rpm\" \"dist/gitcroc-${VERSION}-1.x86_64.rpm\"",
    ] {
        assert!(packages.contains(made), "packages.yml makes {made}");
    }
    assert_eq!(binary, "gitcroc", "the AppImage the workflow renames");
    assert_eq!(rpm_name, "gitcroc", "the .rpm the workflow renames");
    assert_eq!(rpm_release, "1", "the .rpm the workflow renames");

    let names = [
        format!("{binary}_{version}_x64-setup.exe"),
        format!("gitcroc-{version}-windows-x64-portable.zip"),
        format!("{product}_{version}_universal.dmg"),
        "gitcroc.app.tar.gz".to_owned(),
        format!("gitcroc_{version}_amd64.AppImage"),
        format!("{binary}_{version}_amd64.deb"),
        format!("{rpm_name}-{version}-{rpm_release}.x86_64.rpm"),
    ];
    let found = release_manifest::Packages::find(&names, &version)
        .unwrap_or_else(|e| panic!("release-manifest finds {names:?}: {e}"));
    // rc.1's names, as RELEASING.md lists them.
    assert_eq!(found.installer, format!("gitcroc_{version}_x64-setup.exe"));
    assert_eq!(
        found.portable,
        format!("gitcroc-{version}-windows-x64-portable.zip")
    );
    assert_eq!(found.dmg, format!("gitcroc_{version}_universal.dmg"));
    assert_eq!(found.app_archive, "gitcroc.app.tar.gz");
    assert_eq!(found.appimage, format!("gitcroc_{version}_amd64.AppImage"));
    assert_eq!(found.deb, format!("gitcroc_{version}_amd64.deb"));
    assert_eq!(found.rpm, format!("gitcroc-{version}-1.x86_64.rpm"));

    // The app inside them is gitcroc, `dev.gitcroc.desktop` (ADR 0002).
    assert_eq!(product, "gitcroc");
    assert_eq!(text(&packager, "identifier"), "dev.gitcroc.desktop");
    assert_eq!(text(&packager, "publisher"), "gitcroc");
}

/// D6: GPUI panics at its first text when no font is installed (M6-03), and
/// no font is embedded, so the `.deb` and the `.rpm` depend on one. The rest
/// are the libraries M6-03's `ldd` found linked or opened at run time.
#[test]
fn d6_linux_packages_depend_on_a_font() {
    let packager = packager();
    let deb = packager["deb"].as_table().expect("[deb]");
    let depends: Vec<&str> = deb["depends"]
        .as_array()
        .expect("depends is a list")
        .iter()
        .map(|item| item.as_str().expect("a dependency"))
        .collect();
    assert!(depends.contains(&"fonts-dejavu-core"), "{depends:?}");
    assert_eq!(
        depends,
        [
            "libc6 (>= 2.35)",
            "libxcb1",
            "libxkbcommon0",
            "libxkbcommon-x11-0",
            "libvulkan1",
            "libegl1",
            "libwayland-client0",
            "libwayland-egl1",
            "fonts-dejavu-core",
        ]
    );

    let rpm = metadata("generate-rpm");
    let requires = rpm["requires"].as_table().expect("[requires]");
    assert!(requires.contains_key("dejavu-sans-fonts"), "{requires:?}");
    let mut names: Vec<&str> = requires.keys().map(String::as_str).collect();
    names.sort_unstable();
    assert_eq!(
        names,
        [
            "dejavu-sans-fonts",
            "libglvnd-egl",
            "libwayland-client",
            "libwayland-egl",
            "libxcb",
            "libxkbcommon",
            "libxkbcommon-x11",
            "vulkan-loader",
        ]
    );
    assert!(
        requires
            .values()
            .all(|version| version.as_str() == Some("*")),
        "{requires:?}"
    );
}

/// D6, PLAT-4: 1.0.0-rc.1's `.deb` and `.rpm` have no epoch, and their
/// version `1.0.0-rc.1` sorts after `1.0.0` in dpkg and rpm. So from rc.2 on
/// both carry epoch 1 and write a pre-release with `~` (`1:1.0.0~rc.2`, then
/// `1:1.0.0`), which makes rc.1 to rc.2 to 1.0.0 upgrades (M6-31, Eric,
/// 2026-11-06). An epoch is never lowered or removed.
#[test]
fn d6_linux_packages_carry_epoch_1() {
    let rpm = metadata("generate-rpm");
    assert_eq!(
        rpm.get("epoch").and_then(Value::as_integer),
        Some(1),
        "[package.metadata.generate-rpm] epoch"
    );

    let packages = workflow("packages.yml");
    // The .rpm's version: the workspace's with `~` for `-`.
    assert!(
        packages.contains(r#"--set-metadata "version = \"${VERSION//-/\~}\"""#),
        "packages.yml sets the .rpm's version with ~"
    );
    // cargo-packager has no epoch setting, so the .deb's control file is
    // rewritten after it is built, and the result checked.
    assert!(
        packages.contains(r#"deb_version="1:${VERSION//-/\~}""#),
        "packages.yml sets the .deb's version to 1:<version with ~>"
    );
    assert!(
        packages.contains(r#"test "$(dpkg-deb -f "$deb" Version)" = "$deb_version""#),
        "packages.yml checks the .deb's version"
    );
    assert!(
        packages.contains("dpkg-deb -b --root-owner-group"),
        "packages.yml repacks the .deb with root as owner"
    );
}

/// PLAT-4: the GPUI installer installs over 1.0.0-rc.1's, so its template
/// keeps Tauri's install directory `%LOCALAPPDATA%\gitcroc`, uninstall key
/// and the `MainBinaryName` value, and handles rc.1's updater's `/UPDATE`
/// and `/ARGS` as Tauri's template does.
#[test]
fn plat_4_nsis_template_keeps_tauris_install_location() {
    let packager = packager();
    let nsis = packager["nsis"].as_table().expect("[nsis]");
    assert_eq!(text(nsis, "installer-mode"), "currentUser");
    assert_eq!(text(&packager, "product-name"), "gitcroc");
    assert_eq!(text(&packager, "publisher"), "gitcroc");
    assert_eq!(main_binary(&packager), "gitcroc");

    let template = nsis_template();
    for line in [
        "!define PRODUCTNAME \"{{product_name}}\"",
        "!define MANUFACTURER \"{{manufacturer}}\"",
        "!define MAINBINARYNAME \"{{main_binary_name}}\"",
        // %LOCALAPPDATA%\gitcroc for the current user.
        "StrCpy $INSTDIR \"$LOCALAPPDATA\\${PRODUCTNAME}\"",
        // HKCU\Software\Microsoft\Windows\CurrentVersion\Uninstall\gitcroc
        "!define UNINSTKEY \"Software\\Microsoft\\Windows\\CurrentVersion\\Uninstall\\${PRODUCTNAME}\"",
        // HKCU\Software\gitcroc\gitcroc, where the install directory is kept.
        "!define MANUPRODUCTKEY \"Software\\${MANUFACTURER}\\${PRODUCTNAME}\"",
        "WriteRegStr SHCTX \"${UNINSTKEY}\" \"MainBinaryName\" \"${MAINBINARYNAME}.exe\"",
        "DeleteRegKey HKCU \"${UNINSTKEY}\"",
        // Tauri's /UPDATE and /ARGS.
        "${GetOptions} $CMDLINE \"/UPDATE\" $UpdateMode",
        "${GetOptions} $CMDLINE \"/ARGS\" $R0",
    ] {
        assert!(
            template.lines().any(|candidate| candidate.trim() == line),
            "the template has: {line}"
        );
    }
    assert!(
        packager["windows"]
            .get("allow-downgrades")
            .and_then(Value::as_bool)
            == Some(false),
        "a downgrade is refused"
    );

    // The comment at the top names the template it is based on, and the
    // workflow installs that cargo-packager.
    let header: String = template
        .lines()
        .take_while(|line| line.starts_with(';'))
        .collect::<Vec<_>>()
        .join("\n");
    assert!(
        header.contains(&format!("cargo-packager {CARGO_PACKAGER}")),
        "{header}"
    );
    let packages = workflow("packages.yml");
    assert!(packages.contains(&format!("cargo-packager@{CARGO_PACKAGER}")));
}
