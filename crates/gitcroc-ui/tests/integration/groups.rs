//! M6-21: groups in the tree (UI-5, UI-7, GRP-2, GRP-3): the group, repo
//! and Ungrouped menus, the group dialogs, dragging repos and groups, and
//! the counts of a group's Branches node. Repos are real ones made by
//! `repo-gen` and added through the app (QA-2), in a workspace with two
//! groups that share a repo. Every wait is for the core's answer, never
//! for time.

use gitcroc_app::app::EVENT_CAPACITY;
use gitcroc_core::{GroupId, RepoId, TargetSet};
use gitcroc_ui::groups::groups_of;
use gitcroc_ui::strings;
use gitcroc_ui::window::GitcrocWindow;
use gpui_kit::test::{TestEventExt, TestWindowExt};
use gpui_kit::{
    ElementId, Entity, Modifiers, MouseButton, Role, TestAppContext, VisualTestContext, point,
};

use crate::history::{
    add, click, id_of, label_of, labels_in, press, repo, repos, tree_part, tree_row,
    wait_for_element, wait_until,
};
use crate::support::{Fixture, main_window};

/// The workspace of every test: api, web, cli and docs; Backend has api
/// and web, Frontend web and cli, and docs is in no group.
struct Setup {
    fixture: Fixture,
    made: repo_gen::Fixture,
    added: Vec<(String, RepoId)>,
    backend: GroupId,
    frontend: GroupId,
}

fn setup() -> Setup {
    let fixture = Fixture::start(EVENT_CAPACITY);
    let made = repos(&["api", "web", "cli", "docs"]);
    let added = add(&fixture, &made);
    let backend = crate::history_merged::group(
        &fixture,
        "Backend",
        vec![id_of(&added, "api"), id_of(&added, "web")],
    );
    let frontend = crate::history_merged::group(
        &fixture,
        "Frontend",
        vec![id_of(&added, "web"), id_of(&added, "cli")],
    );
    Setup {
        fixture,
        made,
        added,
        backend,
        frontend,
    }
}

impl Setup {
    fn id(&self, name: &str) -> RepoId {
        id_of(&self.added, name)
    }

    fn name(&self, id: &RepoId) -> String {
        self.added
            .iter()
            .find(|(_, added)| added == id)
            .map(|(name, _)| name.clone())
            .unwrap_or_default()
    }

    /// The group row of `group`.
    fn group(&self, group: &GroupId) -> String {
        crate::history_merged::group_node(group)
    }

    /// The row of repo `name` under `group`.
    fn member(&self, group: &GroupId, name: &str) -> String {
        format!("group:{}:repo:{}", group.0, self.id(name).0)
    }

    /// The row of repo `name` under Ungrouped.
    fn ungrouped(&self, name: &str) -> String {
        format!("repo:{}", self.id(name).0)
    }

    /// The groups as "Name: members", in the user's order.
    fn describe(&self, roots: &[gitcroc_core::TreeNode]) -> Vec<String> {
        groups_of(roots)
            .into_iter()
            .map(|group| {
                let members: Vec<String> = group.repos.iter().map(|repo| self.name(repo)).collect();
                format!("{}: {}", group.name, members.join(" "))
                    .trim_end()
                    .to_owned()
            })
            .collect()
    }

    /// The groups as the core has them.
    fn groups(&self) -> Vec<String> {
        let roots = self.fixture.app().tree_get(&self.fixture.active()).unwrap();
        self.describe(&roots)
    }

    /// The groups as the tree shows them.
    fn shown(&self, cx: &mut VisualTestContext, window: &Entity<GitcrocWindow>) -> Vec<String> {
        cx.update(|_, cx| {
            let tree = window.read(cx).tree(cx).unwrap();
            self.describe(tree.read(cx).tree().roots())
        })
    }

    /// Waits until the core and the tree both have `expected`.
    #[track_caller]
    fn wait_for(
        &self,
        cx: &mut VisualTestContext,
        window: &Entity<GitcrocWindow>,
        expected: &[&str],
    ) {
        wait_until(cx, |cx| {
            self.groups() == expected && self.shown(cx, window) == expected
        });
    }

