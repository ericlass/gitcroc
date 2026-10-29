//! The UI-side benchmark runner (PERF-8): it reads the script
//! `gitcroc-cli bench-ui` writes and plays every step of it in a window.
//! Nothing here is timed (no timing in CI); `just bench-ui` measures on the
//! reference machine. Repos are real ones made by `repo-gen` (QA-2).

use std::time::{Duration, SystemTime};

use gitcroc_app::app::EVENT_CAPACITY;
use gitcroc_core::{PAGE_SIZE, RepoId, WorkspaceId};
use gitcroc_ui::perf::{Runner, Script, Step, Tick, Trace};
use gitcroc_ui::window::GitcrocWindow;
use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{Entity, TestAppContext, VisualTestContext};

use crate::history::{add, id_of, repo, repos};
use crate::support::{Fixture, main_window};

/// The script `gitcroc-cli bench-ui` writes; its test compares its own
/// output with the same file.
const DRIVER_SCRIPT: &str = include_str!("bench_script.json");

#[test]
fn perf_8_the_runner_reads_the_drivers_script() {
    let script: Script = serde_json::from_str(DRIVER_SCRIPT).unwrap();
    assert_eq!(script.started_ms, Some(1_700_000_000_000));
    assert_eq!(
        script.steps[..3],
        [
            Step::ColdStart {
                scenario: "cold-start".into()
            },
            Step::Idle,
            Step::SelectRepo {
                scenario: Some("select-largest-repo".into()),
                repo: RepoId("r-largest".into()),
            },
        ]
    );
    assert!(script.steps.contains(&Step::SwitchTab {
        scenario: None,
        tab: WorkspaceId("w-diff".into()),
    }));
    assert_eq!(
        script.steps.last(),
        Some(&Step::ScrollDiff {
            scenario: "scroll-diff".into()
        })
    );
}

/// A workspace like the driver's, small: "api" with over a page of
/// commits and a commit "Files" 60 rows down, and "web"; a second tab of the same
/// repos; a third tab with one long file. Returns the ids and the hash of
/// "Files".
fn workspaces(
    fixture: &Fixture,
) -> (
    Vec<(String, RepoId)>,
    [WorkspaceId; 3],
    RepoId,
    String,
    String,
) {
    let made = repos(&["api", "web"]);
    let api = repo(&made, "api");
    let messages = |from: usize, count: usize| -> Vec<String> {
        (from..from + count)
            .map(|n| format!("Change {n}"))
            .collect()
    };
    api.empty_commits(&messages(0, PAGE_SIZE)).unwrap();
    api.write("a.txt", "a\n".repeat(40)).unwrap();
    api.write("b.txt", "b\n".repeat(40)).unwrap();
    let files = api.commit_all("Files").unwrap();
    // On the first page, below the window: the runner scrolls to it.
    api.empty_commits(&messages(PAGE_SIZE, 60)).unwrap();
    let main = fixture.active();
    let added = add(fixture, &made);

    let app = fixture.app().clone();
    let create = |name: &str| {
        fixture
            .runtime
            .block_on(app.workspace_create(name.to_owned()))
            .unwrap()
            .active
            .unwrap()
    };
    let second = create("Second tab");
    fixture.add_repos(&second, made.path());
    let long = repos(&["long"]);
    let long_repo = repo(&long, "long");
    let text: String = (0..400).map(|n| format!("fn line_{n}() {{}}\n")).collect();
    long_repo.write("src/long.rs", text).unwrap();
    let long_commit = long_repo.commit_all("Add a long file").unwrap();
    let diff_tab = create("Diff");
    let long_id = fixture.add_repos(&diff_tab, long.path())[0].1.clone();
    fixture
        .runtime
        .block_on(app.workspace_activate(main.clone()))
        .unwrap();
    // The repo dirs live as long as the test.
    std::mem::forget((made, long));
    (added, [main, second, diff_tab], long_id, files, long_commit)
}

