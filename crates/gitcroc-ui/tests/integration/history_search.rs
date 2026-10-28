//! The history's search box, its "Uncommitted changes (n)" row and the
//! commit-graph offer (UI-12, UI-10, PERF-2). Repos are real ones made by
//! `repo-gen` and added through the app (QA-2).

use std::time::Duration;

use gitcroc_app::app::EVENT_CAPACITY;
use gitcroc_ui::history::Selection;
use gitcroc_ui::window::GitcrocWindow;
use gpui_kit::test::TestWindowExt;
use gpui_kit::{Entity, TestAppContext, VisualTestContext};

use crate::history::{
    add, click, history_labels, history_rows, label_of, press, repo, repo_node, repos, tree_part,
    tree_row, wait_for_element, wait_for_rows, wait_until, with_history,
};
use crate::support::{Fixture, Options, main_window};

/// Replaces the search text with `text`, typed a character at a time; each
/// character asks the core again before the previous answer is in.
fn search(cx: &mut VisualTestContext, text: &str) {
    click(cx, "history-search".into());
    press(
        cx,
        if cfg!(target_os = "macos") {
            "cmd-a"
        } else {
            "ctrl-a"
        },
    );
    press(cx, "backspace");
    for character in text.chars() {
        cx.update(|window, cx| window.input(&character.to_string(), cx));
    }
    cx.run_until_parked();
}

fn search_text(cx: &mut VisualTestContext) -> String {
    cx.update(|window, cx| {
        window.render_frame(cx);
        window
            .find("history-search")
            .value()
            .unwrap_or_default()
            .to_owned()
    })
}

fn subjects(cx: &mut VisualTestContext, window: &Entity<GitcrocWindow>) -> Vec<String> {
    with_history(cx, window, |history| {
        history
            .rows()
            .iter()
            .map(|row| row.subject.to_string())
            .collect()
    })
}

fn shown(cx: &mut VisualTestContext, id: &'static str) -> bool {
    cx.update(|window, cx| {
        window.render_frame(cx);
        window.try_find(id).is_some()
    })
}

/// The history pane's selection, as the changes pane reads it (M6-12).
fn selection(cx: &mut VisualTestContext, window: &Entity<GitcrocWindow>) -> Option<Selection> {
    cx.update(|_, cx| {
        let pane = window.read(cx).history(cx).expect("no history pane");
        pane.read(cx).selection()
    })
}

/// Opens the Local node of repo node `node` and returns its branch `name`'s
/// node.
fn open_branch(cx: &mut VisualTestContext, node: &str, name: &str) -> String {
    click(cx, tree_part(node, "chevron"));
    let local = format!("{node}:local");
    wait_for_element(cx, tree_part(&local, "chevron"));
    click(cx, tree_part(&local, "chevron"));
    let branch = format!("{local}:ref:{name}");
    wait_for_element(cx, tree_row(&branch));
    branch
}

/// Waits until the history shows the source of tree node `key` with its
/// first page read.
fn wait_for_source(cx: &mut VisualTestContext, window: &Entity<GitcrocWindow>, key: &str) {
    wait_until(cx, |cx| {
        with_history(cx, window, |history| {
            history.source().is_some_and(|source| source.key == key)
                && !history.loading()
                && history.handle().is_some()
        })
    });
}

