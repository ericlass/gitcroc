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

#[test]
fn ai_5_cli_prints_the_ai_context_as_json() {
    let fixture = Fixture::new().unwrap();
    let git = fixture.repo("app").unwrap();
    git.commit_file("a.txt", "a\nb\nc\n", "Base").unwrap();
    git.commit_file(".env", "base\n", "Env").unwrap();
    git.branch("feature").unwrap();
    git.write(".env", "main\n").unwrap();
    git.commit_file("a.txt", "main a\nb\nmain c\n", "Main")
        .unwrap();
    git.switch("feature").unwrap();
    git.write(".env", "feature\n").unwrap();
    git.commit_file("a.txt", "feature a\nb\nfeature c\n", "Feature")
        .unwrap();
    git.switch("main").unwrap();
    git.merge_with_conflicts("feature").unwrap();
    // No settings file: the default exclude list.
    let config = fixture.path().join("config");

    let out = Command::new(BIN)
        .args(["conflict", "ai-context"])
        .arg(git.path())
        .args(["a.txt", "--hunks", "2", "--config-dir"])
        .arg(&config)
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let read: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(
        read,
        serde_json::json!({
            "kind": "context",
            "context": {
                "path": "a.txt",
                "operation": "merge",
                "ours_label": "main",
                "theirs_label": "feature",
                "hunks": [{
                    "id": 2,
                    "before": "main a\nb\n",
                    "ours": "main c\n",
                    "base": "c\n",
                    "theirs": "feature c\n",
                    "after": "",
                }],
                "ours_commits": ["Main"],
                "theirs_commits": ["Feature"],
            },
        })
    );

    let env = Command::new(BIN)
        .args(["conflict", "ai-context"])
        .arg(git.path())
        .args([".env", "--config-dir"])
        .arg(&config)
        .output()
        .unwrap();
    assert!(env.status.success());
    let read: serde_json::Value = serde_json::from_slice(&env.stdout).unwrap();
    assert_eq!(
        read,
        serde_json::json!({
            "kind": "ineligible",
            "reason": { "kind": "excluded", "pattern": ".env*" },
            "message": "matches the exclude list (.env*)",
        })
    );
}