    /// The workspace's repos, which no group change touches (GRP-3).
    fn assert_repos_stay(&self) {
        let open = self.fixture.app().workspace_get().open;
        let view = open
            .iter()
            .find(|view| view.id == self.fixture.active())
            .unwrap();
        assert_eq!(view.repos.len(), 4);
        for name in ["api", "web", "cli", "docs"] {
            assert!(
                repo(&self.made, name).path().join(".git").exists(),
                "{name}"
            );
        }
    }
}

fn open(cx: &mut TestAppContext, setup: &Setup) -> (VisualTestContext, Entity<GitcrocWindow>) {
    let (mut cx, window) = main_window(cx, &setup.fixture);
    crate::stage::enlarge(&mut cx);
    wait_for_element(&mut cx, tree_row(&setup.member(&setup.frontend, "cli")));
    (cx, window)
}

fn menu_item(at: usize) -> ElementId {
    ElementId::NamedInteger("menu-item".into(), at as u64)
}

fn submenu_item(at: usize) -> ElementId {
    ElementId::NamedInteger("submenu-item".into(), at as u64)
}

fn focused(cx: &mut VisualTestContext, id: ElementId) -> bool {
    cx.update(|window, cx| {
        window.render_frame(cx);
        window.find(id).focused() == Some(true)
    })
}

fn shown(cx: &mut VisualTestContext, id: impl Into<ElementId>) -> bool {
    let id = id.into();
    cx.update(|window, cx| {
        window.render_frame(cx);
        window.try_find(id).is_some()
    })
}

fn right_click(cx: &mut VisualTestContext, node: &str) {
    cx.update(|window, cx| {
        window.render_frame(cx);
        window.right_click(tree_row(node), cx);
    });
    cx.run_until_parked();
    wait_for_element(cx, "menu".into());
}

fn menu_labels(cx: &mut VisualTestContext) -> Vec<String> {
    labels_in(cx, "menu".into(), Role::MenuItem)
}

fn submenu_labels(cx: &mut VisualTestContext) -> Vec<String> {
    labels_in(cx, "submenu".into(), Role::MenuItem)
}

/// Opens row `node`'s Add to group menu with the mouse.
fn add_to_group_menu(cx: &mut VisualTestContext, node: &str) {
    right_click(cx, node);
    click(cx, menu_item(2));
    wait_for_element(cx, "submenu".into());
}

fn type_name(cx: &mut VisualTestContext, name: &str) {
    let all = if cfg!(target_os = "macos") {
        "cmd-a"
    } else {
        "ctrl-a"
    };
    press(cx, all);
    press(cx, "backspace");
    if !name.is_empty() {
        cx.update(|window, cx| window.input(name, cx));
        cx.run_until_parked();
    }
}

/// Window points of the middle of `from`'s row and of `fraction` of the
/// height of `to`'s.
fn ends(
    cx: &mut VisualTestContext,
    from: &str,
    to: &str,
    fraction: f32,
) -> (
    gpui_kit::Point<gpui_kit::Pixels>,
    gpui_kit::Point<gpui_kit::Pixels>,
) {
    cx.update(|window, cx| {
        window.render_frame(cx);
        let start = window.find(tree_row(from)).bounds().center();
        let bounds = window.find(tree_row(to)).bounds();
        let end = point(
            bounds.center().x,
            bounds.origin.y + bounds.size.height * fraction,
        );
        (start, end)
    })
}

/// Drags row `from` to `fraction` of the height of row `to` and lets go,
/// holding `modifiers`.
fn drag(cx: &mut VisualTestContext, from: &str, to: &str, fraction: f32, modifiers: Modifiers) {
    let (start, end) = ends(cx, from, to, fraction);
    cx.update(|window, cx| {
        window.change_modifiers(modifiers, cx);
        window.drag(start, end, cx);
        window.change_modifiers(Modifiers::default(), cx);
    });
    cx.run_until_parked();
}