#[gpui::test]
fn ui_12_search_filters_in_the_core(cx: &mut TestAppContext) {
    let fixture = Fixture::start(EVENT_CAPACITY);
    let made = repos(&["api", "web"]);
    let api = repo(&made, "api");
    api.commit_file("a.txt", "a\n", "Add parser").unwrap();
    api.commit_file("b.txt", "b\n", "Fix parser bug").unwrap();
    api.commit_file("c.txt", "c\n", "Write docs").unwrap();
    let docs = api.head().unwrap();
    let added = add(&fixture, &made);
    let (mut cx, window) = main_window(cx, &fixture);
    let node = repo_node(&added, "api");
    wait_for_element(&mut cx, tree_row(&node));
    click(&mut cx, tree_row(&node));
    wait_for_rows(&mut cx, &window, 4);
    assert_eq!(
        label_of(&mut cx, "history-search".into()).as_deref(),
        Some("Search history")
    );

    // The core filters by subject; the pane shows its answer as it is.
    // "r" matches every row by its author, "rit" only one: an older answer
    // that arrives late must not replace the newer one.
    search(&mut cx, "rit");
    wait_until(&mut cx, |cx| subjects(cx, &window) == ["Write docs"]);
    for _ in 0..20 {
        wait_until(&mut cx, |_| true);
        assert_eq!(subjects(&mut cx, &window), ["Write docs"]);
    }
    assert_eq!(history_labels(&mut cx), ["Write docs"]);

    // Filtered rows have no graph.
    search(&mut cx, "parser");
    wait_until(&mut cx, |cx| {
        subjects(cx, &window) == ["Fix parser bug", "Add parser"]
    });
    assert!(with_history(&mut cx, &window, |history| {
        history.rows().iter().all(|row| row.graph.is_none())
    }));

    // A hash prefix finds its commit.
    search(&mut cx, &docs[..8]);
    wait_until(&mut cx, |cx| subjects(cx, &window) == ["Write docs"]);

    // Nothing matches.
    search(&mut cx, "no such commit");
    wait_until(&mut cx, |cx| {
        label_of(cx, "history-empty".into()).as_deref() == Some("No commits match.")
    });

    // The text belongs to the selection: another one starts without it, and
    // it comes back with its answer on return.
    search(&mut cx, "parser");
    wait_until(&mut cx, |cx| subjects(cx, &window).len() == 2);
    let web = repo_node(&added, "web");
    click(&mut cx, tree_row(&web));
    wait_for_rows(&mut cx, &window, 1);
    assert_eq!(search_text(&mut cx), "");
    click(&mut cx, tree_row(&node));
    wait_until(&mut cx, |cx| {
        subjects(cx, &window) == ["Fix parser bug", "Add parser"]
    });
    assert_eq!(search_text(&mut cx), "parser");

    // Clearing the text shows the whole history again, with its graph.
    search(&mut cx, "");
    wait_for_rows(&mut cx, &window, 4);
    assert!(with_history(&mut cx, &window, |history| {
        history.rows().iter().all(|row| row.graph.is_some())
    }));
}

#[gpui::test]
fn ui_10_uncommitted_row_leads_a_dirty_repo(cx: &mut TestAppContext) {
    let fixture = Fixture::start(EVENT_CAPACITY);
    let made = repos(&["api"]);
    let api = repo(&made, "api");
    api.branch("topic").unwrap();
    api.write("README.md", "changed\n").unwrap();
    api.write("notes.txt", "notes\n").unwrap();
    let added = add(&fixture, &made);
    let api_id = crate::history::id_of(&added, "api");
    let (mut cx, window) = main_window(cx, &fixture);
    let node = repo_node(&added, "api");
    wait_for_element(&mut cx, tree_row(&node));

    // A dirty repo leads with the row, counting its changed paths.
    click(&mut cx, tree_row(&node));
    wait_for_rows(&mut cx, &window, 1);
    wait_for_element(&mut cx, "history-uncommitted".into());
    assert_eq!(
        history_labels(&mut cx),
        ["Uncommitted changes (2)", "Initial"]
    );

    // Selecting it reports the repo's uncommitted target.
    click(&mut cx, "history-uncommitted".into());
    assert_eq!(
        selection(&mut cx, &window),
        Some(Selection::Uncommitted {
            repo: api_id.clone()
        })
    );
    // The keys move from it to the commits and back.
    press(&mut cx, "down");
    assert!(matches!(
        selection(&mut cx, &window),
        Some(Selection::Commit { .. })
    ));
    press(&mut cx, "home");
    assert_eq!(
        selection(&mut cx, &window),
        Some(Selection::Uncommitted {
            repo: api_id.clone()
        })
    );

    // The current branch shows it too; a branch not checked out does not.
    let main = open_branch(&mut cx, &node, "main");
    click(&mut cx, tree_row(&main));
    wait_for_source(&mut cx, &window, &format!("branch:{}:main", api_id.0));
    assert!(shown(&mut cx, "history-uncommitted"));
    let topic = format!("{node}:local:ref:topic");
    click(&mut cx, tree_row(&topic));
    wait_for_source(&mut cx, &window, &format!("branch:{}:topic", api_id.0));
    assert!(!shown(&mut cx, "history-uncommitted"));
    assert_eq!(history_labels(&mut cx), ["Initial"]);

    // Once the working copy is clean, the row goes.
    click(&mut cx, tree_row(&node));
    wait_for_element(&mut cx, "history-uncommitted".into());
    api.git(["checkout", "--", "README.md"]).unwrap();
    std::fs::remove_file(made.path().join("api").join("notes.txt")).unwrap();
    fixture.refresh(&fixture.active());
    wait_until(&mut cx, |cx| !shown(cx, "history-uncommitted"));
    assert_eq!(history_labels(&mut cx), ["Initial"]);
}

