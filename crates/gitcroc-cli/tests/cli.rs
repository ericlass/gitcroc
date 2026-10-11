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
    for args in [&[][..], &["--help"], &["--version", "extra"]] {
        let out = Command::new(BIN).args(args).output().unwrap();
        assert!(!out.status.success(), "accepted {args:?}");
        assert!(out.stdout.is_empty());
    }
}
