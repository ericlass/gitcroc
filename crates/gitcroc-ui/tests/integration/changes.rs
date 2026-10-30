//! The changes pane (UI-14, UI-16): a commit's header and files, flat or
//! as a tree, a merge commit's parents, and the uncommitted lists of a repo
//! (UI-15) or, sectioned by repo, of a group (UI-34). It follows the
//! history's selection, reads the working copy again when it changes
//! (OPS-7) and is kept per tab (UI-29). Repos are real ones made by
//! `repo-gen` and added through the app (QA-2).

use gitcroc_app::app::EVENT_CAPACITY;
use std::time::Duration;

use gitcroc_core::{ChangesContent, FileListMode, RepoId, WorkspaceId};
use gitcroc_ui::changes::{Changes, FileRow};
use gitcroc_ui::history::Selection;
use gitcroc_ui::window::GitcrocWindow;
use gpui_kit::test::TestWindowExt;
use gpui_kit::{
    ElementId, Entity, Role, ScrollDelta, TestAppContext, VisualTestContext, point, px, size,
};

use crate::history::{
    add, click, id_of, label_of, labels_in, press, repo, repo_node, repos, tab, tree_row,
    wait_for_element, wait_until, with_history,
};
use crate::history_merged::{group, group_node, present};
use crate::history_search::selection;
use crate::support::{Fixture, main_window, settle};

fn name(id: String) -> ElementId {
    ElementId::Name(id.into())
}

pub(crate) fn history_row(index: usize) -> ElementId {
    name(format!("history-row-{index}"))
}

pub(crate) fn row(index: usize) -> ElementId {
    name(format!("changes-row-{index}"))
}

fn row_part(index: usize, part: &str) -> ElementId {
    name(format!("changes-row-{index}-{part}"))
}

/// Widens the window from 1200 to 1400 pixels. At 1200 the changes pane
/// is 352 wide, its header wraps to four lines, and only a few file rows
/// are on screen.
fn widen(cx: &mut VisualTestContext) {
    cx.simulate_resize(size(px(1400.), px(800.)));
    cx.run_until_parked();
}

/// Runs `f` on the active tab's changes pane.
pub(crate) fn with_changes<R>(
    cx: &mut VisualTestContext,
    window: &Entity<GitcrocWindow>,
    f: impl FnOnce(&Changes) -> R,
) -> R {
    cx.update(|_, cx| {
        let pane = window.read(cx).changes(cx).expect("no changes pane");
        let pane = pane.read(cx);
        f(pane.changes())
    })
}

/// Waits until the changes pane shows a view read for the history's
/// selection.
#[track_caller]
pub(crate) fn wait_for_view(cx: &mut VisualTestContext, window: &Entity<GitcrocWindow>) {
    wait_until(cx, |cx| {
        let shown = selection(cx, window);
        with_changes(cx, window, |changes| {
            shown.is_some()
                && changes.selection() == shown.as_ref()
                && changes.view().is_some()
                && !changes.loading()
        })
    });
}

/// The hash of the commit the changes pane shows.
fn shown_hash(cx: &mut VisualTestContext, window: &Entity<GitcrocWindow>) -> Option<String> {
    with_changes(cx, window, |changes| {
        match changes.view().map(|view| &view.content) {
            Some(ChangesContent::Commit { details, .. }) => Some(details.hash.clone()),
            _ => None,
        }
    })
}

/// The file rows on screen, by name.
fn files(cx: &mut VisualTestContext) -> Vec<String> {
    labels_in(cx, "changes-files".into(), Role::TreeItem)
}

/// The section and repo headings on screen.
fn headings(cx: &mut VisualTestContext) -> Vec<String> {
    labels_in(cx, "changes-files".into(), Role::Heading)
}

/// The index of the row named `label`.
pub(crate) fn index_of(
    cx: &mut VisualTestContext,
    window: &Entity<GitcrocWindow>,
    label: &str,
) -> usize {
    with_changes(cx, window, |changes| {
        changes.rows().iter().position(|row| match row {
            FileRow::File { name, .. } | FileRow::Folder { name, .. } => name == label,
            _ => false,
        })
    })
    .unwrap_or_else(|| panic!("no row {label}"))
}

/// The selected file's path, as the diff area names it.
pub(crate) fn shown_file(cx: &mut VisualTestContext) -> Option<String> {
    label_of(cx, "changes-file".into())
}

fn selected(cx: &mut VisualTestContext, id: ElementId) -> Option<bool> {
    cx.update(|window, cx| {
        window.render_frame(cx);
        window.try_find(id).and_then(|found| found.selected())
    })
}

