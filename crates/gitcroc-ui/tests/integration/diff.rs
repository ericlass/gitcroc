//! The diff of the selected file (UI-17): inline and side by side, the
//! options of the pane header and wrapping, F7 between hunks (UI-22), and
//! each tab's own diff (UI-29). Repos are real ones made by `repo-gen` and
//! added through the app (QA-2).

use gitcroc_app::app::EVENT_CAPACITY;

use gitcroc_core::{ChangesSettings, Context, DiffMode, FileListMode};
use gitcroc_ui::diff_view::DiffView;
use gitcroc_ui::window::GitcrocWindow;
use gpui_kit::test::TestWindowExt;
use gpui_kit::{
    ElementId, Entity, Pixels, Role, ScrollDelta, TestAppContext, VisualTestContext, point, px,
};

use crate::changes::{
    history_row, index_of, pressed, row, show_repo, switch_to, wait_for_view, with_changes,
};
use crate::history::{add, click, label_of, labels_in, press, repo, repos, tab, wait_until};
use crate::support::{Fixture, main_window};

/// Runs `f` on the active tab's diff.
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

/// Waits until the selected file's diff is read and shown.
#[track_caller]
fn wait_for_diff(cx: &mut VisualTestContext, window: &Entity<GitcrocWindow>) {
    wait_until(cx, |cx| {
        let target = with_changes(cx, window, |changes| changes.diff_target());
        with_diff(cx, window, |diff| {
            target.is_some()
                && diff.target() == target.as_ref()
                && !diff.loading()
                && diff.content().is_some()
        })
    });
}

/// The diff's lines on screen, as a screen reader reads them.
fn lines(cx: &mut VisualTestContext) -> Vec<String> {
    labels_in(cx, "diff-lines".into(), Role::ListItem)
}

fn hunks(cx: &mut VisualTestContext) -> Vec<String> {
    labels_in(cx, "diff-lines".into(), Role::Heading)
}

fn folds(cx: &mut VisualTestContext) -> Vec<String> {
    labels_in(cx, "diff-lines".into(), Role::Button)
}

fn checked(cx: &mut VisualTestContext, id: &'static str) -> Option<bool> {
    cx.update(|window, cx| {
        window.render_frame(cx);
        window.try_find(id).and_then(|found| found.checked())
    })
}

fn height(cx: &mut VisualTestContext, id: ElementId) -> Pixels {
    cx.update(|window, cx| {
        window.render_frame(cx);
        window.find(id).bounds().size.height
    })
}

fn scroll_diff(cx: &mut VisualTestContext, x: f32, y: f32) {
    cx.update(|window, cx| {
        window.scroll("diff-lines", ScrollDelta::Pixels(point(px(x), px(y))), cx);
        window.render_frame(cx);
    });
    cx.run_until_parked();
}

/// Lines `l1` to `l{count}`.
fn numbered(count: usize) -> String {
    (1..=count).map(|n| format!("l{n}\n")).collect()
}

/// Opens the window on a repo whose last commit changes `a.txt` from `old`
/// to `new`, and shows that commit's diff. The repo lives as long as the
/// `repo_gen::Fixture` returned.
fn open_change(
    cx: &mut TestAppContext,
    fixture: &Fixture,
    old: &str,
    new: &str,
) -> (VisualTestContext, Entity<GitcrocWindow>, repo_gen::Fixture) {
    let made = repos(&["api"]);
    let api = repo(&made, "api");
    api.write("a.txt", old).unwrap();
    api.commit_all("Base").unwrap();
    api.write("a.txt", new).unwrap();
    api.commit_all("Change").unwrap();
    let added = add(fixture, &made);
    let (mut cx, window) = main_window(cx, fixture);
    show_repo(&mut cx, &window, &added, "api");
    click(&mut cx, history_row(0));
    wait_for_view(&mut cx, &window);
    wait_for_diff(&mut cx, &window);
    (cx, window, made)
}

