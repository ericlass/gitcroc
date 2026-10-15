//! Requirement coverage (QA-3): finds the requirement IDs a milestone's task
//! files implement and reports those that no test mentions.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// Errors returned by `req-coverage`.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("I/O error on {path}: {source}")]
    Io {
        path: PathBuf,
        source: std::io::Error,
    },
    #[error("{task}: {item:?} on an \"{label}\" line is not a requirement ID")]
    NotAnId {
        task: String,
        label: &'static str,
        item: String,
    },
    #[error("no task files match docs/tasks/{0}-*.md")]
    NoTasks(String),
}

const IMPLEMENTS: &str = "Implements:";
const NO_TEST_NEEDED: &str = "No test needed:";

/// The IDs one task file names on its "Implements:" and "No test needed:" lines.
#[derive(Debug, Default, PartialEq, Eq)]
struct TaskIds {
    implements: Vec<String>,
    /// ID and the reason given in parentheses (empty when there is none).
    no_test_needed: Vec<(String, String)>,
}

/// Reads the requirement IDs from the text of a task file. `task` names the
/// file in error messages.
fn parse_task(task: &str, text: &str) -> Result<TaskIds, Error> {
    let mut ids = TaskIds::default();
    for line in text.lines().map(str::trim) {
        if let Some(rest) = line.strip_prefix(IMPLEMENTS) {
            for (id, _) in parse_items(task, IMPLEMENTS, rest)? {
                ids.implements.push(id);
            }
        } else if let Some(rest) = line.strip_prefix(NO_TEST_NEEDED) {
            ids.no_test_needed
                .extend(parse_items(task, NO_TEST_NEEDED, rest)?);
        }
    }
    Ok(ids)
}

/// Splits `QA-1 (note), ARCH-5` into `[("QA-1", "note"), ("ARCH-5", "")]`.
/// Commas inside parentheses do not split.
fn parse_items(
    task: &str,
    label: &'static str,
    list: &str,
) -> Result<Vec<(String, String)>, Error> {
    let mut items = Vec::new();
    let mut depth = 0usize;
    let mut start = 0;
    for (i, c) in list.char_indices() {
        match c {
            '(' => depth += 1,
            ')' => depth = depth.saturating_sub(1),
            ',' if depth == 0 => {
                items.push(&list[start..i]);
                start = i + 1;
            }
            _ => {}
        }
    }
    items.push(&list[start..]);

    items
        .into_iter()
        .map(str::trim)
        .filter(|item| !item.is_empty())
        .map(|item| {
            let (id, note) = match item.split_once('(') {
                Some((id, note)) => (id.trim(), note.trim_end().trim_end_matches(')').trim()),
                None => (item, ""),
            };
            if is_id(id) {
                Ok((id.to_owned(), note.to_owned()))
            } else {
                Err(Error::NotAnId {
                    task: task.to_owned(),
                    label,
                    item: item.to_owned(),
                })
            }
        })
        .collect()
}

/// A requirement ID: upper-case letters, an optional hyphen, digits
/// (`GRP-6`, `D14`).
fn is_id(s: &str) -> bool {
    let letters = s.bytes().take_while(u8::is_ascii_uppercase).count();
    let rest = &s[letters..];
    let digits = rest.strip_prefix('-').unwrap_or(rest);
    letters > 0 && !digits.is_empty() && digits.bytes().all(|b| b.is_ascii_digit())
}

/// Whether `text` mentions `id` as `GRP-6` or as `grp_6`. The match must not
/// continue a longer word before it or a longer number after it, so `GRP-6`
/// is not found in `GRP-60` and `grp_6` is found in `fn grp_6_works`.
fn mentions(text: &str, id: &str) -> bool {
    let snake = id.to_ascii_lowercase().replace('-', "_");
    [id, snake.as_str()].into_iter().any(|form| {
        text.match_indices(form).any(|(at, _)| {
            let before = text[..at].bytes().next_back();
            let after = text[at + form.len()..].bytes().next();
            !before.is_some_and(|b| b.is_ascii_alphanumeric())
                && !after.is_some_and(|b| b.is_ascii_digit())
        })
    })
}

const SKIPPED_DIRS: [&str; 4] = ["target", "node_modules", "dist", ".git"];
const UI_TEST_DIRS: [&str; 3] = ["tests", "e2e", "__tests__"];
const UI_EXTENSIONS: [&str; 8] = ["ts", "tsx", "mts", "cts", "js", "jsx", "mjs", "cjs"];

