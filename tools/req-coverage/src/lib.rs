//! Requirement coverage (QA-3): finds the requirement IDs a milestone's task
//! files implement, or that the whole spec defines, and reports those that no
//! test mentions.

use std::collections::{BTreeMap, BTreeSet};
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

/// The requirement IDs the text of a spec file defines, in order of first
/// appearance: every bold `**ID**`, and the first cell of every table row that
/// is `D<n>` (the decision log). Bold words that are not IDs are ignored.
fn parse_spec(text: &str) -> Vec<String> {
    let mut ids = Vec::new();
    let mut push = |id: &str| {
        if !ids.iter().any(|known| known == id) {
            ids.push(id.to_owned());
        }
    };
    for line in text.lines().map(str::trim) {
        if let Some(cell) = line
            .strip_prefix('|')
            .and_then(|row| row.split('|').next())
            .map(str::trim)
            .filter(|cell| cell.starts_with('D') && is_id(cell) && !cell.contains('-'))
        {
            push(cell);
        }
        let mut parts = line.split("**");
        parts.next();
        while let (Some(bold), Some(_)) = (parts.next(), parts.next()) {
            if is_id(bold) {
                push(bold);
            }
        }
    }
    ids
}

/// Sorts each spec ID into covered (a test mentions it), no test needed (a
/// task says so; the first such task in file order is named) or missing.
fn spec_report(spec_ids: &[String], tasks: &[Task], test_code: &[String]) -> Report {
    let ids: BTreeSet<&String> = spec_ids.iter().collect();
    let mut out = Report::default();
    for id in ids {
        if test_code.iter().any(|code| mentions(code, id)) {
            out.covered.push(id.clone());
        } else if let Some((name, reason)) = tasks.iter().find_map(|(name, task)| {
            task.no_test_needed
                .iter()
                .find(|(exempt, _)| exempt == id)
                .map(|(_, reason)| (name, reason))
        }) {
            out.no_test_needed
                .push((id.clone(), name.clone(), reason.clone()));
        } else {
            out.missing.push(id.clone());
        }
    }
    out
}

/// The `.md` files of `dir` whose name (without `.md`) passes `keep`, sorted
/// by name, with their text.
fn read_markdown(dir: &Path, keep: impl Fn(&str) -> bool) -> Result<Vec<(String, String)>, Error> {
    let io = |source| Error::Io {
        path: dir.to_owned(),
        source,
    };
    let mut entries = std::fs::read_dir(dir)
        .map_err(io)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(io)?;
    entries.sort_by_key(|e| e.file_name());
    let mut files = Vec::new();
    for entry in entries {
        let path = entry.path();
        let Some(name) = path
            .file_name()
            .and_then(|n| n.to_str())
            .and_then(|n| n.strip_suffix(".md"))
            .filter(|n| keep(n))
        else {
            continue;
        };
        let text = std::fs::read_to_string(&path).map_err(|source| Error::Io {
            path: path.clone(),
            source,
        })?;
        files.push((name.to_owned(), text));
    }
    Ok(files)
}

/// The task files in `docs/tasks` whose name passes `keep`.
fn read_tasks(root: &Path, keep: impl Fn(&str) -> bool) -> Result<Vec<Task>, Error> {
    read_markdown(&root.join("docs/tasks"), keep)?
        .into_iter()
        .map(|(name, text)| {
            let ids = parse_task(&name, &text)?;
            Ok((name, ids))
        })
        .collect()
}

/// The test code under `crates/` and `ui/`.
fn all_test_code(root: &Path) -> Result<Vec<String>, Error> {
    let mut code = collect_test_code(&root.join("crates"))?;
    code.extend(collect_test_code(&root.join("ui"))?);
    Ok(code)
}

/// The milestone that rewrites the UI in GPUI, and where its tests live.
const PORT_MILESTONE: &str = "M6";
const OLD_UI: &str = "ui";
const NEW_UI: &str = "crates/gitcroc-ui";

/// The spec IDs a test of the old UI mentions and no test of the new one
/// does: what is still to port.
fn unported(spec_ids: &[String], old_ui: &[String], new_ui: &[String]) -> Vec<String> {
    let ids: BTreeSet<&String> = spec_ids.iter().collect();
    ids.into_iter()
        .filter(|id| old_ui.iter().any(|code| mentions(code, id)))
        .filter(|id| !new_ui.iter().any(|code| mentions(code, id)))
        .cloned()
        .collect()
}

/// Every requirement ID `docs/spec/*.md` defines.
fn spec_ids(root: &Path) -> Result<Vec<String>, Error> {
    let mut ids = Vec::new();
    for (_, text) in read_markdown(&root.join("docs/spec"), |_| true)? {
        ids.extend(parse_spec(&text));
    }
    Ok(ids)
}

/// Reads `docs/tasks/<milestone>-*.md` and the test code under `crates/` and
/// `ui/` of the repository at `root`, and reports the milestone's coverage.
/// For M6 the missing IDs also include every spec ID that a test under
/// `ui/` mentions and no test under `crates/gitcroc-ui/` does, so the list
/// is what the GPUI UI has still to port; M6-30 starts when it is empty.
pub fn run(root: &Path, milestone: &str) -> Result<Report, Error> {
    let prefix = format!("{milestone}-");
    let tasks = read_tasks(root, |name| name.starts_with(&prefix))?;
    if tasks.is_empty() {
        return Err(Error::NoTasks(milestone.to_owned()));
    }
    let mut out = report(&tasks, &all_test_code(root)?);
    if milestone == PORT_MILESTONE {
        let old_ui = collect_test_code(&root.join(OLD_UI))?;
        let new_ui = collect_test_code(&root.join(NEW_UI))?;
        let missing: BTreeSet<String> = out
            .missing
            .drain(..)
            .chain(unported(&spec_ids(root)?, &old_ui, &new_ui))
            .collect();
        out.covered.retain(|id| !missing.contains(id));
        out.missing = missing.into_iter().collect();
    }
    Ok(out)
}

