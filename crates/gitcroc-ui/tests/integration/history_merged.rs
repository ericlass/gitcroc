//! The merged history of a group or several repos (section 8 "History
//! pane"): its repo column (UI-9), the summary strip (UI-11) and "Uncommitted
//! changes (n repos)" (UI-34), searched in the core (UI-12) and kept per tab
//! (UI-29). Repos are real ones made by `repo-gen` and added through the app
//! (QA-2).

use gitcroc_app::app::EVENT_CAPACITY;
use gitcroc_core::{GroupId, RepoId, TargetSet};
use gitcroc_ui::history::Selection;
use gitcroc_ui::window::GitcrocWindow;
use gpui_kit::test::TestWindowExt;
use gpui_kit::{
    ElementId, Entity, Modifiers, Role, ScrollDelta, TestAppContext, VisualTestContext, point, px,
};

use crate::history::{
    add, click, history_labels, history_rows, id_of, label_of, labels_in, repo, repos, scroll, tab,
    tree_row, wait_for_element, wait_until, with_history,
};
use crate::history_search::{search, search_text, selection, wait_for_source};
use crate::support::{Fixture, main_window};

/// A strip line's height; the strip shows up to eight.
const LINE_HEIGHT: f32 = 24.;

pub(crate) fn group(fixture: &Fixture, name: &str, members: Vec<RepoId>) -> GroupId {
    let (app, workspace) = (fixture.app().clone(), fixture.active());
    fixture
        .runtime
        .block_on(app.group_create(workspace, name.to_owned(), members))
        .unwrap()
}

pub(crate) fn group_node(group: &GroupId) -> String {
    format!("group:{}", group.0)
}

fn member_node(group: &GroupId, repo: &RepoId) -> String {
    format!("group:{}:repo:{}", group.0, repo.0)
}

fn name(id: String) -> ElementId {
    ElementId::Name(id.into())
}

fn row(index: usize) -> ElementId {
    name(format!("history-row-{index}"))
}

fn repo_cell(index: usize) -> ElementId {
    name(format!("history-row-{index}-repo"))
}

fn graph_cell(index: usize) -> ElementId {
    name(format!("history-row-{index}-graph"))
}

fn strip_line(index: usize) -> ElementId {
    name(format!("history-strip-{index}"))
}

fn strip_part(index: usize, part: &str) -> ElementId {
    name(format!("history-strip-{index}-{part}"))
}

pub(crate) fn present(cx: &mut VisualTestContext, id: ElementId) -> bool {
    cx.update(|window, cx| {
        window.render_frame(cx);
        window.try_find(id).is_some()
    })
}

/// The strip's lines on screen, by repo name.
fn strip_names(cx: &mut VisualTestContext) -> Vec<String> {
    labels_in(cx, "history-strip".into(), Role::ListItem)
}

/// The marks of strip line `index`, as a screen reader names them.
fn strip_marks(cx: &mut VisualTestContext, index: usize) -> Vec<String> {
    labels_in(cx, strip_line(index), Role::Image)
}

fn click_with(cx: &mut VisualTestContext, id: ElementId, modifiers: Modifiers) {
    cx.update(|window, cx| {
        window.render_frame(cx);
        window.click_with_modifiers(id, modifiers, cx);
    });
    cx.run_until_parked();
}

/// Cmd on macOS, Ctrl elsewhere.
fn secondary_modifiers() -> Modifiers {
    if cfg!(target_os = "macos") {
        Modifiers::command()
    } else {
        Modifiers::control()
    }
}

/// The tree's selected row.
fn tree_selected(cx: &mut VisualTestContext, window: &Entity<GitcrocWindow>) -> Option<String> {
    cx.update(|_, cx| {
        let tree = window.read(cx).tree(cx).expect("no tree pane");
        tree.read(cx).tree().selected().map(str::to_owned)
    })
}

/// Waits until the history shows a merged history of several repos, read.
fn wait_for_merged(cx: &mut VisualTestContext, window: &Entity<GitcrocWindow>) {
    wait_until(cx, |cx| {
        with_history(cx, window, |history| {
            history
                .source()
                .is_some_and(|source| source.repo.is_none() && source.key.starts_with("repos:"))
                && !history.loading()
                && history.handle().is_some()
        })
    });
}

fn header_label(cx: &mut VisualTestContext) -> Option<String> {
    label_of(cx, "history-column-graph".into())
}

