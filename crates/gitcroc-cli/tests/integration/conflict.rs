use std::process::Command;

use repo_gen::Fixture;

const BIN: &str = env!("CARGO_BIN_EXE_gitcroc-cli");

#[test]
fn ops_8_cli_prints_a_conflict_as_json() {
    let fixture = Fixture::new().unwrap();
    let git = fixture.repo("app").unwrap();
    git.commit_file("a.txt", "a\n", "Base").unwrap();
    git.branch("feature").unwrap();
    git.commit_file("a.txt", "main\n", "Main").unwrap();
    git.switch("feature").unwrap();
    git.commit_file("a.txt", "feature\n", "Feature").unwrap();
    git.switch("main").unwrap();
    git.merge_with_conflicts("feature").unwrap();

    let out = Command::new(BIN)
        .arg("conflict")
        .arg(git.path())
        .arg("a.txt")
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let conflict: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(
        conflict,
        serde_json::json!({
            "path": "a.txt",
            "labels": { "ours": "main", "theirs": "feature" },
            "content": {
                "kind": "text",
                "chunks": [{ "kind": "hunk", "ours": "main\n", "base": "a\n", "theirs": "feature\n" }],
            },
            "edited_outside": false,
            "on_disk": null,
        })
    );

    git.git(["add", "a.txt"]).unwrap();
    let resolved = Command::new(BIN)
        .arg("conflict")
        .arg(git.path())
        .arg("a.txt")
        .output()
        .unwrap();
    assert_eq!(resolved.status.code(), Some(1));
    assert!(resolved.stdout.is_empty());
}
