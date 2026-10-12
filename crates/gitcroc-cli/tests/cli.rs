// Integration test crate: helpers outside #[test] fns are test code too (QA-4).
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::process::Command;

const BIN: &str = env!("CARGO_BIN_EXE_gitcroc-cli");

#[test]
fn version_flag_prints_workspace_version() {
    let out = Command::new(BIN).arg("--version").output().unwrap();
    assert!(out.status.success());
    let stdout = String::from_utf8(out.stdout).unwrap();
    assert_eq!(
        stdout.trim(),
        format!("gitcroc-cli {}", env!("CARGO_PKG_VERSION"))
    );
}

#[test]
fn other_arguments_are_rejected() {
    for args in [
        &[][..],
        &["--help"],
        &["--version", "extra"],
        &["git-info", "extra"],
    ] {
        let out = Command::new(BIN).args(args).output().unwrap();
        assert!(!out.status.success(), "accepted {args:?}");
        assert!(out.stdout.is_empty());
    }
}

fn git_info(command: &mut Command) -> (Option<i32>, serde_json::Value) {
    let out = command.arg("git-info").output().unwrap();
    let json = serde_json::from_slice(&out.stdout).unwrap();
    (out.status.code(), json)
}

#[test]
fn plat_3_git_info_prints_the_usable_git() {
    let (code, json) = git_info(&mut Command::new(BIN));
    assert_eq!(code, Some(0), "{json}");
    assert_eq!(json["kind"], "usable");
    assert!(json["path"].as_str().unwrap().contains("git"), "{json}");
    assert!(
        json["version"].as_str().unwrap().starts_with("2."),
        "{json}"
    );
}

#[test]
fn plat_3_git_info_exits_1_without_git() {
    let mut command = Command::new(BIN);
    command
        .env("PATH", "")
        .env_remove("ProgramFiles")
        .env_remove("ProgramFiles(x86)")
        .env_remove("LocalAppData");
    let (code, json) = git_info(&mut command);
    assert_eq!(code, Some(1), "{json}");
    assert_eq!(
        json,
        serde_json::json!({ "kind": "not_found", "searched": [] })
    );
}