/// The part of a source file that is test code, if any.
///
/// Rust: a whole file under a `tests/` directory, otherwise everything from
/// the first `#[cfg(test)]`-style attribute on. Front end: files named
/// `*.test.*` or `*.spec.*`, or under `tests/`, `e2e/` or `__tests__/`.
fn test_code<'a>(path: &Path, text: &'a str) -> Option<&'a str> {
    let in_dir = |names: &[&str]| {
        path.parent().is_some_and(|dir| {
            dir.components()
                .any(|c| names.iter().any(|n| c.as_os_str() == *n))
        })
    };
    let extension = path.extension().and_then(|e| e.to_str()).unwrap_or("");
    let file_name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");

    if extension == "rs" {
        if in_dir(&["tests"]) {
            return Some(text);
        }
        let mut offset = 0;
        for line in text.split_inclusive('\n') {
            let line_trimmed = line.trim_start();
            if line_trimmed.starts_with("#[cfg(") && line_trimmed.contains("test") {
                return Some(&text[offset..]);
            }
            offset += line.len();
        }
        None
    } else if UI_EXTENSIONS.contains(&extension) {
        let named_as_test = file_name.contains(".test.") || file_name.contains(".spec.");
        (named_as_test || in_dir(&UI_TEST_DIRS)).then_some(text)
    } else {
        None
    }
}

/// The test code of every source file under `dir`, recursively. A missing
/// `dir` yields nothing.
fn collect_test_code(dir: &Path) -> Result<Vec<String>, Error> {
    let mut found = Vec::new();
    if dir.is_dir() {
        walk(dir, dir, &mut found)?;
    }
    Ok(found)
}

fn walk(base: &Path, dir: &Path, found: &mut Vec<String>) -> Result<(), Error> {
    let io = |source| Error::Io {
        path: dir.to_owned(),
        source,
    };
    let mut entries = std::fs::read_dir(dir)
        .map_err(io)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(io)?;
    entries.sort_by_key(|e| e.file_name());
    for entry in entries {
        let path = entry.path();
        let file_type = entry.file_type().map_err(io)?;
        if file_type.is_dir() {
            if !SKIPPED_DIRS.iter().any(|s| entry.file_name() == *s) {
                walk(base, &path, found)?;
            }
        } else if file_type.is_file() {
            let relative = path.strip_prefix(base).unwrap_or(&path);
            let extension = relative.extension().and_then(|e| e.to_str()).unwrap_or("");
            if extension != "rs" && !UI_EXTENSIONS.contains(&extension) {
                continue;
            }
            let text = std::fs::read_to_string(&path).map_err(|source| Error::Io {
                path: path.clone(),
                source,
            })?;
            if let Some(code) = test_code(relative, &text) {
                found.push(code.to_owned());
            }
        }
    }
    Ok(())
}

/// What `run` found for one milestone.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Report {
    /// Implemented IDs a test mentions.
    pub covered: Vec<String>,
    /// Implemented IDs no test mentions.
    pub missing: Vec<String>,
    /// IDs a task marks as proven without a test: ID, task, reason.
    pub no_test_needed: Vec<(String, String, String)>,
}

/// One task file: its name (`M0-02-quality-gates`) and its IDs.
type Task = (String, TaskIds);

/// What the tasks of a milestone say about one implemented ID.
#[derive(Default)]
struct Claims<'a> {
    /// Tasks that mark the ID as proven without a test, with their reasons.
    exemptions: Vec<(&'a str, &'a str)>,
    /// Whether some task implements the ID without exempting it.
    needs_test: bool,
}

/// Sorts each implemented ID into covered, missing or no test needed. An ID
/// counts as no test needed only when every task implementing it says so.
fn report(tasks: &[Task], test_code: &[String]) -> Report {
    let mut ids: BTreeMap<&str, Claims> = BTreeMap::new();
    for (name, task) in tasks {
        for id in &task.implements {
            let claims = ids.entry(id).or_default();
            match task.no_test_needed.iter().find(|(exempt, _)| exempt == id) {
                Some((_, reason)) => claims.exemptions.push((name, reason)),
                None => claims.needs_test = true,
            }
        }
    }

    let mut out = Report::default();
    for (id, claims) in ids {
        if !claims.needs_test {
            out.no_test_needed.extend(
                claims
                    .exemptions
                    .into_iter()
                    .map(|(task, reason)| (id.to_owned(), task.to_owned(), reason.to_owned())),
            );
        } else if test_code.iter().any(|code| mentions(code, id)) {
            out.covered.push(id.to_owned());
        } else {
            out.missing.push(id.to_owned());
        }
    }
    out
}

/// Reads `docs/tasks/<milestone>-*.md` and the test code under `crates/` and
/// `ui/` of the repository at `root`, and reports the milestone's coverage.
pub fn run(root: &Path, milestone: &str) -> Result<Report, Error> {
    let tasks_dir = root.join("docs/tasks");
    let io = |source| Error::Io {
        path: tasks_dir.clone(),
        source,
    };
    let prefix = format!("{milestone}-");
    let mut tasks = Vec::new();
    let mut entries = std::fs::read_dir(&tasks_dir)
        .map_err(io)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(io)?;
    entries.sort_by_key(|e| e.file_name());
    for entry in entries {
        let path = entry.path();
        let Some(name) = path
            .file_name()
            .and_then(|n| n.to_str())
            .and_then(|n| n.strip_suffix(".md"))
            .filter(|n| n.starts_with(&prefix))
        else {
            continue;
        };
        let text = std::fs::read_to_string(&path).map_err(|source| Error::Io {
            path: path.clone(),
            source,
        })?;
        tasks.push((name.to_owned(), parse_task(name, &text)?));
    }
    if tasks.is_empty() {
        return Err(Error::NoTasks(milestone.to_owned()));
    }

    let mut code = collect_test_code(&root.join("crates"))?;
    code.extend(collect_test_code(&root.join("ui"))?);
    Ok(report(&tasks, &code))
}