#[gpui::test]
fn ui_9_merged_list_shows_the_repo_column(cx: &mut TestAppContext) {
    let fixture = Fixture::start(EVENT_CAPACITY);
    let made = repos(&["api", "web"]);
    let (api, web) = (repo(&made, "api"), repo(&made, "web"));
    // Web's commit is the newest, api's the one before.
    api.set_clock(100);
    api.commit_file("a.txt", "a\n", "Api change").unwrap();
    web.set_clock(200);
    web.commit_file("w.txt", "w\n", "Web change").unwrap();
    let web_head = web.head().unwrap();
    let added = add(&fixture, &made);
    let (api_id, web_id) = (id_of(&added, "api"), id_of(&added, "web"));
    let backend = group(&fixture, "Backend", vec![api_id.clone(), web_id.clone()]);
    let (mut cx, window) = main_window(cx, &fixture);

    // The group shows one list of its repos' commits, newest first by
    // commit date.
    let node = group_node(&backend);
    wait_for_element(&mut cx, tree_row(&node));
    click(&mut cx, tree_row(&node));
    wait_for_source(&mut cx, &window, &node);
    let labels = history_labels(&mut cx);
    assert_eq!(labels.len(), 4, "{labels:?}");
    assert_eq!(labels[..2], ["Web change", "Api change"]);

    // The repo's name has the graph column's place, and its header says so.
    assert_eq!(label_of(&mut cx, repo_cell(0)).as_deref(), Some("web"));
    assert_eq!(label_of(&mut cx, repo_cell(1)).as_deref(), Some("api"));
    assert!(!present(&mut cx, graph_cell(0)));
    assert_eq!(header_label(&mut cx).as_deref(), Some("Repository"));

    // A row selects that commit of that repo.
    click(&mut cx, row(0));
    assert_eq!(
        selection(&mut cx, &window),
        Some(Selection::Commit {
            repo: web_id.clone(),
            hash: web_head,
        })
    );

    // Search keeps the repo column.
    search(&mut cx, "Web");
    wait_until(&mut cx, |cx| history_labels(cx) == ["Web change"]);
    assert_eq!(label_of(&mut cx, repo_cell(0)).as_deref(), Some("web"));

    // A repo of the group shows its own history, with the graph again.
    let member = member_node(&backend, &api_id);
    click(&mut cx, tree_row(&member));
    wait_for_source(&mut cx, &window, &format!("repo:{}", api_id.0));
    assert_eq!(history_labels(&mut cx), ["Api change", "Initial"]);
    assert!(present(&mut cx, graph_cell(0)));
    assert!(!present(&mut cx, repo_cell(0)));
    assert_eq!(header_label(&mut cx).as_deref(), Some("Graph"));
}

