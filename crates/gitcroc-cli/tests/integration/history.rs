use std::process::Command;

use repo_gen::Fixture;

const BIN: &str = env!("CARGO_BIN_EXE_gitcroc-cli");

#[test]
fn perf_2_cli_history_prints_pages_as_json() {
    let fixture = Fixture::new().unwrap();
    let git = fixture.repo("app").unwrap();
    git.commit_file("a.txt", "a\n", "First").unwrap();
    git.branch("feature").unwrap();
    git.commit_file("b.txt", "b\n", "Second").unwrap();

    let out = Command::new(BIN)
        .arg("history")
        .arg(git.path())
        .args(["--ref", "feature", "--pages", "2"])
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let pages: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let pages = pages.as_array().unwrap();
    assert_eq!(pages.len(), 1, "the history ends on page one");
    assert_eq!(pages[0]["more"], false);
    let rows = pages[0]["rows"].as_array().unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0]["subject"], "First");
    assert_eq!(rows[0]["graph"]["lane"], 0);
    assert_eq!(rows[0]["refs"][0]["name"], "feature");

    let missing = Command::new(BIN)
        .arg("history")
        .arg(fixture.path().join("nothing-here"))
        .output()
        .unwrap();
    assert_eq!(missing.status.code(), Some(2));
    assert!(missing.stdout.is_empty());
}

#[test]
fn perf_5_cli_history_of_a_group_or_repos() {
    let fixture = Fixture::new().unwrap();
    let api = fixture.repo("repos/api").unwrap();
    let web = fixture.repo("repos/web").unwrap();
    api.set_clock(0);
    api.commit_file("a.txt", "a\n", "api 1").unwrap();
    web.set_clock(10);
    web.commit_file("a.txt", "a\n", "web 1").unwrap();
    api.set_clock(20);
    api.commit_file("a.txt", "b\n", "api 2").unwrap();
    let config = tempfile::tempdir().unwrap();
    let cli = |args: &[&std::ffi::OsStr]| {
        let out = Command::new(BIN).args(args).output().unwrap();
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        serde_json::from_slice::<serde_json::Value>(&out.stdout).unwrap()
    };
    let dir = config.path().as_os_str();
    cli(&[
        "workspace".as_ref(),
        "--config-dir".as_ref(),
        dir,
        "add".as_ref(),
        fixture.path().join("repos").as_os_str(),
    ]);
    let shown = cli(&[
        "workspace".as_ref(),
        "--config-dir".as_ref(),
        dir,
        "show".as_ref(),
    ]);
    let id = |name: &str| {
        shown["repos"]
            .as_array()
            .unwrap()
            .iter()
            .find(|repo| repo["name"] == name)
            .unwrap()["id"]
            .as_str()
            .unwrap()
            .to_owned()
    };
    let (api, web) = (id("api"), id("web"));
    cli(&[
        "group".as_ref(),
        "--config-dir".as_ref(),
        dir,
        "create".as_ref(),
        "Both".as_ref(),
        web.as_ref(),
        api.as_ref(),
    ]);
    let rows = |pages: serde_json::Value| -> Vec<(String, String)> {
        pages[0]["rows"]
            .as_array()
            .unwrap()
            .iter()
            .map(|row| {
                let text = |key: &str| row[key].as_str().unwrap().to_owned();
                (text("repo_id"), text("subject"))
            })
            .collect()
    };

    let group = cli(&[
        "history".as_ref(),
        "--config-dir".as_ref(),
        dir,
        "--group".as_ref(),
        "both".as_ref(),
    ]);
    assert_eq!(group[0]["more"], false);
    let expected = [
        (api.clone(), "api 2".to_owned()),
        (web.clone(), "web 1".to_owned()),
        (api.clone(), "api 1".to_owned()),
    ];
    assert_eq!(rows(group), expected);
    let picked = format!("{api},{web}");
    let repos = cli(&[
        "history".as_ref(),
        "--config-dir".as_ref(),
        dir,
        "--repos".as_ref(),
        picked.as_ref(),
        "--filter".as_ref(),
        "api".as_ref(),
    ]);
    assert_eq!(rows(repos), [expected[0].clone(), expected[2].clone()]);

    let unknown = Command::new(BIN)
        .args(["history".as_ref(), "--config-dir".as_ref(), dir])
        .args(["--group", "nope"])
        .output()
        .unwrap();
    assert_eq!(unknown.status.code(), Some(2));
}
