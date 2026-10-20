//! PLAT-1: platform differences sit behind one trait per concern, in
//! `src/platform`. This scans the crate's sources for platform `cfg`s.

use std::fs;
use std::path::{Path, PathBuf};

const PLATFORM_DIR: &str = "src/platform";

/// The `cfg` predicates that pick a platform.
const PLATFORM_PREDICATES: [&str; 6] = [
    "unix",
    "windows",
    "target_os",
    "target_family",
    "target_vendor",
    "target_env",
];

/// The items a platform `cfg` may pick in `src/platform`: a module of
/// implementations, or the `use` that names one as the platform's.
const PICKS: [&str; 4] = ["mod ", "pub mod ", "use ", "pub use "];

fn sources(dir: &Path, found: &mut Vec<PathBuf>) {
    for entry in fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            sources(&path, found);
        } else if path.extension().is_some_and(|ext| ext == "rs") {
            found.push(path);
        }
    }
}

/// Whether `line` is a `cfg` attribute, or a `cfg_attr` whose condition,
/// names a platform. Quoted values, such as a target's name, are not
/// predicates.
fn platform_cfg(line: &str) -> bool {
    let code = line.trim_start();
    let code = code
        .strip_prefix("#![")
        .or_else(|| code.strip_prefix("#["))
        .unwrap_or_default();
    let (condition, attr) = if let Some(rest) = code.strip_prefix("cfg_attr(") {
        (rest, true)
    } else if let Some(rest) = code.strip_prefix("cfg(") {
        (rest, false)
    } else {
        return false;
    };
    // The condition ends at the first comma outside parentheses.
    let mut depth = 0;
    let mut in_string = false;
    let mut words = String::new();
    for c in condition.chars() {
        match c {
            '"' => in_string = !in_string,
            _ if in_string => continue,
            '(' => depth += 1,
            ')' if depth == 0 => break,
            ')' => depth -= 1,
            ',' if depth == 0 && attr => break,
            _ => {}
        }
        words.push(if c.is_alphanumeric() || c == '_' {
            c
        } else {
            ' '
        });
    }
    words
        .split_whitespace()
        .any(|word| PLATFORM_PREDICATES.contains(&word))
}

#[test]
fn plat_1_platform_code_sits_behind_traits() {
    // The scan sees a platform cfg where there is one, and only there.
    assert!(platform_cfg("#[cfg(target_os = \"linux\")]"));
    assert!(platform_cfg("    #[cfg(not(windows))]"));
    assert!(platform_cfg("#[cfg_attr(unix, allow(dead_code))]"));
    assert!(!platform_cfg(
        "#[cfg(any(debug_assertions, feature = \"perf-trace\"))]"
    ));
    assert!(!platform_cfg("#[cfg(test)]"));
    assert!(!platform_cfg(
        "#![cfg_attr(not(debug_assertions), windows_subsystem = \"windows\")]"
    ));

    let crate_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut files = Vec::new();
    sources(&crate_dir.join("src"), &mut files);
    let mut platform_files = 0;
    for file in files {
        let shown = file
            .strip_prefix(crate_dir)
            .unwrap()
            .to_string_lossy()
            .replace('\\', "/");
        let in_platform = shown.starts_with(PLATFORM_DIR);
        platform_files += usize::from(in_platform);
        let text = fs::read_to_string(&file).unwrap();
        let lines: Vec<&str> = text.lines().collect();
        for (at, line) in lines.iter().enumerate() {
            if !platform_cfg(line) {
                continue;
            }
            assert!(
                in_platform,
                "{shown}:{}: platform code outside {PLATFORM_DIR}; put it behind a trait there: {line}",
                at + 1
            );
            // The item the attribute applies to, past any other attributes.
            let item = lines[at + 1..]
                .iter()
                .map(|line| line.trim_start())
                .find(|line| !line.starts_with("#["))
                .unwrap_or_default();
            assert!(
                PICKS.iter().any(|pick| item.starts_with(pick)),
                "{shown}:{}: a platform cfg in {PLATFORM_DIR} only picks a module or an implementation, not `{item}`",
                at + 1
            );
        }
    }
    assert!(platform_files > 1, "no sources found in {PLATFORM_DIR}");
}

