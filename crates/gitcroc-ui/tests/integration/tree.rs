//! The tree pane (UI-1 to UI-4, UI-6, UI-8): rows from the core's tree,
//! summaries on repo and group rows, the filter, and selection by mouse and
//! keyboard, kept per tab (UI-29). Repos are real ones made by `repo-gen`
//! and added through the app, as the user adds them (QA-2).

use gitcroc_app::app::EVENT_CAPACITY;
use gitcroc_core::{GroupId, RepoId, TargetSet};
use gitcroc_ui::window::GitcrocWindow;
use gpui_kit::test::TestWindowExt;
use gpui_kit::{
    ElementId, Entity, Modifiers, Role, ScrollDelta, TestAppContext, VisualTestContext, point, px,
};

use crate::support::{Fixture, main_window, settle};

/// `keys` with Cmd on macOS and Ctrl elsewhere.
fn secondary(keys: &str) -> String {
    let modifier = if cfg!(target_os = "macos") {
        "cmd"
    } else {
        "ctrl"
    };
    format!("{modifier}-{keys}")
}

/// Cmd on macOS, Ctrl elsewhere.
fn secondary_modifiers() -> Modifiers {
    if cfg!(target_os = "macos") {
        Modifiers::command()
    } else {
        Modifiers::control()
    }
}

fn press(cx: &mut VisualTestContext, keys: &str) {
    cx.update(|window, cx| window.press(keys, cx));
    cx.run_until_parked();
}

/// Repos `names` in a new folder, each with one commit on `main`. Dropping
/// it deletes them, so a test keeps it until it ends.
fn repos(names: &[&str]) -> repo_gen::Fixture {
    let repos = repo_gen::Fixture::new().unwrap();
    for name in names {
        repos
            .repo(name)
            .unwrap()
            .commit_file("README.md", "hello\n", "First")
            .unwrap();
    }
    repos
}

/// `count` repos named `r00`, `r01`, …
fn many(count: usize) -> repo_gen::Fixture {
    let names: Vec<String> = (0..count).map(|n| format!("r{n:02}")).collect();
    repos(&names.iter().map(String::as_str).collect::<Vec<_>>())
}

/// The repo `name` of `repos`.
fn repo(repos: &repo_gen::Fixture, name: &str) -> repo_gen::Repo {
    repo_gen::Repo::open(repos.path().join(name))
}

/// The id of `name` among the added repos.
fn id_of(added: &[(String, RepoId)], name: &str) -> RepoId {
    added
        .iter()
        .find(|(added, _)| added == name)
        .map(|(_, id)| id.clone())
        .unwrap_or_else(|| panic!("{name} was not added"))
}

/// Adds the repos of `made` to the active workspace.
fn add(fixture: &Fixture, made: &repo_gen::Fixture) -> Vec<(String, RepoId)> {
    fixture.add_repos(&fixture.active(), made.path())
}

fn group(fixture: &Fixture, name: &str, members: Vec<RepoId>) -> GroupId {
    let (app, workspace) = (fixture.app().clone(), fixture.active());
    fixture
        .runtime
        .block_on(app.group_create(workspace, name.to_owned(), members))
        .unwrap()
}

/// Reads every repo of the active workspace again, as when the window
/// regains the focus.
fn refresh(fixture: &Fixture) {
    let (app, workspace) = (fixture.app().clone(), fixture.active());
    fixture
        .runtime
        .block_on(app.workspace_refresh(workspace))
        .unwrap();
}

/// The element id of the row of node `node`.
fn row(node: &str) -> ElementId {
    ElementId::Name(format!("tree-row-{node}").into())
}

fn part(node: &str, part: &str) -> ElementId {
    ElementId::Name(format!("tree-row-{node}-{part}").into())
}

fn repo_node(added: &[(String, RepoId)], name: &str) -> String {
    format!("repo:{}", id_of(added, name).0)
}

/// A row of the tree as a screen reader reads it.
#[derive(Debug, Clone, PartialEq)]
struct Shown {
    /// The node's id.
    node: String,
    label: String,
    selected: bool,
    expanded: Option<bool>,
}