/// Starts dragging row `from` and holds it over `fraction` of row `to`.
fn hold_over(cx: &mut VisualTestContext, from: &str, to: &str, fraction: f32) {
    let (start, end) = ends(cx, from, to, fraction);
    cx.update(|window, cx| {
        window.pointer_move(start, None, cx);
        window.pointer_down(start, MouseButton::Left, cx);
        for step in 1..=8 {
            let at = step as f32 / 8.;
            let position = point(
                start.x + (end.x - start.x) * at,
                start.y + (end.y - start.y) * at,
            );
            window.pointer_move(position, Some(MouseButton::Left), cx);
        }
    });
    cx.run_until_parked();
}

fn let_go(cx: &mut VisualTestContext, at: &str, fraction: f32) {
    let (_, end) = ends(cx, at, at, fraction);
    cx.update(|window, cx| window.pointer_up(end, MouseButton::Left, cx));
    cx.run_until_parked();
}

/// Whether any row shows a drag mark.
fn any_marks(cx: &mut VisualTestContext, window: &Entity<GitcrocWindow>) -> bool {
    cx.update(|_, cx| {
        let tree = window.read(cx).tree(cx).unwrap();
        tree.read(cx).is_dragging()
    })
}

fn drop_mark(node: &str, place: &str) -> ElementId {
    tree_part(node, &format!("drop-{place}"))
}

#[gpui::test]
fn ui_7_drag_moves_between_groups_and_ctrl_or_alt_adds(cx: &mut TestAppContext) {
    let s = setup();
    let (mut cx, window) = open(cx, &s);
    let (backend, frontend) = (s.group(&s.backend), s.group(&s.frontend));

    // Dragged onto another group, a repo moves from the group it came
    // from; the dragged row and the target are marked meanwhile.
    let api = s.member(&s.backend, "api");
    hold_over(&mut cx, &api, &frontend, 0.5);
    assert!(shown(&mut cx, tree_part(&api, "dragged")));
    assert!(shown(&mut cx, drop_mark(&frontend, "into")));
    let_go(&mut cx, &frontend, 0.5);
    s.wait_for(&mut cx, &window, &["Backend: web", "Frontend: web cli api"]);
    assert!(!shown(&mut cx, tree_part(&api, "dragged")));
    assert!(!shown(&mut cx, drop_mark(&frontend, "into")));
    assert!(!any_marks(&mut cx, &window));

    // Ctrl adds instead, and so does Alt (macOS).
    let cli = s.member(&s.frontend, "cli");
    drag(&mut cx, &cli, &backend, 0.5, Modifiers::control());
    s.wait_for(
        &mut cx,
        &window,
        &["Backend: web cli", "Frontend: web cli api"],
    );
    let api = s.member(&s.frontend, "api");
    drag(&mut cx, &api, &backend, 0.5, Modifiers::alt());
    s.wait_for(
        &mut cx,
        &window,
        &["Backend: web cli api", "Frontend: web cli api"],
    );

    // A repo in no group dropped on Ungrouped changes nothing: no request
    // is sent, so nothing is waited for.
    let before = s.groups();
    drag(
        &mut cx,
        &s.ungrouped("docs"),
        "ungrouped",
        0.5,
        Modifiers::default(),
    );
    assert!(!any_marks(&mut cx, &window));
    assert_eq!(s.groups(), before);

    // From Ungrouped a repo is always added; dropped above a member, it
    // goes there, and the target's before mark shows meanwhile.
    hold_over(
        &mut cx,
        &s.ungrouped("docs"),
        &s.member(&s.backend, "cli"),
        0.2,
    );
    assert!(shown(
        &mut cx,
        drop_mark(&s.member(&s.backend, "cli"), "before")
    ));
    assert!(!shown(
        &mut cx,
        drop_mark(&s.member(&s.backend, "cli"), "after")
    ));
    assert!(!shown(
        &mut cx,
        drop_mark(&s.member(&s.backend, "cli"), "into")
    ));
    let_go(&mut cx, &s.member(&s.backend, "cli"), 0.2);
    s.wait_for(
        &mut cx,
        &window,
        &["Backend: web docs cli api", "Frontend: web cli api"],
    );

    // Dropped on Ungrouped, a repo leaves the group it came from only.
    drag(
        &mut cx,
        &s.member(&s.frontend, "web"),
        "ungrouped",
        0.5,
        Modifiers::default(),
    );
    s.wait_for(
        &mut cx,
        &window,
        &["Backend: web docs cli api", "Frontend: cli api"],
    );

    // Within a group, dropping between members reorders: api below web,
    // with the target's after mark.
    hold_over(
        &mut cx,
        &s.member(&s.backend, "api"),
        &s.member(&s.backend, "web"),
        0.9,
    );
    assert!(shown(
        &mut cx,
        drop_mark(&s.member(&s.backend, "web"), "after")
    ));
    assert!(!shown(
        &mut cx,
        drop_mark(&s.member(&s.backend, "web"), "before")
    ));
    let_go(&mut cx, &s.member(&s.backend, "web"), 0.9);
    s.wait_for(
        &mut cx,
        &window,
        &["Backend: web api docs cli", "Frontend: cli api"],
    );

    // A group dropped above another moves there, with the target's before
    // mark.
    hold_over(&mut cx, &frontend, &backend, 0.2);
    assert!(shown(&mut cx, drop_mark(&backend, "before")));
    assert!(!shown(&mut cx, drop_mark(&backend, "after")));
    let_go(&mut cx, &backend, 0.2);
    s.wait_for(
        &mut cx,
        &window,
        &["Frontend: cli api", "Backend: web api docs cli"],
    );

    // Nothing else is a drop target for a group.
    let target = s.member(&s.frontend, "cli");
    hold_over(&mut cx, &backend, &target, 0.2);
    for place in ["into", "before", "after"] {
        assert!(!shown(&mut cx, drop_mark(&target, place)), "{place}");
    }
    assert!(shown(&mut cx, tree_part(&backend, "dragged")));
    let_go(&mut cx, &target, 0.2);
    assert!(!any_marks(&mut cx, &window));
    assert_eq!(
        s.groups(),
        ["Frontend: cli api", "Backend: web api docs cli"]
    );

    // The workspace's repos stay throughout (GRP-3).
    s.assert_repos_stay();
}

