//! Every operation of section 6 has git integration tests for success,
//! failure and, where git can stop in one, conflict (section 13, M2 row;
//! M2-15). The operations and their tests are listed in `ops_matrix.txt`
//! beside this file; this checks the list against the tests under `crates/`
//! and against the writes in `gitcroc-git::ops`, so an operation added
//! there without tests fails here.

// Integration test crate: helpers outside #[test] fns are test code too (QA-4).
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

/// The operations git can leave in conflict state (M2-15); every other row
/// says "n/a".
const CONFLICT: [&str; 7] = [
    "Pull",
    "Merge",
    "Rebase",
    "Cherry-pick",
    "Revert",
    "Apply",
    "Pop",
];

/// One row of the list.
struct Row {
    line: usize,
    operation: String,
    ops: Vec<String>,
    success: Vec<String>,
    failure: Vec<String>,
    conflict: Vec<String>,
}

/// `a, b` as `[a, b]`; `-` and `n/a` as none.
fn items(column: &str) -> Vec<String> {
    match column.trim() {
        "-" | "n/a" => Vec::new(),
        list => list.split(',').map(|item| item.trim().to_owned()).collect(),
    }
}

fn rows() -> Vec<Row> {
    let list = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/ops_matrix.txt");
    let text = fs::read_to_string(list).unwrap();
    let mut rows = Vec::new();
    for (at, line) in text.lines().enumerate() {
        if line.trim().is_empty() || line.starts_with('#') {
            continue;
        }
        let columns: Vec<&str> = line.split(" | ").collect();
        let [_, operation, ops, success, failure, conflict] = columns[..] else {
            panic!("ops_matrix.txt:{}: not six columns: {line}", at + 1);
        };
        if conflict.trim() == "-" {
            panic!("ops_matrix.txt:{}: conflict is tests or n/a", at + 1);
        }
        rows.push(Row {
            line: at + 1,
            operation: operation.trim().to_owned(),
            ops: items(ops),
            success: items(success),
            failure: items(failure),
            conflict: items(conflict),
        });
    }
    rows
}

fn collect(dir: &Path, files: &mut Vec<PathBuf>) {
    for entry in fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            collect(&path, files);
        } else if path.extension().is_some_and(|ext| ext == "rs") {
            files.push(path);
        }
    }
}

/// The name of the function `line` declares, if it declares one.
fn declared(line: &str, prefix: &str) -> Option<String> {
    let rest = line.trim_start().strip_prefix(prefix)?;
    let end = rest.find(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))?;
    Some(rest[..end].to_owned())
}

/// The names of the test functions under `crates/`.
fn tests() -> BTreeSet<String> {
    let crates = Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
    let mut files = Vec::new();
    collect(&crates, &mut files);
    let mut names = BTreeSet::new();
    for file in files {
        let text = fs::read_to_string(&file).unwrap();
        let mut attribute = false;
        for line in text.lines().map(str::trim) {
            if line == "#[test]" || line.starts_with("#[tokio::test") {
                attribute = true;
            } else if line.starts_with("#[") {
                // `#[ignore]` and the like between the attribute and the fn.
            } else if attribute {
                let name = declared(line, "async fn ").or_else(|| declared(line, "fn "));
                names.extend(name);
                attribute = false;
            }
        }
    }
    names
}

/// The writes of `gitcroc-git::ops`: its `pub async fn`s.
fn writes() -> BTreeSet<String> {
    let ops = Path::new(env!("CARGO_MANIFEST_DIR")).join("src/ops");
    let mut files = Vec::new();
    collect(&ops, &mut files);
    let mut names = BTreeSet::new();
    for file in files {
        let text = fs::read_to_string(&file).unwrap();
        names.extend(
            text.lines()
                .filter_map(|line| declared(line, "pub async fn ")),
        );
    }
    names
}

#[test]
fn ops_matrix_every_operation_has_success_failure_and_conflict_tests() {
    let rows = rows();
    let tests = tests();
    let writes = writes();
    assert!(tests.len() > 100, "found only {} tests", tests.len());
    assert!(writes.len() > 30, "found only {} writes", writes.len());

    let mut problems = Vec::new();
    for row in &rows {
        let at = format!("ops_matrix.txt:{} {}", row.line, row.operation);
        if row.success.is_empty() {
            problems.push(format!("{at}: no success test"));
        }
        if row.failure.is_empty() {
            problems.push(format!("{at}: no failure test"));
        }
        let conflict = CONFLICT.contains(&row.operation.as_str());
        if conflict && row.conflict.is_empty() {
            problems.push(format!("{at}: no conflict test"));
        }
        if !conflict && !row.conflict.is_empty() {
            problems.push(format!("{at}: git cannot stop in conflict here; say n/a"));
        }
        for test in row.success.iter().chain(&row.failure).chain(&row.conflict) {
            if !tests.contains(test) {
                problems.push(format!("{at}: no test named {test} under crates/"));
            }
        }
        for write in &row.ops {
            if !writes.contains(write) {
                problems.push(format!("{at}: gitcroc-git::ops has no {write}"));
            }
        }
    }
    let listed: BTreeSet<&String> = rows.iter().flat_map(|row| &row.ops).collect();
    for write in &writes {
        if !listed.contains(write) {
            problems.push(format!(
                "gitcroc-git::ops::{write} is in no row of ops_matrix.txt; add its operation and tests"
            ));
        }
    }
    for operation in CONFLICT {
        if !rows.iter().any(|row| row.operation == operation) {
            problems.push(format!("ops_matrix.txt has no row {operation}"));
        }
    }
    assert!(problems.is_empty(), "{}", problems.join("\n"));
}
