//! M6-19c4: the kept conflict state's life (OPS-9, UI-29): it survives
//! switching files, repos and tabs, and goes only when its conflict ended,
//! its tab was closed, or the user agreed to lose it for an update. Real
//! repos (QA-2), repo-local identity, no autocrlf.
use crate::changes::{show_repo, switch_to, wait_for_view};
use crate::conflict_banner::repos;
use crate::conflict_flow::{
    job_end, kept_unsaved, lines, mark_resolved, merge_conflict, on_disk, shown, take, unmerged,
    value, wait_for_editor,
};
use crate::history::{add, click, id_of, label_of, repo, wait_for_element, wait_until};
use crate::stage::{enlarge, focused, row_of, show_uncommitted};
use crate::support::{Fixture, Options, main_window};
use gitcroc_app::app::EVENT_CAPACITY;
use gitcroc_core::{Outcome, WorkspaceId};
use gitcroc_ui::changes::List;
use gitcroc_ui::conflict::Take;
use gitcroc_ui::strings;
use gpui_kit::test::TestWindowExt;
use gpui_kit::{Focusable, TestAppContext, VisualTestContext, point, px};

fn press(cx: &mut VisualTestContext, keys: &str) {
    crate::jobs::press(cx, keys);
}

fn secondary(key: &str) -> String {
    if cfg!(target_os = "macos") {
        format!("cmd-{key}")
    } else {
        format!("ctrl-{key}")
    }
}

/// A second workspace named `name`, holding the repos of `made`; the
/// first stays active.
fn second_workspace(
    f: &Fixture,
    name: &str,
    made: &repo_gen::Fixture,
) -> (WorkspaceId, Vec<(String, gitcroc_core::RepoId)>) {
    let first = f.active();
    let app = f.app().clone();
    let second = f.runtime.block_on(async {
        let tabs = app.workspace_create(name.to_owned()).await.unwrap();
        tabs.active.unwrap()
    });
    let added = f.add_repos(&second, made.path());
    f.runtime.block_on(app.workspace_activate(first)).unwrap();
    (second, added)
}

fn present(cx: &mut VisualTestContext, id: impl Into<gpui_kit::ElementId>) -> bool {
    let id = id.into();
    cx.update(|window, cx| {
        window.render_frame(cx);
        window.try_find(id).is_some()
    })
}

