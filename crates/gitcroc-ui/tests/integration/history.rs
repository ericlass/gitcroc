//! The history pane (UI-9, UI-29): rows from the core's pages, the graph,
//! ref labels, selection, paging and each tab's own position. Repos are real
//! ones made by `repo-gen` and added through the app (QA-2).

use std::time::Duration;

use gitcroc_app::app::EVENT_CAPACITY;
use gitcroc_core::{
    ColumnLayout, ColumnOrder, HistoryColumn, PAGE_SIZE, PaneLayout, RepoId, WorkspaceId,
};
use gitcroc_ui::history::History;
use gitcroc_ui::history_columns::{Columns, MIN_SUBJECT_WIDTH, SizedColumn};
use gitcroc_ui::window::GitcrocWindow;
use gpui_kit::test::TestWindowExt;
use gpui_kit::{
    ElementId, Entity, Role, ScrollDelta, TestAppContext, VisualTestContext, point, px,
};

use crate::support::{Fixture, main_window, settle};

/// Repos `names` in a new folder, each with one commit on `main`.
pub(crate) fn repos(names: &[&str]) -> repo_gen::Fixture {
    let repos = repo_gen::Fixture::new().unwrap();
    for name in names {
        repos
            .repo(name)
            .unwrap()
            .commit_file("README.md", "hello\n", "Initial")
            .unwrap();
    }
    repos
}

pub(crate) fn repo(repos: &repo_gen::Fixture, name: &str) -> repo_gen::Repo {
    repo_gen::Repo::open(repos.path().join(name))
}

pub(crate) fn id_of(added: &[(String, RepoId)], name: &str) -> RepoId {
    added
        .iter()
        .find(|(added, _)| added == name)
        .map(|(_, id)| id.clone())
        .unwrap_or_else(|| panic!("{name} was not added"))
}

pub(crate) fn add(fixture: &Fixture, made: &repo_gen::Fixture) -> Vec<(String, RepoId)> {
    fixture.add_repos(&fixture.active(), made.path())
}

pub(crate) fn tree_row(node: &str) -> ElementId {
    ElementId::Name(format!("tree-row-{node}").into())
}

pub(crate) fn tree_part(node: &str, part: &str) -> ElementId {
    ElementId::Name(format!("tree-row-{node}-{part}").into())
}

pub(crate) fn repo_node(added: &[(String, RepoId)], name: &str) -> String {
    format!("repo:{}", id_of(added, name).0)
}

pub(crate) fn tab(id: &WorkspaceId) -> ElementId {
    ElementId::Name(format!("tab-{}", id.0).into())
}

/// Clicks `id` where the latest state draws it.
pub(crate) fn click(cx: &mut VisualTestContext, id: ElementId) {
    cx.update(|window, cx| {
        window.render_frame(cx);
        window.click(id, cx);
    });
    cx.run_until_parked();
}