#[cfg(test)]
mod tests {
    use super::*;

    const TASK: &str = "\
# M0-03 CI

## Spec
Implements: QA-1 (Rust half; M0-04 adds the front-end half), ARCH-5, D14
No test needed: QA-1 (the workflow itself is the proof)
Respects: D6, section 4 \"Crates\" table

## In scope
- Mentions GRP-6 in prose, which is not an Implements line.
";

    #[test]
    fn qa_3_extracts_ids_from_implements_line() {
        let ids = parse_task("M0-03", TASK).unwrap();
        assert_eq!(ids.implements, ["QA-1", "ARCH-5", "D14"]);
    }

    #[test]
    fn qa_3_extracts_no_test_needed_with_reason() {
        let ids = parse_task("M0-03", TASK).unwrap();
        assert_eq!(
            ids.no_test_needed,
            [(
                "QA-1".to_owned(),
                "the workflow itself is the proof".to_owned()
            )]
        );
    }

    #[test]
    fn qa_3_rejects_implements_item_that_is_not_an_id() {
        let err = parse_task("M9-01", "Implements: QA-1, section 4").unwrap_err();
        assert!(matches!(err, Error::NotAnId { ref item, .. } if item == "section 4"));
    }

    #[test]
    fn qa_3_matches_upper_case_form() {
        assert!(mentions("// Covers GRP-6.", "GRP-6"));
    }

    #[test]
    fn qa_3_matches_snake_case_form() {
        assert!(mentions(
            "fn grp_6_one_failure_does_not_stop_others()",
            "GRP-6"
        ));
        assert!(mentions("fn test_grp_6()", "GRP-6"));
        assert!(mentions("fn d14_license_is_mit()", "D14"));
    }

    #[test]
    fn qa_3_does_not_match_longer_ids() {
        assert!(!mentions("GRP-60 and grp_61_x", "GRP-6"));
        assert!(!mentions("SUBGRP-6 and subgrp_6", "GRP-6"));
        assert!(!mentions("grp-6", "GRP-6"));
    }

    #[test]
    fn qa_3_reports_missing_id() {
        let tasks = vec![("M0-03".to_owned(), parse_task("M0-03", TASK).unwrap())];
        let code = vec!["fn arch_5_core_has_no_tauri() {}".to_owned()];
        let report = report(&tasks, &code);
        assert_eq!(report.covered, ["ARCH-5"]);
        assert_eq!(report.missing, ["D14"]);
        assert_eq!(
            report.no_test_needed,
            [(
                "QA-1".to_owned(),
                "M0-03".to_owned(),
                "the workflow itself is the proof".to_owned()
            )]
        );
    }

    #[test]
    fn qa_3_id_needs_test_if_any_implementing_task_does_not_exempt_it() {
        let tasks = vec![
            ("M0-03".to_owned(), parse_task("M0-03", TASK).unwrap()),
            (
                "M0-08".to_owned(),
                parse_task("M0-08", "Implements: QA-1").unwrap(),
            ),
        ];
        let report = report(&tasks, &[]);
        assert!(report.missing.contains(&"QA-1".to_owned()));
        assert!(report.no_test_needed.is_empty());
    }

    #[test]
    fn qa_3_rust_test_code_is_tests_dir_or_cfg_test_module() {
        let src = "// GRP-1 in production code\n#[cfg(test)]\nmod tests { fn grp_2() {} }\n";
        let code = test_code(Path::new("gitcroc-core/src/lib.rs"), src).unwrap();
        assert!(!mentions(code, "GRP-1"));
        assert!(mentions(code, "GRP-2"));

        assert_eq!(
            test_code(Path::new("gitcroc-core/tests/groups.rs"), src),
            Some(src)
        );
        assert_eq!(
            test_code(Path::new("gitcroc-core/src/main.rs"), "// GRP-1"),
            None
        );
    }

    #[test]
    fn qa_3_ui_test_code_is_test_files_only() {
        let text = "test('ui_1 ...')";
        assert!(test_code(Path::new("src/tree.test.ts"), text).is_some());
        assert!(test_code(Path::new("src/tree.spec.tsx"), text).is_some());
        assert!(test_code(Path::new("e2e/tree.ts"), text).is_some());
        assert!(test_code(Path::new("src/tree.ts"), text).is_none());
        assert!(test_code(Path::new("tests/notes.md"), text).is_none());
    }
}
