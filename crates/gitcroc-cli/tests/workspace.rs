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

fn summaries(config: &Path, cache: &Path, args: &[&str]) -> Output {
    Command::new(BIN)
        .arg("summaries")
        .arg("--config-dir")
        .arg(config)
        .arg("--cache-dir")
        .arg(cache)
        .args(args)
        .output()
        .unwrap()
}

#[test]
fn perf_3_cli_prints_cached_then_fresh_summaries() {
    let fixture = Fixture::new().unwrap();
    let repo = Repo::init(fixture.path().join("one")).unwrap();
    let first = repo.commit_file("a.txt", "a\n", "First").unwrap();
    let config = tempfile::tempdir().unwrap();
    let cache = tempfile::tempdir().unwrap();
    json(&workspace(
        config.path(),
        &["add".as_ref(), repo.path().as_os_str()],
    ));

    // Nothing cached yet.
    let cached = json(&summaries(config.path(), cache.path(), &["--cached-only"]));
    assert_eq!(cached[0]["stale"], true);
    assert_eq!(cached[0]["summary"], serde_json::Value::Null);

    let fresh = json(&summaries(config.path(), cache.path(), &[]));
    assert_eq!(fresh[0]["stale"], false);
    assert_eq!(fresh[0]["repo_id"], "r1");
    assert_eq!(fresh[0]["summary"]["head"]["hash"], first.as_str());

    let second = repo.commit_file("b.txt", "b\n", "Second").unwrap();
    let cached = json(&summaries(config.path(), cache.path(), &["--cached-only"]));
    assert_eq!(cached[0]["stale"], true);
    assert_eq!(cached[0]["summary"]["head"]["hash"], first.as_str());
    let fresh = json(&summaries(config.path(), cache.path(), &[]));
    assert_eq!(fresh[0]["summary"]["head"]["hash"], second.as_str());
}
