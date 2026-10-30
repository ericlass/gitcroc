//! Background tabs (UI-31): a tab in the background holds no history rows,
//! file lists or diffs, and on return reads them again and shows them where
//! they were left (UI-29). Repos are real ones made by `repo-gen` and added
//! through the app (QA-2).

use gitcroc_app::app::EVENT_CAPACITY;
use gitcroc_core::{PAGE_SIZE, WorkspaceId};
use gitcroc_ui::diff_view::DiffView;
use gitcroc_ui::window::GitcrocWindow;
use gpui_kit::test::TestWindowExt;
use gpui_kit::{Entity, ScrollDelta, TestAppContext, VisualTestContext, point, px};

use crate::changes::{
    history_row, index_of, row, show_repo, shown_file, switch_to, wait_for_view, with_changes,
};
use crate::history::{add, click, repo, repos, scroll, wait_for_rows, wait_until, with_history};
use crate::history_search::{search, search_text};
use crate::support::{Fixture, main_window};

/// A second workspace, "Java", with no repos; `mine` stays active.
fn second_tab(fixture: &Fixture, mine: &WorkspaceId) -> WorkspaceId {
    let app = fixture.app().clone();
    let java = fixture.runtime.block_on(async {
        let tabs = app.workspace_create("Java".to_owned()).await.unwrap();
        tabs.active.unwrap()
    });
    fixture
        .runtime
        .block_on(app.workspace_activate(mine.clone()))
        .unwrap();
    java
}

/// What tab `id`'s panes hold, whether it is active or not: its history's
/// rows and walk handle, whether its changes pane has a view, and whether
/// its diff has content.
fn held(
    cx: &mut VisualTestContext,
    window: &Entity<GitcrocWindow>,
    id: &WorkspaceId,
) -> (usize, Option<u32>, bool, bool) {
    cx.update(|_, cx| {
        let window = window.read(cx);
        let history = window.history_of(id).expect("no history pane");
        let history = history.read(cx).history();
        let changes = window.changes_of(id).expect("no changes pane");
        let changes = changes.read(cx);
        (
            history.rows().len(),
            history.handle(),
            changes.changes().view().is_some(),
            changes.diff().read(cx).content().is_some(),
        )
    })
}

fn with_diff<R>(
    cx: &mut VisualTestContext,
    window: &Entity<GitcrocWindow>,
    f: impl FnOnce(&DiffView) -> R,
) -> R {
    cx.update(|_, cx| {
        let pane = window.read(cx).changes(cx).expect("no changes pane");
        let diff = pane.read(cx).diff().read(cx);
        f(diff)
    })
}

#[track_caller]
fn wait_for_diff(cx: &mut VisualTestContext, window: &Entity<GitcrocWindow>, path: &str) {
    wait_until(cx, |cx| {
        let target = with_changes(cx, window, |changes| changes.diff_target());
        with_diff(cx, window, |diff| {
            target.as_ref().is_some_and(|target| target.path == path)
                && diff.target() == target.as_ref()
                && !diff.loading()
                && diff.content().is_some()
        })
    });
}

fn scroll_diff(cx: &mut VisualTestContext, y: f32) {
    cx.update(|window, cx| {
        window.scroll("diff-lines", ScrollDelta::Pixels(point(px(0.), px(y))), cx);
        window.render_frame(cx);
    });
    cx.run_until_parked();
}

fn numbered(count: usize) -> String {
    (1..=count).map(|n| format!("l{n}\n")).collect()
}

/// A repo "api" whose commit "Files" adds `a.txt` and `b.txt`, with
/// `after` empty commits on top of it.
fn api_with_files(after: usize) -> repo_gen::Fixture {
    let made = repos(&["api"]);
    let api = repo(&made, "api");
    api.write("a.txt", numbered(300)).unwrap();
    api.write("b.txt", numbered(300)).unwrap();
    api.commit_all("Files").unwrap();
    let messages: Vec<String> = (0..after).map(|n| format!("Change {n}")).collect();
    api.empty_commits(&messages).unwrap();
    made
}