/// UI-29, OPS-9: text, cursor, selection, scroll, undo and the base card
/// survive switching files, repos and tabs; identical paths in another
/// workspace have their own editor; a file resolved elsewhere while its
/// tab is in the background goes, and the others stay.
#[gpui::test]
fn ui_29_conflict_edits_survive_file_repo_and_tab_switches(root: &mut TestAppContext) {
    let f = Fixture::start(EVENT_CAPACITY);
    let made = repos(&["api", "web"]);
    let api = repo(&made, "api");
    merge_conflict(&api, &["a.txt", "b.txt", "c.txt"]);
    let other = repos(&["api"]);
    merge_conflict(&repo(&other, "api"), &["a.txt"]);
    let added = add(&f, &made);
    let api_id = id_of(&added, "api");
    let first = f.active();
    let (second, second_added) = second_workspace(&f, "Second", &other);
    let (mut cx, window) = main_window(root, &f);
    show_uncommitted(&mut cx, &window, &added, "api");
    let open = |cx: &mut VisualTestContext, path: &str| {
        let row = row_of(cx, &window, List::Unmerged, path);
        click(cx, crate::changes::row(row));
        wait_for_editor(cx, &window, path)
    };
    // c.txt is opened, so it has kept state.
    open(&mut cx, "c.txt");
    // b.txt shows its base.
    open(&mut cx, "b.txt");
    wait_for_element(&mut cx, "conflict-0-base".into());
    click(&mut cx, "conflict-0-base".into());
    wait_for_element(&mut cx, "conflict-base-0".into());
    // a.txt takes a side, with a selection and a scroll.
    let editor = open(&mut cx, "a.txt");
    take(&mut cx, &editor, Take::Theirs);
    let text = value(&mut cx, &editor);
    cx.update(|_, cx| {
        editor.read(cx).editor.clone().update(cx, |state, cx| {
            state.set_selected_range(3..5, cx);
            state.set_scroll_offset(point(px(0.), px(-40.)), cx);
        })
    });

    // Another repo, then another tab with an api/a.txt of its own.
    show_repo(&mut cx, &window, &added, "web");
    switch_to(&mut cx, &f, &window, &second);
    show_uncommitted(&mut cx, &window, &second_added, "api");
    let theirs = wait_for_editor(&mut cx, &window, "a.txt");
    assert_ne!(theirs, editor);
    assert!(value(&mut cx, &theirs).contains("<<<<<<< "));
    // Meanwhile c.txt is resolved in a terminal.
    api.write("c.txt", lines("terminal")).unwrap();
    api.git(["add", "c.txt"]).unwrap();

    switch_to(&mut cx, &f, &window, &first);
    show_repo(&mut cx, &window, &added, "api");
    click(&mut cx, "history-uncommitted".into());
    // The rows are api's once its uncommitted changes have loaded.
    wait_for_view(&mut cx, &window);
    let again = open(&mut cx, "a.txt");
    assert_eq!(again, editor);
    assert_eq!(value(&mut cx, &again), text);
    assert_eq!(
        cx.update(|_, cx| again.read(cx).editor.read(cx).selected_range()),
        3..5
    );
    assert_eq!(
        cx.update(|_, cx| again.read(cx).editor.read(cx).scroll_offset().y),
        px(-40.)
    );
    // c.txt's state went with its conflict; the others stay.
    wait_until(&mut cx, |cx| {
        cx.update(|_, cx| {
            let pane = window.read(cx).changes(cx).unwrap();
            pane.read(cx).kept_conflict(&api_id, "c.txt").is_none()
        })
    });
    assert_eq!(kept_unsaved(&mut cx, &window, &api_id, "a.txt"), Some(true));
    // Undo still has the take.
    cx.update(|window, cx| {
        let state = again.read(cx).editor.clone();
        state.update(cx, |state, cx| state.focus(window, cx));
    });
    press(&mut cx, &secondary("z"));
    assert!(value(&mut cx, &again).contains("<<<<<<< "));
    // b.txt still shows its base.
    open(&mut cx, "b.txt");
    wait_for_element(&mut cx, "conflict-base-0".into());
}

/// OPS-9: a failed Abort keeps the edits; a successful one drops its
/// repo's; a refresh of a repo still in conflict keeps them; a terminal
/// abort drops them once a fresh summary shows it; another repo's stay.
#[gpui::test]
fn ops_9_abort_and_external_resolution_clear_only_finished_state(root: &mut TestAppContext) {
    let f = Fixture::start(EVENT_CAPACITY);
    let made = repos(&["api", "web"]);
    let api = repo(&made, "api");
    let web = repo(&made, "web");
    merge_conflict(&api, &["a.txt"]);
    merge_conflict(&web, &["a.txt"]);
    let added = add(&f, &made);
    let api_id = id_of(&added, "api");
    let web_id = id_of(&added, "web");
    let (mut cx, window) = main_window(root, &f);
    show_uncommitted(&mut cx, &window, &added, "web");
    let web_editor = wait_for_editor(&mut cx, &window, "a.txt");
    take(&mut cx, &web_editor, Take::Ours);
    show_repo(&mut cx, &window, &added, "api");
    click(&mut cx, "history-uncommitted".into());
    let editor = wait_for_editor(&mut cx, &window, "a.txt");
    take(&mut cx, &editor, Take::Ours);

    // A refresh while still in conflict keeps everything.
    f.refresh(&f.active());
    crate::support::settle(&mut cx, |_, _| true);
    assert_eq!(kept_unsaved(&mut cx, &window, &api_id, "a.txt"), Some(true));

    // A failed Abort keeps the edits.
    let lock = api.path().join(".git").join("index.lock");
    std::fs::write(&lock, "").unwrap();
    let mut events = f.app().subscribe();
    click(&mut cx, "conflict-banner-abort".into());
    assert_eq!(job_end(&mut cx, &mut events), Some(Outcome::Failed));
    f.refresh(&f.active());
    crate::support::settle(&mut cx, |_, _| true);
    assert_eq!(kept_unsaved(&mut cx, &window, &api_id, "a.txt"), Some(true));
    std::fs::remove_file(&lock).unwrap();

    // A successful Abort drops api's, not web's.
    let mut events = f.app().subscribe();
    click(&mut cx, "conflict-banner-abort".into());
    assert_eq!(job_end(&mut cx, &mut events), Some(Outcome::Succeeded));
    wait_until(&mut cx, |cx| {
        kept_unsaved(cx, &window, &api_id, "a.txt").is_none()
    });
    assert!(unmerged(&api).is_empty());
    assert_eq!(kept_unsaved(&mut cx, &window, &web_id, "a.txt"), Some(true));

    // A terminal abort of web drops its state once a fresh summary says so.
    web.git(["merge", "--abort"]).unwrap();
    f.refresh(&f.active());
    wait_until(&mut cx, |cx| {
        kept_unsaved(cx, &window, &web_id, "a.txt").is_none()
    });
}

