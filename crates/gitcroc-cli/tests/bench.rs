//! The benchmark harness (PERF-8) against a small generated set.

// Integration test crate: helpers outside #[test] fns are test code too (QA-4).
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::process::Command;

use repo_gen::{RepoSpec, SetSpec};

const BIN: &str = env!("CARGO_BIN_EXE_gitcroc-cli");

/// A `target-project` far smaller than the real one, so the test is quick.
fn small_target_project() -> SetSpec {
    let repo = |name: &str, files| RepoSpec {
        name: name.into(),
        files,
        lines: u64::from(files) * 5,
        commits: 40,
        branches: 3,
    };
    SetSpec {
        name: "target-project".into(),
        repos: vec![repo("small", 50), repo("largest", 200)],
        groups: Vec::new(),
    }
}

#[test]
fn perf_8_harness_prints_json_with_the_open_repo_history_and_diff_scenarios() {
    let data = tempfile::tempdir().unwrap();
    repo_gen::generate_set(&small_target_project(), data.path(), &|_| {}).unwrap();

    let out = Command::new(BIN)
        .args(["bench", "--runs", "3", "--data"])
        .arg(data.path())
        .output()
        .unwrap();
    assert_eq!(
        out.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let results: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let results = results.as_array().unwrap();
    let open = results
        .iter()
        .find(|r| r["scenario"] == "open-repo" && r["data_set"] == "target-project")
        .unwrap_or_else(|| panic!("no open-repo result in {results:?}"));
    assert_eq!(open["runs"], 3);
    assert!(open["median_ms"].as_f64().unwrap() >= 0.0);
    assert!(open["p95_ms"].as_f64().unwrap() >= open["median_ms"].as_f64().unwrap());
    assert!(open["budget_ms"].is_null());
    assert_eq!(open["pass"], true);

    let history = results
        .iter()
        .find(|r| r["scenario"] == "history-first-page")
        .unwrap_or_else(|| panic!("no history-first-page result in {results:?}"));
    assert_eq!(history["budget_ms"], 200.0);
    // 40 commits have no 10th page; the scenario is skipped, not failed.
    assert!(!results.iter().any(|r| r["scenario"] == "history-page-10"));

    let budget = |scenario: &str| {
        results
            .iter()
            .find(|r| r["scenario"] == scenario)
            .unwrap_or_else(|| panic!("no {scenario} result in {results:?}"))["budget_ms"]
            .clone()
    };
    assert_eq!(budget("select-commit"), 150.0);
    assert!(budget("select-commit-large").is_null());
}

#[test]
fn perf_8_harness_without_data_explains_how_to_build_it() {
    let data = tempfile::tempdir().unwrap();
    let out = Command::new(BIN)
        .args(["bench", "--data"])
        .arg(data.path())
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(2));
    assert!(out.stdout.is_empty());
    let stderr = String::from_utf8(out.stderr).unwrap();
    assert!(
        stderr.contains("just bench-data target-project"),
        "{stderr}"
    );
}