/// The tree's rows on screen, top to bottom.
fn shown(cx: &mut VisualTestContext) -> Vec<Shown> {
    cx.update(|window, cx| {
        window.render_frame(cx);
        if window.try_find("tree-list").is_none() {
            return Vec::new();
        }
        window
            .within("tree-list")
            .find_all_by_role(Role::TreeItem)
            .iter()
            .map(|found| {
                let node = match found.path().last() {
                    Some(ElementId::Name(name)) => name.trim_start_matches("tree-row-").to_owned(),
                    other => panic!("a row named {other:?}"),
                };
                Shown {
                    node,
                    label: found.label().unwrap_or_default().to_owned(),
                    selected: found.selected() == Some(true),
                    expanded: found.expanded(),
                }
            })
            .collect()
    })
}

fn labels(cx: &mut VisualTestContext) -> Vec<String> {
    shown(cx).into_iter().map(|row| row.label).collect()
}

fn selected(cx: &mut VisualTestContext) -> Vec<String> {
    shown(cx)
        .into_iter()
        .filter(|row| row.selected)
        .map(|row| row.label)
        .collect()
}

/// The labels of the marks of the row of node `node` (UI-3, UI-4).
fn marks(cx: &mut VisualTestContext, node: &str) -> Vec<String> {
    cx.update(|window, cx| {
        window.render_frame(cx);
        if window.try_find(row(node)).is_none() {
            return Vec::new();
        }
        window
            .within(row(node))
            .find_all_by_role(Role::Image)
            .iter()
            .filter_map(|found| found.label().map(str::to_owned))
            .collect()
    })
}

/// The label of element `id`, if it is on screen.
fn label_of(cx: &mut VisualTestContext, id: ElementId) -> Option<String> {
    cx.update(|window, cx| {
        window.render_frame(cx);
        window
            .try_find(id)
            .and_then(|found| found.label().map(str::to_owned))
    })
}