#[gpui::test]
fn ui_17_inline_diff_shows_the_hunks_with_both_line_numbers(cx: &mut TestAppContext) {
    let fixture = Fixture::start(EVENT_CAPACITY);
    let made = repos(&["api"]);
    let api = repo(&made, "api");
    api.write("a.txt", "one\ntwo\nthree\n").unwrap();
    api.commit_all("Base").unwrap();
    api.write("a.txt", "one\nTWO\nthree\nfour\n").unwrap();
    api.write("b.txt", "end").unwrap();
    api.commit_all("Change").unwrap();
    let added = add(&fixture, &made);
    let (mut cx, window) = main_window(cx, &fixture);
    show_repo(&mut cx, &window, &added, "api");
    click(&mut cx, history_row(0));
    wait_for_view(&mut cx, &window);
    wait_for_diff(&mut cx, &window);

    assert_eq!(pressed(&mut cx, "changes-inline".into()), Some(true));
    assert_eq!(
        label_of(&mut cx, "diff-lines".into()).as_deref(),
        Some("Diff of a.txt")
    );
    assert_eq!(hunks(&mut cx), ["Hunk 1: @@ -1,3 +1,4 @@"]);
    assert_eq!(
        lines(&mut cx),
        [
            "Unchanged, old 1, new 1: one",
            "Removed, old 2: two",
            "Added, new 2: TWO",
            "Unchanged, old 3, new 3: three",
            "Added, new 4: four",
        ]
    );
    assert_eq!(label_of(&mut cx, "diff-no-newline".into()), None);

    // A file without a final newline says so.
    let b = index_of(&mut cx, &window, "b.txt");
    click(&mut cx, row(b));
    wait_for_diff(&mut cx, &window);
    assert_eq!(lines(&mut cx), ["Added, new 1: end"]);
    assert_eq!(
        label_of(&mut cx, "diff-no-newline".into()).as_deref(),
        Some("No newline at the end of the new file.")
    );
}

#[gpui::test]
fn ui_17_side_by_side_pairs_the_changed_lines_and_folds_the_rest(cx: &mut TestAppContext) {
    let fixture = Fixture::start(EVENT_CAPACITY);
    let old = numbered(30);
    let new = old.replace("l15\n", "L15\n");
    let (mut cx, window, _repos) = open_change(cx, &fixture, &old, &new);

    click(&mut cx, "changes-side-by-side".into());
    wait_until(&mut cx, |cx| {
        with_diff(cx, &window, |diff| {
            !diff.loading() && diff.doc().is_some_and(|doc| doc.side_by_side_mode())
        })
    });
    assert_eq!(pressed(&mut cx, "changes-side-by-side".into()), Some(true));
    assert_eq!(pressed(&mut cx, "changes-inline".into()), Some(false));
    assert!(with_diff(&mut cx, &window, |diff| diff
        .asked()
        .unwrap()
        .full_text));

    // Fourteen unchanged lines before the change keep three; fifteen
    // after it keep three.
    assert_eq!(
        folds(&mut cx),
        ["Show 11 unchanged lines", "Show 12 unchanged lines"]
    );
    assert_eq!(hunks(&mut cx), ["Hunk 1: @@ -12,7 +12,7 @@"]);
    let pair = |n: usize| format!("Unchanged, old {n}: l{n}; Unchanged, new {n}: l{n}");
    assert_eq!(
        lines(&mut cx),
        [
            pair(12),
            pair(13),
            pair(14),
            "Removed, old 15: l15; Added, new 15: L15".to_owned(),
            pair(16),
            pair(17),
            pair(18),
        ]
    );

    // A click shows the folded lines; the other fold is below the screen.
    click(&mut cx, "diff-fold-0".into());
    assert_eq!(
        with_diff(&mut cx, &window, |diff| diff.doc().unwrap().len()),
        14 + 2 + 3 + 1
    );
    assert_eq!(lines(&mut cx).first(), Some(&pair(1)));
}

