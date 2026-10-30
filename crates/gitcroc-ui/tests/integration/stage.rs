//! Stage, unstage and discard in "Uncommitted changes" (UI-15): the
//! controls of the file rows and lists, several files at once, the hunk
//! controls in both diff modes, F7 to a hunk's control (UI-22), the discard
//! dialog (GRP-9, SEC-3), and a blocked repo's controls (GRP-8). Repos are
//! real ones made by `repo-gen` and added through the app (QA-2); each
//! action is a job, waited for by the lists it changes, never by time.

use gitcroc_app::app::EVENT_CAPACITY;
use gitcroc_core::{ChangesContent, JobView, RepoId};
use gitcroc_ui::changes::{FileRow, List};
use gitcroc_ui::diff_view::DiffView;
use gitcroc_ui::window::GitcrocWindow;
use gpui_kit::test::TestWindowExt;
use gpui_kit::{ElementId, Entity, Modifiers, TestAppContext, VisualTestContext, px, size};

use crate::changes::{row, show_repo, wait_for_view, with_changes};
use crate::history::{
    add, click, id_of, label_of, labels_in, press, repo, repos, tree_row, wait_for_element,
    wait_until,
};
use crate::history_merged::{group, group_node, present};
use crate::support::{Fixture, main_window};

fn name(id: String) -> ElementId {
    ElementId::Name(id.into())
}

pub(crate) fn row_part(index: usize, part: &str) -> ElementId {
    name(format!("changes-row-{index}-{part}"))
}

fn hunk_control(hunk: usize, part: &str) -> ElementId {
    name(format!("diff-hunk-{hunk}-{part}"))
}

/// Lines `l1` to `l{count}`.
pub(crate) fn numbered(count: usize) -> String {
    (1..=count).map(|n| format!("l{n}\n")).collect()
}

/// Makes the window tall and wide enough that the lists, the commit box
/// and the diff all have room. At 1200 by 800 the narrow changes pane's
/// header wraps, and the lists above the commit box show three rows.
pub(crate) fn enlarge(cx: &mut VisualTestContext) {
    cx.simulate_resize(size(px(1400.), px(1200.)));
    cx.run_until_parked();
}

/// Shows "Uncommitted changes" of repo `name`, in an enlarged window.
#[track_caller]
pub(crate) fn show_uncommitted(
    cx: &mut VisualTestContext,
    window: &Entity<GitcrocWindow>,
    added: &[(String, RepoId)],
    name: &str,
) {
    enlarge(cx);
    show_repo(cx, window, added, name);
    wait_for_element(cx, "history-uncommitted".into());
    click(cx, "history-uncommitted".into());
    wait_for_view(cx, window);
}

/// The paths of the shown repo's staged and unstaged lists, sorted.
pub(crate) fn lists(
    cx: &mut VisualTestContext,
    window: &Entity<GitcrocWindow>,
) -> (Vec<String>, Vec<String>) {
    with_changes(cx, window, |changes| {
        match changes.view().map(|view| &view.content) {
            Some(ChangesContent::Uncommitted {
                staged, unstaged, ..
            }) => {
                let paths = |files: &[gitcroc_core::ChangedFile]| {
                    let mut paths: Vec<String> =
                        files.iter().map(|file| file.path.clone()).collect();
                    paths.sort();
                    paths
                };
                (paths(staged), paths(unstaged))
            }
            _ => (Vec::new(), Vec::new()),
        }
    })
}