#[gpui::test]
fn ui_2_a_repo_in_two_groups_is_dragged_from_one_place(cx: &mut TestAppContext) {
    let s = setup();
    let (mut cx, window) = open(cx, &s);
    // web is in both groups; dragged from Backend onto Ungrouped, only
    // Backend loses it.
    drag(
        &mut cx,
        &s.member(&s.backend, "web"),
        "ungrouped",
        0.5,
        Modifiers::default(),
    );
    s.wait_for(&mut cx, &window, &["Backend: api", "Frontend: web cli"]);

    // Dragged from Frontend into Backend, it leaves Frontend only.
    drag(
        &mut cx,
        &s.member(&s.frontend, "web"),
        &s.group(&s.backend),
        0.5,
        Modifiers::default(),
    );
    s.wait_for(&mut cx, &window, &["Backend: api web", "Frontend: cli"]);
    s.assert_repos_stay();
}

#[gpui::test]
fn ui_7_group_and_repo_context_menus_offer_the_group_operations(cx: &mut TestAppContext) {
    let s = setup();
    let (mut cx, window) = open(cx, &s);
    let (backend, frontend) = (s.group(&s.backend), s.group(&s.frontend));
    let group_entries = [
        "Fetch",
        "Pull",
        "Push",
        "Check out branch…",
        "New branch…",
        "Delete branch…",
        "Merge…",
        "Rebase…",
        "Create tag…",
        "Rename…",
        "Delete…",
        "New group…",
        "Move up",
        "Move down",
    ];

    // M6-22a enables Fetch, Pull and Push; the branch and tag operations
    // wait for M6-22b. Move up and Move down are enabled but at the ends of
    // the shorter group.
    right_click(&mut cx, &backend);
    assert_eq!(menu_labels(&mut cx), group_entries);
    cx.update(|window, _| {
        for at in 3..9 {
            assert_eq!(
                window.find(menu_item(at)).description(),
                Some(strings::tree_menu::NOT_AVAILABLE),
                "{at}"
            );
        }
    });
    // The focus goes only to the entries that can be chosen.
    assert!(focused(&mut cx, menu_item(0)));
    press(&mut cx, "end");
    assert!(focused(&mut cx, menu_item(13)));
    press(&mut cx, "up");
    assert!(focused(&mut cx, menu_item(11)), "Move up is disabled first");
    press(&mut cx, "escape");
    // M6-22a: Fetch on a group runs through its preflight.
    right_click(&mut cx, &backend);
    click(&mut cx, menu_item(0));
    wait_for_element(&mut cx, "preflight".into());
    press(&mut cx, "escape");
    right_click(&mut cx, &frontend);
    press(&mut cx, "end");
    assert!(
        focused(&mut cx, menu_item(12)),
        "Move down is disabled last"
    );
    press(&mut cx, "escape");

    // Ungrouped has New group.
    right_click(&mut cx, "ungrouped");
    assert_eq!(menu_labels(&mut cx), ["New group…"]);
    assert!(focused(&mut cx, menu_item(0)));
    press(&mut cx, "escape");

    // A member's menu, and a repo's in no group.
    right_click(&mut cx, &s.member(&s.backend, "api"));
    assert_eq!(
        menu_labels(&mut cx),
        [
            "New branch…",
            "New tag…",
            "Add to group",
            "Remove from group",
            "Move up",
            "Move down",
            "Open in file manager",
            "Open in terminal",
            "Remove from workspace",
        ]
    );
    cx.update(|window, _| assert_eq!(window.find(menu_item(2)).expanded(), Some(false)));
    press(&mut cx, "home");
    press(&mut cx, "down");
    press(&mut cx, "down");
    press(&mut cx, "down");
    assert!(focused(&mut cx, menu_item(3)));
    press(&mut cx, "down");
    assert!(focused(&mut cx, menu_item(5)), "Move up is disabled first");
    press(&mut cx, "escape");
    right_click(&mut cx, &s.ungrouped("docs"));
    let labels = menu_labels(&mut cx);
    assert!(labels.contains(&"Add to group".to_owned()));
    assert!(!labels.contains(&"Remove from group".to_owned()));
    press(&mut cx, "escape");

    // Add to group lists the groups the repo is not in, then New group,
    // beside it; Escape closes it first, then the menu.
    add_to_group_menu(&mut cx, &s.member(&s.backend, "api"));
    assert_eq!(
        label_of(&mut cx, "submenu".into()).as_deref(),
        Some("Add to group")
    );
    assert_eq!(submenu_labels(&mut cx), ["Frontend", "New group…"]);
    assert!(focused(&mut cx, submenu_item(0)));
    cx.update(|window, _| assert_eq!(window.find(menu_item(2)).expanded(), Some(true)));
    press(&mut cx, "escape");
    assert!(!shown(&mut cx, "submenu"));
    assert!(shown(&mut cx, "menu"));
    assert!(focused(&mut cx, menu_item(2)));
    press(&mut cx, "escape");
    assert!(!shown(&mut cx, "menu"));

    // From the keyboard: Right or Enter opens it, Left goes back.
    click(&mut cx, tree_row(&s.member(&s.backend, "web")));
    press(&mut cx, "shift-f10");
    wait_for_element(&mut cx, "menu".into());
    press(&mut cx, "down");
    press(&mut cx, "down");
    assert!(focused(&mut cx, menu_item(2)));
    press(&mut cx, "right");
    assert_eq!(submenu_labels(&mut cx), ["New group…"]);
    assert!(focused(&mut cx, submenu_item(0)));
    press(&mut cx, "left");
    assert!(!shown(&mut cx, "submenu"));
    assert!(focused(&mut cx, menu_item(2)));
    press(&mut cx, "enter");
    assert!(focused(&mut cx, submenu_item(0)));
    press(&mut cx, "enter");
    wait_for_element(&mut cx, "name-dialog".into());
    assert_eq!(
        label_of(&mut cx, "dialog-title".into()).as_deref(),
        Some("New group")
    );
    type_name(&mut cx, "Shared");
    press(&mut cx, "enter");
    s.wait_for(
        &mut cx,
        &window,
        &["Backend: api web", "Frontend: web cli", "Shared: web"],
    );
    assert!(!shown(&mut cx, "name-dialog"));

    // An entry of the second menu with the mouse.
    add_to_group_menu(&mut cx, &s.ungrouped("docs"));
    assert_eq!(
        submenu_labels(&mut cx),
        ["Backend", "Frontend", "Shared", "New group…"]
    );
    click(&mut cx, submenu_item(1));
    s.wait_for(
        &mut cx,
        &window,
        &["Backend: api web", "Frontend: web cli docs", "Shared: web"],
    );

    // Every drag is in a menu too (UI-22): reorder members and groups,
    // and remove.
    right_click(&mut cx, &s.member(&s.backend, "api"));
    click(&mut cx, menu_item(5));
    s.wait_for(
        &mut cx,
        &window,
        &["Backend: web api", "Frontend: web cli docs", "Shared: web"],
    );
    let shared = groups_of(&s.fixture.app().tree_get(&s.fixture.active()).unwrap())
        .into_iter()
        .find(|group| group.name == "Shared")
        .unwrap()
        .id;
    right_click(&mut cx, &s.group(&shared));
    click(&mut cx, menu_item(12));
    s.wait_for(
        &mut cx,
        &window,
        &["Backend: web api", "Shared: web", "Frontend: web cli docs"],
    );
    right_click(&mut cx, &s.member(&s.frontend, "cli"));
    click(&mut cx, menu_item(3));
    s.wait_for(
        &mut cx,
        &window,
        &["Backend: web api", "Shared: web", "Frontend: web docs"],
    );
    s.assert_repos_stay();
}