/// Each argument of `launch` is `path` whole, or does not hold it at all:
/// the path is never split or joined into another argument (ARCH-3).
fn path_args(launch: &gitcroc_app::platform::Launch, path: &Path) -> usize {
    let whole = path.as_os_str();
    let text = path.to_string_lossy();
    launch
        .args
        .iter()
        .filter(|arg| {
            let arg_text = arg.to_string_lossy();
            assert!(
                *arg == whole || !arg_text.contains(text.as_ref()),
                "{launch:?}: the path is part of an argument"
            );
            *arg == whole
        })
        .count()
}

#[test]
fn arch_3_open_commands_take_the_path_as_one_argument() {
    use gitcroc_app::platform::{FileManager, PlatformFileManager, PlatformTerminal, Terminal};

    let path = std::env::temp_dir().join("my repo; 'quoted' & -dashed");
    let shown = PlatformFileManager::show(&path);
    assert!(
        ["xdg-open", "open", "explorer"].contains(&shown.program.to_string_lossy().as_ref()),
        "{shown:?}"
    );
    assert_eq!(path_args(&shown, &path), 1, "{shown:?}");
    assert_eq!(shown.cwd, path);

    let terminals = PlatformTerminal::open_in(&path);
    assert!(!terminals.is_empty());
    for launch in &terminals {
        // In the folder, and given the path at most once, as `wt -d` is.
        assert_eq!(launch.cwd, path, "{launch:?}");
        assert!(path_args(launch, &path) <= 1, "{launch:?}");
    }
}

#[test]
fn arch_3_the_download_page_is_one_argument() {
    use gitcroc_app::platform::{PlatformBrowser, WebBrowser};

    let url = gitcroc_app::update::RELEASES_PAGE;
    let launch = PlatformBrowser::open(url);
    assert!(
        ["xdg-open", "open", "explorer"].contains(&launch.program.to_string_lossy().as_ref()),
        "{launch:?}"
    );
    assert_eq!(launch.args, [std::ffi::OsString::from(url)], "{launch:?}");
    assert!(launch.cwd.is_dir(), "{launch:?}");
}

#[test]
fn plat_1_a_missing_terminal_names_what_was_tried() {
    use gitcroc_app::platform::{Launch, LaunchError, start};

    let dir = tempfile::tempdir().unwrap();
    let missing = ["gitcroc-no-such-terminal", "gitcroc-nor-this-one"]
        .map(|program| Launch::new(program, [], dir.path().to_owned()));
    let error = start(&missing).unwrap_err();
    assert!(matches!(error, LaunchError::NotFound { .. }), "{error:?}");
    assert_eq!(
        error.to_string(),
        "none of gitcroc-no-such-terminal, gitcroc-nor-this-one was found"
    );
}

#[test]
fn plat_1_a_repo_folder_that_is_gone_is_reported_and_nothing_starts() {
    use gitcroc_app::platform::{
        FileManager, LaunchError, PlatformFileManager, PlatformTerminal, Terminal, start,
    };

    let dir = tempfile::tempdir().unwrap();
    let gone = dir.path().join("deleted repo");
    // The folder is checked before any program is started.
    for candidates in [
        vec![PlatformFileManager::show(&gone)],
        PlatformTerminal::open_in(&gone),
    ] {
        let error = start(&candidates).unwrap_err();
        assert!(matches!(error, LaunchError::NoFolder { .. }), "{error:?}");
        assert_eq!(
            error.to_string(),
            format!("the folder {} does not exist", gone.display())
        );
    }
}
