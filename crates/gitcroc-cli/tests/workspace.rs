// Integration test crate: helpers outside #[test] fns are test code too (QA-4).
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::path::Path;
use std::process::{Command, Output};

use repo_gen::{Fixture, Repo};

const BIN: &str = env!("CARGO_BIN_EXE_gitcroc-cli");

fn workspace(config: &Path, args: &[&std::ffi::OsStr]) -> Output {
    Command::new(BIN)
        .arg("workspace")
        .arg("--config-dir")
        .arg(config)
        .args(args)
        .output()
        .unwrap()
}

fn json(output: &Output) -> serde_json::Value {
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).unwrap()
}

#[test]
fn grp_1_cli_adds_every_repo_under_a_folder_and_shows_them() {
    let fixture = Fixture::new().unwrap();
    for name in ["one", "group/two"] {
        let repo = Repo::init(fixture.path().join("repos").join(name)).unwrap();
        repo.commit_file("a.txt", "a\n", "First").unwrap();
    }
    let config = tempfile::tempdir().unwrap();

    let added = json(&workspace(
        config.path(),
        &["add".as_ref(), fixture.path().join("repos").as_os_str()],
    ));
    let names: Vec<&str> = added["added"]
        .as_array()
        .unwrap()
        .iter()
        .map(|repo| repo["name"].as_str().unwrap())
        .collect();
    assert_eq!(names, ["two", "one"]);
    assert_eq!(added["refused"], serde_json::json!([]));

    let shown = json(&workspace(config.path(), &["show".as_ref()]));
    assert_eq!(shown["name"], "My workspace");
    assert_eq!(shown["repos"], added["added"]);
    assert_eq!(shown["id"], added["workspace_id"]);

    let removed = json(&workspace(
        config.path(),
        &["remove".as_ref(), "r1".as_ref()],
    ));
    assert_eq!(removed["repos"].as_array().unwrap().len(), 1);
    assert_eq!(removed["repos"][0]["id"], "r2");

    let unknown = workspace(config.path(), &["remove".as_ref(), "r9".as_ref()]);
    assert_eq!(unknown.status.code(), Some(2));
    assert!(unknown.stdout.is_empty());
}