#[gpui::test]
fn ui_7_a_multi_selection_is_added_to_a_group_or_a_new_one(cx: &mut TestAppContext) {
    let s = setup();
    let (mut cx, window) = open(cx, &s);
    let target = |cx: &mut VisualTestContext| {
        cx.update(|_, cx| window.read(cx).tree(cx).unwrap().read(cx).target())
    };
    let secondary = if cfg!(target_os = "macos") {
        Modifiers::command()
    } else {
        Modifiers::control()
    };
    let select_api_and_docs = |cx: &mut VisualTestContext| {
        click(cx, tree_row(&s.member(&s.backend, "api")));
        cx.update(|window, cx| {
            window.click_with_modifiers(tree_row(&s.ungrouped("docs")), secondary, cx)
        });
        cx.run_until_parked();
    };
    let both = Some(TargetSet::Repos {
        repo_ids: vec![s.id("api"), s.id("docs")],
    });
    select_api_and_docs(&mut cx);
    assert_eq!(target(&mut cx), both);

    // The menu keeps the selection; Frontend gets both.
    right_click(&mut cx, &s.ungrouped("docs"));
    assert_eq!(target(&mut cx), both);
    press(&mut cx, "escape");
    add_to_group_menu(&mut cx, &s.ungrouped("docs"));
    assert_eq!(
        submenu_labels(&mut cx),
        ["Backend", "Frontend", "New group…"]
    );
    click(&mut cx, submenu_item(1));
    s.wait_for(
        &mut cx,
        &window,
        &["Backend: api web", "Frontend: web cli api docs"],
    );

    // Backend has api already: only docs is added, and api keeps its
    // place.
    select_api_and_docs_in_groups(&mut cx, &s, secondary);
    assert_eq!(target(&mut cx), both);
    add_to_group_menu(&mut cx, &s.member(&s.frontend, "docs"));
    assert_eq!(submenu_labels(&mut cx), ["Backend", "New group…"]);
    click(&mut cx, submenu_item(0));
    s.wait_for(
        &mut cx,
        &window,
        &["Backend: api web docs", "Frontend: web cli api docs"],
    );

    // New group… starts with the selected repos.
    select_api_and_docs_in_groups(&mut cx, &s, secondary);
    assert_eq!(target(&mut cx), both);
    add_to_group_menu(&mut cx, &s.member(&s.backend, "api"));
    assert_eq!(submenu_labels(&mut cx), ["New group…"]);
    click(&mut cx, submenu_item(0));
    wait_for_element(&mut cx, "name-dialog".into());
    type_name(&mut cx, "Pair");
    click(&mut cx, "dialog-submit".into());
    s.wait_for(
        &mut cx,
        &window,
        &[
            "Backend: api web docs",
            "Frontend: web cli api docs",
            "Pair: api docs",
        ],
    );

    // A repo outside the selection: its menu selects it alone and acts on
    // it only.
    add_to_group_menu(&mut cx, &s.member(&s.frontend, "cli"));
    assert_eq!(
        target(&mut cx),
        Some(TargetSet::Repo {
            repo_id: s.id("cli")
        })
    );
    assert_eq!(submenu_labels(&mut cx), ["Backend", "Pair", "New group…"]);
    click(&mut cx, submenu_item(1));
    s.wait_for(
        &mut cx,
        &window,
        &[
            "Backend: api web docs",
            "Frontend: web cli api docs",
            "Pair: api docs cli",
        ],
    );
}