#[gpui::test]
fn ui_31_a_background_tab_releases_history_and_diffs(cx: &mut TestAppContext) {
    let fixture = Fixture::start(EVENT_CAPACITY);
    let mine = fixture.active();
    let made = api_with_files(3);
    let added = add(&fixture, &made);
    let java = second_tab(&fixture, &mine);
    let (mut cx, window) = main_window(cx, &fixture);

    // The commit "Files" and its first file's diff.
    show_repo(&mut cx, &window, &added, "api");
    click(&mut cx, history_row(3));
    wait_for_view(&mut cx, &window);
    wait_for_diff(&mut cx, &window, "a.txt");
    let (rows, handle, ..) = held(&mut cx, &window, &mine);
    assert_eq!(rows, 5);
    let handle = handle.expect("no walk");

    // In the background, nothing of it is held, in the UI or in the core.
    switch_to(&mut cx, &fixture, &window, &java);
    wait_until(&mut cx, |cx| {
        held(cx, &window, &mine) == (0, None, false, false)
    });
    let app = fixture.app().clone();
    let more = fixture
        .runtime
        .block_on(app.history_more(mine.clone(), handle))
        .unwrap();
    assert!(more.is_none(), "the core still walks the background tab");
}

#[gpui::test]
fn ui_31_returning_reads_the_same_rows_and_scrolls_back(cx: &mut TestAppContext) {
    let fixture = Fixture::start(EVENT_CAPACITY);
    let mine = fixture.active();
    let made = api_with_files(PAGE_SIZE + 100);
    let added = add(&fixture, &made);
    let java = second_tab(&fixture, &mine);
    let (mut cx, window) = main_window(cx, &fixture);

    // Into the second page, to the commit "Files", the second row from the
    // end, and its second file.
    show_repo(&mut cx, &window, &added, "api");
    wait_for_rows(&mut cx, &window, PAGE_SIZE);
    scroll(&mut cx, -((PAGE_SIZE * 24) as f32 + 100.));
    let all = PAGE_SIZE + 102;
    wait_for_rows(&mut cx, &window, all);
    scroll(&mut cx, -20_000.);
    click(&mut cx, history_row(all - 2));
    wait_for_view(&mut cx, &window);
    let b = index_of(&mut cx, &window, "b.txt");
    click(&mut cx, row(b));
    wait_for_diff(&mut cx, &window, "b.txt");
    scroll_diff(&mut cx, -100. * 20.);

    let state = |cx: &mut VisualTestContext| {
        let history = with_history(cx, &window, |history| {
            (
                history.source().map(|source| source.key.clone()),
                history.selected().map(str::to_owned),
                history.rows().len(),
                history.scroll_top(),
            )
        });
        let diff = with_diff(cx, &window, |diff| {
            (
                diff.target().map(|target| target.path.clone()),
                diff.top_row(),
            )
        });
        (history, shown_file(cx), diff)
    };
    let before = state(&mut cx);
    assert!(before.0.3 > 0., "the history did not scroll");
    assert!(before.2.1 > 0, "the diff did not scroll");

    // Away, and the tab lets go of it all; back, it reads it all again.
    switch_to(&mut cx, &fixture, &window, &java);
    wait_until(&mut cx, |cx| {
        held(cx, &window, &mine) == (0, None, false, false)
    });
    switch_to(&mut cx, &fixture, &window, &mine);
    wait_for_rows(&mut cx, &window, all);
    wait_for_view(&mut cx, &window);
    wait_for_diff(&mut cx, &window, "b.txt");
    let after = state(&mut cx);
    assert_eq!(after.0.0, before.0.0);
    assert_eq!(after.0.1, before.0.1);
    assert!(
        (after.0.3 - before.0.3).abs() < 1.,
        "the history moved from {} to {}",
        before.0.3,
        after.0.3
    );
    assert_eq!(after.1, before.1);
    assert_eq!(after.2, before.2);
}

#[gpui::test]
fn ui_31_a_search_is_kept(cx: &mut TestAppContext) {
    let fixture = Fixture::start(EVENT_CAPACITY);
    let mine = fixture.active();
    let made = api_with_files(20);
    let added = add(&fixture, &made);
    let java = second_tab(&fixture, &mine);
    let (mut cx, window) = main_window(cx, &fixture);

    // "Change 1" and "Change 10" to "Change 19".
    show_repo(&mut cx, &window, &added, "api");
    search(&mut cx, "Change 1");
    wait_for_rows(&mut cx, &window, 11);
    let subjects = |cx: &mut VisualTestContext| {
        with_history(cx, &window, |history| {
            history
                .rows()
                .iter()
                .map(|row| row.subject.to_string())
                .collect::<Vec<_>>()
        })
    };
    let before = subjects(&mut cx);

    switch_to(&mut cx, &fixture, &window, &java);
    wait_until(&mut cx, |cx| held(cx, &window, &mine).0 == 0);
    switch_to(&mut cx, &fixture, &window, &mine);
    wait_for_rows(&mut cx, &window, 11);
    assert_eq!(subjects(&mut cx), before);
    assert_eq!(search_text(&mut cx), "Change 1");
}
