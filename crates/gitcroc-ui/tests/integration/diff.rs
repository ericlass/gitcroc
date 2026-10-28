//! The diff of the selected file (UI-17): inline and side by side, the
//! options of the pane header and wrapping, F7 between hunks (UI-22), and
//! each tab's own diff (UI-29); its syntax colours (UI-24), and the diffs
//! that are not text (UI-18). Repos are real ones made by `repo-gen` and
//! added through the app (QA-2).

use gitcroc_app::app::EVENT_CAPACITY;

use gitcroc_core::{
    ChangesSettings, CodeTheme, Context, DiffMode, FileListMode, Settings, Theme as ThemeSetting,
};
use gitcroc_ui::diff::{DiffRow, Shown};
use gitcroc_ui::diff_view::DiffView;
use gitcroc_ui::special_diff::Special;
use gitcroc_ui::theme::AppTheme;
use gitcroc_ui::theme::code_themes::code_theme;
use gitcroc_ui::theme::tokens::Color;
use gitcroc_ui::window::GitcrocWindow;
use gpui_kit::test::TestWindowExt;
use gpui_kit::{
    ElementId, Entity, Pixels, Role, ScrollDelta, TestAppContext, VisualTestContext, point, px,
};

use crate::changes::{
    history_row, index_of, pressed, row, show_repo, switch_to, wait_for_view, with_changes,
};
use crate::history::{
    add, click, label_of, labels_in, press, repo, repos, tab, wait_for_element, wait_until,
};
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
    open_commit(cx, fixture, |api| {
        api.write("a.txt", old).unwrap();
        api.commit_all("Base").unwrap();
        api.write("a.txt", new).unwrap();
        api.commit_all("Change").unwrap();
    })
}

/// Opens the window on a repo `make` commits to, and shows the diff of its
/// last commit's first file.
fn open_commit(
    cx: &mut TestAppContext,
    fixture: &Fixture,
    make: impl FnOnce(&repo_gen::Repo),
) -> (VisualTestContext, Entity<GitcrocWindow>, repo_gen::Fixture) {
    let made = repos(&["api"]);
    make(&repo(&made, "api"));
    let added = add(fixture, &made);
    let (mut cx, window) = main_window(cx, fixture);
    show_repo(&mut cx, &window, &added, "api");
    click(&mut cx, history_row(0));
    wait_for_view(&mut cx, &window);
    wait_for_diff(&mut cx, &window);
    (cx, window, made)
}

/// Selects file `name` of the changes pane and waits for its diff.
fn select_file(cx: &mut VisualTestContext, window: &Entity<GitcrocWindow>, name: &str) {
    let at = index_of(cx, window, name);
    click(cx, row(at));
    wait_for_diff(cx, window);
}

/// Waits until the diff's syntax colours have arrived.
fn wait_for_colours(cx: &mut VisualTestContext, window: &Entity<GitcrocWindow>) {
    wait_until(cx, |cx| with_diff(cx, window, DiffView::highlighted));
}

/// The colour of `word` in the first shown row whose text has it, and the
/// row's index.
fn colour_of(
    cx: &mut VisualTestContext,
    window: &Entity<GitcrocWindow>,
    word: &str,
) -> (usize, Color) {
    cx.update(|_, cx| {
        let pane = window.read(cx).changes(cx).expect("no changes pane");
        let diff = pane.read(cx).diff().read(cx);
        let len = diff.doc().map_or(0, |doc| doc.len());
        (0..len)
            .find_map(|index| {
                diff.drawn(index, cx)
                    .into_iter()
                    .flatten()
                    .find(|(text, _)| text.trim() == word)
                    .map(|(_, colour)| (index, colour))
            })
            .unwrap_or_else(|| {
                let drawn: Vec<_> = (0..len).map(|index| diff.drawn(index, cx)).collect();
                panic!(
                    "no {word:?} in the diff of {:?}: {drawn:?}",
                    diff.target().map(|target| &target.path)
                )
            })
    })
}

