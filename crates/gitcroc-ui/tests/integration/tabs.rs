//! The workspace tabs (UI-27, UI-28): the tabs follow the core, and "+",
//! the tab menu, the keys and the dialogs send the changes the core then
//! makes. Workspaces are created through the app, as another window or an
//! earlier run would have left them.

use std::rc::Rc;

use gitcroc_core::{SETTINGS_SCHEMA, WorkspaceId};
use gitcroc_ui::workspaces::Workspaces;
use gpui_kit::test::{TestQueryExt, TestWindowExt};
use gpui_kit::{AppContext, ElementId, Role, TestAppContext, VisualTestContext, point, px};

use crate::support::{
    Asked, FakeControls, FakeGit, Fixture, Options, main_window, opened_main_window_with,
    settings_with_git, settle,
};

/// `keys` with Cmd on macOS and Ctrl elsewhere.
fn secondary(keys: &str) -> String {
    let modifier = if cfg!(target_os = "macos") {
        "cmd"
    } else {
        "ctrl"
    };
    format!("{modifier}-{keys}")
}

fn press(cx: &mut VisualTestContext, keys: &str) {
    cx.update(|window, cx| window.press(keys, cx));
    cx.run_until_parked();
}

/// Creates workspaces `names`, each opening as the active last tab, and
/// closes those in `closed`. The app starts with "My workspace".
fn workspaces(fixture: &Fixture, names: &[&str], closed: &[&str]) {
    let app = fixture.app().clone();
    fixture.runtime.block_on(async {
        for name in names {
            app.workspace_create((*name).to_owned()).await.unwrap();
        }
        for name in closed {
            let id = id_of(&app.workspace_get(), name);
            app.workspace_close(id).await.unwrap();
        }
    });
}

fn id_of(tabs: &gitcroc_core::WorkspaceTabs, name: &str) -> WorkspaceId {
    tabs.open
        .iter()
        .find(|view| view.name == name)
        .map(|view| view.id.clone())
        .unwrap_or_else(|| panic!("no tab {name}"))
}

/// The element id of open workspace `name`'s tab.
fn tab(fixture: &Fixture, name: &str) -> ElementId {
    ElementId::Name(format!("tab-{}", id_of(&fixture.app().workspace_get(), name).0).into())
}

fn close_button(fixture: &Fixture, name: &str) -> ElementId {
    let id = id_of(&fixture.app().workspace_get(), name);
    ElementId::Name(format!("tab-close-{}", id.0).into())
}

/// The tabs' names, left to right.
fn tab_names(cx: &mut VisualTestContext) -> Vec<String> {
    labels(cx, Role::Tab)
}

fn labels(cx: &mut VisualTestContext, role: Role) -> Vec<String> {
    cx.update(|window, cx| {
        window.render_frame(cx);
        window
            .find_all_by_role(role)
            .iter()
            .filter_map(|found| found.label().map(str::to_owned))
            .collect()
    })
}

/// The selected tab's name.
fn active_tab(cx: &mut VisualTestContext) -> Option<String> {
    cx.update(|window, _| {
        window
            .find_all_by_role(Role::Tab)
            .into_iter()
            .find(|tab| tab.selected() == Some(true))
            .and_then(|tab| tab.label().map(str::to_owned))
    })
}

fn shown(cx: &mut VisualTestContext, id: impl Into<ElementId>) -> bool {
    let id = id.into();
    cx.update(|window, cx| {
        window.render_frame(cx);
        window.try_find(id).is_some()
    })
}

fn label(cx: &mut VisualTestContext, id: impl Into<ElementId>) -> Option<String> {
    let id = id.into();
    cx.update(|window, _| {
        window
            .try_find(id)
            .and_then(|found| found.label().map(str::to_owned))
    })
}

fn focused(cx: &mut VisualTestContext, id: impl Into<ElementId>) -> bool {
    let id = id.into();
    cx.update(|window, cx| {
        window.render_frame(cx);
        window.try_find(id).and_then(|found| found.focused()) == Some(true)
    })
}

fn click(cx: &mut VisualTestContext, id: impl Into<ElementId>) {
    let id = id.into();
    cx.update(|window, cx| window.click(id, cx));
    cx.run_until_parked();
}

