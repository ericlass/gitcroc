// Integration test crate: helpers outside #[test] fns are test code too (QA-4).
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::process::Command;

use repo_gen::Fixture;

const BIN: &str = env!("CARGO_BIN_EXE_gitcroc-cli");

#[test]
fn perf_2_cli_history_prints_pages_as_json() {
    let fixture = Fixture::new().unwrap();
    let git = fixture.repo("app").unwrap();
    git.commit_file("a.txt", "a\n", "First").unwrap();
    git.branch("feature").unwrap();
    git.commit_file("b.txt", "b\n", "Second").unwrap();

    let out = Command::new(BIN)
        .arg("history")
        .arg(git.path())
        .args(["--ref", "feature", "--pages", "2"])
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let pages: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let pages = pages.as_array().unwrap();
    assert_eq!(pages.len(), 1, "the history ends on page one");
    assert_eq!(pages[0]["more"], false);
    let rows = pages[0]["rows"].as_array().unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0]["subject"], "First");
    assert_eq!(rows[0]["graph"]["lane"], 0);
    assert_eq!(rows[0]["refs"][0]["name"], "feature");

    let missing = Command::new(BIN)
        .arg("history")
        .arg(fixture.path().join("nothing-here"))
        .output()
        .unwrap();
    assert_eq!(missing.status.code(), Some(2));
    assert!(missing.stdout.is_empty());
}
