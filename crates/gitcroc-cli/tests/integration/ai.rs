//! `gitcroc-cli ai providers`: the configured providers as JSON (M4-05),
//! with keys read only from the variable `--key-env` names (SEC-1).

use std::fs;
use std::process::Command;

const BIN: &str = env!("CARGO_BIN_EXE_gitcroc-cli");

const SECRET: &str = "sk-test-cli-0123456789";

const SETTINGS: &str = "schema = 2\n\n[ai]\ndefault = \"p2\"\n\n\
    [[ai.provider]]\nid = \"p1\"\nname = \"Work\"\nkind = \"anthropic\"\nmodel = \"model-a\"\n\n\
    [[ai.provider]]\nid = \"p2\"\nname = \"Codex\"\nkind = \"codex\"\npath = \"/opt/codex/bin/codex\"\n";

fn config() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("settings.toml"), SETTINGS).unwrap();
    dir
}

#[test]
fn sec_1_cli_reads_keys_only_from_the_named_variable() {
    let dir = config();
    let out = Command::new(BIN)
        .args(["ai", "providers", "--config-dir"])
        .arg(dir.path())
        .args(["--key-env", "GITCROC_TEST_AI_KEY"])
        .env("GITCROC_TEST_AI_KEY", SECRET)
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8(out.stdout).unwrap();
    assert!(!stdout.contains(SECRET), "{stdout}");
    let json: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    assert_eq!(json["enabled"], true);
    assert_eq!(json["default"], "p2");
    let providers = json["providers"].as_array().unwrap();
    assert_eq!(providers.len(), 2);
    assert_eq!(providers[0]["id"], "p1");
    assert_eq!(providers[0]["kind"], "anthropic");
    assert_eq!(providers[0]["model"], "model-a");
    assert_eq!(providers[0]["context_budget"], 100_000);
    // The API provider gets the key; a CLI provider uses its own login.
    assert_eq!(providers[0]["has_key"], true);
    assert_eq!(providers[1]["path"], "/opt/codex/bin/codex");
    assert_eq!(providers[1]["has_key"], false);

    // Without the variable, or without `--key-env`, there is no key.
    for args in [&["--key-env", "GITCROC_TEST_AI_KEY_UNSET"][..], &[]] {
        let out = Command::new(BIN)
            .args(["ai", "providers", "--config-dir"])
            .arg(dir.path())
            .args(args)
            .env("GITCROC_TEST_AI_KEY", SECRET)
            .output()
            .unwrap();
        assert!(out.status.success());
        let json: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
        assert_eq!(json["providers"][0]["has_key"], false, "{args:?}");
    }
}

#[test]
fn sec_1_cli_takes_no_key_as_an_argument() {
    let dir = config();
    for args in [
        &["--key", SECRET][..],
        &["--key-env"],
        &["--key-env", ""],
        &["--key-env", SECRET, "extra"],
    ] {
        let out = Command::new(BIN)
            .args(["ai", "providers", "--config-dir"])
            .arg(dir.path())
            .args(args)
            .output()
            .unwrap();
        assert_eq!(out.status.code(), Some(2), "accepted {args:?}");
        assert!(out.stdout.is_empty());
    }
}

/// `ai eval` on fixture 01 with `replies` from the mock.
fn eval(replies: &[&str], args: &[&str]) -> std::process::Output {
    let dir = tempfile::tempdir().unwrap();
    let mock = dir.path().join("replies.json");
    fs::write(&mock, serde_json::to_string(replies).unwrap()).unwrap();
    Command::new(BIN)
        .args(["ai", "eval", "--mock"])
        .arg(&mock)
        .args(["--only", "01"])
        .args(args)
        .output()
        .unwrap()
}

const LEFT: &str = r#"{"hunks":[{"id":1,"status":"unresolved","explanation":"Both imports may be needed.","confidence":"medium"}]}"#;

#[test]
fn ai_15_eval_json_report_has_every_field() {
    let out = eval(&[LEFT], &["--json"]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let json: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(json["prompt"], gitcroc_ai::PROMPT_VERSION);
    assert_eq!(json["provider"], "mock");
    let fixture = &json["fixtures"][0];
    assert_eq!(json["fixtures"].as_array().unwrap().len(), 1);
    assert_eq!(fixture["name"], "01-rust-import-block");
    assert_eq!(fixture["expected"], "resolution");
    assert_eq!(fixture["correct"], false);
    assert_eq!(fixture["matches"], serde_json::Value::Null);
    assert_eq!(fixture["failed"], serde_json::Value::Null);
    assert_eq!(fixture["usage"], serde_json::json!([null]));
    assert_eq!(
        fixture["hunks"],
        serde_json::json!([{ "id": 1, "score": "unresolved", "correct": false, "confidence": "medium" }])
    );
    let counts = serde_json::json!({
        "matches": 0, "correct_unresolved": 0, "unexpected_unresolved": 0, "differs": 0
    });
    let medium = serde_json::json!({
        "matches": 0, "correct_unresolved": 0, "unexpected_unresolved": 1, "differs": 0
    });
    assert_eq!(
        json["summary"],
        serde_json::json!({
            "fixtures": 1,
            "correct_fixtures": 0,
            "hunks": 1,
            "matches": 0,
            "correct_unresolved": 0,
            "unexpected_unresolved": 1,
            "differs": 0,
            "failed": 0,
            "confidence": { "high": counts, "medium": medium, "low": counts },
            "usage": { "requests": 1, "reported": 0, "input_tokens": 0, "output_tokens": 0 },
        })
    );

    // The table says the same.
    let out = eval(&[LEFT], &[]);
    assert!(out.status.success());
    let table = String::from_utf8(out.stdout).unwrap();
    let title = format!("prompt {} · mock", gitcroc_ai::PROMPT_VERSION);
    assert!(table.contains(&title), "{table}");
    assert!(table.contains("01-rust-import-block"), "{table}");
    assert!(
        table.contains("1 hunks: 0 match, 0 correct unresolved, 1 unresolved"),
        "{table}"
    );
}

#[test]
fn ai_15_eval_compare_lists_changed_fixtures_as_json() {
    // The second run gets no reply: the mock fails it, so its score changes.
    let out = eval(&[LEFT], &["--compare", "v1,v1", "--json"]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let json: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(json["runs"].as_array().unwrap().len(), 2);
    assert_eq!(
        json["runs"][1]["fixtures"][0]["hunks"][0]["score"],
        "failed"
    );
    let changed = json["changed"].as_array().unwrap();
    assert_eq!(changed.len(), 1);
    assert_eq!(changed[0]["fixture"], "01-rust-import-block");
    assert_eq!(changed[0]["hunks"][0][0]["score"], "unresolved");
    assert_eq!(changed[0]["hunks"][1][0]["score"], "failed");
}

#[test]
fn ai_15_eval_refuses_an_unknown_prompt_version_or_fixture() {
    for args in [&["--prompt", "v0"][..], &["--only", "01", "--only", "02"]] {
        let out = eval(&[LEFT], args);
        assert_eq!(out.status.code(), Some(2), "{args:?}");
    }
    let out = Command::new(BIN)
        .args(["ai", "eval", "--mock", "missing.json", "--only", "99"])
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&out.stderr).contains("no fixture 99"));
}