/// A 1×1 red PNG, 69 bytes.
const RED_PNG: &[u8] = b"\x89\x50\x4e\x47\x0d\x0a\x1a\x0a\x00\x00\x00\x0d\x49\x48\x44\x52\x00\x00\x00\x01\x00\x00\x00\x01\x08\x02\x00\x00\x00\x90\x77\x53\xde\x00\x00\x00\x0c\x49\x44\x41\x54\x78\x9c\x63\xf8\xcf\xc0\x00\x00\x03\x01\x01\x00\xc9\xfe\x92\xef\x00\x00\x00\x00\x49\x45\x4e\x44\xae\x42\x60\x82";
/// A 2×1 blue PNG, 70 bytes.
const BLUE_PNG: &[u8] = b"\x89\x50\x4e\x47\x0d\x0a\x1a\x0a\x00\x00\x00\x0d\x49\x48\x44\x52\x00\x00\x00\x02\x00\x00\x00\x01\x08\x02\x00\x00\x00\x7b\x40\xe8\xdd\x00\x00\x00\x0d\x49\x44\x41\x54\x78\x9c\x63\x60\x60\xf8\x0f\x44\x00\x05\x02\x01\xff\xc1\x2b\x96\x9f\x00\x00\x00\x00\x49\x45\x4e\x44\xae\x42\x60\x82";

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

#[gpui::test]
fn ui_18_binary_files_show_their_sizes(cx: &mut TestAppContext) {
    let fixture = Fixture::start(EVENT_CAPACITY);
    let (mut cx, window, _made) = open_commit(cx, &fixture, |api| {
        api.write("a.bin", [0u8; 100]).unwrap();
        api.commit_all("Base").unwrap();
        api.write("a.bin", [0u8; 1536]).unwrap();
        api.write("b.bin", [1u8, 0, 2]).unwrap();
        api.commit_all("Change").unwrap();
    });

    assert_eq!(
        label_of(&mut cx, "diff-binary".into()).as_deref(),
        Some("Binary file changed. Before: 100 B, after: 1.5 KB.")
    );
    assert!(lines(&mut cx).is_empty());
    select_file(&mut cx, &window, "b.bin");
    assert_eq!(
        label_of(&mut cx, "diff-binary".into()).as_deref(),
        Some("Binary file changed. Before: none, after: 3 B.")
    );
}

#[gpui::test]
fn ui_18_images_show_before_and_after(cx: &mut TestAppContext) {
    let fixture = Fixture::start(EVENT_CAPACITY);
    let (mut cx, window, _made) = open_commit(cx, &fixture, |api| {
        api.write("logo.png", RED_PNG).unwrap();
        api.commit_all("Base").unwrap();
        api.write("logo.png", BLUE_PNG).unwrap();
        api.write("new.png", RED_PNG).unwrap();
        api.commit_all("Change").unwrap();
    });

    assert_eq!(
        label_of(&mut cx, "diff-image-old".into()).as_deref(),
        Some("Before: PNG, 69 B")
    );
    assert_eq!(
        label_of(&mut cx, "diff-image-new".into()).as_deref(),
        Some("After: PNG, 70 B")
    );
    // Both are drawn from their bytes, at their own size: never scaled up.
    let special = with_diff(&mut cx, &window, |diff| format!("{:?}", diff.special()));
    assert_eq!(
        label_of(&mut cx, "diff-image-old-image".into()).as_deref(),
        Some("Before image"),
        "{special}"
    );
    assert_eq!(
        label_of(&mut cx, "diff-image-new-image".into()).as_deref(),
        Some("After image")
    );
    let size = |cx: &mut VisualTestContext, id: &'static str| {
        cx.update(|window, cx| {
            window.render_frame(cx);
            window.find(id).bounds().size
        })
    };
    let new = size(&mut cx, "diff-image-new-image");
    assert!(
        new.width <= px(2. + 2.) && new.height <= px(1. + 2.),
        "{new:?}"
    );
    let decoded = with_diff(&mut cx, &window, |diff| match diff.special() {
        Some(Special::Image {
            old: Some(old),
            new: Some(new),
        }) => old.image.is_some() && new.image.is_some(),
        other => panic!("not an image diff: {other:?}"),
    });
    assert!(decoded);

    select_file(&mut cx, &window, "new.png");
    assert_eq!(
        label_of(&mut cx, "diff-image-old".into()).as_deref(),
        Some("Before: none")
    );
    assert!(label_of(&mut cx, "diff-image-old-image".into()).is_none());
    assert_eq!(
        label_of(&mut cx, "diff-image-new".into()).as_deref(),
        Some("After: PNG, 69 B")
    );
}