/// UI-29: closing a tab with unsaved conflict edits asks first, naming the
/// files; Cancel keeps the tab and its exact state; Close tab closes it and
/// lets its editors go.
#[gpui::test]
fn ui_29_closing_unsaved_conflict_tab_asks_before_discarding(root: &mut TestAppContext) {
    let f = Fixture::start(EVENT_CAPACITY);
    let made = repos(&["api"]);
    let api = repo(&made, "api");
    merge_conflict(&api, &["a.txt"]);
    let added = add(&f, &made);
    let first = f.active();
    let other = repos(&["web"]);
    let _ = second_workspace(&f, "Second", &other);
    let (mut cx, window) = main_window(root, &f);
    show_uncommitted(&mut cx, &window, &added, "api");
    let editor = wait_for_editor(&mut cx, &window, "a.txt");
    take(&mut cx, &editor, Take::Theirs);
    let text = value(&mut cx, &editor);
    let close = gpui_kit::ElementId::Name(format!("tab-close-{}", first.0).into());

    click(&mut cx, close.clone());
    wait_for_element(&mut cx, "question".into());
    assert!(focused(&mut cx, "dialog-cancel".into()));
    let said = crate::history::labels_in(&mut cx, "question".into(), gpui_kit::Role::Paragraph);
    assert!(
        said.iter()
            .any(|line| line == strings::workspaces::UNSAVED_LOST),
        "{said:?}"
    );
    assert!(said.iter().any(|line| line == "api: a.txt"), "{said:?}");
    click(&mut cx, "dialog-cancel".into());
    assert!(cx.update(|_, cx| {
        window
            .read(cx)
            .tabs(cx)
            .open
            .iter()
            .any(|tab| tab.id == first)
    }));
    assert_eq!(shown(&mut cx, &window), Some(editor.clone()));
    assert_eq!(value(&mut cx, &editor), text);

    let weak = editor.downgrade();
    drop(editor);
    click(&mut cx, close);
    wait_for_element(&mut cx, "question".into());
    click(&mut cx, "dialog-submit".into());
    wait_until(&mut cx, |cx| {
        !cx.update(|_, cx| {
            window
                .read(cx)
                .tabs(cx)
                .open
                .iter()
                .any(|tab| tab.id == first)
        })
    });
    crate::support::settle(&mut cx, |_, _| true);
    assert!(cx.update(|_, cx| window.read(cx).changes_of(&first).is_none()));
    assert!(weak.upgrade().is_none());
    assert!(unmerged(&api).contains("a.txt"));
}