#[gpui::test]
fn ui_17_diff_options_are_kept_and_read_the_diff_again(cx: &mut TestAppContext) {
    let fixture = Fixture::start(EVENT_CAPACITY);
    let (mut cx, window, _repos) = open_change(cx, &fixture, "a\n  b\nc\n", "a\n    b\nc\nd\n");
    assert!(lines(&mut cx).contains(&"Removed, old 2:   b".to_owned()));
    assert_eq!(checked(&mut cx, "changes-ignore-whitespace"), Some(false));
    let reads = |cx: &mut VisualTestContext| with_diff(cx, &window, DiffView::reads);

    // Ignoring whitespace reads the diff again without the indented line.
    let before = reads(&mut cx);
    click(&mut cx, "changes-ignore-whitespace".into());
    assert_eq!(checked(&mut cx, "changes-ignore-whitespace"), Some(true));
    wait_until(&mut cx, |cx| reads(cx) > before);
    wait_for_diff(&mut cx, &window);
    assert!(with_diff(&mut cx, &window, |diff| diff
        .asked()
        .unwrap()
        .ignore_whitespace));
    let shown = lines(&mut cx);
    assert!(
        !shown.iter().any(|line| line.starts_with("Removed")),
        "{shown:?}"
    );
    assert!(shown.contains(&"Added, new 4: d".to_owned()), "{shown:?}");

    // So does the context.
    let before = reads(&mut cx);
    click(&mut cx, "changes-context-ten".into());
    assert_eq!(pressed(&mut cx, "changes-context-ten".into()), Some(true));
    assert_eq!(
        pressed(&mut cx, "changes-context-three".into()),
        Some(false)
    );
    wait_until(&mut cx, |cx| reads(cx) > before);
    wait_for_diff(&mut cx, &window);
    assert_eq!(
        with_diff(&mut cx, &window, |diff| diff.asked().unwrap().context),
        Context::Ten
    );

    // Wrapping only lays the lines out again.
    let before = reads(&mut cx);
    click(&mut cx, "changes-wrap-lines".into());
    assert_eq!(checked(&mut cx, "changes-wrap-lines"), Some(true));
    assert!(with_diff(&mut cx, &window, DiffView::wrapping));
    assert_eq!(reads(&mut cx), before);

    // All are kept in the settings.
    let app = fixture.app().clone();
    wait_until(&mut cx, |_| {
        app.settings_get().settings.changes
            == ChangesSettings {
                diff_mode: DiffMode::Inline,
                file_list: FileListMode::Flat,
                ignore_whitespace: true,
                wrap_lines: true,
                context: Context::Ten,
            }
    });
}

#[gpui::test]
fn ui_17_wrap_lines_wraps_long_lines(cx: &mut TestAppContext) {
    let fixture = Fixture::start(EVENT_CAPACITY);
    let long = "word ".repeat(200);
    let (mut cx, window, _repos) = open_change(cx, &fixture, "a\n", &format!("a\n{long}\n"));
    let line = || ElementId::from("diff-row-2");
    let one_line = height(&mut cx, "diff-row-1".into());
    assert_eq!(height(&mut cx, line()), one_line);

    // Without wrapping, the lines scroll sideways together, and only
    // sideways.
    scroll_diff(&mut cx, -200., 0.);
    assert_eq!(with_diff(&mut cx, &window, DiffView::offset), px(200.));
    assert_eq!(with_diff(&mut cx, &window, DiffView::top_row), 0);
    scroll_diff(&mut cx, 500., 0.);
    assert_eq!(with_diff(&mut cx, &window, DiffView::offset), px(0.));

    click(&mut cx, "changes-wrap-lines".into());
    wait_until(&mut cx, |cx| height(cx, line()) > one_line * 2.);
    assert_eq!(height(&mut cx, "diff-row-1".into()), one_line);
}

#[gpui::test]
fn ui_22_f7_moves_between_hunks(cx: &mut TestAppContext) {
    let fixture = Fixture::start(EVENT_CAPACITY);
    let old = numbered(60);
    let new = old
        .replace("l5\n", "L5\n")
        .replace("l30\n", "L30\n")
        .replace("l55\n", "L55\n");
    let (mut cx, window, _repos) = open_change(cx, &fixture, &old, &new);
    assert_eq!(
        with_diff(&mut cx, &window, |diff| diff.doc().unwrap().hunk_count()),
        3
    );
    let focused = |cx: &mut VisualTestContext, hunk: usize| {
        cx.update(|window, cx| {
            window.render_frame(cx);
            window
                .try_find(ElementId::Name(format!("diff-hunk-{hunk}").into()))
                .and_then(|found| found.focused())
        })
    };
    let current = |cx: &mut VisualTestContext| with_diff(cx, &window, DiffView::hunk);

    press(&mut cx, "f7");
    assert_eq!(current(&mut cx), Some(0));
    assert_eq!(focused(&mut cx, 0), Some(true));
    press(&mut cx, "f7");
    press(&mut cx, "f7");
    assert_eq!(current(&mut cx), Some(2));
    assert_eq!(focused(&mut cx, 2), Some(true));
    // Past the last hunk, back to the first, and back again.
    press(&mut cx, "f7");
    assert_eq!(current(&mut cx), Some(0));
    press(&mut cx, "shift-f7");
    assert_eq!(current(&mut cx), Some(2));
    assert_eq!(focused(&mut cx, 2), Some(true));
}