/// Reads every requirement ID defined in `docs/spec/*.md`, the "No test
/// needed:" lines of every task file in `docs/tasks/` and the test code, and
/// reports the coverage of the whole spec.
pub fn run_spec(root: &Path) -> Result<Report, Error> {
    let ids = spec_ids(root)?;
    let tasks = read_tasks(root, |name| name != "README")?;
    Ok(spec_report(&ids, &tasks, &all_test_code(root)?))
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

    const SPEC: &str = "\
# 4. Architecture

- **ARCH-1** All repo writes go through the git CLI runner.
* **UI-34** The group commit dialog.
- **Mark resolved** is a button, not a requirement.

| ID | Topic | Decision | Source |
| --- | --- | --- | --- |
| D1 | Core language | Rust | Eric |
| D14 | License | MIT, see **ARCH-1** | Eric |
";

    #[test]
    fn qa_3_spec_mode_reads_bold_definitions_and_decision_rows() {
        assert_eq!(parse_spec(SPEC), ["ARCH-1", "UI-34", "D1", "D14"]);
    }

    #[test]
    fn qa_3_spec_mode_reports_missing_and_covered() {
        let code = vec!["fn arch_5_x() {}\n// UI-34 and d1_rust".to_owned()];
        let report = spec_report(&parse_spec(SPEC), &[], &code);
        assert_eq!(report.covered, ["D1", "UI-34"]);
        assert_eq!(report.missing, ["ARCH-1", "D14"]);
        assert!(report.no_test_needed.is_empty());
    }

    #[test]
    fn qa_3_spec_mode_takes_no_test_needed_from_any_task() {
        let tasks = vec![
            (
                "M2-04".to_owned(),
                parse_task("M2-04", "Implements: UI-34").unwrap(),
            ),
            (
                "M5-02".to_owned(),
                parse_task("M5-02", "No test needed: D14 (kept by review)").unwrap(),
            ),
        ];
        let report = spec_report(&parse_spec(SPEC), &tasks, &[]);
        assert_eq!(report.missing, ["ARCH-1", "D1", "UI-34"]);
        assert_eq!(
            report.no_test_needed,
            [(
                "D14".to_owned(),
                "M5-02".to_owned(),
                "kept by review".to_owned()
            )]
        );
    }

    #[test]
    fn qa_3_spec_mode_counts_an_id_once() {
        let spec = format!("{SPEC}\n- **ARCH-1** again\n| D1 | again | x | y |\n");
        assert_eq!(parse_spec(&spec), ["ARCH-1", "UI-34", "D1", "D14"]);

        let tasks = vec![
            (
                "M5-02".to_owned(),
                parse_task("M5-02", "No test needed: D1 (first)").unwrap(),
            ),
            (
                "M5-03".to_owned(),
                parse_task("M5-03", "No test needed: D1 (second)").unwrap(),
            ),
        ];
        let report = spec_report(&parse_spec(&spec), &tasks, &["// ARCH-1 ARCH-1".to_owned()]);
        assert_eq!(report.covered, ["ARCH-1"]);
        assert_eq!(report.missing, ["D14", "UI-34"]);
        assert_eq!(
            report.no_test_needed,
            [("D1".to_owned(), "M5-02".to_owned(), "first".to_owned())]
        );
    }

    /// Writes `files` (path, text) under a new temporary folder.
    fn tree(files: &[(&str, &str)]) -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        for (path, text) in files {
            let path = dir.path().join(path);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, text).unwrap();
        }
        dir
    }

    #[test]
    fn qa_3_m6_lists_ui_tests_not_yet_ported() {
        let spec = "- **UI-1** The tree.\n- **UI-2** The history.\n- **UI-3** The title bar.\n- **ARCH-6** Events.\n";
        let dir = tree(&[
            ("docs/spec/08-ui.md", spec),
            ("docs/tasks/M6-06-skeleton.md", "Implements: UI-3, ARCH-6\n"),
            ("docs/tasks/M5-01-old.md", "Implements: UI-2\n"),
            (
                "ui/src/tree.test.ts",
                "test('ui_1 rows'); test('UI-2 columns');",
            ),
            ("ui/src/tree.ts", "// UI-3, not a test"),
            (
                "crates/gitcroc-ui/tests/history.rs",
                "fn ui_2_columns() {}\nfn ui_3_buttons() {}",
            ),
            // UI-1 has a test, but not in the GPUI UI.
            (
                "crates/gitcroc-core/tests/tree.rs",
                "fn ui_1_rows_of_the_core() {}",
            ),
        ]);

        let m6 = run(dir.path(), "M6").unwrap();
        assert_eq!(m6.missing, ["ARCH-6", "UI-1"]);
        assert_eq!(m6.covered, ["UI-3"]);

        // Other milestones are as before.
        let m5 = run(dir.path(), "M5").unwrap();
        assert_eq!(m5.missing, Vec::<String>::new());
        assert_eq!(m5.covered, ["UI-2"]);
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
