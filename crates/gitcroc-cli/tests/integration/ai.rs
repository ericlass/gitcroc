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