#[gpui::test]
fn ui_18_a_large_diff_waits_for_load_diff(cx: &mut TestAppContext) {
    let fixture = Fixture::start(EVENT_CAPACITY);
    let (mut cx, window, _made) = open_commit(cx, &fixture, |api| {
        api.write("many.txt", numbered(20_001)).unwrap();
        api.write("small.txt", "small\n").unwrap();
        api.commit_all("Many lines").unwrap();
    });

    assert_eq!(
        label_of(&mut cx, "diff-large".into()).as_deref(),
        Some(
            "This diff is large: 20,001 lines added and 0 removed. \
             Showing it may take a while."
        )
    );
    assert!(lines(&mut cx).is_empty());
    assert_eq!(
        label_of(&mut cx, "diff-load".into()).as_deref(),
        Some("Load diff")
    );

    click(&mut cx, "diff-load".into());
    wait_for_diff(&mut cx, &window);
    assert!(with_diff(&mut cx, &window, |diff| diff
        .asked()
        .unwrap()
        .load_anyway));
    assert_eq!(lines(&mut cx)[0], "Added, new 1: l1");
    let rows = with_diff(&mut cx, &window, |diff| diff.doc().map(|doc| doc.len()));
    assert_eq!(rows, Some(20_001 + 1));

    // An option read again keeps the choice for the file.
    click(&mut cx, "changes-context-ten".into());
    wait_for_diff(&mut cx, &window);
    assert!(with_diff(&mut cx, &window, |diff| diff
        .asked()
        .unwrap()
        .load_anyway));
    assert!(label_of(&mut cx, "diff-large".into()).is_none());

    // Another file ends it.
    select_file(&mut cx, &window, "small.txt");
    assert!(!with_diff(&mut cx, &window, DiffView::loaded_anyway));
    select_file(&mut cx, &window, "many.txt");
    assert!(label_of(&mut cx, "diff-large".into()).is_some());
}

#[gpui::test]
fn ui_18_folders_and_submodules(cx: &mut TestAppContext) {
    let fixture = Fixture::start(EVENT_CAPACITY);
    let first = "1111111111111111111111111111111111111111";
    let second = "2222222222222222222222222222222222222222";
    let (mut cx, window, made) = open_commit(cx, &fixture, |api| {
        api.git([
            "update-index",
            "--add",
            "--cacheinfo",
            "160000",
            first,
            "lib",
        ])
        .unwrap();
        api.git(["commit", "-m", "Add submodule"]).unwrap();
        api.git(["update-index", "--cacheinfo", "160000", second, "lib"])
            .unwrap();
        api.git(["commit", "-m", "Move submodule"]).unwrap();
    });

    assert_eq!(
        label_of(&mut cx, "diff-submodule-old".into()),
        Some(format!("Submodule commit before: {first}"))
    );
    assert_eq!(
        label_of(&mut cx, "diff-submodule-new".into()),
        Some(format!("Submodule commit after: {second}"))
    );
    click(&mut cx, history_row(1));
    wait_for_view(&mut cx, &window);
    wait_for_diff(&mut cx, &window);
    assert_eq!(
        label_of(&mut cx, "diff-submodule-old".into()).as_deref(),
        Some("Submodule commit before: none")
    );

    // An untracked folder is one entry, its files unread.
    repo(&made, "api")
        .write("folder/inner/b.txt", "b\n")
        .unwrap();
    fixture.refresh(&fixture.active());
    wait_for_element(&mut cx, "history-uncommitted".into());
    click(&mut cx, "history-uncommitted".into());
    wait_for_view(&mut cx, &window);
    wait_for_diff(&mut cx, &window);
    assert_eq!(
        label_of(&mut cx, "diff-folder".into()).as_deref(),
        Some("An untracked folder. Its files are not read.")
    );
}

