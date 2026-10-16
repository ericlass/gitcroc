//! The benchmark harness (PERF-8) against a small generated set.

use std::process::Command;

use repo_gen::{RepoSpec, SetSpec};

const BIN: &str = env!("CARGO_BIN_EXE_gitcroc-cli");

fn repo(name: &str, files: u32) -> RepoSpec {
    RepoSpec {
        name: name.into(),
        files,
        lines: u64::from(files) * 5,
        commits: 40,
        branches: 3,
    }
}

/// A `target-project` far smaller than the real one, so the test is quick.
fn small_target_project() -> SetSpec {
    SetSpec {
        name: "target-project".into(),
        repos: vec![repo("small", 50), repo("largest", 200)],
        groups: Vec::new(),
    }
}

#[test]
fn perf_8_harness_prints_json_with_the_open_repo_history_diff_and_status_scenarios() {
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
    let group = results
        .iter()
        .find(|r| r["scenario"] == "group-history-first-page")
        .unwrap_or_else(|| panic!("no group-history-first-page result in {results:?}"));
    assert_eq!(group["budget_ms"], 300.0);
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
    assert_eq!(budget("status-60k"), 500.0);
    assert_eq!(budget("refresh-all"), 2000.0);
    // A stretch scenario, without --stretch.
    assert!(!results.iter().any(|r| r["scenario"] == "group-fetch"));
}

/// The stretch run on a small `many-repos`: the refresh of every repo and a
/// group fetch of all of them, which fails the run unless every repo
/// fetched; the set itself is never written.
#[test]
fn perf_8_stretch_measures_a_refresh_and_a_group_fetch_of_many_repos() {
    let data = tempfile::tempdir().unwrap();
    repo_gen::generate_set(&small_target_project(), data.path(), &|_| {}).unwrap();
    let many = SetSpec {
        name: "many-repos".into(),
        repos: vec![repo("a", 20), repo("b", 20), repo("c", 20)],
        groups: Vec::new(),
    };
    repo_gen::generate_set(&many, data.path(), &|_| {}).unwrap();
    let head = |name: &str| {
        let out = Command::new("git")
            .arg("-C")
            .arg(data.path().join("many-repos").join(name))
            .args(["rev-parse", "HEAD"])
            .output()
            .unwrap();
        String::from_utf8(out.stdout).unwrap()
    };
    let before = head("a");

    let out = Command::new(BIN)
        .args(["bench", "--runs", "2", "--stretch", "--data"])
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
    let on_many = |scenario: &str| {
        results
            .as_array()
            .unwrap()
            .iter()
            .find(|r| r["scenario"] == scenario && r["data_set"] == "many-repos")
            .unwrap_or_else(|| panic!("no {scenario} on many-repos in {results}"))
            .clone()
    };
    assert_eq!(on_many("refresh-all")["budget_ms"], 5000.0);
    let fetch = on_many("group-fetch");
    assert_eq!(fetch["runs"], 2);
    assert!(fetch["budget_ms"].is_null());
    assert_eq!(head("a"), before);
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