#[gpui::test]
fn ui_11_summary_strip_shows_each_repo_and_selects_it_in_the_tree(cx: &mut TestAppContext) {
    let fixture = Fixture::start(EVENT_CAPACITY);
    let upstream = repos(&["origin"]);
    let origin = repo(&upstream, "origin");
    origin.commit_file("a.txt", "2\n", "Second").unwrap();
    let made = repos(&["dirty", "clash", "gone"]);
    origin
        .clone_to(made.path().join("ahead"))
        .unwrap()
        .commit_file("b.txt", "b\n", "Mine")
        .unwrap();
    let dirty = repo(&made, "dirty");
    dirty.write("x.txt", "x\n").unwrap();
    dirty.write("y.txt", "y\n").unwrap();
    let clash = repo(&made, "clash");
    clash.branch("other").unwrap();
    clash.commit_file("README.md", "ours\n", "Ours").unwrap();
    clash.switch("other").unwrap();
    clash
        .commit_file("README.md", "theirs\n", "Theirs")
        .unwrap();
    clash.switch("main").unwrap();
    clash.merge_with_conflicts("other").unwrap();
    let added = add(&fixture, &made);
    let members: Vec<RepoId> = ["ahead", "dirty", "clash", "gone"]
        .iter()
        .map(|name| id_of(&added, name))
        .collect();
    let all = group(&fixture, "All", members.clone());
    let (mut cx, window) = main_window(cx, &fixture);

    // One line per repo, in the group's order.
    let node = group_node(&all);
    wait_for_element(&mut cx, tree_row(&node));
    click(&mut cx, tree_row(&node));
    wait_until(&mut cx, |cx| {
        strip_names(cx) == ["ahead", "dirty", "clash", "gone"]
            && strip_marks(cx, 2).contains(&"Conflict state".to_owned())
    });
    assert_eq!(
        label_of(&mut cx, "history-strip".into()).as_deref(),
        Some("Repositories of the selection")
    );

    // Each line has the branch, ahead and behind, the dirty count and the
    // conflict flag, named for screen readers.
    assert_eq!(
        label_of(&mut cx, strip_part(0, "head")).as_deref(),
        Some("main")
    );
    assert_eq!(strip_marks(&mut cx, 0), ["1 ahead"]);
    assert_eq!(strip_marks(&mut cx, 1), ["2 changed files"]);
    assert!(strip_marks(&mut cx, 2).contains(&"Conflict state".to_owned()));
    assert!(strip_marks(&mut cx, 3).is_empty());
    cx.update(|window, _| assert_eq!(window.find(strip_line(0)).description(), None));

    // A repo that cannot be read again keeps its last known summary, dimmed
    // (PERF-3).
    std::fs::remove_dir_all(made.path().join("gone")).unwrap();
    fixture.refresh(&fixture.active());
    wait_until(&mut cx, |cx| {
        strip_marks(cx, 3).contains(&"Folder is missing".to_owned())
    });
    cx.update(|window, _| {
        assert_eq!(
            window.find(strip_line(3)).description(),
            Some("Last known state, not read again yet")
        );
    });

    // Clicking a line selects its repo in the tree, under the group.
    click(&mut cx, strip_line(1));
    wait_for_source(&mut cx, &window, &format!("repo:{}", members[1].0));
    assert_eq!(
        tree_selected(&mut cx, &window),
        Some(member_node(&all, &members[1]))
    );
    assert!(!present(&mut cx, "history-strip".into()));
}

#[gpui::test]
fn ui_11_a_group_without_repos_says_so(cx: &mut TestAppContext) {
    let fixture = Fixture::start(EVENT_CAPACITY);
    let made = repos(&["api"]);
    add(&fixture, &made);
    let empty = group(&fixture, "Empty", Vec::new());
    let (mut cx, _) = main_window(cx, &fixture);

    let node = group_node(&empty);
    wait_for_element(&mut cx, tree_row(&node));
    click(&mut cx, tree_row(&node));
    wait_for_element(&mut cx, "history-no-repos".into());
    assert_eq!(
        label_of(&mut cx, "history-no-repos".into()).as_deref(),
        Some("This group has no repositories.")
    );
    assert!(!present(&mut cx, "history-strip".into()));
}

#[gpui::test]
fn perf_1_the_summary_strip_lays_out_only_the_lines_on_screen(cx: &mut TestAppContext) {
    let fixture = Fixture::start(EVENT_CAPACITY);
    let names: Vec<String> = (0..20).map(|n| format!("r{n:02}")).collect();
    let made = repos(&names.iter().map(String::as_str).collect::<Vec<_>>());
    let added = add(&fixture, &made);
    let members: Vec<RepoId> = names.iter().map(|name| id_of(&added, name)).collect();
    let all = group(&fixture, "All", members);
    let (mut cx, _) = main_window(cx, &fixture);

    let node = group_node(&all);
    wait_for_element(&mut cx, tree_row(&node));
    click(&mut cx, tree_row(&node));
    wait_until(&mut cx, |cx| {
        strip_names(cx).first().map(String::as_str) == Some("r00")
    });

    // Eight lines show; the rest scroll, and only those on screen are laid
    // out.
    let (lines, height) = cx.update(|window, cx| {
        window.render_frame(cx);
        let height = window.find("history-strip").bounds().size.height;
        (
            window
                .within("history-strip")
                .find_all_by_role(Role::ListItem)
                .len(),
            height,
        )
    });
    assert!(
        (f32::from(height) - 8. * LINE_HEIGHT).abs() < 1.,
        "the strip is {height:?} high"
    );
    assert!((8..=9).contains(&lines), "{lines} lines laid out for 8");

    cx.update(|window, cx| {
        window.scroll(
            "history-strip",
            ScrollDelta::Pixels(point(px(0.), px(-20. * LINE_HEIGHT))),
            cx,
        );
        window.render_frame(cx);
    });
    cx.run_until_parked();
    assert!(present(&mut cx, strip_line(19)));
    assert!(!present(&mut cx, strip_line(0)));
}