#[gpui::test]
fn ui_24_the_diff_is_highlighted_by_its_syntax(cx: &mut TestAppContext) {
    let fixture = Fixture::start(EVENT_CAPACITY);
    let (mut cx, window, _made) = open_commit(cx, &fixture, |api| {
        api.write("main.rs", "fn main() {\n    /* a\n    let b */\n}\n")
            .unwrap();
        api.write("notes.unknown", "fn main() {}\n").unwrap();
        api.commit_all("Code").unwrap();
    });
    select_file(&mut cx, &window, "main.rs");
    wait_for_colours(&mut cx, &window);
    let colours = cx.update(|_, cx| AppTheme::global(cx).code_theme().colors);

    let (_, keyword) = colour_of(&mut cx, &window, "fn");
    assert_eq!(keyword, colours.keyword);
    // The comment's second line is a comment from the line above.
    let (_, comment) = colour_of(&mut cx, &window, "let b */");
    assert_eq!(comment, colours.comment);

    // A theme change recolours the diff without reading it again.
    let reads = with_diff(&mut cx, &window, DiffView::reads);
    cx.update(|_, cx| {
        let mut chosen = Settings {
            theme: ThemeSetting::Light,
            ..Settings::default()
        };
        chosen.appearance.code_theme_light = CodeTheme::SolarizedLight;
        AppTheme::apply_settings(&chosen, cx);
    });
    let solarized = code_theme(CodeTheme::SolarizedLight).colors;
    assert_ne!(solarized.keyword, colours.keyword);
    assert_eq!(colour_of(&mut cx, &window, "fn").1, solarized.keyword);
    assert_eq!(with_diff(&mut cx, &window, DiffView::reads), reads);

    // An unknown type is plain text: the line is one run.
    select_file(&mut cx, &window, "notes.unknown");
    wait_for_colours(&mut cx, &window);
    assert_eq!(
        colour_of(&mut cx, &window, "fn main() {}").1,
        solarized.text
    );
}

#[gpui::test]
fn perf_1_highlighting_runs_off_the_ui_thread(cx: &mut TestAppContext) {
    let fixture = Fixture::start(EVENT_CAPACITY);
    let (mut cx, window, _made) = open_commit(cx, &fixture, |api| {
        api.write("a.rs", "fn a() {}\n").unwrap();
        api.write("b.rs", "fn b() {}\n").unwrap();
        api.commit_all("Code").unwrap();
    });
    let colours = cx.update(|_, cx| AppTheme::global(cx).code_theme().colors);

    // Another file, one task at a time: its rows are drawn plain before
    // the background has highlighted them.
    let other = if with_diff(&mut cx, &window, |diff| {
        diff.target().is_some_and(|target| target.path == "a.rs")
    }) {
        "b.rs"
    } else {
        "a.rs"
    };
    let at = index_of(&mut cx, &window, other);
    cx.update(|window, cx| {
        window.render_frame(cx);
        window.click(row(at), cx);
    });
    let line = format!("fn {}() {{}}", &other[..1]);
    let mut plain = false;
    for _ in 0..10_000 {
        if !cx.executor().tick() {
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
        let (shown, highlighted) = with_diff(&mut cx, &window, |diff| {
            let shown = diff.target().is_some_and(|target| target.path == other)
                && !diff.loading()
                && diff.doc().is_some();
            (shown, diff.highlighted())
        });
        if shown && !highlighted {
            assert_eq!(lines(&mut cx), [format!("Added, new 1: {line}")]);
            assert_eq!(colour_of(&mut cx, &window, &line).1, colours.text);
            plain = true;
        }
        if shown && highlighted {
            break;
        }
    }
    assert!(plain, "the rows were never drawn before their colours");
    assert!(with_diff(&mut cx, &window, DiffView::highlighted));
    let (index, keyword) = colour_of(&mut cx, &window, "fn");
    assert_eq!(keyword, colours.keyword);
    let row = with_diff(&mut cx, &window, |diff| {
        matches!(
            diff.doc().and_then(|doc| doc.get(index)),
            Some(Shown::Row(DiffRow::Line { .. }))
        )
    });
    assert!(row);
}
