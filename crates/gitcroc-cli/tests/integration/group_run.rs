//! `gitcroc-cli group run --test-op` (M3-08): a group operation through the
//! core's engine, from the preflight to the report.

use std::ffi::OsStr;
use std::process::{Command, Output};

use repo_gen::Fixture;

const BIN: &str = env!("CARGO_BIN_EXE_gitcroc-cli");

fn cli(args: &[&OsStr]) -> Output {
    Command::new(BIN).args(args).output().unwrap()
}

fn json(out: &Output) -> serde_json::Value {
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    serde_json::from_slice(&out.stdout).unwrap()
}

/// Each report line as (repo name, outcome kind).
fn report(result: &serde_json::Value) -> Vec<(String, String)> {
    result["job"]["report"]
        .as_array()
        .unwrap()
        .iter()
        .map(|line| {
            let text = |value: &serde_json::Value| value.as_str().unwrap().to_owned();
            (text(&line["repo_name"]), text(&line["outcome"]["kind"]))
        })
        .collect()
}

#[test]
fn grp_7_cli_group_run_reports_every_repo() {
    let fixture = Fixture::new().unwrap();
    for name in ["repos/a", "repos/b", "repos/c"] {
        let repo = fixture.repo(name).unwrap();
        repo.commit_file("a.txt", "a\n", "First").unwrap();
    }
    let config = tempfile::tempdir().unwrap();
    let dir = config.path().as_os_str();
    json(&cli(&[
        "workspace".as_ref(),
        "--config-dir".as_ref(),
        dir,
        "add".as_ref(),
        fixture.path().join("repos").as_os_str(),
    ]));
    let shown = json(&cli(&[
        "workspace".as_ref(),
        "--config-dir".as_ref(),
        dir,
        "show".as_ref(),
    ]));
    let ids: Vec<String> = ["c", "a", "b"]
        .iter()
        .map(|name| {
            shown["repos"]
                .as_array()
                .unwrap()
                .iter()
                .find(|repo| repo["name"] == *name)
                .unwrap()["id"]
                .as_str()
                .unwrap()
                .to_owned()
        })
        .collect();
    let mut create: Vec<&OsStr> = vec![
        "group".as_ref(),
        "--config-dir".as_ref(),
        dir,
        "create".as_ref(),
        "Three".as_ref(),
    ];
    create.extend(ids.iter().map(OsStr::new));
    json(&cli(&create));
    let run = |extra: &[&str]| {
        let mut args: Vec<&OsStr> = vec![
            "group".as_ref(),
            "--config-dir".as_ref(),
            dir,
            "run".as_ref(),
            "three".as_ref(),
            "--test-op".as_ref(),
        ];
        args.extend(extra.iter().map(OsStr::new));
        cli(&args)
    };

    // Every repo will run: no confirmation, and one outcome per repo in
    // group order.
    let result = json(&run(&[]));
    assert_eq!(result["preflight"]["needs_confirmation"], false);
    assert_eq!(result["job"]["label"], "Test · Three (3 repos)");
    let nothing = |name: &str| (name.to_owned(), "nothing_to_do".to_owned());
    assert_eq!(report(&result), [nothing("c"), nothing("a"), nothing("b")]);

    // A blocked repo needs confirmation, then is reported skipped.
    std::fs::remove_dir_all(fixture.path().join("repos/a")).unwrap();
    let refused = run(&[]);
    assert_eq!(refused.status.code(), Some(2));
    let preflight: serde_json::Value = serde_json::from_slice(&refused.stdout).unwrap();
    assert_eq!(preflight["preflight"]["needs_confirmation"], true);
    assert_eq!(
        preflight["preflight"]["repos"][1]["class"]["kind"],
        "blocked"
    );
    assert!(preflight["job"].is_null());
    let result = json(&run(&["--confirm"]));
    assert_eq!(
        report(&result),
        [
            nothing("c"),
            ("a".to_owned(), "skipped".to_owned()),
            nothing("b")
        ]
    );
    assert_eq!(
        result["job"]["report"][1]["outcome"]["reason"],
        serde_json::json!({ "kind": "blocked", "reason": "the repo's folder is missing" })
    );
}