/// Clicks the element named `name` inside `scope`.
fn click_named(cx: &mut VisualTestContext, scope: &'static str, name: &str) {
    let id = cx.update(|window, _| {
        let found = window.within(scope).find_by_label(name);
        found.path().last().cloned().unwrap()
    });
    click(cx, id);
}

/// Waits until `done` holds, rendering frames meanwhile.
fn wait(cx: &mut VisualTestContext, mut done: impl FnMut(&mut VisualTestContext) -> bool) {
    for _ in 0..1000 {
        cx.run_until_parked();
        if done(cx) {
            return;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    panic!("the window never got there");
}

/// Opens "+"'s menu once the list is in.
fn open_plus_menu(cx: &mut VisualTestContext) {
    click(cx, "tab-add");
    wait(cx, |cx| shown(cx, "menu"));
}

/// Replaces the name field's text with `text`.
fn type_name(cx: &mut VisualTestContext, text: &str) {
    press(cx, &secondary("a"));
    cx.update(|window, cx| window.input(text, cx));
    cx.run_until_parked();
}

fn submit_and_wait(cx: &mut VisualTestContext, until: impl FnMut(&mut VisualTestContext) -> bool) {
    click(cx, "dialog-submit");
    wait(cx, until);
}

fn workspace_names(fixture: &Fixture) -> Vec<String> {
    let app = fixture.app().clone();
    fixture.runtime.block_on(async move {
        app.workspace_list()
            .await
            .unwrap()
            .into_iter()
            .map(|item| item.name)
            .collect()
    })
}

#[gpui::test]
fn ui_27_tabs_follow_the_open_workspaces_and_the_active_one(cx: &mut TestAppContext) {
    let fixture = Fixture::start(gitcroc_app::app::EVENT_CAPACITY);
    workspaces(&fixture, &["Java"], &[]);
    let (mut cx, _) = main_window(cx, &fixture);

    assert_eq!(tab_names(&mut cx), ["My workspace", "Java"]);
    assert_eq!(active_tab(&mut cx).as_deref(), Some("Java"));
    // The tabs are in the title bar (UI-35), in a tab list.
    let java = tab(&fixture, "Java");
    cx.update(|window, _| {
        let path = window.find(java.clone()).path().to_vec();
        assert!(path.contains(&ElementId::from("title-bar")), "{path:?}");
        assert!(path.contains(&ElementId::from("tabs")), "{path:?}");
        assert_eq!(window.find("tabs").role(), Some(Role::TabList));
        assert_eq!(window.find("tabs").label(), Some("Workspaces"));
    });

    // What the app changes elsewhere shows: a rename, and another tab
    // made active.
    let app = fixture.app().clone();
    let tabs = app.workspace_get();
    let (java_id, main_id) = (id_of(&tabs, "Java"), id_of(&tabs, "My workspace"));
    fixture.runtime.block_on(async {
        app.workspace_rename(java_id, "Kotlin".to_owned())
            .await
            .unwrap();
        app.workspace_activate(main_id).await.unwrap();
    });
    settle(&mut cx, |window, _| {
        window.try_find_by_label("Kotlin").is_some()
    });
    assert_eq!(tab_names(&mut cx), ["My workspace", "Kotlin"]);
    assert_eq!(active_tab(&mut cx).as_deref(), Some("My workspace"));
}

#[gpui::test]
fn ui_27_with_every_tab_closed_the_window_offers_the_workspaces(cx: &mut TestAppContext) {
    let fixture = Fixture::start(gitcroc_app::app::EVENT_CAPACITY);
    workspaces(&fixture, &["Java"], &["Java", "My workspace"]);
    let (mut cx, _) = main_window(cx, &fixture);
    wait(&mut cx, |cx| {
        labels(cx, Role::Button).contains(&"New workspace…".to_owned())
    });

    assert!(tab_names(&mut cx).is_empty());
    assert_eq!(
        label(&mut cx, "no-workspace").as_deref(),
        Some("No workspace is open. Open one or create a new one.")
    );
    for region in ["toolbar", "tree", "history", "changes", "status-bar"] {
        assert!(!shown(&mut cx, region), "{region} is shown");
    }
    // The "+" menu's entries, in place, as buttons (UI-28).
    let entries: Vec<String> = cx.update(|window, _| {
        window
            .within("workspace-start-entries")
            .find_all_by_role(Role::Button)
            .iter()
            .filter_map(|found| found.label().map(str::to_owned))
            .collect()
    });
    assert_eq!(
        entries,
        [
            "Java",
            "Delete Java…",
            "My workspace",
            "Delete My workspace…",
            "New workspace…",
            "Import workspace…",
        ]
    );
    cx.update(|window, _| {
        let group = window.find("workspace-start-entries");
        assert_eq!(group.role(), Some(Role::Group));
        assert_eq!(group.label(), Some("Workspaces"));
    });

    click_named(&mut cx, "workspace-start-entries", "My workspace");
    wait(&mut cx, |cx| shown(cx, "tree"));
    assert_eq!(tab_names(&mut cx), ["My workspace"]);
    assert!(!shown(&mut cx, "workspace-start"));
}

#[gpui::test]
fn ui_28_plus_menu_lists_workspaces_and_offers_new_and_import(cx: &mut TestAppContext) {
    let fixture = Fixture::start(gitcroc_app::app::EVENT_CAPACITY);
    workspaces(&fixture, &["Java", "archive"], &["Java", "archive"]);
    let (mut cx, _) = main_window(cx, &fixture);

    open_plus_menu(&mut cx);
    assert_eq!(label(&mut cx, "menu").as_deref(), Some("Open workspace"));
    assert_eq!(
        labels(&mut cx, Role::MenuItem),
        [
            "archive",
            "Delete archive…",
            "Java",
            "Delete Java…",
            "My workspace (open)",
            "New workspace…",
            "Import workspace…",
        ]
    );
    assert!(focused(
        &mut cx,
        ElementId::NamedInteger("menu-item".into(), 0)
    ));

    // Choosing a closed workspace opens it as the active last tab (UI-27).
    click_named(&mut cx, "menu", "Java");
    wait(&mut cx, |cx| tab_names(cx).len() == 2);
    assert!(!shown(&mut cx, "menu"));
    assert_eq!(tab_names(&mut cx), ["My workspace", "Java"]);
    assert_eq!(active_tab(&mut cx).as_deref(), Some("Java"));
    assert!(focused(&mut cx, tab(&fixture, "Java")));

    // Choosing an open one switches to it; no tab is added.
    open_plus_menu(&mut cx);
    click_named(&mut cx, "menu", "My workspace (open)");
    wait(&mut cx, |cx| {
        active_tab(cx).as_deref() == Some("My workspace")
    });
    assert_eq!(tab_names(&mut cx), ["My workspace", "Java"]);

    // The keys move in the menu, and Escape closes it with nothing done,
    // the focus back on "+".
    open_plus_menu(&mut cx);
    press(&mut cx, "down");
    assert!(focused(
        &mut cx,
        ElementId::NamedInteger("menu-item".into(), 1)
    ));
    press(&mut cx, "up");
    press(&mut cx, "up");
    // Java is open now, so it has no Delete: the last of six entries.
    assert!(focused(
        &mut cx,
        ElementId::NamedInteger("menu-item".into(), 5)
    ));
    press(&mut cx, "escape");
    assert!(!shown(&mut cx, "menu"));
    assert!(focused(&mut cx, "tab-add"));
    assert_eq!(tab_names(&mut cx), ["My workspace", "Java"]);

    // Enter chooses: the first entry opens "archive".
    open_plus_menu(&mut cx);
    press(&mut cx, "enter");
    wait(&mut cx, |cx| tab_names(cx).len() == 3);
    assert_eq!(active_tab(&mut cx).as_deref(), Some("archive"));
}

#[gpui::test]
fn ui_28_new_and_rename_show_the_core_refusal_inline(cx: &mut TestAppContext) {
    let fixture = Fixture::start(gitcroc_app::app::EVENT_CAPACITY);
    workspaces(&fixture, &["Java", "Rust"], &[]);
    let (mut cx, _) = main_window(cx, &fixture);

    open_plus_menu(&mut cx);
    click_named(&mut cx, "menu", "New workspace…");
    assert!(shown(&mut cx, "name-dialog"));
    cx.update(|window, _| {
        let dialog = window.find("name-dialog");
        assert_eq!(dialog.role(), Some(Role::Dialog));
        assert_eq!(dialog.label(), Some("New workspace"));
        assert_eq!(window.find("name-field").label(), Some("Name"));
        assert_eq!(window.find("dialog-submit").label(), Some("Create"));
    });
    assert!(focused(&mut cx, "name-field"));

    type_name(&mut cx, "  JAVA ");
    submit_and_wait(&mut cx, |cx| shown(cx, "dialog-error"));
    let refusal = label(&mut cx, "dialog-error").unwrap();
    assert!(refusal.starts_with("a workspace named"), "{refusal}");
    assert!(refusal.ends_with("exists already"), "{refusal}");
    assert_eq!(
        cx.update(|window, _| window.find("dialog-error").role()),
        Some(Role::Alert)
    );
    // Typing clears the refusal.
    cx.update(|window, cx| window.input("x", cx));
    assert!(!shown(&mut cx, "dialog-error"));
    type_name(&mut cx, "   ");
    submit_and_wait(&mut cx, |cx| shown(cx, "dialog-error"));
    assert_eq!(
        label(&mut cx, "dialog-error").as_deref(),
        Some("a workspace name cannot be blank")
    );
    type_name(&mut cx, "C#");
    submit_and_wait(&mut cx, |cx| !shown(cx, "name-dialog"));
    wait(&mut cx, |cx| tab_names(cx).len() == 4);
    assert_eq!(tab_names(&mut cx), ["My workspace", "Java", "Rust", "C#"]);
    assert_eq!(active_tab(&mut cx).as_deref(), Some("C#"));

    // The tab's menu renames, starting from the name.
    cx.update(|window, cx| window.right_click(tab(&fixture, "Rust"), cx));
    cx.run_until_parked();
    assert_eq!(label(&mut cx, "menu").as_deref(), Some("Tab actions"));
    assert_eq!(
        labels(&mut cx, Role::MenuItem),
        ["Rename…", "Export workspace…", "Close"]
    );
    click_named(&mut cx, "menu", "Rename…");
    assert_eq!(
        label(&mut cx, "name-dialog").as_deref(),
        Some("Rename Rust")
    );
    assert_eq!(
        cx.update(|window, _| window.find("name-field").value().map(str::to_owned))
            .as_deref(),
        Some("Rust")
    );
    type_name(&mut cx, "java");
    submit_and_wait(&mut cx, |cx| shown(cx, "dialog-error"));
    type_name(&mut cx, "Kotlin");
    submit_and_wait(&mut cx, |cx| !shown(cx, "name-dialog"));
    wait(&mut cx, |cx| tab_names(cx).contains(&"Kotlin".to_owned()));
    assert_eq!(tab_names(&mut cx), ["My workspace", "Java", "Kotlin", "C#"]);
}

#[gpui::test]
fn ui_28_delete_asks_first_and_leaves_the_repos(cx: &mut TestAppContext) {
    let fixture = Fixture::start(gitcroc_app::app::EVENT_CAPACITY);
    workspaces(&fixture, &["archive"], &["archive"]);
    let (mut cx, _) = main_window(cx, &fixture);

    open_plus_menu(&mut cx);
    click_named(&mut cx, "menu", "Delete archive…");
    cx.update(|window, _| {
        let question = window.find("question");
        assert_eq!(question.role(), Some(Role::AlertDialog));
        assert_eq!(question.label(), Some("Delete the workspace archive?"));
        assert_eq!(
            window.find(("question-text", 0_usize)).label(),
            Some("Its workspace file is deleted. The repositories on disk are not touched.")
        );
        assert_eq!(window.find("dialog-submit").label(), Some("Delete"));
    });
    assert!(focused(&mut cx, "dialog-cancel"));

    // Cancel deletes nothing.
    click(&mut cx, "dialog-cancel");
    assert!(!shown(&mut cx, "question"));
    assert!(workspace_names(&fixture).contains(&"archive".to_owned()));

    open_plus_menu(&mut cx);
    click_named(&mut cx, "menu", "Delete archive…");
    assert!(workspace_names(&fixture).contains(&"archive".to_owned()));
    submit_and_wait(&mut cx, |cx| !shown(cx, "question"));
    assert_eq!(workspace_names(&fixture), ["My workspace"]);
    assert_eq!(tab_names(&mut cx), ["My workspace"]);
}

#[gpui::test]
fn ui_28_tabs_reorder_by_drag_and_close(cx: &mut TestAppContext) {
    let fixture = Fixture::start(gitcroc_app::app::EVENT_CAPACITY);
    workspaces(&fixture, &["Java", "Rust"], &[]);
    let (mut cx, _) = main_window(cx, &fixture);
    assert_eq!(tab_names(&mut cx), ["My workspace", "Java", "Rust"]);

    // Dragging Rust onto the first tab puts it first; the core keeps the
    // order.
    let (rust, main) = (tab(&fixture, "Rust"), tab(&fixture, "My workspace"));
    cx.update(|window, cx| window.drag_to(rust, main, cx));
    wait(&mut cx, |cx| tab_names(cx)[0] == "Rust");
    assert_eq!(tab_names(&mut cx), ["Rust", "My workspace", "Java"]);
    let names: Vec<String> = fixture
        .app()
        .workspace_get()
        .open
        .into_iter()
        .map(|view| view.name)
        .collect();
    assert_eq!(names, ["Rust", "My workspace", "Java"]);

    // Closing the active tab activates the one that takes its place; the
    // file stays.
    click(&mut cx, tab(&fixture, "Java"));
    wait(&mut cx, |cx| active_tab(cx).as_deref() == Some("Java"));
    cx.update(|window, _| {
        assert_eq!(
            window.find(close_button(&fixture, "Java")).label(),
            Some("Close Java")
        );
    });
    click(&mut cx, close_button(&fixture, "Java"));
    wait(&mut cx, |cx| tab_names(cx).len() == 2);
    assert_eq!(tab_names(&mut cx), ["Rust", "My workspace"]);
    assert_eq!(active_tab(&mut cx).as_deref(), Some("My workspace"));
    assert!(workspace_names(&fixture).contains(&"Java".to_owned()));

    // The tab menu's Close.
    cx.update(|window, cx| window.right_click(tab(&fixture, "Rust"), cx));
    cx.run_until_parked();
    click_named(&mut cx, "menu", "Close");
    wait(&mut cx, |cx| tab_names(cx).len() == 1);
    assert_eq!(tab_names(&mut cx), ["My workspace"]);
}

#[gpui::test]
fn ui_22_tabs_switch_and_move_from_the_keyboard(cx: &mut TestAppContext) {
    let fixture = Fixture::start(gitcroc_app::app::EVENT_CAPACITY);
    workspaces(&fixture, &["Java", "Rust"], &[]);
    let (mut cx, _) = main_window(cx, &fixture);
    assert_eq!(active_tab(&mut cx).as_deref(), Some("Rust"));

    // F6 into the tab bar lands on the active tab.
    press(&mut cx, "shift-f6");
    for _ in 0..5 {
        if focused(&mut cx, tab(&fixture, "Rust")) {
            break;
        }
        press(&mut cx, "shift-f6");
    }
    assert!(focused(&mut cx, tab(&fixture, "Rust")));

    // Ctrl/Cmd+Shift+PageUp and PageDown move the active tab, and stop at
    // the ends.
    for expected in [
        ["My workspace", "Rust", "Java"],
        ["Rust", "My workspace", "Java"],
        ["Rust", "My workspace", "Java"],
    ] {
        press(&mut cx, &secondary("shift-pageup"));
        wait(&mut cx, |cx| tab_names(cx) == expected);
    }
    press(&mut cx, &secondary("shift-pagedown"));
    wait(&mut cx, |cx| {
        tab_names(cx) == ["My workspace", "Rust", "Java"]
    });
    assert!(focused(&mut cx, tab(&fixture, "Rust")));

    // Ctrl/Cmd+PageDown and PageUp, the arrows, Home and End switch tabs.
    let steps = [
        (secondary("pagedown"), "Java"),
        (secondary("pageup"), "Rust"),
        ("left".to_owned(), "My workspace"),
        ("right".to_owned(), "Rust"),
        ("end".to_owned(), "Java"),
        ("home".to_owned(), "My workspace"),
    ];
    for (keys, expected) in steps {
        press(&mut cx, &keys);
        wait(&mut cx, |cx| active_tab(cx).as_deref() == Some(expected));
        assert!(focused(&mut cx, tab(&fixture, expected)), "{keys}");
    }

    // Shift+F10 opens the focused tab's menu; Escape gives the focus back.
    press(&mut cx, "shift-f10");
    assert_eq!(label(&mut cx, "menu").as_deref(), Some("Tab actions"));
    assert!(focused(
        &mut cx,
        ElementId::NamedInteger("menu-item".into(), 0)
    ));
    press(&mut cx, "escape");
    assert!(!shown(&mut cx, "menu"));
    assert!(focused(&mut cx, tab(&fixture, "My workspace")));

    // Delete closes the focused tab.
    press(&mut cx, "delete");
    wait(&mut cx, |cx| tab_names(cx).len() == 2);
    assert_eq!(tab_names(&mut cx), ["Rust", "Java"]);
}

#[gpui::test]
fn ui_22_dialogs_trap_the_focus_and_close_with_escape(cx: &mut TestAppContext) {
    let fixture = Fixture::start(gitcroc_app::app::EVENT_CAPACITY);
    workspaces(&fixture, &["Java"], &[]);
    let (mut cx, _) = main_window(cx, &fixture);

    open_plus_menu(&mut cx);
    click_named(&mut cx, "menu", "New workspace…");
    let in_dialog = |cx: &mut VisualTestContext| {
        ["name-field", "dialog-cancel", "dialog-submit"]
            .into_iter()
            .filter(|id| focused(cx, *id))
            .count()
            == 1
    };
    assert!(in_dialog(&mut cx));
    for keys in ["tab", "tab", "tab", "tab", "shift-tab", "shift-tab"] {
        press(&mut cx, keys);
        assert!(in_dialog(&mut cx), "after {keys}");
    }

    // The tab keys do nothing under a dialog.
    press(&mut cx, &secondary("pageup"));
    press(&mut cx, &secondary("shift-pageup"));
    assert_eq!(tab_names(&mut cx), ["My workspace", "Java"]);
    assert_eq!(active_tab(&mut cx).as_deref(), Some("Java"));

    // Escape closes it, from the field too, and the focus goes back to "+".
    while !focused(&mut cx, "name-field") {
        press(&mut cx, "tab");
    }
    press(&mut cx, "escape");
    assert!(!shown(&mut cx, "name-dialog"));
    assert!(focused(&mut cx, "tab-add"));
    assert_eq!(tab_names(&mut cx), ["My workspace", "Java"]);
}

#[gpui::test]
fn ui_33_missing_workspace_files_are_named_until_dismissed(cx: &mut TestAppContext) {
    // A name long enough that the note must wrap to keep its button in
    // the window, as macOS's long temporary paths need.
    let long = "a-workspace-whose-file-name-is-long-enough-to-fill-the-window".repeat(4);
    let fixture = Fixture::start_with(Options {
        settings: Some(format!(
            "schema = {SETTINGS_SCHEMA}\n[session]\ntabs = [\"gone\", \"{long}\"]\nactive = \"gone\"\n"
        )),
        ..Options::default()
    });
    let (mut cx, _) = main_window(cx, &fixture);

    let note = label(&mut cx, "missing-workspaces").unwrap();
    assert!(
        note.starts_with("These workspace files were not found, so their tabs were not restored: "),
        "{note}"
    );
    assert!(note.contains("gone.toml"), "{note}");
    assert_eq!(
        cx.update(|window, _| window.find("missing-workspaces").role()),
        Some(Role::Alert)
    );
    click(&mut cx, "missing-workspaces-dismiss");
    assert!(!shown(&mut cx, "missing-workspaces"));
}

#[gpui::test]
fn ui_35_the_tab_bar_is_the_title_bar(cx: &mut TestAppContext) {
    let fixture = Fixture::start(gitcroc_app::app::EVENT_CAPACITY);
    workspaces(&fixture, &["Java"], &[]);
    let controls = Rc::new(FakeControls::default());
    let (mut window_cx, _) = opened_main_window_with(cx, &fixture, controls.clone());
    let tabs_cx = &mut window_cx;
    settle(tabs_cx, |window, _| window.try_find("toolbar").is_some());

    // A press on a tab, on its close button or on "+" never moves the
    // window; one on the empty space does.
    for id in [
        tab(&fixture, "Java"),
        close_button(&fixture, "My workspace"),
        ElementId::from("tab-add"),
    ] {
        let at = tabs_cx.update(|window, _| window.find(id.clone()).bounds().center());
        tabs_cx.update(|window, cx| window.drag(at, at + point(px(0.), px(40.)), cx));
        assert_eq!(*controls.asked.borrow(), [], "{id:?}");
    }
    let space = tabs_cx.update(|window, _| window.find("title-bar-space").bounds().center());
    tabs_cx.update(|window, cx| window.drag(space, space + point(px(30.), px(5.)), cx));
    assert_eq!(*controls.asked.borrow(), [Asked::Move]);
    assert_eq!(tab_names(tabs_cx), ["My workspace", "Java"]);

    // A screen without tabs, the setup screen, shows the app's name.
    let git = FakeGit::new("2.30.0");
    let fixture = Fixture::start_with(Options {
        settings: Some(settings_with_git(git.path(), SETTINGS_SCHEMA)),
        ..Options::default()
    });
    let (mut cx, _) = main_window(cx, &fixture);
    assert!(shown(&mut cx, "git-setup"));
    assert!(tab_names(&mut cx).is_empty());
    assert!(!shown(&mut cx, "tab-add"));
}

#[gpui::test]
fn sec_3_workspace_names_render_as_text(cx: &mut TestAppContext) {
    let name = "<b>Java</b> & {x} <script>";
    let fixture = Fixture::start(gitcroc_app::app::EVENT_CAPACITY);
    workspaces(&fixture, &[name], &[]);
    let (mut cx, _) = main_window(cx, &fixture);

    assert_eq!(tab_names(&mut cx), ["My workspace", name]);
    open_plus_menu(&mut cx);
    assert!(
        labels(&mut cx, Role::MenuItem).contains(&format!("{name} (open)")),
        "{:?}",
        labels(&mut cx, Role::MenuItem)
    );
    press(&mut cx, "escape");
    cx.update(|window, cx| window.right_click(tab(&fixture, name), cx));
    cx.run_until_parked();
    click_named(&mut cx, "menu", "Rename…");
    assert_eq!(
        label(&mut cx, "name-dialog"),
        Some(format!("Rename {name}"))
    );
    assert_eq!(
        label(&mut cx, "dialog-title"),
        Some(format!("Rename {name}"))
    );
}

/// UI-27: a call's answer can arrive after the event of a newer change.
/// The event the core sent last wins, so a late answer never puts back a
/// tab the user has left.
#[gpui::test]
fn ui_27_a_late_answer_never_undoes_a_newer_tab_change(cx: &mut TestAppContext) {
    let fixture = Fixture::start(gitcroc_app::app::EVENT_CAPACITY);
    let (to_java, to_mine) = fixture.runtime.block_on(async {
        let app = fixture.app();
        let to_java = app.workspace_create("Java".to_owned()).await.unwrap();
        let mine = id_of(&to_java, "My workspace");
        let to_mine = app.workspace_activate(mine).await.unwrap();
        (to_java, to_mine)
    });
    let services = fixture.services.clone();
    let workspaces = cx.new(|_| Workspaces::new(services));

    cx.update(|cx| {
        workspaces.update(cx, |workspaces, cx| {
            // A switch to Java starts, and its event shows it.
            let asked = workspaces.heard();
            workspaces.hear_tabs(to_java.clone(), cx);
            // A switch back starts; its event arrives before Java's answer.
            workspaces.hear_tabs(to_mine.clone(), cx);
            workspaces.answer_tabs(asked, to_java.clone(), cx);
            assert_eq!(workspaces.tabs(), &to_mine);

            // An answer with no event since is the newest: it shows at once.
            let asked = workspaces.heard();
            workspaces.answer_tabs(asked, to_java.clone(), cx);
            assert_eq!(workspaces.tabs(), &to_java);
        });
    });
}