#[gpui::test]
fn ui_34_merged_uncommitted_row_counts_dirty_repos(cx: &mut TestAppContext) {
    let fixture = Fixture::start(EVENT_CAPACITY);
    let made = repos(&["api", "web", "lib"]);
    // Api has three changed paths and web one: two dirty repos.
    let api = repo(&made, "api");
    api.write("README.md", "changed\n").unwrap();
    api.write("a.txt", "a\n").unwrap();
    api.write("b.txt", "b\n").unwrap();
    repo(&made, "web").write("w.txt", "w\n").unwrap();
    let added = add(&fixture, &made);
    let (api_id, web_id, lib_id) = (
        id_of(&added, "api"),
        id_of(&added, "web"),
        id_of(&added, "lib"),
    );
    let backend = group(
        &fixture,
        "Backend",
        vec![api_id.clone(), web_id, lib_id.clone()],
    );
    let clean = group(&fixture, "Clean", vec![lib_id.clone()]);
    let (mut cx, window) = main_window(cx, &fixture);

    // A dirty group leads with the dirty repos' count.
    let node = group_node(&backend);
    wait_for_element(&mut cx, tree_row(&node));
    click(&mut cx, tree_row(&node));
    wait_for_element(&mut cx, "history-uncommitted".into());
    wait_until(&mut cx, |cx| {
        history_labels(cx).first().map(String::as_str) == Some("Uncommitted changes (2 repos)")
    });

    // Selecting it reports the merged uncommitted target.
    click(&mut cx, "history-uncommitted".into());
    assert_eq!(
        selection(&mut cx, &window),
        Some(Selection::MergedUncommitted {
            target: TargetSet::Group {
                group_id: backend.clone(),
            },
        })
    );

    // A clean group has no such row.
    let clean_node = group_node(&clean);
    click(&mut cx, tree_row(&clean_node));
    wait_for_source(&mut cx, &window, &clean_node);
    assert_eq!(history_labels(&mut cx), ["Initial"]);
    assert!(!present(&mut cx, "history-uncommitted".into()));

    // Several repos, one of them dirty.
    click(&mut cx, tree_row(&member_node(&clean, &lib_id)));
    click_with(
        &mut cx,
        tree_row(&member_node(&backend, &api_id)),
        secondary_modifiers(),
    );
    wait_for_merged(&mut cx, &window);
    wait_until(&mut cx, |cx| {
        history_labels(cx).first().map(String::as_str) == Some("Uncommitted changes (1 repo)")
    });

    // Once that repo is clean, the row goes.
    api.git(["add", "--all"]).unwrap();
    api.commit_all("Tidy").unwrap();
    fixture.refresh(&fixture.active());
    wait_until(&mut cx, |cx| {
        !present(cx, "history-uncommitted".into())
            && history_labels(cx).first().map(String::as_str) == Some("Tidy")
    });
}

#[gpui::test]
fn ui_12_merged_search_filters_in_the_core(cx: &mut TestAppContext) {
    let fixture = Fixture::start(EVENT_CAPACITY);
    let made = repos(&["api", "web"]);
    let (api, web) = (repo(&made, "api"), repo(&made, "web"));
    api.set_clock(100);
    api.commit_file("a.txt", "a\n", "Api change").unwrap();
    web.set_clock(200);
    web.commit_file("w.txt", "w\n", "Web change").unwrap();
    web.write("dirty.txt", "dirty\n").unwrap();
    let added = add(&fixture, &made);
    let backend = group(
        &fixture,
        "Backend",
        vec![id_of(&added, "api"), id_of(&added, "web")],
    );
    let (mut cx, window) = main_window(cx, &fixture);
    let node = group_node(&backend);
    wait_for_element(&mut cx, tree_row(&node));
    click(&mut cx, tree_row(&node));
    wait_for_element(&mut cx, "history-uncommitted".into());

    // The core answers each character; the pane shows its last answer as
    // it is, with the repo column and without the uncommitted row.
    search(&mut cx, "Api");
    wait_until(&mut cx, |cx| history_labels(cx) == ["Api change"]);
    assert_eq!(label_of(&mut cx, repo_cell(0)).as_deref(), Some("api"));
    assert!(!present(&mut cx, "history-uncommitted".into()));
    assert_eq!(history_rows(&mut cx, &window), 1);

    // Cleared, the whole merged history is back, led by the row.
    search(&mut cx, "");
    wait_until(&mut cx, |cx| {
        history_labels(cx).first().map(String::as_str) == Some("Uncommitted changes (1 repo)")
            && history_labels(cx).len() == 5
    });
}