#[gpui::test]
fn ui_29_each_tab_keeps_its_diff(cx: &mut TestAppContext) {
    let fixture = Fixture::start(EVENT_CAPACITY);
    let mine = fixture.active();
    let made = repos(&["api"]);
    let api = repo(&made, "api");
    api.write("a.txt", numbered(300)).unwrap();
    api.commit_all("Long file").unwrap();
    let added = add(&fixture, &made);
    // A second workspace, for the options' reach.
    let app = fixture.app().clone();
    let java = fixture.runtime.block_on(async {
        let tabs = app.workspace_create("Java".to_owned()).await.unwrap();
        tabs.active.unwrap()
    });
    let other = repos(&["core"]);
    repo(&other, "core")
        .commit_file("core.txt", numbered(3), "Core")
        .unwrap();
    let other_added = fixture.add_repos(&java, other.path());
    fixture
        .runtime
        .block_on(app.workspace_activate(mine.clone()))
        .unwrap();

    let (mut cx, window) = main_window(cx, &fixture);
    show_repo(&mut cx, &window, &added, "api");
    click(&mut cx, history_row(0));
    wait_for_view(&mut cx, &window);
    wait_for_diff(&mut cx, &window);
    click(&mut cx, "changes-side-by-side".into());
    wait_until(&mut cx, |cx| {
        with_diff(cx, &window, |diff| {
            !diff.loading() && diff.doc().is_some_and(|doc| doc.side_by_side_mode())
        })
    });

    // Scroll down the diff.
    scroll_diff(&mut cx, 0., -100. * 20.);
    let state = |cx: &mut VisualTestContext| {
        let top = with_diff(cx, &window, DiffView::top_row);
        (top, lines(cx))
    };
    let before = state(&mut cx);
    assert!(before.0 > 0, "the diff did not scroll");

    // The other tab shows its own diff, side by side too.
    switch_to(&mut cx, &fixture, &java);
    show_repo(&mut cx, &window, &other_added, "core");
    click(&mut cx, history_row(0));
    wait_for_view(&mut cx, &window);
    wait_until(&mut cx, |cx| {
        with_diff(cx, &window, |diff| {
            !diff.loading() && diff.doc().is_some_and(|doc| doc.side_by_side_mode())
        })
    });
    assert_eq!(
        label_of(&mut cx, "diff-lines".into()).as_deref(),
        Some("Diff of core.txt")
    );

    // Back, the diff is where it was left.
    click(&mut cx, tab(&mine));
    wait_until(&mut cx, |_| fixture.active() == mine);
    wait_until(&mut cx, |cx| {
        label_of(cx, "diff-lines".into()).as_deref() == Some("Diff of a.txt")
    });
    assert_eq!(state(&mut cx), before);
}

#[gpui::test]
fn perf_1_the_diff_lays_out_only_the_rows_on_screen(cx: &mut TestAppContext) {
    let fixture = Fixture::start(EVENT_CAPACITY);
    let (mut cx, window, _repos) = open_change(cx, &fixture, "", &numbered(2000));
    assert_eq!(
        with_diff(&mut cx, &window, |diff| diff.doc().unwrap().len()),
        2001
    );
    let laid_out = lines(&mut cx).len();
    assert!(
        (1..100).contains(&laid_out),
        "{laid_out} rows laid out for 2001"
    );
}

#[gpui::test]
fn sec_3_diff_content_renders_as_text(cx: &mut TestAppContext) {
    let fixture = Fixture::start(EVENT_CAPACITY);
    let markup = "<script>alert(1)</script> &amp; <b>bold</b>";
    let (mut cx, _window, _repos) = open_change(cx, &fixture, "", &format!("{markup}\n"));
    assert_eq!(lines(&mut cx), [format!("Added, new 1: {markup}")]);
}