/// Selects api under Backend and docs under Frontend, once docs has left
/// Ungrouped.
fn select_api_and_docs_in_groups(cx: &mut VisualTestContext, s: &Setup, secondary: Modifiers) {
    click(cx, tree_row(&s.member(&s.backend, "api")));
    cx.update(|window, cx| {
        window.click_with_modifiers(tree_row(&s.member(&s.frontend, "docs")), secondary, cx)
    });
    cx.run_until_parked();
}

#[gpui::test]
fn grp_2_rename_and_new_group_show_the_refusal_inline(cx: &mut TestAppContext) {
    let s = setup();
    let (mut cx, window) = open(cx, &s);
    let refusal = |cx: &mut VisualTestContext| label_of(cx, "dialog-error".into());
    let submit = |cx: &mut VisualTestContext, name: &str| {
        type_name(cx, name);
        // Enter in the field, which keeps the focus for the next name.
        press(cx, "enter");
        wait_until(cx, |cx| refusal(cx).is_some());
    };

    right_click(&mut cx, "ungrouped");
    click(&mut cx, menu_item(0));
    wait_for_element(&mut cx, "name-dialog".into());
    submit(&mut cx, "");
    assert_eq!(
        refusal(&mut cx).as_deref(),
        Some("a group name cannot be blank")
    );
    submit(&mut cx, &"x".repeat(65));
    assert_eq!(
        refusal(&mut cx).as_deref(),
        Some("a group name has at most 64 characters")
    );
    submit(&mut cx, "backend");
    assert_eq!(
        refusal(&mut cx).as_deref(),
        Some("a group named \"Backend\" exists already")
    );
    assert!(shown(&mut cx, "name-dialog"));
    // Typing clears the refusal; Escape closes the dialog.
    cx.update(|window, cx| window.input("s", cx));
    cx.run_until_parked();
    assert_eq!(refusal(&mut cx), None);
    press(&mut cx, "escape");
    assert!(!shown(&mut cx, "name-dialog"));
    assert_eq!(s.groups(), ["Backend: api web", "Frontend: web cli"]);

    // Rename starts with the name; another group's in other case is
    // refused.
    right_click(&mut cx, &s.group(&s.frontend));
    click(&mut cx, menu_item(9));
    wait_for_element(&mut cx, "name-dialog".into());
    assert_eq!(
        label_of(&mut cx, "dialog-title".into()).as_deref(),
        Some("Rename Frontend")
    );
    cx.update(|window, _| {
        assert_eq!(window.find("name-field").value(), Some("Frontend"));
    });
    submit(&mut cx, " BACKEND ");
    assert_eq!(
        refusal(&mut cx).as_deref(),
        Some("a group named \"Backend\" exists already")
    );
    type_name(&mut cx, "Web");
    press(&mut cx, "enter");
    s.wait_for(&mut cx, &window, &["Backend: api web", "Web: web cli"]);
    assert!(!shown(&mut cx, "name-dialog"));
}

