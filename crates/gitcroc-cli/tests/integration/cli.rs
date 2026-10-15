use std::path::PathBuf;
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

/// Where detection looks besides PATH on this platform: the Git for Windows
/// locations or Homebrew's. Windows gives every process `%ProgramFiles%`
/// even when the parent removes it, so these cannot be hidden from the CLI.
fn fallback_locations() -> Vec<PathBuf> {
    if cfg!(windows) {
        let under = |var: &str, rest: &[&str]| {
            std::env::var_os(var)
                .filter(|root| !root.is_empty())
                .map(|root| rest.iter().fold(PathBuf::from(root), |p, s| p.join(s)))
        };
        [
            under("ProgramFiles", &["Git", "cmd", "git.exe"]),
            under("ProgramFiles(x86)", &["Git", "cmd", "git.exe"]),
            under("LocalAppData", &["Programs", "Git", "cmd", "git.exe"]),
        ]
        .into_iter()
        .flatten()
        .collect()
    } else if cfg!(target_os = "macos") {
        vec![
            PathBuf::from("/opt/homebrew/bin/git"),
            PathBuf::from("/usr/local/bin/git"),
        ]
    } else {
        vec![]
    }
}

#[test]
fn plat_3_git_info_exits_1_without_git() {
    let (code, json) = git_info(Command::new(BIN).env("PATH", ""));

    // With PATH empty, only the platform's fallback locations are left.
    let fallbacks = fallback_locations();
    if let Some(git) = fallbacks.iter().find(|git| git.is_file()) {
        assert_eq!(code, Some(0), "{json}");
        assert_eq!(json["path"], git.to_str().unwrap(), "{json}");
        return;
    }
    let searched: Vec<&str> = fallbacks.iter().map(|p| p.to_str().unwrap()).collect();
    assert_eq!(code, Some(1), "{json}");
    assert_eq!(
        json,
        serde_json::json!({ "kind": "not_found", "searched": searched })
    );
}