/// Runs the window and the app until `done` holds.
#[track_caller]
pub(crate) fn wait_until(
    cx: &mut VisualTestContext,
    mut done: impl FnMut(&mut VisualTestContext) -> bool,
) {
    for _ in 0..1000 {
        settle(cx, |_, _| true);
        if done(cx) {
            return;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    panic!("the history never got there");
}

pub(crate) fn wait_for_element(cx: &mut VisualTestContext, id: ElementId) {
    wait_until(cx, |cx| {
        cx.update(|window, cx| {
            window.render_frame(cx);
            window.try_find(id.clone()).is_some()
        })
    });
}

/// Runs `f` on the active tab's history.
pub(crate) fn with_history<R>(
    cx: &mut VisualTestContext,
    window: &Entity<GitcrocWindow>,
    f: impl FnOnce(&History) -> R,
) -> R {
    cx.update(|_, cx| {
        let pane = window.read(cx).history(cx).expect("no history pane");
        let pane = pane.read(cx);
        f(pane.history())
    })
}

pub(crate) fn history_rows(cx: &mut VisualTestContext, window: &Entity<GitcrocWindow>) -> usize {
    with_history(cx, window, |history| history.rows().len())
}

#[track_caller]
pub(crate) fn wait_for_rows(
    cx: &mut VisualTestContext,
    window: &Entity<GitcrocWindow>,
    rows: usize,
) {
    wait_until(cx, |cx| history_rows(cx, window) == rows);
}

/// The label of element `id`, if it is on screen.
pub(crate) fn label_of(cx: &mut VisualTestContext, id: ElementId) -> Option<String> {
    cx.update(|window, cx| {
        window.render_frame(cx);
        window
            .try_find(id)
            .and_then(|found| found.label().map(str::to_owned))
    })
}

/// The labels of the elements of role `role` below `id`.
pub(crate) fn labels_in(cx: &mut VisualTestContext, id: ElementId, role: Role) -> Vec<String> {
    cx.update(|window, cx| {
        window.render_frame(cx);
        if window.try_find(id.clone()).is_none() {
            return Vec::new();
        }
        window
            .within(id)
            .find_all_by_role(role)
            .iter()
            .filter_map(|found| found.label().map(str::to_owned))
            .collect()
    })
}

/// The history rows on screen, top to bottom.
pub(crate) fn history_labels(cx: &mut VisualTestContext) -> Vec<String> {
    labels_in(cx, "history-list".into(), Role::ListBoxOption)
}

pub(crate) fn scroll(cx: &mut VisualTestContext, pixels: f32) {
    cx.update(|window, cx| {
        window.scroll(
            "history-list",
            ScrollDelta::Pixels(point(px(0.), px(pixels))),
            cx,
        );
        window.render_frame(cx);
    });
    cx.run_until_parked();
}

#[gpui::test]
fn ui_9_history_shows_graph_columns_and_refs(cx: &mut TestAppContext) {
    let fixture = Fixture::start(EVENT_CAPACITY);
    let made = repos(&["api"]);
    let api = repo(&made, "api");
    api.commit_file("README.md", "two\n", "Second").unwrap();
    api.tag("v1").unwrap();
    let head = api.head().unwrap();
    api.git(["update-ref", "refs/remotes/origin/main", &head])
        .unwrap();
    let added = add(&fixture, &made);
    let (mut cx, window) = main_window(cx, &fixture);

    // A repo row selects the whole repo.
    let node = repo_node(&added, "api");
    wait_for_element(&mut cx, tree_row(&node));
    click(&mut cx, tree_row(&node));
    wait_for_rows(&mut cx, &window, 2);

    // The first row is the newest commit: graph, subject with ref labels,
    // author, date and short hash.
    let (short_hash, date, has_graph) = with_history(&mut cx, &window, |history| {
        let row = &history.rows()[0];
        (
            row.short_hash.to_string(),
            row.date.to_string(),
            row.graph.is_some(),
        )
    });
    assert!(has_graph, "the first row has no graph");
    assert!(
        cx.update(|window, _| window.try_find("history-row-0-graph").is_some()),
        "the graph cell is not drawn"
    );
    let labels = labels_in(&mut cx, "history-row-0".into(), Role::Label);
    assert!(labels.contains(&"Second".to_owned()), "{labels:?}");
    assert!(
        labels.contains(&"Current branch main".to_owned()),
        "{labels:?}"
    );
    assert!(labels.contains(&"Tag v1".to_owned()), "{labels:?}");
    assert!(
        labels.contains(&"Remote branch origin/main".to_owned()),
        "{labels:?}"
    );
    assert!(labels.contains(&"Fixture Author".to_owned()), "{labels:?}");
    assert!(labels.contains(&short_hash), "{labels:?}");
    assert!(labels.contains(&date), "{labels:?}");

    // The columns have their default order.
    let header = labels_in(&mut cx, "history-header".into(), Role::ColumnHeader);
    assert_eq!(header, ["Graph", "Subject", "Author", "Date", "Hash"]);
}

#[gpui::test]
fn ui_9_branch_tag_and_stash_show_their_history(cx: &mut TestAppContext) {
    let fixture = Fixture::start(EVENT_CAPACITY);
    let made = repos(&["api"]);
    let api = repo(&made, "api");
    api.branch("topic").unwrap();
    api.switch("topic").unwrap();
    api.commit_file("topic.txt", "topic\n", "Topic only")
        .unwrap();
    api.switch("main").unwrap();
    api.commit_file("main.txt", "main\n", "Main only").unwrap();
    api.tag("v1").unwrap();
    api.write("stash.txt", "stash\n").unwrap();
    api.stash("Stashed work").unwrap();
    let added = add(&fixture, &made);
    let (mut cx, window) = main_window(cx, &fixture);
    let node = repo_node(&added, "api");
    wait_for_element(&mut cx, tree_row(&node));

    // The branch shows only what is reachable from it.
    click(&mut cx, tree_part(&node, "chevron"));
    let local = format!("{node}:local");
    wait_for_element(&mut cx, tree_part(&local, "chevron"));
    click(&mut cx, tree_part(&local, "chevron"));
    let branch = format!("{local}:ref:topic");
    wait_for_element(&mut cx, tree_row(&branch));
    click(&mut cx, tree_row(&branch));
    wait_for_rows(&mut cx, &window, 2);
    let subjects = with_history(&mut cx, &window, |history| {
        history
            .rows()
            .iter()
            .map(|row| row.subject.to_string())
            .collect::<Vec<_>>()
    });
    assert_eq!(subjects, ["Topic only", "Initial"]);

    // A tag shows the commits reachable from it.
    let tags = format!("{node}:tags");
    click(&mut cx, tree_part(&tags, "chevron"));
    let tag = format!("{tags}:ref:v1");
    wait_for_element(&mut cx, tree_row(&tag));
    click(&mut cx, tree_row(&tag));
    wait_for_rows(&mut cx, &window, 2);
    assert_eq!(
        with_history(&mut cx, &window, |history| history
            .rows()
            .iter()
            .map(|row| row.subject.to_string())
            .collect::<Vec<_>>()),
        ["Main only", "Initial"]
    );

    // A stash is one row.
    let stashes = format!("{node}:stashes");
    click(&mut cx, tree_part(&stashes, "chevron"));
    let stash = format!("{stashes}:ref:0");
    wait_for_element(&mut cx, tree_row(&stash));
    click(&mut cx, tree_row(&stash));
    wait_until(&mut cx, |cx| {
        with_history(cx, &window, |history| {
            history.rows().len() == 1 && history.rows()[0].subject.contains("Stashed work")
        })
    });

    // Ungrouped shows no source.
    click(&mut cx, tree_row("ungrouped"));
    wait_until(&mut cx, |cx| {
        with_history(cx, &window, |history| history.source().is_none())
    });
    assert_eq!(
        label_of(&mut cx, "history-empty".into()).as_deref(),
        Some("Select a repo, branch, tag or stash to see its history.")
    );
}

#[gpui::test]
fn perf_1_history_lays_out_only_the_rows_on_screen(cx: &mut TestAppContext) {
    let fixture = Fixture::start(EVENT_CAPACITY);
    let made = repos(&["api"]);
    let api = repo(&made, "api");
    let messages: Vec<String> = (0..59).map(|n| format!("Change {n}")).collect();
    api.empty_commits(&messages).unwrap();
    let added = add(&fixture, &made);
    let (mut cx, window) = main_window(cx, &fixture);
    let node = repo_node(&added, "api");
    wait_for_element(&mut cx, tree_row(&node));
    click(&mut cx, tree_row(&node));
    wait_for_rows(&mut cx, &window, 60);

    // The list has room for far fewer than 60 rows.
    let (rendered, height) = cx.update(|window, cx| {
        window.render_frame(cx);
        let height = window.find("history-list").bounds().size.height;
        (
            window
                .within("history-list")
                .find_all_by_role(Role::ListBoxOption)
                .len(),
            height,
        )
    });
    let fit = (f32::from(height) / 24.).ceil() as usize;
    assert!(fit < 60, "the window has room for every row: {fit}");
    assert!(
        rendered <= fit + 2,
        "{rendered} rows laid out for {fit} on screen"
    );
    assert!(
        rendered >= fit.saturating_sub(1),
        "the history is filled: {rendered} of {fit}"
    );

    // At the end, the last rows are laid out and the first is not.
    let first = with_history(&mut cx, &window, |history| {
        history.rows()[0].subject.to_string()
    });
    let last = with_history(&mut cx, &window, |history| {
        history.rows()[59].subject.to_string()
    });
    scroll(&mut cx, -2_000.);
    let shown = history_labels(&mut cx);
    assert!(shown.contains(&last), "{shown:?}");
    assert!(!shown.contains(&first), "{shown:?}");
}

#[gpui::test]
fn arch_8_history_loads_pages_as_the_list_scrolls(cx: &mut TestAppContext) {
    let fixture = Fixture::start(EVENT_CAPACITY);
    let made = repos(&["api"]);
    let api = repo(&made, "api");
    let messages: Vec<String> = (0..600).map(|n| format!("Change {n}")).collect();
    api.empty_commits(&messages).unwrap();
    let added = add(&fixture, &made);
    let (mut cx, window) = main_window(cx, &fixture);
    let node = repo_node(&added, "api");
    wait_for_element(&mut cx, tree_row(&node));
    click(&mut cx, tree_row(&node));

    // The first page alone is loaded.
    let page = PAGE_SIZE;
    wait_for_rows(&mut cx, &window, page);
    assert!(
        with_history(&mut cx, &window, |history| !history.done()),
        "the first page says the history ended"
    );

    // Scrolling to its end loads the next page, and only then the end shows.
    scroll(&mut cx, -((page * 24) as f32 + 100.));
    wait_until(&mut cx, |cx| {
        with_history(cx, &window, |history| history.rows().len() > page)
    });
    wait_until(&mut cx, |cx| {
        with_history(cx, &window, |history| {
            history.done() && history.rows().len() == 601
        })
    });
}

#[gpui::test]
fn ui_29_each_tab_keeps_its_history(cx: &mut TestAppContext) {
    let fixture = Fixture::start(EVENT_CAPACITY);
    let mine = fixture.active();
    let made = repos(&["api"]);
    let api = repo(&made, "api");
    let messages: Vec<String> = (0..39).map(|n| format!("Change {n}")).collect();
    api.empty_commits(&messages).unwrap();
    let added = add(&fixture, &made);
    let app = fixture.app().clone();
    let java = fixture.runtime.block_on(async {
        let tabs = app.workspace_create("Java".to_owned()).await.unwrap();
        tabs.active.unwrap()
    });
    let other = repos(&["core"]);
    let java_added = fixture.add_repos(&java, other.path());
    fixture
        .runtime
        .block_on(app.workspace_activate(mine.clone()))
        .unwrap();

    let (mut cx, window) = main_window(cx, &fixture);
    let node = repo_node(&added, "api");
    wait_for_element(&mut cx, tree_row(&node));
    click(&mut cx, tree_row(&node));
    wait_for_rows(&mut cx, &window, 40);

    // Select a row and scroll away from it.
    click(&mut cx, ElementId::Name("history-row-20".into()));
    scroll(&mut cx, -5. * 24.);
    let before = with_history(&mut cx, &window, |history| {
        (
            history.source().map(|source| source.key.clone()),
            history.selected().map(str::to_owned),
            history.rows().len(),
            history.scroll_top(),
        )
    });
    assert!(before.1.is_some());
    assert!(before.3 > 0., "the list did not scroll: {:?}", before);

    // The other tab has its own history.
    click(&mut cx, tab(&java));
    wait_until(&mut cx, |cx| {
        with_history(cx, &window, |history| history.source().is_none())
    });
    let core = repo_node(&java_added, "core");
    wait_for_element(&mut cx, tree_row(&core));
    click(&mut cx, tree_row(&core));
    wait_for_rows(&mut cx, &window, 1);

    // Back, everything is as it was left.
    click(&mut cx, tab(&mine));
    wait_until(&mut cx, |cx| history_rows(cx, &window) == before.2);
    let after = with_history(&mut cx, &window, |history| {
        (
            history.source().map(|source| source.key.clone()),
            history.selected().map(str::to_owned),
            history.rows().len(),
            history.scroll_top(),
        )
    });
    assert_eq!(after.0, before.0);
    assert_eq!(after.1, before.1);
    assert_eq!(after.2, before.2);
    assert!(
        (after.3 - before.3).abs() < 1.,
        "the scroll position moved: {} to {}",
        before.3,
        after.3
    );
}

#[gpui::test]
fn sec_3_history_subjects_and_ref_names_render_as_text(cx: &mut TestAppContext) {
    let fixture = Fixture::start(EVENT_CAPACITY);
    let made = repos(&["api"]);
    let api = repo(&made, "api");
    let subject = "<img src=x> & subject";
    let hash = api.commit_file("README.md", "text\n", subject).unwrap();
    // Windows forbids `<` and `>` in file names, so the tag goes in
    // `packed-refs`, where git keeps a fetched one.
    std::fs::write(
        made.path().join("api").join(".git").join("packed-refs"),
        format!("{hash} refs/tags/<i>tag\n"),
    )
    .unwrap();
    let added = add(&fixture, &made);
    let (mut cx, window) = main_window(cx, &fixture);
    let node = repo_node(&added, "api");
    wait_for_element(&mut cx, tree_row(&node));
    click(&mut cx, tree_row(&node));
    wait_for_rows(&mut cx, &window, 2);

    let labels = labels_in(&mut cx, "history-row-0".into(), Role::Label);
    assert!(labels.contains(&subject.to_owned()), "{labels:?}");
    assert!(labels.contains(&"Tag <i>tag".to_owned()), "{labels:?}");
    assert_eq!(
        label_of(&mut cx, "history-row-0".into()).as_deref(),
        Some(subject)
    );
}

pub(crate) fn press(cx: &mut VisualTestContext, keys: &str) {
    cx.update(|window, cx| window.press(keys, cx));
    cx.run_until_parked();
}

fn right_click(cx: &mut VisualTestContext, id: ElementId) {
    cx.update(|window, cx| window.right_click(id, cx));
    cx.run_until_parked();
}

fn double_click(cx: &mut VisualTestContext, id: ElementId) {
    cx.update(|window, cx| window.double_click(id, cx));
    cx.run_until_parked();
}

fn drag(cx: &mut VisualTestContext, id: ElementId, by: f32) {
    let from = element_center(cx, id);
    let to = from + point(px(by), px(0.));
    cx.update(|window, cx| window.drag(from, to, cx));
    cx.run_until_parked();
}

fn element_center(cx: &mut VisualTestContext, id: ElementId) -> gpui_kit::Point<gpui_kit::Pixels> {
    cx.update(|window, _| window.find(id).bounds().center())
}

fn element_width(cx: &mut VisualTestContext, id: ElementId) -> f32 {
    cx.update(|window, _| f32::from(window.find(id).bounds().size.width))
}

fn element_x(cx: &mut VisualTestContext, id: ElementId) -> f32 {
    cx.update(|window, _| f32::from(window.find(id).bounds().origin.x))
}

fn with_columns<R>(
    cx: &mut VisualTestContext,
    window: &Entity<GitcrocWindow>,
    f: impl FnOnce(&Columns) -> R,
) -> R {
    cx.update(|_, cx| {
        let pane = window.read(cx).history(cx).expect("no history pane");
        f(pane.read(cx).columns())
    })
}

fn header_labels(cx: &mut VisualTestContext) -> Vec<String> {
    labels_in(cx, "history-header".into(), Role::ColumnHeader)
}

fn click_menu(cx: &mut VisualTestContext, name: &str) {
    let id = cx.update(|window, _| {
        window
            .within("menu")
            .find_by_label(name)
            .path()
            .last()
            .cloned()
            .expect("no menu item")
    });
    click(cx, id);
}

fn menu_labels(cx: &mut VisualTestContext) -> Vec<String> {
    labels_in(cx, "menu".into(), Role::MenuItem)
}

fn workspace_id(fixture: &Fixture, name: &str) -> WorkspaceId {
    fixture
        .app()
        .workspace_get()
        .open
        .into_iter()
        .find(|view| view.name == name)
        .map(|view| view.id)
        .unwrap_or_else(|| panic!("no workspace {name}"))
}

/// Creates workspaces `names`, the last one active.
fn create_workspaces(fixture: &Fixture, names: &[&str]) {
    let app = fixture.app().clone();
    fixture.runtime.block_on(async {
        for name in names {
            app.workspace_create((*name).to_owned()).await.unwrap();
        }
    });
}

#[gpui::test]
fn ui_36_columns_resize_move_and_reset(cx: &mut TestAppContext) {
    let fixture = Fixture::start(EVENT_CAPACITY);
    let made = repos(&["api"]);
    let added = add(&fixture, &made);
    let (mut cx, window) = main_window(cx, &fixture);
    let node = repo_node(&added, "api");
    wait_for_element(&mut cx, tree_row(&node));
    click(&mut cx, tree_row(&node));
    wait_for_rows(&mut cx, &window, 1);

    assert_eq!(
        header_labels(&mut cx),
        ["Graph", "Subject", "Author", "Date", "Hash"]
    );

    // The author's edge is left of the subject: dragging it left widens
    // the author, and the subject gives up the width.
    let subject = element_width(&mut cx, "history-row-0-subject".into());
    let author = with_columns(&mut cx, &window, |columns| {
        columns.width_of(SizedColumn::Author)
    });
    let edge = ElementId::from("history-edge-author");
    drag(&mut cx, edge.clone(), -24.);
    wait_until(&mut cx, |cx| {
        with_columns(cx, &window, |columns| columns.width_of(SizedColumn::Author)) == author + 24
    });
    assert_eq!(
        element_width(&mut cx, "history-row-0-author".into()),
        (author + 24) as f32
    );

    // Further left than the subject can give, it stops at its minimum.
    let expected = author + (subject - MIN_SUBJECT_WIDTH).floor() as u32;
    drag(&mut cx, edge.clone(), -400.);
    wait_until(&mut cx, |cx| {
        with_columns(cx, &window, |columns| columns.width_of(SizedColumn::Author)) == expected
    });
    let subject_now = element_width(&mut cx, "history-row-0-subject".into());
    assert!(
        (subject_now - MIN_SUBJECT_WIDTH).abs() <= 1.,
        "{subject_now}"
    );

    // A double-click on the edge gives the author its default width.
    double_click(&mut cx, edge);
    wait_until(&mut cx, |cx| {
        with_columns(cx, &window, |columns| columns.width_of(SizedColumn::Author)) == 140
    });

    // Dragging Hash's header before Graph moves the column in the header
    // and in the rows.
    let hash = ElementId::from("history-column-hash");
    let graph = ElementId::from("history-column-graph");
    cx.update(|window, cx| window.drag_to(hash.clone(), graph.clone(), cx));
    cx.run_until_parked();
    wait_until(&mut cx, |cx| {
        header_labels(cx) == ["Hash", "Graph", "Subject", "Author", "Date"]
    });
    assert!(element_x(&mut cx, hash.clone()) < element_x(&mut cx, graph.clone()));

    // The header menu moves a column by mouse. Move right is enabled on
    // Graph, which is second.
    right_click(&mut cx, graph.clone());
    wait_for_element(&mut cx, "menu".into());
    assert_eq!(
        menu_labels(&mut cx),
        ["Move left", "Move right", "Reset columns"]
    );
    click_menu(&mut cx, "Move right");
    wait_until(&mut cx, |cx| {
        header_labels(cx) == ["Hash", "Subject", "Graph", "Author", "Date"]
    });

    // The first column's Move left is disabled, not hidden: choosing it
    // does nothing.
    right_click(&mut cx, hash.clone());
    wait_for_element(&mut cx, "menu".into());
    click_menu(&mut cx, "Move left");
    assert!(
        cx.update(|window, _| window.try_find("menu").is_some()),
        "a disabled item closed the menu"
    );
    assert_eq!(
        header_labels(&mut cx),
        ["Hash", "Subject", "Graph", "Author", "Date"]
    );
    press(&mut cx, "escape");

    // Reset columns restores the default order and widths.
    right_click(&mut cx, hash);
    wait_for_element(&mut cx, "menu".into());
    click_menu(&mut cx, "Reset columns");
    wait_until(&mut cx, |cx| {
        header_labels(cx) == ["Graph", "Subject", "Author", "Date", "Hash"]
            && with_columns(cx, &window, |columns| columns.width_of(SizedColumn::Author)) == 140
    });

    // The subject moves too; dragging it onto Date puts it after Author.
    let subject_header = ElementId::from("history-column-subject");
    let author_header = ElementId::from("history-column-author");
    let date = ElementId::from("history-column-date");
    cx.update(|window, cx| window.drag_to(subject_header.clone(), date, cx));
    cx.run_until_parked();
    wait_until(&mut cx, |cx| {
        header_labels(cx) == ["Graph", "Author", "Subject", "Date", "Hash"]
    });
    assert!(element_x(&mut cx, subject_header) > element_x(&mut cx, author_header));
}

#[gpui::test]
fn ui_36_columns_are_kept_per_workspace(cx: &mut TestAppContext) {
    let fixture = Fixture::start(EVENT_CAPACITY);
    let made = repos(&["api"]);
    create_workspaces(&fixture, &["Java"]);
    let java = workspace_id(&fixture, "Java");
    let mine = workspace_id(&fixture, "My workspace");
    fixture.add_repos(&java, made.path());
    fixture.add_repos(&mine, made.path());
    fixture.set_layout(
        &java,
        PaneLayout {
            columns: Some(ColumnLayout {
                order: Some(ColumnOrder(vec![
                    HistoryColumn::Subject,
                    HistoryColumn::Hash,
                    HistoryColumn::Graph,
                    HistoryColumn::Date,
                    HistoryColumn::Author,
                ])),
                graph: Some(90),
                author: Some(170),
                date: Some(100),
                hash: Some(60),
            }),
            ..PaneLayout::default()
        },
    );
    let (mut cx, window) = main_window(cx, &fixture);
    let node = "repo:r1".to_owned();
    wait_for_element(&mut cx, tree_row(&node));
    click(&mut cx, tree_row(&node));
    wait_for_rows(&mut cx, &window, 1);

    // The window starts from the workspace's saved columns.
    assert_eq!(
        header_labels(&mut cx),
        ["Subject", "Hash", "Graph", "Date", "Author"]
    );
    assert_eq!(element_width(&mut cx, "history-row-0-author".into()), 170.);
    assert_eq!(element_width(&mut cx, "history-row-0-hash".into()), 60.);

    // Each workspace has its own.
    click(&mut cx, tab(&mine));
    wait_until(&mut cx, |cx| {
        with_columns(cx, &window, |columns| {
            columns.order() == HistoryColumn::DEFAULT_ORDER
        })
    });
    wait_for_element(&mut cx, tree_row(&node));
    click(&mut cx, tree_row(&node));
    wait_for_rows(&mut cx, &window, 1);
    assert_eq!(
        header_labels(&mut cx),
        ["Graph", "Subject", "Author", "Date", "Hash"]
    );
    assert_eq!(element_width(&mut cx, "history-row-0-author".into()), 140.);
    assert!(!fixture.workspace_file(&mine).contains("columns"));

    // Back in Java, the columns are as they were left.
    click(&mut cx, tab(&java));
    wait_until(&mut cx, |cx| {
        header_labels(cx) == ["Subject", "Hash", "Graph", "Date", "Author"]
    });

    // A change is saved to Java's workspace file.
    let edge = ElementId::from("history-edge-author");
    drag(&mut cx, edge, -16.);
    wait_until(&mut cx, |cx| {
        with_columns(cx, &window, |columns| columns.width_of(SizedColumn::Author)) == 186
    });
    wait_until(&mut cx, |_| {
        fixture.workspace_file(&java).contains("author = 186")
    });
    assert!(fixture.workspace_file(&java).contains("columns"));
    let saved = fixture.layout(&java).columns.expect("no columns saved");
    assert_eq!(saved.author, Some(186));

    // Reset columns removes them from the workspace file.
    right_click(&mut cx, "history-column-subject".into());
    wait_for_element(&mut cx, "menu".into());
    click_menu(&mut cx, "Reset columns");
    wait_until(&mut cx, |cx| {
        header_labels(cx) == ["Graph", "Subject", "Author", "Date", "Hash"]
    });
    wait_until(&mut cx, |_| {
        !fixture.workspace_file(&java).contains("columns")
    });
}

#[gpui::test]
fn ui_22_history_headers_are_one_tab_stop(cx: &mut TestAppContext) {
    let fixture = Fixture::start(EVENT_CAPACITY);
    let made = repos(&["api"]);
    let added = add(&fixture, &made);
    let (mut cx, window) = main_window(cx, &fixture);
    let node = repo_node(&added, "api");
    wait_for_element(&mut cx, tree_row(&node));
    click(&mut cx, tree_row(&node));
    wait_for_rows(&mut cx, &window, 1);

    // A click opens the menu; Escape closes it and gives the header the
    // focus.
    click(&mut cx, "history-column-graph".into());
    wait_for_element(&mut cx, "menu".into());
    press(&mut cx, "escape");
    assert!(cx.update(|window, _| window.find("history-header").focused()) == Some(true));
    assert_eq!(
        with_columns(&mut cx, &window, |columns| columns.active()),
        HistoryColumn::Graph
    );

    // The arrows, Home and End move between the headers.
    press(&mut cx, "right");
    assert_eq!(
        with_columns(&mut cx, &window, |columns| columns.active()),
        HistoryColumn::Subject
    );
    press(&mut cx, "end");
    assert_eq!(
        with_columns(&mut cx, &window, |columns| columns.active()),
        HistoryColumn::Hash
    );
    press(&mut cx, "home");
    assert_eq!(
        with_columns(&mut cx, &window, |columns| columns.active()),
        HistoryColumn::Graph
    );

    // Shift+F10 opens the focused header's menu; the keys move in it and
    // choose Move right, and the focus returns to the header.
    press(&mut cx, "right");
    press(&mut cx, "shift-f10");
    wait_for_element(&mut cx, "menu".into());
    assert_eq!(
        cx.update(|window, _| window.find("menu").label().map(str::to_owned)),
        Some("Subject column".to_owned())
    );
    press(&mut cx, "down");
    press(&mut cx, "enter");
    wait_until(&mut cx, |cx| {
        header_labels(cx) == ["Graph", "Author", "Subject", "Date", "Hash"]
    });
    assert!(cx.update(|window, _| window.find("history-header").focused()) == Some(true));

    // The context-menu key opens the same menu; Escape returns the focus.
    press(&mut cx, "menu");
    wait_for_element(&mut cx, "menu".into());
    assert_eq!(
        cx.update(|window, _| window.find("menu").label().map(str::to_owned)),
        Some("Subject column".to_owned())
    );
    press(&mut cx, "escape");
    assert!(cx.update(|window, _| window.try_find("menu").is_none()));
    assert!(cx.update(|window, _| window.find("history-header").focused()) == Some(true));
}