#[gpui::test]
fn grp_3_deleting_a_group_keeps_its_repos(cx: &mut TestAppContext) {
    let s = setup();
    let (mut cx, window) = open(cx, &s);
    right_click(&mut cx, &s.group(&s.backend));
    click(&mut cx, menu_item(10));
    wait_for_element(&mut cx, "question".into());
    assert_eq!(
        label_of(&mut cx, "dialog-title".into()).as_deref(),
        Some("Delete the group Backend?")
    );
    let what = label_of(&mut cx, ElementId::NamedInteger("question-text".into(), 0)).unwrap();
    assert!(what.contains("stay in the workspace"), "{what}");
    assert!(what.contains("Nothing on disk is touched."), "{what}");
    // Cancel has the focus.
    assert!(focused(&mut cx, "dialog-cancel".into()));
    click(&mut cx, "dialog-submit".into());
    s.wait_for(&mut cx, &window, &["Frontend: web cli"]);
    assert!(!shown(&mut cx, "question"));
    // api, in no group now, is under Ungrouped.
    wait_for_element(&mut cx, tree_row(&s.ungrouped("api")));
    s.assert_repos_stay();
}

#[gpui::test]
fn ui_5_the_branches_node_counts_the_members(cx: &mut TestAppContext) {
    let s = setup();
    let (mut cx, _) = open(cx, &s);
    let node = format!("{}:branches", s.group(&s.backend));
    click(&mut cx, tree_part(&node, "chevron"));
    let count = |cx: &mut VisualTestContext, branch: &str| {
        label_of(cx, tree_part(&format!("{node}:ref:{branch}"), "present"))
    };
    wait_until(&mut cx, |cx| {
        count(cx, "main").as_deref() == Some("in 2 of 2 members")
    });

    // A member checks out a new branch.
    repo(&s.made, "api")
        .git(["switch", "--quiet", "-c", "topic"])
        .unwrap();
    s.fixture.refresh(&s.fixture.active());
    wait_until(&mut cx, |cx| {
        count(cx, "topic").as_deref() == Some("in 1 of 2 members")
    });
    assert_eq!(count(&mut cx, "main").as_deref(), Some("in 2 of 2 members"));

    // A repo joins.
    let (app, workspace) = (s.fixture.app().clone(), s.fixture.active());
    s.fixture
        .runtime
        .block_on(app.group_add_repos(
            workspace.clone(),
            s.backend.clone(),
            vec![s.id("cli")],
            None,
        ))
        .unwrap();
    wait_until(&mut cx, |cx| {
        count(cx, "main").as_deref() == Some("in 3 of 3 members")
            && count(cx, "topic").as_deref() == Some("in 1 of 3 members")
    });

    // And one leaves.
    s.fixture
        .runtime
        .block_on(app.group_remove_repos(workspace, s.backend.clone(), vec![s.id("api")]))
        .unwrap();
    wait_until(&mut cx, |cx| {
        count(cx, "main").as_deref() == Some("in 2 of 2 members") && count(cx, "topic").is_none()
    });
}

