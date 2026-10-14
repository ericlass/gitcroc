//! PLAT-1: platform differences sit behind one trait per concern, in
//! `src/platform`. This scans the crate's sources for platform `cfg`s.

// Integration test crate: helpers outside #[test] fns are test code too (QA-4).
#![allow(clippy::unwrap_used, clippy::expect_used)]

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