#[gpui::test]
fn perf_2_commit_graph_offer_once_per_repo(cx: &mut TestAppContext) {
    // Every first page is "slow" against a zero threshold.
    let fixture = Fixture::start_with(Options {
        commit_graph_threshold: Duration::ZERO,
        ..Options::default()
    });
    let made = repos(&["api", "lib", "web"]);
    // Git cannot take lib's lock, so its write fails.
    let info = made
        .path()
        .join("lib")
        .join(".git")
        .join("objects")
        .join("info");
    std::fs::create_dir_all(&info).unwrap();
    std::fs::write(info.join("commit-graph.lock"), "").unwrap();
    let added = add(&fixture, &made);
    let workspace = fixture.active();
    let (mut cx, window) = main_window(cx, &fixture);
    let api = repo_node(&added, "api");
    wait_for_element(&mut cx, tree_row(&api));

    // A slow first page without the file brings the offer.
    click(&mut cx, tree_row(&api));
    wait_for_element(&mut cx, "history-offer".into());
    assert_eq!(
        label_of(&mut cx, "history-offer".into()).as_deref(),
        Some("Commit-graph offer")
    );
    // Another selection in the same repo keeps the one offer.
    let main = open_branch(&mut cx, &api, "main");
    click(&mut cx, tree_row(&main));
    let api_id = crate::history::id_of(&added, "api");
    wait_for_source(&mut cx, &window, &format!("branch:{}:main", api_id.0));
    assert!(shown(&mut cx, "history-offer"));

    // Declining hides it, keeps the answer in the workspace, and no other
    // selection in the repo brings it back.
    click(&mut cx, "history-offer-decline".into());
    assert!(!shown(&mut cx, "history-offer"));
    wait_until(&mut cx, |_| {
        fixture.workspace_file(&workspace).contains("declined")
    });
    click(&mut cx, tree_row(&api));
    wait_for_source(&mut cx, &window, &format!("repo:{}", api_id.0));
    assert!(!shown(&mut cx, "history-offer"));
    assert_eq!(history_rows(&mut cx, &window), 1);

    // Writing starts the job, which writes the file.
    let web = repo_node(&added, "web");
    click(&mut cx, tree_row(&web));
    wait_for_element(&mut cx, "history-offer".into());
    click(&mut cx, "history-offer-write".into());
    assert!(!shown(&mut cx, "history-offer"));
    let written = made
        .path()
        .join("web")
        .join(".git")
        .join("objects")
        .join("info")
        .join("commit-graph");
    wait_until(&mut cx, |_| written.is_file());

    // A failed write says why inline; history keeps working without it.
    let lib = repo_node(&added, "lib");
    click(&mut cx, tree_row(&lib));
    wait_for_element(&mut cx, "history-offer".into());
    click(&mut cx, "history-offer-write".into());
    wait_for_element(&mut cx, "history-offer-error".into());
    let error = label_of(&mut cx, "history-offer-error".into()).unwrap();
    assert!(
        error.starts_with("Cannot write the commit-graph file: "),
        "{error}"
    );
    assert!(!shown(&mut cx, "history-offer"));
    assert_eq!(history_rows(&mut cx, &window), 1);
    click(&mut cx, "history-offer-error-dismiss".into());
    assert!(!shown(&mut cx, "history-offer-error"));

    // An offer answered this session is not shown again.
    click(&mut cx, tree_row(&web));
    wait_for_source(
        &mut cx,
        &window,
        &format!("repo:{}", crate::history::id_of(&added, "web").0),
    );
    click(&mut cx, tree_row(&lib));
    wait_for_source(
        &mut cx,
        &window,
        &format!("repo:{}", crate::history::id_of(&added, "lib").0),
    );
    assert!(!shown(&mut cx, "history-offer"));
}