/// Runs the window and the app until `done` holds. A failure names the
/// caller's line.
#[track_caller]
fn wait(cx: &mut VisualTestContext, mut done: impl FnMut(&mut VisualTestContext) -> bool) {
    for _ in 0..1000 {
        settle(cx, |_, _| true);
        if done(cx) {
            return;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    panic!("the tree never got there: {:#?}", shown(cx));
}

#[track_caller]
fn wait_for_labels(cx: &mut VisualTestContext, expected: &[&str]) {
    wait(cx, |cx| labels(cx) == expected);
}

/// What the active tab's tree acts on (GRP-5).
fn target(cx: &mut VisualTestContext, window: &Entity<GitcrocWindow>) -> Option<TargetSet> {
    cx.update(|_, cx| {
        window
            .read(cx)
            .tree(cx)
            .and_then(|tree| tree.read(cx).target())
    })
}

/// Clicks `id` where the latest state draws it.
fn click(cx: &mut VisualTestContext, id: ElementId) {
    click_with(cx, id, Modifiers::default());
}

fn click_with(cx: &mut VisualTestContext, id: ElementId, modifiers: Modifiers) {
    cx.update(|window, cx| {
        window.render_frame(cx);
        window.click_with_modifiers(id, modifiers, cx);
    });
    cx.run_until_parked();
}

/// Types `text` into the filter box, replacing what it holds.
fn filter(cx: &mut VisualTestContext, text: &str) {
    click(cx, "tree-filter".into());
    press(cx, &secondary("a"));
    press(cx, "backspace");
    if !text.is_empty() {
        cx.update(|window, cx| window.input(text, cx));
        cx.run_until_parked();
    }
}

fn filter_text(cx: &mut VisualTestContext) -> String {
    cx.update(|window, cx| {
        window.render_frame(cx);
        window
            .find("tree-filter")
            .value()
            .unwrap_or_default()
            .to_owned()
    })
}

/// Scrolls the tree down by `rows` rows.
fn scroll_down(cx: &mut VisualTestContext, rows: f32) {
    let delta = ScrollDelta::Pixels(point(px(0.), px(-22. * rows)));
    cx.update(|window, cx| window.scroll("tree-list", delta, cx));
    cx.run_until_parked();
}

#[gpui::test]
fn ui_1_tree_shows_groups_then_ungrouped_with_repos_and_ref_nodes(cx: &mut TestAppContext) {
    let fixture = Fixture::start(EVENT_CAPACITY);
    let made = repos(&["api", "web"]);
    repo(&made, "web").branch("feature/x").unwrap();
    let added = add(&fixture, &made);
    let (api, web) = (id_of(&added, "api"), id_of(&added, "web"));
    let backend = group(&fixture, "Backend", vec![api.clone()]);
    let (mut cx, _) = main_window(cx, &fixture);

    // Each group with its Branches node and members, then Ungrouped with
    // the repos in no group. Groups and Ungrouped start open.
    wait_for_labels(&mut cx, &["Backend", "Branches", "api", "Ungrouped", "web"]);
    let rows = shown(&mut cx);
    assert_eq!(
        rows.iter().map(|row| row.node.clone()).collect::<Vec<_>>(),
        [
            format!("group:{}", backend.0),
            format!("group:{}:branches", backend.0),
            format!("group:{}:repo:{}", backend.0, api.0),
            "ungrouped".to_owned(),
            format!("repo:{}", web.0),
        ]
    );
    assert_eq!(
        rows.iter().map(|row| row.expanded).collect::<Vec<_>>(),
        [
            Some(true),
            Some(false),
            Some(false),
            Some(true),
            Some(false)
        ]
    );

    // A repo holds its four ref nodes; Local loads when first expanded,
    // with names containing `/` folded into folders, folders first.
    let web_node = format!("repo:{}", web.0);
    click(&mut cx, part(&web_node, "chevron"));
    wait_for_labels(
        &mut cx,
        &[
            "Backend",
            "Branches",
            "api",
            "Ungrouped",
            "web",
            "Local",
            "Remotes",
            "Tags",
            "Stashes",
        ],
    );
    let local = format!("{web_node}:local");
    click(&mut cx, part(&local, "chevron"));
    wait(&mut cx, |cx| labels(cx).contains(&"main".to_owned()));
    assert_eq!(labels(&mut cx)[6..8], ["feature", "main"]);
    let current = format!("{local}:ref:main");
    assert_eq!(marks(&mut cx, &current), ["Current branch"]);
    click(&mut cx, part(&format!("{local}:dir:feature/"), "chevron"));
    wait(&mut cx, |cx| labels(cx).contains(&"x".to_owned()));
    assert_eq!(labels(&mut cx)[6..9], ["feature", "x", "main"]);

    // The tree follows a change to the workspace.
    group(&fixture, "Frontend", vec![web]);
    wait(&mut cx, |cx| labels(cx).contains(&"Frontend".to_owned()));
}

#[gpui::test]
fn ui_1_an_empty_workspace_says_it_has_no_repositories(cx: &mut TestAppContext) {
    let fixture = Fixture::start(EVENT_CAPACITY);
    let (mut cx, _) = main_window(cx, &fixture);

    wait(&mut cx, |cx| label_of(cx, "tree-empty".into()).is_some());
    assert_eq!(
        label_of(&mut cx, "tree-empty".into()).as_deref(),
        Some("This workspace has no repositories yet.")
    );
}

#[gpui::test]
fn ui_1_keyboard_moves_opens_and_closes_rows(cx: &mut TestAppContext) {
    let fixture = Fixture::start(EVENT_CAPACITY);
    let made = repos(&["api", "web"]);
    add(&fixture, &made);
    let (mut cx, _) = main_window(cx, &fixture);
    wait_for_labels(&mut cx, &["Ungrouped", "api", "web"]);

    // The tree has the focus when the window opens; Down starts at the top.
    press(&mut cx, "down");
    assert_eq!(selected(&mut cx), ["Ungrouped"]);
    press(&mut cx, "up");
    assert_eq!(selected(&mut cx), ["Ungrouped"], "Up stops at the top");
    press(&mut cx, "down");
    assert_eq!(selected(&mut cx), ["api"]);
    // Right opens a row, then goes to its first child.
    press(&mut cx, "right");
    assert_eq!(
        labels(&mut cx)[2..6],
        ["Local", "Remotes", "Tags", "Stashes"]
    );
    press(&mut cx, "right");
    assert_eq!(selected(&mut cx), ["Local"]);
    // Left goes to the parent, then closes it.
    press(&mut cx, "left");
    assert_eq!(selected(&mut cx), ["api"]);
    press(&mut cx, "left");
    assert_eq!(labels(&mut cx), ["Ungrouped", "api", "web"]);
    press(&mut cx, "end");
    assert_eq!(selected(&mut cx), ["web"]);
    press(&mut cx, "home");
    assert_eq!(selected(&mut cx), ["Ungrouped"]);
    // Enter closes and opens.
    press(&mut cx, "enter");
    assert_eq!(labels(&mut cx), ["Ungrouped"]);
    press(&mut cx, "enter");
    assert_eq!(labels(&mut cx), ["Ungrouped", "api", "web"]);
}

#[gpui::test]
fn ui_2_a_repo_in_two_groups_shows_and_selects_in_each(cx: &mut TestAppContext) {
    let fixture = Fixture::start(EVENT_CAPACITY);
    let made = repos(&["api"]);
    let added = add(&fixture, &made);
    let api = id_of(&added, "api");
    let first = group(&fixture, "First", vec![api.clone()]);
    let second = group(&fixture, "Second", vec![api.clone()]);
    let (mut cx, window) = main_window(cx, &fixture);
    wait_for_labels(
        &mut cx,
        &[
            "First",
            "Branches",
            "api",
            "Second",
            "Branches",
            "api",
            "Ungrouped",
        ],
    );

    // Selecting one place selects that row, and the repo.
    let in_second = format!("group:{}:repo:{}", second.0, api.0);
    click(&mut cx, row(&in_second));
    let picked: Vec<String> = shown(&mut cx)
        .into_iter()
        .filter(|row| row.selected)
        .map(|row| row.node)
        .collect();
    assert_eq!(picked, std::slice::from_ref(&in_second));
    assert_eq!(
        target(&mut cx, &window),
        Some(TargetSet::Repo {
            repo_id: api.clone()
        })
    );

    // Each place loads its own refs.
    let in_first = format!("group:{}:repo:{}", first.0, api.0);
    for place in [&in_first, &in_second] {
        click(&mut cx, part(place, "chevron"));
        click(&mut cx, part(&format!("{place}:local"), "chevron"));
    }
    wait(&mut cx, |cx| {
        labels(cx).iter().filter(|label| *label == "main").count() == 2
    });
}

#[gpui::test]
fn ui_3_repo_row_shows_branch_counts_and_marks(cx: &mut TestAppContext) {
    let fixture = Fixture::start(EVENT_CAPACITY);
    let upstream = repos(&["origin"]);
    let origin = repo(&upstream, "origin");
    origin.commit_file("a.txt", "2\n", "Second").unwrap();
    let made = repos(&["clean", "dirty", "detached", "moved"]);
    origin
        .clone_to(made.path().join("ahead"))
        .unwrap()
        .commit_file("b.txt", "b\n", "Mine")
        .unwrap();
    origin
        .clone_to(made.path().join("behind"))
        .unwrap()
        .git(["reset", "--quiet", "--hard", "HEAD~1"])
        .unwrap();
    repo(&made, "dirty").write("new.txt", "new\n").unwrap();
    let detached = repo(&made, "detached");
    detached.git(["checkout", "--quiet", "--detach"]).unwrap();
    let hash = detached.head().unwrap();
    let added = add(&fixture, &made);
    let node = |name: &str| repo_node(&added, name);
    let (mut cx, _) = main_window(cx, &fixture);
    wait(&mut cx, |cx| {
        labels(cx).len() == 7 && marks(cx, &node("dirty")) == ["Uncommitted changes"]
    });

    // Clean shows its name and branch only, read in this run.
    assert_eq!(
        label_of(&mut cx, part(&node("clean"), "head")).as_deref(),
        Some("main")
    );
    assert!(marks(&mut cx, &node("clean")).is_empty());
    cx.update(|window, _| assert_eq!(window.find(row(&node("clean"))).description(), None));
    assert_eq!(marks(&mut cx, &node("dirty")), ["Uncommitted changes"]);
    assert_eq!(marks(&mut cx, &node("ahead")), ["1 ahead"]);
    assert_eq!(marks(&mut cx, &node("behind")), ["1 behind"]);
    // A detached head shows its short hash.
    assert_eq!(
        label_of(&mut cx, part(&node("detached"), "head")),
        Some(hash[..7].to_owned())
    );

    // A folder that is gone shows the warning, with the last known state,
    // stale (PERF-3).
    std::fs::remove_dir_all(made.path().join("moved")).unwrap();
    refresh(&fixture);
    wait(&mut cx, |cx| {
        marks(cx, &node("moved")).contains(&"Folder is missing".to_owned())
    });
    cx.update(|window, _| {
        assert_eq!(
            window.find(row(&node("moved"))).description(),
            Some("Last known state, not read again yet")
        );
    });
}

#[gpui::test]
fn ui_3_conflict_state_shows_the_conflict_icon(cx: &mut TestAppContext) {
    let fixture = Fixture::start(EVENT_CAPACITY);
    let made = repos(&["api"]);
    let api = repo(&made, "api");
    api.branch("other").unwrap();
    api.commit_file("README.md", "ours\n", "Ours").unwrap();
    api.switch("other").unwrap();
    api.commit_file("README.md", "theirs\n", "Theirs").unwrap();
    api.switch("main").unwrap();
    api.merge_with_conflicts("other").unwrap();
    let added = add(&fixture, &made);
    let (mut cx, _) = main_window(cx, &fixture);

    let node = repo_node(&added, "api");
    wait(&mut cx, |cx| {
        marks(cx, &node).contains(&"Conflict state".to_owned())
    });
}

#[gpui::test]
fn ui_4_group_row_shows_count_branch_and_totals(cx: &mut TestAppContext) {
    let fixture = Fixture::start(EVENT_CAPACITY);
    let made = repos(&["api", "web"]);
    let added = add(&fixture, &made);
    let backend = group(
        &fixture,
        "Backend",
        vec![id_of(&added, "api"), id_of(&added, "web")],
    );
    let (mut cx, _) = main_window(cx, &fixture);
    let node = format!("group:{}", backend.0);
    wait(&mut cx, |cx| {
        label_of(cx, part(&node, "branch")).as_deref() == Some("main")
    });
    assert_eq!(marks(&mut cx, &node), ["2 members"]);

    // A member's change: the row follows (GRP-4).
    let web = repo(&made, "web");
    web.write("new.txt", "new\n").unwrap();
    web.git(["switch", "--quiet", "-c", "dev"]).unwrap();
    refresh(&fixture);
    wait(&mut cx, |cx| {
        label_of(cx, part(&node, "branch")).as_deref() == Some("mixed (2)")
            && marks(cx, &node) == ["2 members", "1 member with changes"]
    });
    cx.update(|window, _| {
        assert_eq!(
            window.find(part(&node, "branch")).description(),
            Some("The members are on different branches")
        );
    });
}

#[gpui::test]
fn ui_6_ctrl_and_shift_click_build_a_multi_selection_of_repo_rows_only(cx: &mut TestAppContext) {
    let fixture = Fixture::start(EVENT_CAPACITY);
    let made = repos(&["a", "b", "c", "d"]);
    let added = add(&fixture, &made);
    let ids =
        |names: &[&str]| -> Vec<RepoId> { names.iter().map(|name| id_of(&added, name)).collect() };
    let node = |name: &str| repo_node(&added, name);
    let (mut cx, window) = main_window(cx, &fixture);
    wait_for_labels(&mut cx, &["Ungrouped", "a", "b", "c", "d"]);

    click(&mut cx, row(&node("a")));
    assert_eq!(selected(&mut cx), ["a"]);
    assert_eq!(
        target(&mut cx, &window),
        Some(TargetSet::Repo {
            repo_id: id_of(&added, "a")
        })
    );
    // Shift selects the repo rows from the anchor.
    click_with(&mut cx, row(&node("c")), Modifiers::shift());
    assert_eq!(selected(&mut cx), ["a", "b", "c"]);
    assert_eq!(
        target(&mut cx, &window),
        Some(TargetSet::Repos {
            repo_ids: ids(&["a", "b", "c"])
        })
    );
    // Ctrl/Cmd takes a repo row out, or puts it in.
    click_with(&mut cx, row(&node("b")), secondary_modifiers());
    assert_eq!(selected(&mut cx), ["a", "c"]);
    click_with(&mut cx, row(&node("d")), secondary_modifiers());
    assert_eq!(selected(&mut cx), ["a", "c", "d"]);
    // Any other row is selected alone.
    click_with(&mut cx, row("ungrouped"), secondary_modifiers());
    assert_eq!(selected(&mut cx), ["Ungrouped"]);
    assert_eq!(target(&mut cx, &window), None);

    // The same from the keyboard.
    click(&mut cx, row(&node("a")));
    press(&mut cx, "shift-down");
    press(&mut cx, "shift-down");
    assert_eq!(selected(&mut cx), ["a", "b", "c"]);
    press(&mut cx, &secondary("down"));
    assert_eq!(
        selected(&mut cx),
        ["a", "b", "c"],
        "the keyboard moved alone"
    );
    press(&mut cx, &secondary("space"));
    assert_eq!(selected(&mut cx), ["a", "b", "c", "d"]);
    press(&mut cx, &secondary("space"));
    assert_eq!(selected(&mut cx), ["a", "b", "c"]);

    // A branch row is selected alone; it acts on its repo.
    click(&mut cx, part(&node("a"), "chevron"));
    click(&mut cx, part(&format!("{}:local", node("a")), "chevron"));
    let main = format!("{}:local:ref:main", node("a"));
    wait(&mut cx, |cx| label_of(cx, row(&main)).is_some());
    click_with(&mut cx, row(&main), secondary_modifiers());
    assert_eq!(selected(&mut cx), ["main"]);
    assert_eq!(
        target(&mut cx, &window),
        Some(TargetSet::Repo {
            repo_id: id_of(&added, "a")
        })
    );
}

#[gpui::test]
fn ui_8_the_filter_shows_the_core_answer_expanded_and_clears(cx: &mut TestAppContext) {
    let fixture = Fixture::start(EVENT_CAPACITY);
    let made = repos(&["api", "web"]);
    repo(&made, "web").branch("feature/login").unwrap();
    let added = add(&fixture, &made);
    group(&fixture, "Backend", vec![id_of(&added, "api")]);
    let (mut cx, _) = main_window(cx, &fixture);
    let whole = ["Backend", "Branches", "api", "Ungrouped", "web"];
    wait_for_labels(&mut cx, &whole);

    // The core's answer, with every node it sent with children open.
    filter(&mut cx, "LOG");
    wait_for_labels(&mut cx, &["Ungrouped", "web", "Local", "feature", "login"]);
    assert!(
        shown(&mut cx)[..4]
            .iter()
            .all(|row| row.expanded == Some(true))
    );
    // Group names match too.
    filter(&mut cx, "back");
    wait(&mut cx, |cx| {
        let shown = labels(cx);
        shown.first().map(String::as_str) == Some("Backend")
            && shown.contains(&"api".to_owned())
            && !shown.contains(&"web".to_owned())
    });
    // Cleared, the whole tree is back as it was.
    filter(&mut cx, "");
    wait_for_labels(&mut cx, &whole);
}

#[gpui::test]
fn ui_29_each_tab_keeps_its_tree(cx: &mut TestAppContext) {
    let fixture = Fixture::start(EVENT_CAPACITY);
    let mine = fixture.active();
    let made = many(40);
    let added = add(&fixture, &made);
    let app = fixture.app().clone();
    let java = fixture.runtime.block_on(async {
        let tabs = app.workspace_create("Java".to_owned()).await.unwrap();
        tabs.active.unwrap()
    });
    let java_repos = repos(&["core"]);
    fixture.add_repos(&java, java_repos.path());
    fixture
        .runtime
        .block_on(app.workspace_activate(mine.clone()))
        .unwrap();
    let (mut cx, window) = main_window(cx, &fixture);
    wait(&mut cx, |cx| {
        labels(cx).first().map(String::as_str) == Some("Ungrouped")
    });

    // Select a repo, open it, scroll, and filter.
    let r01 = repo_node(&added, "r01");
    click(&mut cx, row(&r01));
    click(&mut cx, part(&r01, "chevron"));
    scroll_down(&mut cx, 3.);
    let top = shown(&mut cx)[0].node.clone();
    assert_eq!(top, format!("{r01}:local"), "the tree scrolled by 3 rows");
    // What the tree holds, whether or not its rows are on screen.
    let kept = |cx: &mut VisualTestContext| {
        cx.update(|_, cx| {
            let tree = window.read(cx).tree(cx).unwrap();
            let tree = tree.read(cx);
            let opened = tree.rows().iter().any(|row| row.id == top);
            (tree.tree().selected().map(str::to_owned), opened)
        })
    };
    assert_eq!(kept(&mut cx), (Some(r01.clone()), true));
    let tab = |id: &gitcroc_core::WorkspaceId| ElementId::Name(format!("tab-{}", id.0).into());

    // The other tab shows its own tree.
    click(&mut cx, tab(&java));
    wait_for_labels(&mut cx, &["Ungrouped", "core"]);
    assert!(selected(&mut cx).is_empty());
    assert_eq!(filter_text(&mut cx), "");

    // Back, everything is as it was left.
    click(&mut cx, tab(&mine));
    wait(&mut cx, |cx| {
        shown(cx).first().map(|row| row.node.as_str()) == Some(top.as_str())
    });
    assert_eq!(kept(&mut cx), (Some(r01.clone()), true));

    // The filter text too, with its answer.
    filter(&mut cx, "r07");
    let filtered = ["Ungrouped", "r07", "Local", "Remotes", "Tags", "Stashes"];
    wait_for_labels(&mut cx, &filtered);
    click(&mut cx, tab(&java));
    wait_for_labels(&mut cx, &["Ungrouped", "core"]);
    assert_eq!(filter_text(&mut cx), "");
    click(&mut cx, tab(&mine));
    wait_for_labels(&mut cx, &filtered);
    assert_eq!(filter_text(&mut cx), "r07");
}

#[gpui::test]
fn perf_1_the_tree_lays_out_only_the_rows_on_screen(cx: &mut TestAppContext) {
    let fixture = Fixture::start(EVENT_CAPACITY);
    let made = many(60);
    add(&fixture, &made);
    let (mut cx, _) = main_window(cx, &fixture);
    wait(&mut cx, |cx| {
        labels(cx).first().map(String::as_str) == Some("Ungrouped")
    });

    // 61 rows, and the tree has room for far fewer.
    let (rendered, height) = cx.update(|window, cx| {
        window.render_frame(cx);
        let height = window.find("tree-list").bounds().size.height;
        (
            window
                .within("tree-list")
                .find_all_by_role(Role::TreeItem)
                .len(),
            height,
        )
    });
    let fit = (f32::from(height) / 22.).ceil() as usize;
    assert!(fit < 61, "the window has room for every row: {fit}");
    assert!(
        rendered <= fit + 1,
        "{rendered} rows laid out for {fit} on screen"
    );
    assert!(
        rendered >= fit - 1,
        "the tree is filled: {rendered} of {fit}"
    );

    // At the end, the last rows are laid out and the first are not.
    scroll_down(&mut cx, 100.);
    let shown_now = labels(&mut cx);
    assert_eq!(shown_now.last().map(String::as_str), Some("r59"));
    assert!(!shown_now.contains(&"Ungrouped".to_owned()));
}

#[gpui::test]
fn sec_3_repo_and_branch_names_render_as_text(cx: &mut TestAppContext) {
    let fixture = Fixture::start(EVENT_CAPACITY);
    let made = repos(&["a&amp;b"]);
    let branch = "<b>bold&amp;";
    // A loose ref is a file, and Windows forbids `<` in file names, so the
    // branch goes in `packed-refs`, where git keeps a fetched one.
    let hash = repo(&made, "a&amp;b").head().unwrap();
    std::fs::write(
        made.path().join("a&amp;b").join(".git").join("packed-refs"),
        format!("{hash} refs/heads/{branch}\n"),
    )
    .unwrap();
    let added = add(&fixture, &made);
    let group_name = "<i>Group</i>";
    group(&fixture, group_name, vec![id_of(&added, "a&amp;b")]);
    let (mut cx, _) = main_window(cx, &fixture);
    wait(&mut cx, |cx| labels(cx).contains(&group_name.to_owned()));

    let node = shown(&mut cx)
        .into_iter()
        .find(|row| row.label == "a&amp;b")
        .map(|row| row.node)
        .unwrap();
    click(&mut cx, part(&node, "chevron"));
    click(&mut cx, part(&format!("{node}:local"), "chevron"));
    wait(&mut cx, |cx| labels(cx).contains(&branch.to_owned()));
    assert_eq!(labels(&mut cx)[0], group_name);
}