/// Whether toggle button `id` is pressed, as a screen reader reads it.
pub(crate) fn pressed(cx: &mut VisualTestContext, id: ElementId) -> Option<bool> {
    cx.update(|window, cx| {
        window.render_frame(cx);
        window.try_find(id).and_then(|found| found.checked())
    })
}

/// Selects repo `name`'s row in the tree and waits for its history.
#[track_caller]
pub(crate) fn show_repo(
    cx: &mut VisualTestContext,
    window: &Entity<GitcrocWindow>,
    added: &[(String, RepoId)],
    name: &str,
) {
    let node = repo_node(added, name);
    wait_for_element(cx, tree_row(&node));
    click(cx, tree_row(&node));
    for _ in 0..1000 {
        settle(cx, |_, _| true);
        let shown = with_history(cx, window, |history| {
            history.source().is_some_and(|source| source.key == node)
                && !history.loading()
                && !history.rows().is_empty()
        });
        if shown {
            return;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    // Where it stopped, to tell a lost selection from a failed read.
    let tree = cx.update(|_, cx| {
        let tree = window.read(cx).tree(cx).expect("no tree pane");
        tree.read(cx).tree().selected().map(str::to_owned)
    });
    let history = with_history(cx, window, |history| {
        format!(
            "source {:?}, loading {}, {} rows, error {:?}",
            history.source().map(|source| source.key.clone()),
            history.loading(),
            history.rows().len(),
            history.error()
        )
    });
    panic!("{node} never showed its history: tree selects {tree:?}; {history}");
}

/// Switches to tab `id` and waits until the core has it active and the
/// window shows its panes. Repo ids repeat across workspaces, so a row
/// clicked before then can be the old tab's row of the same id.
pub(crate) fn switch_to(
    cx: &mut VisualTestContext,
    fixture: &Fixture,
    window: &Entity<GitcrocWindow>,
    id: &WorkspaceId,
) {
    click(cx, tab(id));
    wait_until(cx, |cx| {
        fixture.active() == *id
            && cx.update(|_, cx| {
                let shown = window.read(cx);
                shown.tabs(cx).active.as_ref() == Some(id) && shown.tree(cx).is_some()
            })
    });
}

#[gpui::test]
fn ui_14_commit_header_and_file_list(cx: &mut TestAppContext) {
    let fixture = Fixture::start(EVENT_CAPACITY);
    let made = repos(&["api"]);
    let api = repo(&made, "api");
    api.write("old.txt", "old\n").unwrap();
    let base = api.commit_all("Base").unwrap();
    api.write("README.md", "hello\nworld\n").unwrap();
    api.write("src/new.rs", "fn main() {}\n").unwrap();
    api.git(["mv", "old.txt", "renamed.txt"]).unwrap();
    let change = api.commit_all("Change files\n\nWith a body.").unwrap();
    let added = add(&fixture, &made);
    let (mut cx, window) = main_window(cx, &fixture);
    widen(&mut cx);
    assert_eq!(
        label_of(&mut cx, "changes-empty".into()).as_deref(),
        Some("Select a commit or the uncommitted changes to see what changed.")
    );
    show_repo(&mut cx, &window, &added, "api");

    click(&mut cx, history_row(0));
    wait_for_view(&mut cx, &window);
    assert_eq!(
        label_of(&mut cx, "changes-commit-hash".into()),
        Some(format!("Commit: {change}"))
    );
    let author = label_of(&mut cx, "changes-author".into()).unwrap();
    assert!(
        author.starts_with(&format!(
            "Author: {} <{}>  ",
            repo_gen::FIXTURE_NAME,
            repo_gen::FIXTURE_EMAIL
        )),
        "{author}"
    );
    assert!(
        label_of(&mut cx, "changes-committer".into())
            .unwrap()
            .starts_with("Committer: ")
    );
    assert_eq!(
        label_of(&mut cx, "changes-message".into()).as_deref(),
        Some("Change files\n\nWith a body.")
    );
    assert_eq!(
        label_of(&mut cx, "changes-parent-0".into()),
        Some(format!("Select {base}"))
    );
    assert!(!present(&mut cx, "changes-compare".into()));
    assert_eq!(
        label_of(&mut cx, "changes-file-count".into()).as_deref(),
        Some("3 files")
    );

    // Each file with its status, the rename's old path and line counts.
    let mut shown = files(&mut cx);
    shown.sort();
    assert_eq!(shown, ["README.md", "renamed.txt", "src/new.rs"]);
    let readme = index_of(&mut cx, &window, "README.md");
    assert_eq!(
        label_of(&mut cx, row_part(readme, "status")).as_deref(),
        Some("Modified")
    );
    assert_eq!(
        label_of(&mut cx, row_part(readme, "counts")).as_deref(),
        Some("1 line added, 0 removed")
    );
    let renamed = index_of(&mut cx, &window, "renamed.txt");
    assert_eq!(
        label_of(&mut cx, row_part(renamed, "status")).as_deref(),
        Some("Renamed")
    );
    assert_eq!(
        label_of(&mut cx, row_part(renamed, "old")).as_deref(),
        Some("Renamed from old.txt")
    );
    let new = index_of(&mut cx, &window, "src/new.rs");
    assert_eq!(
        label_of(&mut cx, row_part(new, "status")).as_deref(),
        Some("Added")
    );

    // The first file is selected and named over the diff.
    let first = with_changes(&mut cx, &window, |changes| changes.selected_index());
    assert_eq!(first, Some(0));
    assert_eq!(selected(&mut cx, row(0)), Some(true));
    let first_name = files(&mut cx).remove(0);
    let expected = if first_name == "renamed.txt" {
        "old.txt → renamed.txt".to_owned()
    } else {
        first_name
    };
    assert_eq!(shown_file(&mut cx), Some(expected));

    // A click selects another file.
    click(&mut cx, row(new));
    assert_eq!(shown_file(&mut cx).as_deref(), Some("src/new.rs"));
    assert_eq!(selected(&mut cx, row(new)), Some(true));

    // The parent link selects the parent in the history.
    click(&mut cx, "changes-parent-0".into());
    wait_for_view(&mut cx, &window);
    assert_eq!(shown_hash(&mut cx, &window), Some(base.clone()));
    let api_id = id_of(&added, "api");
    assert_eq!(
        selection(&mut cx, &window),
        Some(Selection::Commit {
            repo: api_id,
            hash: base
        })
    );
}

#[gpui::test]
fn ui_14_file_list_flat_and_tree(cx: &mut TestAppContext) {
    let fixture = Fixture::start(EVENT_CAPACITY);
    let mine = fixture.active();
    let made = repos(&["api"]);
    let api = repo(&made, "api");
    api.write("src/util/deep/x.rs", "x\n").unwrap();
    api.write("src/b.rs", "b\n").unwrap();
    api.write("README.md", "changed\n").unwrap();
    api.commit_all("Change").unwrap();
    let added = add(&fixture, &made);
    // A second workspace, for the setting's reach.
    let app = fixture.app().clone();
    let java = fixture.runtime.block_on(async {
        let tabs = app.workspace_create("Java".to_owned()).await.unwrap();
        tabs.active.unwrap()
    });
    let other = repos(&["core"]);
    repo(&other, "core")
        .commit_file("lib/core.rs", "c\n", "Core")
        .unwrap();
    let other_added = fixture.add_repos(&java, other.path());
    fixture
        .runtime
        .block_on(app.workspace_activate(mine.clone()))
        .unwrap();

    let (mut cx, window) = main_window(cx, &fixture);
    widen(&mut cx);
    show_repo(&mut cx, &window, &added, "api");
    click(&mut cx, history_row(0));
    wait_for_view(&mut cx, &window);
    assert_eq!(pressed(&mut cx, "changes-flat".into()), Some(true));
    let mut flat = files(&mut cx);
    flat.sort();
    assert_eq!(flat, ["README.md", "src/b.rs", "src/util/deep/x.rs"]);

    // The tree: folders first, a single-child folder as one row.
    click(&mut cx, "changes-tree".into());
    assert_eq!(pressed(&mut cx, "changes-tree".into()), Some(true));
    assert_eq!(
        files(&mut cx),
        ["src", "util/deep", "x.rs", "b.rs", "README.md"]
    );
    wait_until(&mut cx, |_| {
        app.settings_get().settings.changes.file_list == FileListMode::Tree
    });

    // A folder collapses and expands.
    let src = index_of(&mut cx, &window, "src");
    let expanded = |cx: &mut VisualTestContext| {
        cx.update(|window, cx| {
            window.render_frame(cx);
            window.find(row(src)).expanded()
        })
    };
    assert_eq!(expanded(&mut cx), Some(true));
    click(&mut cx, row(src));
    assert_eq!(files(&mut cx), ["src", "README.md"]);
    assert_eq!(expanded(&mut cx), Some(false));
    click(&mut cx, row(src));
    assert_eq!(files(&mut cx).len(), 5);

    // The keys move between files, past the folders.
    let x = index_of(&mut cx, &window, "x.rs");
    click(&mut cx, row(x));
    press(&mut cx, "down");
    assert_eq!(shown_file(&mut cx).as_deref(), Some("src/b.rs"));
    press(&mut cx, "end");
    assert_eq!(shown_file(&mut cx).as_deref(), Some("README.md"));
    press(&mut cx, "home");
    assert_eq!(shown_file(&mut cx).as_deref(), Some("src/util/deep/x.rs"));
    press(&mut cx, "up");
    assert_eq!(shown_file(&mut cx).as_deref(), Some("src/util/deep/x.rs"));

    // The other tab lists as a tree too.
    switch_to(&mut cx, &fixture, &window, &java);
    show_repo(&mut cx, &window, &other_added, "core");
    click(&mut cx, history_row(0));
    wait_for_view(&mut cx, &window);
    assert_eq!(files(&mut cx), ["lib", "core.rs"]);
    assert_eq!(pressed(&mut cx, "changes-tree".into()), Some(true));
}

#[gpui::test]
fn ui_16_merge_commit_parent_selector(cx: &mut TestAppContext) {
    let fixture = Fixture::start(EVENT_CAPACITY);
    let made = repos(&["api"]);
    let api = repo(&made, "api");
    api.branch("feature").unwrap();
    api.switch("feature").unwrap();
    api.commit_file("feature.txt", "f\n", "Feature").unwrap();
    api.switch("main").unwrap();
    let main_head = api.commit_file("main.txt", "m\n", "Main").unwrap();
    let merge = api.merge("feature", "Merge feature").unwrap();
    let added = add(&fixture, &made);
    let (mut cx, window) = main_window(cx, &fixture);
    show_repo(&mut cx, &window, &added, "api");

    click(&mut cx, history_row(0));
    wait_for_view(&mut cx, &window);
    assert_eq!(shown_hash(&mut cx, &window), Some(merge));
    assert!(present(&mut cx, "changes-compare".into()));
    assert_eq!(pressed(&mut cx, "changes-compare-0".into()), Some(true));
    assert_eq!(pressed(&mut cx, "changes-compare-1".into()), Some(false));
    // Against the first parent, the merge brought in the feature's file.
    assert_eq!(files(&mut cx), ["feature.txt"]);

    click(&mut cx, "changes-compare-1".into());
    wait_until(&mut cx, |cx| {
        with_changes(cx, &window, |changes| {
            changes.parent() == 1 && !changes.loading()
        })
    });
    assert_eq!(files(&mut cx), ["main.txt"]);
    assert_eq!(pressed(&mut cx, "changes-compare-1".into()), Some(true));

    // A commit with one parent has no selector.
    let main_row = cx.update(|_, cx| {
        let pane = window.read(cx).history(cx).unwrap();
        let history = pane.read(cx).history();
        history
            .rows()
            .iter()
            .position(|row| row.hash == main_head)
            .unwrap()
    });
    click(&mut cx, history_row(main_row));
    wait_for_view(&mut cx, &window);
    assert_eq!(shown_hash(&mut cx, &window), Some(main_head));
    assert!(!present(&mut cx, "changes-compare".into()));
}

#[gpui::test]
fn ui_15_uncommitted_lists_staged_unstaged_and_unmerged(cx: &mut TestAppContext) {
    let fixture = Fixture::start(EVENT_CAPACITY);
    let made = repos(&["api", "web"]);
    let api = repo(&made, "api");
    api.write("staged.txt", "s\n").unwrap();
    api.git(["add", "staged.txt"]).unwrap();
    api.write("README.md", "changed\n").unwrap();
    api.write("untracked.txt", "u\n").unwrap();
    // Web stops in a merge with a conflict.
    let web = repo(&made, "web");
    web.branch("other").unwrap();
    web.switch("other").unwrap();
    web.commit_file("README.md", "theirs\n", "Theirs").unwrap();
    web.switch("main").unwrap();
    web.commit_file("README.md", "ours\n", "Ours").unwrap();
    web.merge_with_conflicts("other").unwrap();
    let added = add(&fixture, &made);
    let (mut cx, window) = main_window(cx, &fixture);

    show_repo(&mut cx, &window, &added, "api");
    wait_for_element(&mut cx, "history-uncommitted".into());
    click(&mut cx, "history-uncommitted".into());
    wait_for_view(&mut cx, &window);
    assert_eq!(
        label_of(&mut cx, "changes-title".into()).as_deref(),
        Some("Uncommitted changes")
    );
    assert_eq!(headings(&mut cx), ["Staged (1)", "Unstaged (2)"]);
    let shown = files(&mut cx);
    assert_eq!(shown[0], "staged.txt");
    let mut unstaged = shown[1..].to_vec();
    unstaged.sort();
    assert_eq!(unstaged, ["README.md", "untracked.txt"]);
    let untracked = index_of(&mut cx, &window, "untracked.txt");
    assert_eq!(
        label_of(&mut cx, row_part(untracked, "status")).as_deref(),
        Some("Untracked")
    );
    assert_eq!(shown_file(&mut cx).as_deref(), Some("staged.txt"));

    // In a conflict, Unmerged leads, by full path.
    show_repo(&mut cx, &window, &added, "web");
    wait_for_element(&mut cx, "history-uncommitted".into());
    click(&mut cx, "history-uncommitted".into());
    wait_for_view(&mut cx, &window);
    assert_eq!(headings(&mut cx)[0], "Unmerged (1)");
    assert_eq!(files(&mut cx)[0], "README.md");
    assert_eq!(
        label_of(&mut cx, row_part(1, "status")).as_deref(),
        Some("Conflict")
    );
    assert_eq!(
        label_of(&mut cx, row_part(1, "conflict")).as_deref(),
        Some("Conflict: both modified")
    );
}

#[gpui::test]
fn ui_34_merged_lists_are_sectioned_by_repo(cx: &mut TestAppContext) {
    let fixture = Fixture::start(EVENT_CAPACITY);
    let made = repos(&["api", "docs", "web"]);
    let api = repo(&made, "api");
    api.write("a.txt", "a\n").unwrap();
    api.git(["add", "a.txt"]).unwrap();
    api.write("b.txt", "b\n").unwrap();
    repo(&made, "web").write("w.txt", "w\n").unwrap();
    let added = add(&fixture, &made);
    let backend = group(
        &fixture,
        "Backend",
        vec![
            id_of(&added, "api"),
            id_of(&added, "docs"),
            id_of(&added, "web"),
        ],
    );
    let (mut cx, window) = main_window(cx, &fixture);
    let node = group_node(&backend);
    wait_for_element(&mut cx, tree_row(&node));
    click(&mut cx, tree_row(&node));
    wait_for_element(&mut cx, "history-uncommitted".into());
    click(&mut cx, "history-uncommitted".into());
    wait_for_view(&mut cx, &window);

    // Each section has a part per dirty repo, in target order; the clean
    // repo has none.
    assert_eq!(
        headings(&mut cx),
        ["Staged (1)", "api", "Unstaged (2)", "api", "web"]
    );
    assert_eq!(files(&mut cx), ["a.txt", "b.txt", "w.txt"]);
    assert_eq!(
        label_of(&mut cx, "changes-file-count".into()).as_deref(),
        Some("3 files")
    );
    assert_eq!(shown_file(&mut cx).as_deref(), Some("a.txt"));
}

#[gpui::test]
fn ops_7_changes_read_again_when_the_working_copy_changes(cx: &mut TestAppContext) {
    let fixture = Fixture::start(EVENT_CAPACITY);
    let made = repos(&["api"]);
    let api = repo(&made, "api");
    api.write("README.md", "changed\n").unwrap();
    api.write("b.txt", "b\n").unwrap();
    let added = add(&fixture, &made);
    let (mut cx, window) = main_window(cx, &fixture);
    show_repo(&mut cx, &window, &added, "api");
    wait_for_element(&mut cx, "history-uncommitted".into());
    click(&mut cx, "history-uncommitted".into());
    wait_for_view(&mut cx, &window);
    let b = index_of(&mut cx, &window, "b.txt");
    click(&mut cx, row(b));
    assert_eq!(shown_file(&mut cx).as_deref(), Some("b.txt"));

    // A new file in the working copy, read on the next refresh.
    api.write("c.txt", "c\n").unwrap();
    fixture.refresh(&fixture.active());
    wait_until(&mut cx, |cx| files(cx).contains(&"c.txt".to_owned()));
    assert_eq!(shown_file(&mut cx).as_deref(), Some("b.txt"));

    // Staged whole, the file is still selected, now in Staged.
    api.git(["add", "b.txt"]).unwrap();
    fixture.refresh(&fixture.active());
    wait_until(&mut cx, |cx| {
        headings(cx).first().map(String::as_str) == Some("Staged (1)")
    });
    assert_eq!(shown_file(&mut cx).as_deref(), Some("b.txt"));
    assert_eq!(
        with_changes(&mut cx, &window, |changes| changes.selected_index()),
        Some(1)
    );
}

#[gpui::test]
fn ui_29_each_tab_keeps_its_changes(cx: &mut TestAppContext) {
    let fixture = Fixture::start(EVENT_CAPACITY);
    let mine = fixture.active();
    let made = repos(&["api"]);
    let api = repo(&made, "api");
    for n in 0..80 {
        api.write(&format!("f{n:02}.txt"), "x\n").unwrap();
    }
    api.commit_all("Many files").unwrap();
    let added = add(&fixture, &made);
    let app = fixture.app().clone();
    let java = fixture.runtime.block_on(async {
        let tabs = app.workspace_create("Java".to_owned()).await.unwrap();
        tabs.active.unwrap()
    });
    fixture
        .runtime
        .block_on(app.workspace_activate(mine.clone()))
        .unwrap();
    let (mut cx, window) = main_window(cx, &fixture);
    show_repo(&mut cx, &window, &added, "api");
    click(&mut cx, history_row(0));
    wait_for_view(&mut cx, &window);

    // Scroll down and select a file there.
    cx.update(|window, cx| {
        window.scroll(
            "changes-files",
            ScrollDelta::Pixels(point(px(0.), px(-30. * 22.))),
            cx,
        );
        window.render_frame(cx);
    });
    cx.run_until_parked();
    let on_screen = files(&mut cx);
    assert!(
        !on_screen.contains(&"f00.txt".to_owned()),
        "the list did not scroll: {on_screen:?}"
    );
    let pick = on_screen[on_screen.len() / 2].clone();
    let at = index_of(&mut cx, &window, &pick);
    click(&mut cx, row(at));
    let state = |cx: &mut VisualTestContext| {
        let selection = with_changes(cx, &window, |changes| changes.selection().cloned());
        (selection, shown_file(cx), files(cx))
    };
    let before = state(&mut cx);
    assert_eq!(before.1.as_deref(), Some(pick.as_str()));

    // The other tab has nothing selected.
    click(&mut cx, tab(&java));
    wait_for_element(&mut cx, "changes-empty".into());

    // Back, the changes pane is as it was left.
    click(&mut cx, tab(&mine));
    wait_for_element(&mut cx, "changes-files".into());
    assert_eq!(state(&mut cx), before);
}

#[gpui::test]
fn perf_1_the_file_list_lays_out_only_the_rows_on_screen(cx: &mut TestAppContext) {
    let fixture = Fixture::start(EVENT_CAPACITY);
    let made = repos(&["api"]);
    let api = repo(&made, "api");
    for n in 0..500 {
        api.write(&format!("f{n:03}.txt"), "x\n").unwrap();
    }
    api.commit_all("Many files").unwrap();
    let added = add(&fixture, &made);
    let (mut cx, window) = main_window(cx, &fixture);
    show_repo(&mut cx, &window, &added, "api");
    click(&mut cx, history_row(0));
    wait_for_view(&mut cx, &window);
    assert_eq!(
        with_changes(&mut cx, &window, |changes| changes.rows().len()),
        500
    );
    let laid_out = files(&mut cx).len();
    assert!(
        (1..100).contains(&laid_out),
        "{laid_out} rows laid out for 500"
    );
}

#[gpui::test]
fn sec_3_commit_text_and_paths_render_as_text(cx: &mut TestAppContext) {
    let fixture = Fixture::start(EVENT_CAPACITY);
    let made = repos(&["api"]);
    let api = repo(&made, "api");
    // Windows forbids `<` and `>` in file names; an entity stays as written.
    let path = "&lt;b&gt;bold&amp;.txt";
    let message = "<img src=x> & message\n\n<script>alert(1)</script>";
    api.write(path, "x\n").unwrap();
    api.commit_all(message).unwrap();
    let added = add(&fixture, &made);
    let (mut cx, window) = main_window(cx, &fixture);
    show_repo(&mut cx, &window, &added, "api");
    click(&mut cx, history_row(0));
    wait_for_view(&mut cx, &window);

    assert_eq!(
        label_of(&mut cx, "changes-message".into()).as_deref(),
        Some(message)
    );
    assert_eq!(files(&mut cx), [path]);
    assert_eq!(shown_file(&mut cx).as_deref(), Some(path));
}