#[gpui::test]
fn ui_29_each_tab_keeps_its_merged_history(cx: &mut TestAppContext) {
    let fixture = Fixture::start(EVENT_CAPACITY);
    let mine = fixture.active();
    let made = repos(&["api", "web"]);
    for name in ["api", "web"] {
        let messages: Vec<String> = (0..20).map(|n| format!("Change {name} {n}")).collect();
        repo(&made, name).empty_commits(&messages).unwrap();
    }
    let added = add(&fixture, &made);
    let backend = group(
        &fixture,
        "Backend",
        vec![id_of(&added, "api"), id_of(&added, "web")],
    );
    let app = fixture.app().clone();
    let java = fixture.runtime.block_on(async {
        let tabs = app.workspace_create("Java".to_owned()).await.unwrap();
        tabs.active.unwrap()
    });
    let other = repos(&["core"]);
    fixture.add_repos(&java, other.path());
    fixture
        .runtime
        .block_on(app.workspace_activate(mine.clone()))
        .unwrap();

    let (mut cx, window) = main_window(cx, &fixture);
    let node = group_node(&backend);
    wait_for_element(&mut cx, tree_row(&node));
    click(&mut cx, tree_row(&node));
    wait_for_source(&mut cx, &window, &node);

    // Search, select a row and scroll away from it.
    search(&mut cx, "Change");
    wait_until(&mut cx, |cx| history_rows(cx, &window) == 40);
    click(&mut cx, row(20));
    scroll(&mut cx, -5. * 24.);
    let state = |cx: &mut VisualTestContext| {
        with_history(cx, &window, |history| {
            (
                history.source().map(|source| source.key.clone()),
                history.text().to_owned(),
                history.selected().map(str::to_owned),
                history.rows().len(),
                history.scroll_top(),
            )
        })
    };
    let before = state(&mut cx);
    assert!(before.2.is_some());
    assert!(before.4 > 0., "the list did not scroll: {before:?}");

    // The other tab has its own history.
    click(&mut cx, tab(&java));
    wait_until(&mut cx, |cx| {
        with_history(cx, &window, |history| history.source().is_none())
    });

    // Back, the merged history is as it was left.
    click(&mut cx, tab(&mine));
    wait_until(&mut cx, |cx| {
        with_history(cx, &window, |history| {
            history.rows().len() == before.3 && !history.loading()
        })
    });
    let after = state(&mut cx);
    assert_eq!(after.0, before.0);
    assert_eq!(after.1, before.1);
    assert_eq!(after.2, before.2);
    assert_eq!(after.3, before.3);
    assert!(
        (after.4 - before.4).abs() < 1.,
        "the scroll position moved: {} to {}",
        before.4,
        after.4
    );
    assert_eq!(search_text(&mut cx), "Change");
    assert!(present(&mut cx, "history-strip".into()));
}

#[gpui::test]
fn sec_3_repo_names_in_the_strip_and_column_render_as_text(cx: &mut TestAppContext) {
    let fixture = Fixture::start(EVENT_CAPACITY);
    let odd = "a&amp;b {x}";
    let made = repos(&[odd, "web"]);
    let added = add(&fixture, &made);
    let both = group(
        &fixture,
        "Both",
        vec![id_of(&added, odd), id_of(&added, "web")],
    );
    let (mut cx, window) = main_window(cx, &fixture);

    let node = group_node(&both);
    wait_for_element(&mut cx, tree_row(&node));
    click(&mut cx, tree_row(&node));
    wait_for_source(&mut cx, &window, &node);
    wait_until(&mut cx, |cx| strip_names(cx).len() == 2);
    assert_eq!(strip_names(&mut cx)[0], odd);
    let cells: Vec<Option<String>> = (0..2)
        .map(|index| label_of(&mut cx, repo_cell(index)))
        .collect();
    assert!(cells.contains(&Some(odd.to_owned())), "{cells:?}");
}
