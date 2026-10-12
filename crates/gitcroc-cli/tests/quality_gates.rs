//! Quality gates: `just check` (QA-5) and the requirement coverage script
//! (QA-3). The script's own unit tests live in `tools/req-coverage`; these
//! tests sit under `crates/` so the script finds them.

// Integration test crate: helpers outside #[test] fns are test code too (QA-4).
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// The body lines of a recipe in the workspace `justfile`, trimmed.
fn recipe(name: &str) -> Vec<String> {
    let text = fs::read_to_string(workspace_root().join("justfile")).unwrap();
    let mut lines = text.lines().skip_while(|line| {
        line.split([' ', ':'])
            .next()
            .is_none_or(|first| first != name)
    });
    assert!(lines.next().is_some(), "justfile has no `{name}` recipe");
    lines
        .take_while(|line| line.starts_with(' ') || line.starts_with('\t'))
        .map(|line| line.trim().to_owned())
        .collect()
}

/// The Rust commands QA-1 requires, as `just check` runs them.
const QA_1_RUST: [&str; 4] = [
    "cargo fmt --check",
    "cargo clippy --workspace --all-targets -- -D warnings",
    "cargo test --workspace",
    "cargo deny check",
];

#[test]
fn qa_5_check_recipe_runs_every_rust_command_of_qa_1() {
    let body = recipe("check");
    for command in QA_1_RUST {
        assert!(
            body.iter().any(|line| line == command),
            "QA-5: `just check` does not run `{command}`; recipe: {body:?}"
        );
    }
}

#[test]
fn qa_5_check_recipe_does_not_ignore_failures() {
    // A leading `-` makes just carry on after a failing line.
    let body = recipe("check");
    assert!(
        body.iter().all(|line| !line.starts_with('-')),
        "QA-5: `just check` ignores a failure: {body:?}"
    );
}

#[test]
fn qa_3_check_recipe_runs_coverage_script() {
    assert!(
        recipe("check")
            .iter()
            .any(|line| line.starts_with("just coverage M0")),
        "QA-3: `just check` does not run the coverage script"
    );
    assert!(
        recipe("coverage")
            .iter()
            .any(|line| line.starts_with("cargo run --quiet -p req-coverage --")),
        "QA-3: `just coverage` does not run req-coverage"
    );
}

fn write(path: &Path, text: &str) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, text).unwrap();
}

/// A repository layout with one task file and some test and production code,
/// in a directory of its own per test.
fn coverage_fixture(test: &str) -> PathBuf {
    let root = Path::new(env!("CARGO_TARGET_TMPDIR")).join(test);
    if root.exists() {
        fs::remove_dir_all(&root).unwrap();
    }
    write(
        &root.join("docs/tasks/M9-01-example.md"),
        "# M9-01 Example\n\n## Spec\n\
         Implements: XA-1, XB-2, XC-3, XD-4, XE-5\n\
         No test needed: XE-5 (proven by review)\n\
         Respects: XF-6\n",
    );
    // Not part of milestone M9.
    write(
        &root.join("docs/tasks/M8-01-other.md"),
        "Implements: XG-7\n",
    );
    write(
        &root.join("crates/demo/tests/demo.rs"),
        "#[test]\nfn xa_1_works() {}\n// Also covers XB-2.\n",
    );
    write(
        &root.join("crates/demo/src/lib.rs"),
        "// XC-3 in production code is not a test.\n\
         #[cfg(test)]\nmod tests {\n    fn xd_4_works() {}\n}\n",
    );
    write(&root.join("ui/src/app.ts"), "// XC-3 outside a test file\n");
    root
}

fn run_coverage(root: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO"))
        .current_dir(workspace_root())
        .args(["run", "--quiet", "-p", "req-coverage", "--", "--root"])
        .arg(root)
        .args(args)
        .output()
        .unwrap()
}

#[test]
fn qa_3_coverage_script_lists_ids_without_test() {
    let root = coverage_fixture("qa_3_coverage_script_lists_ids_without_test");
    let out = run_coverage(&root, &["M9"]);
    let stdout = String::from_utf8(out.stdout).unwrap();
    let missing: Vec<&str> = stdout
        .lines()
        .filter_map(|line| line.strip_prefix("missing: "))
        .collect();

    assert_eq!(missing, ["XC-3"], "stdout:\n{stdout}");
    assert!(
        stdout.contains("no test needed: XE-5 (M9-01-example): proven by review"),
        "stdout:\n{stdout}"
    );
    assert_eq!(out.status.code(), Some(1), "stdout:\n{stdout}");
}

#[test]
fn qa_3_coverage_script_report_only_exits_zero() {
    let root = coverage_fixture("qa_3_coverage_script_report_only_exits_zero");
    let out = run_coverage(&root, &["M9", "--report-only"]);
    assert!(out.status.success());
    assert!(
        String::from_utf8(out.stdout)
            .unwrap()
            .contains("missing: XC-3")
    );
}