/// OPS-9, PLAT-4: Install and restart asks first when any tab has unsaved
/// conflict edits, naming workspace, repo and file; Cancel neither installs
/// nor restarts and keeps the edits; only the explicit answer runs the
/// updater's install and restart.
#[gpui::test]
fn ops_9_update_restart_asks_about_unsaved_edits_in_all_tabs(root: &mut TestAppContext) {
    let mut restarted = root.expect_restart();
    let f = Fixture::start_with(Options {
        updates: Some(crate::update::offer()),
        ..Options::default()
    });
    let made = repos(&["api"]);
    merge_conflict(&repo(&made, "api"), &["a.txt"]);
    let added = add(&f, &made);
    let name = f.app().workspace_get().open[0].name.clone();
    let other = repos(&["web"]);
    let (second, _) = second_workspace(&f, "Second", &other);
    let (mut cx, window) = main_window(root, &f);
    enlarge(&mut cx);
    show_uncommitted(&mut cx, &window, &added, "api");
    let editor = wait_for_editor(&mut cx, &window, "a.txt");
    take(&mut cx, &editor, Take::Theirs);
    let text = value(&mut cx, &editor);
    // Another tab, made active: the question names files of every tab.
    wait_for_element(&mut cx, crate::history::tab(&second));
    switch_to(&mut cx, &f, &window, &second);
    f.app().update_check_now();
    wait_until(&mut cx, |cx| {
        crate::jobs::label_of(cx, "update-notice").is_some()
    });

    click(&mut cx, "update-notice".into());
    click(&mut cx, "update-install".into());
    wait_for_element(&mut cx, "update-unsaved".into());
    let said =
        crate::history::labels_in(&mut cx, "update-dialog".into(), gpui_kit::Role::Paragraph);
    assert!(
        said.iter()
            .any(|line| line == strings::update::UNSAVED_LOST),
        "{said:?}"
    );
    assert!(
        said.iter()
            .any(|line| *line == format!("{name}: api: a.txt")),
        "{said:?}"
    );
    assert!(focused(&mut cx, "update-cancel".into()));
    click(&mut cx, "update-cancel".into());
    crate::support::settle(&mut cx, |_, _| true);
    assert!(!matches!(restarted.try_recv(), Ok(Some(_))));
    assert_eq!(value(&mut cx, &editor), text);

    click(&mut cx, "update-notice".into());
    click(&mut cx, "update-install".into());
    wait_for_element(&mut cx, "update-anyway".into());
    click(&mut cx, "update-anyway".into());
    wait_until(&mut cx, |_| matches!(restarted.try_recv(), Ok(Some(_))));
}

/// OPS-7, GRP-9: a conflicting pop resolves through the conflict editor;
/// once the stash apply ends, the Keep/Drop offer of the kept stash shows,
/// and Drop asks for the core's confirmation.
#[gpui::test]
fn ops_7_conflicting_pop_offer_follows_resolution(root: &mut TestAppContext) {
    use crate::history::{tree_part, tree_row};
    let f = Fixture::start(EVENT_CAPACITY);
    let made = repos(&["api"]);
    let r = repo(&made, "api");
    r.write("README.md", "stashed\n").unwrap();
    r.git(["stash", "push", "-m", "saved"]).unwrap();
    r.commit_file("README.md", "conflicting\n", "Conflict with stash")
        .unwrap();
    let added = add(&f, &made);
    let (mut cx, window) = main_window(root, &f);
    enlarge(&mut cx);
    crate::changes::show_repo(&mut cx, &window, &added, "api");
    let node = format!("repo:{}", added[0].1.0);
    click(&mut cx, tree_part(&node, "chevron"));
    click(&mut cx, tree_part(&format!("{node}:stashes"), "chevron"));
    let row = tree_row(&format!("{node}:stashes:ref:0"));
    wait_for_element(&mut cx, row.clone());
    let mut events = f.app().subscribe();
    cx.update(|window, cx| window.right_click(row, cx));
    cx.run_until_parked();
    click(
        &mut cx,
        gpui_kit::ElementId::NamedInteger("menu-item".into(), 1),
    );
    assert_eq!(job_end(&mut cx, &mut events), Some(Outcome::Conflict));
    crate::changes::show_repo(&mut cx, &window, &added, "api");
    wait_for_element(&mut cx, "history-uncommitted".into());
    click(&mut cx, "history-uncommitted".into());
    let editor = wait_for_editor(&mut cx, &window, "README.md");
    take(&mut cx, &editor, Take::Theirs);
    let offer: gpui_kit::ElementId = format!("stash-offer-{}", added[0].1.0).into();
    assert!(!present(&mut cx, offer.clone()));
    assert_eq!(mark_resolved(&f, &mut cx), Some(Outcome::Succeeded));
    assert_eq!(on_disk(&r, "README.md"), "stashed\n");
    wait_until(&mut cx, |cx| {
        label_of(cx, "conflict-banner-text".into()).is_none()
    });
    wait_for_element(&mut cx, offer.clone());
    click(&mut cx, format!("stash-offer-drop-{}", added[0].1.0).into());
    wait_for_element(&mut cx, "op-confirm-actions".into());
    let mut events = f.app().subscribe();
    click(&mut cx, "dialog-submit".into());
    assert_eq!(job_end(&mut cx, &mut events), Some(Outcome::Succeeded));
    assert!(r.git(["stash", "list"]).unwrap().is_empty());
}

