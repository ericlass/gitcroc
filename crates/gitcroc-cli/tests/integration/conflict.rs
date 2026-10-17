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

/// `settings.toml` with one OpenAI-compatible provider as the default,
/// consented to when `consent`.
fn provider_settings(consent: bool) -> String {
    let consent = if consent {
        "consent = \"localhost:11434\"\n"
    } else {
        ""
    };
    format!(
        "schema = 2\n\n[ai]\ndefault = \"p1\"\n\n[[ai.provider]]\nid = \"p1\"\nname = \"Local\"\n\
         kind = \"openai\"\nmodel = \"m\"\nbase_url = \"http://localhost:11434/v1\"\n{consent}"
    )
}

#[test]
fn d9_cli_resolves_a_conflict_with_the_mock_and_writes_nothing() {
    let fixture = Fixture::new().unwrap();
    let git = fixture.repo("app").unwrap();
    git.commit_file("a.txt", "a\n", "Base").unwrap();
    git.branch("feature").unwrap();
    git.commit_file("a.txt", "main\n", "Main").unwrap();
    git.switch("feature").unwrap();
    git.commit_file("a.txt", "feature\n", "Feature").unwrap();
    git.switch("main").unwrap();
    git.merge_with_conflicts("feature").unwrap();
    let config = fixture.path().join("config");
    std::fs::create_dir_all(&config).unwrap();
    let replies = fixture.path().join("replies.json");
    let reply = serde_json::json!({
        "hunks": [{
            "id": 1,
            "status": "resolved",
            "resolution": "main and feature\n",
            "explanation": "Kept both lines.",
            "confidence": "medium",
        }]
    });
    std::fs::write(
        &replies,
        serde_json::to_string(&[reply.to_string()]).unwrap(),
    )
    .unwrap();
    let resolve = || {
        Command::new(BIN)
            .args(["conflict", "ai-resolve"])
            .arg(git.path())
            .args(["a.txt", "--config-dir"])
            .arg(&config)
            .arg("--mock")
            .arg(&replies)
            .output()
            .unwrap()
    };
    let status = git.git(["status", "--porcelain=v2"]).unwrap();
    let file = std::fs::read(git.path().join("a.txt")).unwrap();

    // Refused as the app refuses: no consent yet.
    std::fs::write(config.join("settings.toml"), provider_settings(false)).unwrap();
    let out = resolve();
    assert_eq!(out.status.code(), Some(3));
    let refused: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(refused["kind"], "refused");
    assert_eq!(refused["refused"]["kind"], "consent_needed");

    std::fs::write(config.join("settings.toml"), provider_settings(true)).unwrap();
    let out = resolve();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let proposal: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(
        proposal,
        serde_json::json!({
            "kind": "proposal",
            "proposal": {
                "hunks": [{
                    "id": 1,
                    "status": "resolved",
                    "resolution": "main and feature\n",
                    "explanation": "Kept both lines.",
                    "confidence": "medium",
                }],
                "usage": [null],
            }
        })
    );
    // Debug logging is off: nothing of the prompt on stderr (AI-7).
    assert!(!String::from_utf8_lossy(&out.stderr).contains("feature"));
    assert_eq!(git.git(["status", "--porcelain=v2"]).unwrap(), status);
    assert_eq!(std::fs::read(git.path().join("a.txt")).unwrap(), file);
}
