use std::process::Command;

use repo_gen::Fixture;

const BIN: &str = env!("CARGO_BIN_EXE_gitcroc-cli");

fn run(command: &str, repo: &std::path::Path) -> serde_json::Value {
    let out = Command::new(BIN).arg(command).arg(repo).output().unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    serde_json::from_slice(&out.stdout).unwrap()
}

#[test]
fn arch_5_cli_prints_status_and_summary_as_json() {
    let fixture = Fixture::new().unwrap();
    let git = fixture.repo("app").unwrap();
    let hash = git.commit_file("a.txt", "a\n", "First").unwrap();
    git.write("a.txt", "b\n").unwrap();
    git.write("b.txt", "new\n").unwrap();
    git.git(["add", "b.txt"]).unwrap();

    let status = run("status", git.path());
    assert_eq!(status["staged"][0]["path"], "b.txt");
    assert_eq!(status["staged"][0]["status"], "added");
    assert_eq!(status["unstaged"][0]["path"], "a.txt");
    assert_eq!(status["unstaged"][0]["status"], "modified");
    assert_eq!(status["unmerged"], serde_json::json!([]));

    let summary = run("summary", git.path());
    assert_eq!(summary["head"]["kind"], "branch");
    assert_eq!(summary["head"]["name"], "main");
    assert_eq!(summary["head"]["hash"], hash.as_str());
    assert_eq!(summary["upstream"], serde_json::Value::Null);
    assert_eq!(summary["dirty"], true);
    assert_eq!(summary["state"]["operation"], serde_json::Value::Null);

    let missing = Command::new(BIN)
        .args(["status"])
        .arg(fixture.path().join("nope"))
        .output()
        .unwrap();
    assert_eq!(missing.status.code(), Some(2));
    assert!(missing.stdout.is_empty());
}