/// UI-22: two files are resolved and the merge continued from the
/// keyboard alone: Enter on the unmerged row moves into its editor,
/// Alt+Down and Alt+1 take a side, Ctrl or Cmd and Enter marks it resolved
/// and the next file's editor takes the focus; Continue is reached with
/// F6 and Tab.
#[gpui::test]
fn ui_22_resolve_two_files_and_continue_using_only_the_keyboard(root: &mut TestAppContext) {
    let f = Fixture::start(EVENT_CAPACITY);
    let made = repos(&["api"]);
    let r = repo(&made, "api");
    merge_conflict(&r, &["a.txt", "b.txt"]);
    let added = add(&f, &made);
    let (mut cx, window) = main_window(root, &f);
    show_uncommitted(&mut cx, &window, &added, "api");
    wait_for_editor(&mut cx, &window, "a.txt");
    // Into the changes pane's list.
    cx.update(|window_, cx| {
        let pane = window.read(cx).changes(cx).unwrap();
        window_.focus(&pane.focus_handle(cx), cx);
    });
    press(&mut cx, "enter");
    for path in ["a.txt", "b.txt"] {
        let editor = wait_for_editor(&mut cx, &window, path);
        wait_until(&mut cx, |cx| {
            cx.update(|window, cx| editor.read(cx).editor.focus_handle(cx).is_focused(window))
        });
        let mut events = f.app().subscribe();
        press(&mut cx, "alt-down");
        press(&mut cx, "alt-1");
        press(&mut cx, &secondary("enter"));
        assert_eq!(job_end(&mut cx, &mut events), Some(Outcome::Succeeded));
    }
    assert!(unmerged(&r).is_empty());
    wait_until(&mut cx, |cx| {
        !cx.update(|window, cx| {
            window.render_frame(cx);
            window.find("conflict-banner-continue").disabled() == Some(true)
        })
    });
    // F6 to the history, then Tab to Continue.
    let history = cx.update(|_, cx| window.read(cx).history(cx).unwrap().focus_handle(cx));
    for _ in 0..8 {
        if cx.update(|window, _| history.is_focused(window)) {
            break;
        }
        press(&mut cx, "f6");
    }
    assert!(cx.update(|window, _| history.is_focused(window)));
    for _ in 0..4 {
        if focused(&mut cx, "conflict-banner-continue".into()) {
            break;
        }
        press(&mut cx, "tab");
    }
    assert!(focused(&mut cx, "conflict-banner-continue".into()));
    let mut events = f.app().subscribe();
    press(&mut cx, "enter");
    // The immediate dialog sends it; its job commits the merge.
    assert_eq!(job_end(&mut cx, &mut events), Some(Outcome::Succeeded));
    assert!(
        r.git(["rev-parse", "-q", "--verify", "MERGE_HEAD"])
            .is_err()
    );
    assert_eq!(on_disk(&r, "b.txt"), lines("main"));
}