/// Waits until the lists are `staged` and `unstaged`, read anew after a
/// job.
#[track_caller]
pub(crate) fn wait_for_lists(
    cx: &mut VisualTestContext,
    window: &Entity<GitcrocWindow>,
    staged: &[&str],
    unstaged: &[&str],
) {
    for _ in 0..1000 {
        crate::support::settle(cx, |_, _| true);
        let loading = with_changes(cx, window, |changes| changes.loading());
        let (now_staged, now_unstaged) = lists(cx, window);
        if !loading && now_staged == staged && now_unstaged == unstaged {
            return;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    let shown = lists(cx, window);
    let notice = with_changes(cx, window, |changes| changes.notice().map(str::to_owned));
    panic!("the lists stay {shown:?}, notice {notice:?}; wanted {staged:?} and {unstaged:?}");
}

/// The row of file `path` in `list`.
#[track_caller]
pub(crate) fn row_of(
    cx: &mut VisualTestContext,
    window: &Entity<GitcrocWindow>,
    list: List,
    path: &str,
) -> usize {
    with_changes(cx, window, |changes| {
        changes.rows().iter().position(|row| match row {
            FileRow::File { file, .. } => {
                file.list == list
                    && changes
                        .entry_of(file)
                        .is_some_and(|entry| entry.path() == path)
            }
            _ => false,
        })
    })
    .unwrap_or_else(|| panic!("no row {path} in {list:?}"))
}

/// The row of the heading of section `list`.
fn section_of(cx: &mut VisualTestContext, window: &Entity<GitcrocWindow>, list: List) -> usize {
    with_changes(cx, window, |changes| {
        changes
            .rows()
            .iter()
            .position(|row| matches!(row, FileRow::Section { list: shown, .. } if *shown == list))
    })
    .unwrap_or_else(|| panic!("no section {list:?}"))
}

/// The paths of the files selected together.
fn marked(cx: &mut VisualTestContext, window: &Entity<GitcrocWindow>) -> Vec<String> {
    with_changes(cx, window, |changes| {
        let mut paths: Vec<String> = changes
            .marked()
            .iter()
            .filter_map(|file| Some(changes.entry_of(file)?.path().to_owned()))
            .collect();
        paths.sort();
        paths
    })
}

fn click_with(cx: &mut VisualTestContext, id: ElementId, modifiers: Modifiers) {
    cx.update(|window, cx| {
        window.render_frame(cx);
        window.click_with_modifiers(id, modifiers, cx);
    });
    cx.run_until_parked();
}

fn double_click(cx: &mut VisualTestContext, id: ElementId) {
    cx.update(|window, cx| {
        window.render_frame(cx);
        window.double_click(id, cx);
    });
    cx.run_until_parked();
}

/// Whether control `id` is disabled; it must be on screen.
#[track_caller]
pub(crate) fn disabled(cx: &mut VisualTestContext, id: ElementId) -> bool {
    cx.update(|window, cx| {
        window.render_frame(cx);
        let found = window
            .try_find(id.clone())
            .unwrap_or_else(|| panic!("no {id:?}"));
        found.disabled() == Some(true)
    })
}

pub(crate) fn focused(cx: &mut VisualTestContext, id: ElementId) -> bool {
    cx.update(|window, cx| {
        window.render_frame(cx);
        window.try_find(id).and_then(|found| found.focused()) == Some(true)
    })
}

/// The jobs whose label starts with `verb`, oldest first.
fn jobs(fixture: &Fixture, verb: &str) -> Vec<JobView> {
    let mut jobs: Vec<JobView> = fixture
        .app()
        .jobs_get()
        .into_iter()
        .filter(|job| job.label.starts_with(verb))
        .collect();
    jobs.sort_by_key(|job| job.job_id.0);
    jobs
}

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

/// Selects file `path` of `list` and waits for its diff, with its hunks'
/// controls when it has some.
#[track_caller]
fn show_diff(cx: &mut VisualTestContext, window: &Entity<GitcrocWindow>, list: List, path: &str) {
    let at = row_of(cx, window, list, path);
    click(cx, row(at));
    wait_until(cx, |cx| {
        let target = with_changes(cx, window, |changes| changes.diff_target());
        with_diff(cx, window, |diff| {
            target.is_some()
                && diff.target() == target.as_ref()
                && !diff.loading()
                && diff.doc().is_some()
        })
    });
}

/// Keeps git from turning line ends into CRLF when it writes files back,
/// as a Windows runner's config asks, so a test can compare their text.
fn exact_line_ends(repo: &repo_gen::Repo) {
    repo.git(["config", "core.autocrlf", "false"]).unwrap();
}

/// A repo `api` whose `a.txt`, committed with 60 lines, has lines 5 and 55
/// changed: two hunks.
fn two_hunks() -> repo_gen::Fixture {
    let made = repos(&["api"]);
    let api = repo(&made, "api");
    exact_line_ends(&api);
    let old = numbered(60);
    api.write("a.txt", &old).unwrap();
    api.commit_all("Lines").unwrap();
    api.write(
        "a.txt",
        old.replace("l5\n", "L5\n").replace("l55\n", "L55\n"),
    )
    .unwrap();
    made
}

#[gpui::test]
fn ui_15_stage_and_unstage_files_from_the_lists(cx: &mut TestAppContext) {
    let fixture = Fixture::start(EVENT_CAPACITY);
    let made = repos(&["api"]);
    let api = repo(&made, "api");
    api.write("c.txt", "c\n").unwrap();
    api.git(["add", "c.txt"]).unwrap();
    api.write("README.md", "changed\n").unwrap();
    api.write("b.txt", "b\n").unwrap();
    let added = add(&fixture, &made);
    let (mut cx, window) = main_window(cx, &fixture);
    show_uncommitted(&mut cx, &window, &added, "api");
    wait_for_lists(&mut cx, &window, &["c.txt"], &["README.md", "b.txt"]);

    // A row's controls name their file.
    let readme = row_of(&mut cx, &window, List::Unstaged, "README.md");
    assert_eq!(
        label_of(&mut cx, row_part(readme, "stage")).as_deref(),
        Some("Stage README.md")
    );
    assert_eq!(
        label_of(&mut cx, row_part(readme, "discard")).as_deref(),
        Some("Discard README.md")
    );
    click(&mut cx, row_part(readme, "stage"));
    wait_for_lists(&mut cx, &window, &["README.md", "c.txt"], &["b.txt"]);

    let c = row_of(&mut cx, &window, List::Staged, "c.txt");
    assert_eq!(
        label_of(&mut cx, row_part(c, "unstage")).as_deref(),
        Some("Unstage c.txt")
    );
    click(&mut cx, row_part(c, "unstage"));
    wait_for_lists(&mut cx, &window, &["README.md"], &["b.txt", "c.txt"]);

    // The lists' buttons move all of them.
    let unstaged = section_of(&mut cx, &window, List::Unstaged);
    assert_eq!(
        label_of(&mut cx, row_part(unstaged, "all")).as_deref(),
        Some("Stage all")
    );
    click(&mut cx, row_part(unstaged, "all"));
    wait_for_lists(&mut cx, &window, &["README.md", "b.txt", "c.txt"], &[]);
    let staged = section_of(&mut cx, &window, List::Staged);
    assert_eq!(
        label_of(&mut cx, row_part(staged, "all")).as_deref(),
        Some("Unstage all")
    );
    click(&mut cx, row_part(staged, "all"));
    wait_for_lists(&mut cx, &window, &[], &["README.md", "b.txt", "c.txt"]);

    // A double-click moves a file to the other list.
    let b = row_of(&mut cx, &window, List::Unstaged, "b.txt");
    double_click(&mut cx, row(b));
    wait_for_lists(&mut cx, &window, &["b.txt"], &["README.md", "c.txt"]);
    let b = row_of(&mut cx, &window, List::Staged, "b.txt");
    double_click(&mut cx, row(b));
    wait_for_lists(&mut cx, &window, &[], &["README.md", "b.txt", "c.txt"]);
}

#[gpui::test]
fn ui_15_several_files_go_in_one_job(cx: &mut TestAppContext) {
    let fixture = Fixture::start(EVENT_CAPACITY);
    let made = repos(&["api"]);
    let api = repo(&made, "api");
    for file in ["a.txt", "b.txt", "c.txt", "d.txt", "e.txt"] {
        api.write(file, "x\n").unwrap();
    }
    let added = add(&fixture, &made);
    let (mut cx, window) = main_window(cx, &fixture);
    show_uncommitted(&mut cx, &window, &added, "api");
    let all = ["README.md", "a.txt", "b.txt", "c.txt", "d.txt", "e.txt"];
    wait_for_lists(&mut cx, &window, &[], &all[1..]);
    let secondary = cx.update(|_, cx| Modifiers::secondary_key(cx));

    // Ctrl or Cmd adds a file, and the row's control acts on both.
    let a = row_of(&mut cx, &window, List::Unstaged, "a.txt");
    click(&mut cx, row(a));
    let c = row_of(&mut cx, &window, List::Unstaged, "c.txt");
    click_with(&mut cx, row(c), secondary);
    assert_eq!(marked(&mut cx, &window), ["a.txt", "c.txt"]);
    // And takes it out again, and back.
    click_with(&mut cx, row(c), secondary);
    assert_eq!(marked(&mut cx, &window), ["a.txt"]);
    click_with(&mut cx, row(c), secondary);
    let before = jobs(&fixture, "Stage").len();
    click(&mut cx, row_part(c, "stage"));
    wait_for_lists(
        &mut cx,
        &window,
        &["a.txt", "c.txt"],
        &["b.txt", "d.txt", "e.txt"],
    );
    let stages = jobs(&fixture, "Stage");
    assert_eq!(stages.len(), before + 1, "{stages:?}");
    assert_eq!(stages.last().unwrap().label, "Stage 2 files");

    // Shift selects the run from the selected file.
    let b = row_of(&mut cx, &window, List::Unstaged, "b.txt");
    click(&mut cx, row(b));
    let e = row_of(&mut cx, &window, List::Unstaged, "e.txt");
    click_with(&mut cx, row(e), Modifiers::shift());
    assert_eq!(marked(&mut cx, &window), ["b.txt", "d.txt", "e.txt"]);

    // A click in the other list starts a new selection.
    let a = row_of(&mut cx, &window, List::Staged, "a.txt");
    click_with(&mut cx, row(a), secondary);
    assert_eq!(marked(&mut cx, &window), ["a.txt"]);

    // Back to the run, which one job stages.
    let b = row_of(&mut cx, &window, List::Unstaged, "b.txt");
    click(&mut cx, row(b));
    let e = row_of(&mut cx, &window, List::Unstaged, "e.txt");
    click_with(&mut cx, row(e), Modifiers::shift());
    let before = jobs(&fixture, "Stage").len();
    let d = row_of(&mut cx, &window, List::Unstaged, "d.txt");
    click(&mut cx, row_part(d, "stage"));
    wait_for_lists(&mut cx, &window, &all[1..], &[]);
    let stages = jobs(&fixture, "Stage");
    assert_eq!(stages.len(), before + 1, "{stages:?}");
    assert_eq!(stages.last().unwrap().label, "Stage 3 files");
}

#[gpui::test]
fn ui_15_stage_unstage_and_discard_a_hunk(cx: &mut TestAppContext) {
    let fixture = Fixture::start(EVENT_CAPACITY);
    let made = two_hunks();
    let api = repo(&made, "api");
    let added = add(&fixture, &made);
    let (mut cx, window) = main_window(cx, &fixture);
    show_uncommitted(&mut cx, &window, &added, "api");

    // Inline: stage the first hunk.
    show_diff(&mut cx, &window, List::Unstaged, "a.txt");
    assert_eq!(
        label_of(&mut cx, hunk_control(0, "stage")).as_deref(),
        Some("Stage hunk 1")
    );
    assert_eq!(
        label_of(&mut cx, hunk_control(1, "discard")).as_deref(),
        Some("Discard hunk 2")
    );
    click(&mut cx, hunk_control(0, "stage"));
    wait_for_lists(&mut cx, &window, &["a.txt"], &["a.txt"]);
    let cached = api.git(["diff", "--cached"]).unwrap();
    assert!(
        cached.contains("+L5") && !cached.contains("+L55"),
        "{cached}"
    );

    // A staged file's hunk has Unstage only.
    show_diff(&mut cx, &window, List::Staged, "a.txt");
    assert!(present(&mut cx, hunk_control(0, "unstage")));
    assert!(!present(&mut cx, hunk_control(0, "discard")));
    click(&mut cx, hunk_control(0, "unstage"));
    wait_for_lists(&mut cx, &window, &[], &["a.txt"]);

    // Side by side: stage the second hunk, then discard the first.
    click(&mut cx, "changes-side-by-side".into());
    show_diff(&mut cx, &window, List::Unstaged, "a.txt");
    wait_until(&mut cx, |cx| {
        with_diff(cx, &window, |diff| {
            diff.doc().is_some_and(|doc| doc.side_by_side_mode())
        })
    });
    click(&mut cx, hunk_control(1, "stage"));
    wait_for_lists(&mut cx, &window, &["a.txt"], &["a.txt"]);
    let cached = api.git(["diff", "--cached"]).unwrap();
    assert!(
        cached.contains("+L55") && !cached.contains("+L5\n"),
        "{cached}"
    );
    show_diff(&mut cx, &window, List::Unstaged, "a.txt");
    click(&mut cx, hunk_control(0, "discard"));
    wait_for_element(&mut cx, "discard-dialog".into());
    assert_eq!(
        label_of(&mut cx, "discard-text".into()).as_deref(),
        Some("This hunk is lost; it cannot be restored:")
    );
    click(&mut cx, "dialog-submit".into());
    wait_for_lists(&mut cx, &window, &["a.txt"], &[]);
    let text = std::fs::read_to_string(api.path().join("a.txt")).unwrap();
    assert!(
        text.contains("\nl5\n") && text.contains("\nL55\n"),
        "{text}"
    );
}

#[gpui::test]
fn ui_15_a_changed_file_refuses_its_hunk(cx: &mut TestAppContext) {
    let fixture = Fixture::start(EVENT_CAPACITY);
    let made = two_hunks();
    let api = repo(&made, "api");
    let added = add(&fixture, &made);
    let (mut cx, window) = main_window(cx, &fixture);
    show_uncommitted(&mut cx, &window, &added, "api");
    show_diff(&mut cx, &window, List::Unstaged, "a.txt");
    let reads = with_diff(&mut cx, &window, DiffView::reads);

    // The file changes after its diff was read; the working tree is not
    // watched, so nothing reads it again before the click.
    let changed = numbered(60)
        .replace("l5\n", "Changed again\n")
        .replace("l55\n", "L55\n");
    api.write("a.txt", &changed).unwrap();
    click(&mut cx, hunk_control(0, "stage"));
    wait_until(&mut cx, |cx| {
        label_of(cx, "changes-notice".into()).as_deref()
            == Some("The file changed; showing it again.")
    });
    // Nothing was staged, and the diff is read again.
    assert_eq!(api.git(["diff", "--cached"]).unwrap(), "");
    wait_until(&mut cx, |cx| {
        with_diff(cx, &window, |diff| {
            diff.reads() > reads && !diff.loading() && diff.doc().is_some()
        })
    });
    let lines: Vec<String> = with_diff(&mut cx, &window, |diff| {
        let doc = diff.doc().unwrap();
        (0..doc.len())
            .filter_map(|at| match doc.get(at) {
                Some(gitcroc_ui::diff::Shown::Row(gitcroc_ui::diff::DiffRow::Line {
                    text,
                    ..
                })) => Some(text.to_string()),
                _ => None,
            })
            .collect()
    });
    assert!(
        lines.iter().any(|line| line == "Changed again"),
        "{lines:?}"
    );
}

#[gpui::test]
fn ui_15_hunks_have_no_controls_while_ignoring_whitespace(cx: &mut TestAppContext) {
    let fixture = Fixture::start(EVENT_CAPACITY);
    let made = two_hunks();
    let added = add(&fixture, &made);
    let (mut cx, window) = main_window(cx, &fixture);
    show_uncommitted(&mut cx, &window, &added, "api");
    show_diff(&mut cx, &window, List::Unstaged, "a.txt");
    assert!(present(&mut cx, hunk_control(0, "stage")));
    assert!(!present(&mut cx, "diff-whitespace-note".into()));

    click(&mut cx, "changes-ignore-whitespace".into());
    wait_until(&mut cx, |cx| {
        with_diff(cx, &window, |diff| {
            diff.asked().is_some_and(|asked| asked.ignore_whitespace)
                && !diff.loading()
                && diff.doc().is_some()
        })
    });
    assert!(!present(&mut cx, hunk_control(0, "stage")));
    assert!(!present(&mut cx, hunk_control(0, "discard")));
    assert_eq!(
        label_of(&mut cx, "diff-whitespace-note".into()).as_deref(),
        Some("Hunks can be staged with \"Ignore whitespace\" off.")
    );
    // The hunks are still there, without controls.
    assert_eq!(
        labels_in(&mut cx, "diff-lines".into(), gpui_kit::Role::Heading).len(),
        2
    );
}

#[gpui::test]
fn grp_9_discard_asks_and_names_the_files(cx: &mut TestAppContext) {
    let fixture = Fixture::start(EVENT_CAPACITY);
    let made = repos(&["api"]);
    let api = repo(&made, "api");
    exact_line_ends(&api);
    api.write("README.md", "changed\n").unwrap();
    let added = add(&fixture, &made);
    let (mut cx, window) = main_window(cx, &fixture);
    show_uncommitted(&mut cx, &window, &added, "api");
    wait_for_lists(&mut cx, &window, &[], &["README.md"]);

    // The dialog names the repo and the file; Cancel has the focus.
    let readme = row_of(&mut cx, &window, List::Unstaged, "README.md");
    click(&mut cx, row_part(readme, "discard"));
    wait_for_element(&mut cx, "discard-dialog".into());
    assert_eq!(
        label_of(&mut cx, "dialog-title".into()).as_deref(),
        Some("Discard changes in api?")
    );
    assert_eq!(
        label_of(&mut cx, "discard-text".into()).as_deref(),
        Some("The changes to this file are lost; they cannot be restored:")
    );
    assert_eq!(
        labels_in(&mut cx, "discard-files".into(), gpui_kit::Role::ListItem),
        ["README.md"]
    );
    assert!(focused(&mut cx, "dialog-cancel".into()));

    // Escape keeps the changes.
    press(&mut cx, "escape");
    assert!(!present(&mut cx, "discard-dialog".into()));
    assert_eq!(
        std::fs::read_to_string(api.path().join("README.md")).unwrap(),
        "changed\n"
    );

    // Discard drops them.
    click(&mut cx, row_part(readme, "discard"));
    wait_for_element(&mut cx, "discard-dialog".into());
    click(&mut cx, "dialog-submit".into());
    wait_for_lists(&mut cx, &window, &[], &[]);
    assert_eq!(
        std::fs::read_to_string(api.path().join("README.md")).unwrap(),
        "hello\n"
    );
    assert!(!present(&mut cx, "discard-dialog".into()));

    // Twelve files: ten names and a count of the rest.
    let names: Vec<String> = (1..=12).map(|n| format!("f{n:02}.txt")).collect();
    for name in &names {
        api.write(name, "x\n").unwrap();
    }
    // The clean repo had no "Uncommitted changes"; it has again.
    fixture.refresh(&fixture.active());
    wait_for_element(&mut cx, "history-uncommitted".into());
    click(&mut cx, "history-uncommitted".into());
    wait_for_view(&mut cx, &window);
    let wanted: Vec<&str> = names.iter().map(String::as_str).collect();
    wait_for_lists(&mut cx, &window, &[], &wanted);
    let first = row_of(&mut cx, &window, List::Unstaged, "f01.txt");
    click(&mut cx, row(first));
    let last = row_of(&mut cx, &window, List::Unstaged, "f12.txt");
    click_with(&mut cx, row(last), Modifiers::shift());
    click(&mut cx, row_part(last, "discard"));
    wait_for_element(&mut cx, "discard-dialog".into());
    assert_eq!(
        label_of(&mut cx, "discard-text".into()).as_deref(),
        Some("The changes to these 12 files are lost; they cannot be restored:")
    );
    let listed = labels_in(&mut cx, "discard-files".into(), gpui_kit::Role::ListItem);
    let mut sorted = listed[..10].to_vec();
    sorted.sort();
    assert_eq!(listed.len(), 11, "{listed:?}");
    assert_eq!(listed[10], "and 2 more");
    assert!(sorted.iter().all(|name| names.contains(name)), "{listed:?}");
    click(&mut cx, "dialog-cancel".into());
    assert!(!present(&mut cx, "discard-dialog".into()));
    assert!(api.path().join("f12.txt").exists());
}

#[gpui::test]
fn grp_8_blocked_repo_has_no_change_controls(cx: &mut TestAppContext) {
    let fixture = Fixture::start(EVENT_CAPACITY);
    let made = repos(&["api", "web"]);
    repo(&made, "api").write("a.txt", "a\n").unwrap();
    // Web stops in a merge with a conflict, with an untracked file.
    let web = repo(&made, "web");
    web.branch("other").unwrap();
    web.switch("other").unwrap();
    web.commit_file("README.md", "theirs\n", "Theirs").unwrap();
    web.switch("main").unwrap();
    web.commit_file("README.md", "ours\n", "Ours").unwrap();
    web.merge_with_conflicts("other").unwrap();
    web.write("w.txt", "w\n").unwrap();
    let added = add(&fixture, &made);
    let backend = group(
        &fixture,
        "Backend",
        vec![id_of(&added, "api"), id_of(&added, "web")],
    );
    let (mut cx, window) = main_window(cx, &fixture);
    enlarge(&mut cx);
    let node = group_node(&backend);
    wait_for_element(&mut cx, tree_row(&node));
    click(&mut cx, tree_row(&node));
    wait_for_element(&mut cx, "history-uncommitted".into());
    click(&mut cx, "history-uncommitted".into());
    wait_for_view(&mut cx, &window);

    let rows = with_changes(&mut cx, &window, |changes| changes.rows().to_vec());
    let file = |path: &str| {
        rows.iter()
            .position(|row| matches!(row, FileRow::File { name, .. } if name == path))
            .unwrap_or_else(|| panic!("no row {path}"))
    };
    let repo_row = |repo: &str| {
        rows.iter()
            .rposition(|row| matches!(row, FileRow::Repo { name, list: List::Unstaged, .. } if name == repo))
            .unwrap_or_else(|| panic!("no part {repo}"))
    };
    // Api's controls work; web's are disabled, with the reason.
    let (a, w) = (file("a.txt"), file("w.txt"));
    assert!(!disabled(&mut cx, row_part(a, "stage")));
    assert!(!disabled(&mut cx, row_part(repo_row("api"), "all")));
    assert!(disabled(&mut cx, row_part(w, "stage")));
    assert!(disabled(&mut cx, row_part(w, "discard")));
    assert!(disabled(&mut cx, row_part(repo_row("web"), "all")));
    assert!(
        cx.update(|window, cx| {
            window.render_frame(cx);
            window
                .try_find(row_part(repo_row("web"), "blocked"))
                .and_then(|found| found.description().map(str::to_owned))
        })
        .is_some_and(|why| !why.is_empty())
    );
    // A click on a disabled control does nothing.
    let before = jobs(&fixture, "Stage").len();
    click(&mut cx, row_part(w, "stage"));
    cx.run_until_parked();
    assert_eq!(jobs(&fixture, "Stage").len(), before);
    // Its unmerged file has none at all (OPS-3).
    let readme = rows
        .iter()
        .position(|row| matches!(row, FileRow::File { file, .. } if file.list == List::Unmerged))
        .unwrap();
    assert!(!present(&mut cx, row_part(readme, "stage")));
}

#[gpui::test]
fn ui_22_f7_reaches_a_hunk_control(cx: &mut TestAppContext) {
    let fixture = Fixture::start(EVENT_CAPACITY);
    let made = two_hunks();
    let added = add(&fixture, &made);
    let (mut cx, window) = main_window(cx, &fixture);
    show_uncommitted(&mut cx, &window, &added, "api");
    show_diff(&mut cx, &window, List::Unstaged, "a.txt");

    press(&mut cx, "f7");
    assert_eq!(with_diff(&mut cx, &window, DiffView::hunk), Some(0));
    assert!(focused(&mut cx, hunk_control(0, "stage")));
    press(&mut cx, "f7");
    assert!(focused(&mut cx, hunk_control(1, "stage")));
    press(&mut cx, "shift-f7");
    assert!(focused(&mut cx, hunk_control(0, "stage")));
    // Enter on the control stages its hunk.
    press(&mut cx, "enter");
    wait_for_lists(&mut cx, &window, &["a.txt"], &["a.txt"]);
}

#[gpui::test]
fn sec_3_discard_dialog_renders_names_as_text(cx: &mut TestAppContext) {
    let fixture = Fixture::start(EVENT_CAPACITY);
    let made = repos(&["api"]);
    let api = repo(&made, "api");
    // Windows forbids `<` and `>` in file names; an entity stays as written.
    let path = "&lt;b&gt;bold&amp;.txt";
    api.write(path, "x\n").unwrap();
    let added = add(&fixture, &made);
    let (mut cx, window) = main_window(cx, &fixture);
    show_uncommitted(&mut cx, &window, &added, "api");
    wait_for_lists(&mut cx, &window, &[], &[path]);

    let at = row_of(&mut cx, &window, List::Unstaged, path);
    click(&mut cx, row_part(at, "discard"));
    wait_for_element(&mut cx, "discard-dialog".into());
    assert_eq!(
        labels_in(&mut cx, "discard-files".into(), gpui_kit::Role::ListItem),
        [path]
    );
    click(&mut cx, "dialog-cancel".into());
}