#[gpui::test]
fn sec_3_group_names_render_as_text(cx: &mut TestAppContext) {
    let s = setup();
    let name = "<b>Ops</b>&amp;";
    let ops = crate::history_merged::group(&s.fixture, name, vec![s.id("docs")]);
    let (mut cx, _) = open(cx, &s);
    wait_for_element(&mut cx, tree_row(&s.group(&ops)));
    assert_eq!(
        label_of(&mut cx, tree_row(&s.group(&ops))).as_deref(),
        Some(name)
    );
    add_to_group_menu(&mut cx, &s.member(&s.backend, "api"));
    assert_eq!(submenu_labels(&mut cx), ["Frontend", name, "New group…"]);
    press(&mut cx, "escape");
    press(&mut cx, "escape");
    right_click(&mut cx, &s.group(&ops));
    click(&mut cx, menu_item(9));
    wait_for_element(&mut cx, "name-dialog".into());
    assert_eq!(
        label_of(&mut cx, "dialog-title".into()),
        Some(format!("Rename {name}"))
    );
    press(&mut cx, "escape");
    right_click(&mut cx, &s.group(&ops));
    click(&mut cx, menu_item(10));
    wait_for_element(&mut cx, "question".into());
    assert_eq!(
        label_of(&mut cx, "dialog-title".into()),
        Some(format!("Delete the group {name}?"))
    );
}
