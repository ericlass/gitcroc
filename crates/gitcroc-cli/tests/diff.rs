// Integration test crate: helpers outside #[test] fns are test code too (QA-4).
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::process::Command;

use repo_gen::Fixture;

const BIN: &str = env!("CARGO_BIN_EXE_gitcroc-cli");

#[test]
fn arch_8_cli_diff_prints_files_and_one_diff_as_json() {
    let fixture = Fixture::new().unwrap();
    let git = fixture.repo("app").unwrap();
    git.commit_file("a.txt", "a\n", "First").unwrap();
    git.write("a.txt", "a\nb\n").unwrap();
    git.write("b.txt", "new\n").unwrap();
    let hash = git.commit_all("Second").unwrap();

    let out = Command::new(BIN)
        .arg("diff")
        .arg(git.path())
        .args([hash.as_str(), "--file", "1", "--context", "all"])
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let json: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(json["commit"]["hash"], hash.as_str());
    assert_eq!(json["commit"]["message"], "Second\n");
    let files = json["files"].as_array().unwrap();
    let paths: Vec<&str> = files.iter().map(|f| f["path"].as_str().unwrap()).collect();
    assert_eq!(paths, ["a.txt", "b.txt"]);
    assert_eq!(files[1]["status"], "added");
    assert_eq!(json["diff"]["kind"], "text");
    assert_eq!(json["diff"]["new_text"], "new\n");
    assert_eq!(json["diff"]["hunks"][0]["lines"][0]["kind"], "added");

    let out_of_range = Command::new(BIN)
        .arg("diff")
        .arg(git.path())
        .args([hash.as_str(), "--file", "2"])
        .output()
        .unwrap();
    assert_eq!(out_of_range.status.code(), Some(2));
    assert!(out_of_range.stdout.is_empty());
}