fn script(
    added: &[(String, RepoId)],
    [main, second, diff_tab]: &[WorkspaceId; 3],
    long: &RepoId,
    files: &str,
    long_commit: &str,
) -> Script {
    let api = id_of(added, "api");
    let web = id_of(added, "web");
    let started = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap()
        .as_millis();
    let named = |name: &str| Some(name.to_owned());
    Script {
        started_ms: Some(u64::try_from(started).unwrap()),
        steps: vec![
            Step::ColdStart {
                scenario: "cold-start".into(),
            },
            Step::Idle,
            Step::SelectRepo {
                scenario: named("select-largest-repo"),
                repo: api.clone(),
            },
            Step::Idle,
            Step::SelectCommit {
                scenario: named("select-commit"),
                hash: files.to_owned(),
            },
            Step::Idle,
            Step::SelectRepo {
                scenario: named("switch-repo"),
                repo: web,
            },
            Step::Idle,
            Step::SelectRepo {
                scenario: named("switch-repo"),
                repo: api,
            },
            Step::Idle,
            Step::ScrollHistory {
                scenario: "scroll-history".into(),
            },
            Step::Idle,
            Step::SwitchTab {
                scenario: named("switch-tab"),
                tab: second.clone(),
            },
            Step::HistoryShown {
                scenario: named("switch-tab-history"),
            },
            Step::Idle,
            Step::SwitchTab {
                scenario: named("switch-tab"),
                tab: main.clone(),
            },
            Step::HistoryShown {
                scenario: named("switch-tab-history"),
            },
            Step::Idle,
            Step::SwitchTab {
                scenario: None,
                tab: diff_tab.clone(),
            },
            Step::Idle,
            Step::SelectRepo {
                scenario: None,
                repo: long.clone(),
            },
            Step::Idle,
            Step::SelectCommit {
                scenario: None,
                hash: long_commit.to_owned(),
            },
            Step::Idle,
            Step::ScrollDiff {
                scenario: "scroll-diff".into(),
            },
        ],
    }
}

/// Plays `script` a frame at a time, as `gitcroc-next --bench` does.
fn play(cx: &mut VisualTestContext, window: &Entity<GitcrocWindow>, script: Script) -> Trace {
    let mut runner = Runner::new(window.clone(), script);
    for _ in 0..20_000 {
        let tick = cx.update(|window, cx| {
            let tick = runner.tick(window, cx);
            window.render_frame(cx);
            tick
        });
        match tick {
            Tick::Running => {}
            Tick::Done(trace) => return trace,
            Tick::Failed(error) => panic!("the script failed: {error}"),
        }
        cx.run_until_parked();
        std::thread::sleep(Duration::from_millis(1));
    }
    panic!("the script never ended");
}

#[gpui::test]
fn perf_8_the_runner_plays_a_script(cx: &mut TestAppContext) {
    let fixture = Fixture::start(EVENT_CAPACITY);
    let (added, tabs, long, files, long_commit) = workspaces(&fixture);
    let (mut cx, window) = main_window(cx, &fixture);
    let trace = play(
        &mut cx,
        &window,
        script(&added, &tabs, &long, &files, &long_commit),
    );

    let mut timed: Vec<&str> = trace
        .samples
        .iter()
        .map(|sample| sample.scenario.as_str())
        .collect();
    timed.sort_unstable();
    assert_eq!(
        timed,
        [
            "cold-start",
            "select-commit",
            "select-largest-repo",
            "switch-repo",
            "switch-repo",
            "switch-tab",
            "switch-tab",
            "switch-tab-history",
            "switch-tab-history",
        ]
    );
    let scrolled: Vec<(&str, bool)> = trace
        .scrolls
        .iter()
        .map(|scroll| (scroll.scenario.as_str(), !scroll.intervals_ms.is_empty()))
        .collect();
    assert_eq!(scrolled, [("scroll-history", true), ("scroll-diff", true)]);
    // The diff of the long file scrolled down to its end.
    let top = cx.update(|_, cx| {
        let changes = window.read(cx).changes(cx).expect("no changes pane");
        changes.read(cx).diff().read(cx).top_row()
    });
    assert!(top > 300, "the diff stopped at line {top}");
}

#[test]
fn perf_8_the_runner_is_only_in_perf_trace_builds() {
    let manifest: toml::Table = include_str!("../../Cargo.toml").parse().unwrap();
    let features = manifest["features"].as_table().unwrap();
    let default = features.get("default").and_then(toml::Value::as_array);
    assert!(
        default.is_none_or(|default| !default.iter().any(|f| f.as_str() == Some("perf-trace"))),
        "perf-trace is a default feature"
    );
    // The module and `--bench` exist only with the feature.
    let lib = include_str!("../../src/lib.rs");
    assert!(lib.contains("#[cfg(feature = \"perf-trace\")]\npub mod perf;"));
    let main = include_str!("../../src/main.rs");
    assert!(
        main.contains(
            "#[cfg(feature = \"perf-trace\")]\n        [flag, _, _] if flag == \"--bench\""
        )
    );
    assert_eq!(main.matches("--bench\"").count(), 2);
}
