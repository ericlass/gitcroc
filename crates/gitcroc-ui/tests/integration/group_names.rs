//! M6-22b: the group operations on a name X typed or picked once: Check out,
//! Create and Delete branch, Merge, Rebase and Create tag (section 5, UI-5,
//! UI-7, UI-20, GRP-13, GRP-14, SEC-3, SEC-4). Repos are real ones made by
//! `repo-gen` and added through the app (QA-2), in a group of three on mixed
//! branches, one without `topic`. A group job's end is the barrier before a
//! fixture changes.

use std::time::Duration;

use gitcroc_app::app::AppEvent;
use gitcroc_core::{GroupId, JobView, Outcome, TargetSet};
use gpui_kit::test::TestWindowExt;
use gpui_kit::{ElementId, Entity, Role, TestAppContext, VisualTestContext};
use tokio::sync::broadcast;

use crate::group_ops::{
    Remote, Setup, open, preflight_rows, ref_exists, shown, wait_preflight_ready,
};
use crate::history::{
    click, click_menu, double_click, label_of, labels_in, menu_labels, press, repo, right_click,
    tree_part, tree_row, wait_for_element, wait_until,
};
use crate::history_merged::{click_with, secondary_modifiers};

/// `api` and `web` have `topic`, `web` is on it; `docs` has no `topic`.
fn mixed(remote: Remote) -> (Setup, GroupId) {
    let mut setup = Setup::new(&[("api", remote), ("web", remote), ("docs", Remote::None)]);
    let api = repo(&setup.made, "api");
    api.branch("topic").unwrap();
    let web = repo(&setup.made, "web");
    web.branch("topic").unwrap();
    web.switch("topic").unwrap();
    setup.add();
    let group = setup.group("Backend", &["api", "web", "docs"]);
    (setup, group)
}

fn current(setup: &Setup, name: &str) -> String {
    repo(&setup.made, name)
        .git(["branch", "--show-current"])
        .unwrap()
        .trim()
        .to_owned()
}

fn input(cx: &mut VisualTestContext, text: &str) {
    cx.update(|window, cx| window.input(text, cx));
    cx.run_until_parked();
}

fn disabled(cx: &mut VisualTestContext, id: &str) -> bool {
    cx.update(|window, cx| {
        window.render_frame(cx);
        window
            .find(ElementId::Name(id.to_owned().into()))
            .disabled()
            == Some(true)
    })
}

fn title(cx: &mut VisualTestContext) -> Option<String> {
    label_of(cx, "dialog-title".into())
}

/// Submits the open group name dialog once the core has checked X.
fn submit(cx: &mut VisualTestContext) {
    wait_until(cx, |cx| !disabled(cx, "group-name-submit"));
    click(cx, "group-name-submit".into());
}

/// Waits for the group job to end, pressing Run when the preflight asks.
#[track_caller]
fn run_through(cx: &mut VisualTestContext, events: &mut broadcast::Receiver<AppEvent>) -> JobView {
    for _ in 0..3000 {
        cx.run_until_parked();
        if shown(cx, "preflight-run") && !disabled(cx, "preflight-run") {
            click(cx, "preflight-run".into());
        }
        while let Ok(event) = events.try_recv() {
            if let AppEvent::JobEnded(job) = event
                && job.report.is_some()
            {
                return job;
            }
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    panic!("no group job ended");
}

fn outcome_of(setup: &Setup, job: &JobView, name: &str) -> Outcome {
    let id = setup.id(name);
    job.report
        .as_ref()
        .unwrap()
        .iter()
        .find(|line| line.repo_id == id)
        .map(|line| line.outcome.clone())
        .unwrap_or_else(|| panic!("{name} is not in the report"))
}

fn branches_node(setup: &Setup, group: &GroupId) -> String {
    format!("{}:branches", setup.group_node(group))
}

/// Opens the group's Branches node and waits for branch `name`'s row.
fn branch_row(cx: &mut VisualTestContext, setup: &Setup, group: &GroupId, name: &str) -> ElementId {
    let node = branches_node(setup, group);
    click(cx, tree_part(&node, "chevron"));
    let row = tree_row(&format!("{node}:ref:{name}"));
    wait_for_element(cx, row.clone());
    row
}

/// Opens the group row's menu and chooses `entry`.
fn group_menu(cx: &mut VisualTestContext, setup: &Setup, group: &GroupId, entry: &str) {
    right_click(cx, tree_row(&setup.group_node(group)));
    click_menu(cx, entry);
}

/// Picks `name` from the dialog's list of the group's branches.
fn pick(cx: &mut VisualTestContext, name: &str) {
    wait_until(cx, |cx| known(cx).iter().any(|known| known == name));
    let id = cx.update(|window, _| {
        window
            .within("group-name-known")
            .find_by_label(name)
            .path()
            .last()
            .cloned()
            .expect("no such name")
    });
    click(cx, id);
}

fn known(cx: &mut VisualTestContext) -> Vec<String> {
    labels_in(cx, "group-name-known".into(), Role::Button)
}

fn member_row(setup: &Setup, group: &GroupId, name: &str) -> ElementId {
    tree_row(&format!("group:{}:repo:{}", group.0, setup.id(name).0))
}

fn selected(
    cx: &mut VisualTestContext,
    window: &Entity<gitcroc_ui::window::GitcrocWindow>,
) -> TargetSet {
    cx.update(|_, cx| window.read(cx).tree(cx).unwrap().read(cx).target().unwrap())
}

/// UI-5: a double-click on a group branch checks it out on every member,
/// through the preflight with the skip policy, never in one repo only; the
/// group then shows "mixed" (GRP-4).
#[gpui::test]
fn ui_5_double_click_on_a_group_branch_starts_a_group_checkout(cx: &mut TestAppContext) {
    let (setup, group) = mixed(Remote::None);
    let (mut cx, _window) = open(cx, &setup, &group);
    let row = branch_row(&mut cx, &setup, &group, "topic");
    wait_until(&mut cx, |cx| {
        label_of(cx, tree_part(&setup.group_node(&group), "branch")).as_deref() == Some("mixed (2)")
    });

    let mut events = setup.fixture.app().subscribe();
    double_click(&mut cx, row);
    // docs has no topic: the preflight asks first.
    wait_for_element(&mut cx, "preflight".into());
    wait_preflight_ready(&mut cx);
    let rows = preflight_rows(&mut cx);
    assert!(
        rows.iter()
            .any(|row| row.contains("docs") && row.contains("Will skip")),
        "{rows:?}"
    );
    assert!(title(&mut cx).unwrap().starts_with("Checkout topic ·"));
    let job = run_through(&mut cx, &mut events);
    assert!(matches!(
        outcome_of(&setup, &job, "docs"),
        Outcome::Skipped { .. }
    ));
    assert_eq!(current(&setup, "api"), "topic");
    assert_eq!(current(&setup, "web"), "topic");
    assert_eq!(current(&setup, "docs"), "main");
    setup.fixture.refresh(&setup.fixture.active());
    wait_until(&mut cx, |cx| {
        label_of(cx, tree_part(&setup.group_node(&group), "branch")).as_deref() == Some("mixed (2)")
    });

    // Once docs has it too, the group shows the one branch.
    repo(&setup.made, "docs")
        .git(["switch", "--quiet", "-c", "topic"])
        .unwrap();
    setup.fixture.refresh(&setup.fixture.active());
    wait_until(&mut cx, |cx| {
        label_of(cx, tree_part(&setup.group_node(&group), "branch")).as_deref() == Some("topic")
    });
}

/// GRP-14: X is resolved in each repo, with each missing-branch policy.
#[gpui::test]
fn grp_14_checkout_resolves_the_name_in_each_repo(cx: &mut TestAppContext) {
    let (setup, group) = mixed(Remote::None);
    let (mut cx, _window) = open(cx, &setup, &group);

    // Skip (the default): docs stays where it is.
    group_menu(&mut cx, &setup, &group, "Check out branch…");
    wait_for_element(&mut cx, "group-name".into());
    assert_eq!(
        title(&mut cx).as_deref(),
        Some("Check out a branch in Backend")
    );
    pick(&mut cx, "topic");
    let mut events = setup.fixture.app().subscribe();
    submit(&mut cx);
    let job = run_through(&mut cx, &mut events);
    assert!(matches!(
        outcome_of(&setup, &job, "docs"),
        Outcome::Skipped { .. }
    ));
    assert_eq!(current(&setup, "api"), "topic");
    assert_eq!(current(&setup, "docs"), "main");

    // Switch to its default branch: docs leaves dev for main.
    repo(&setup.made, "docs")
        .git(["switch", "--quiet", "-c", "dev"])
        .unwrap();
    setup.fixture.refresh(&setup.fixture.active());
    group_menu(&mut cx, &setup, &group, "Check out branch…");
    wait_for_element(&mut cx, "group-name".into());
    input(&mut cx, "topic");
    click(&mut cx, "group-name-missing-1".into());
    let mut events = setup.fixture.app().subscribe();
    submit(&mut cx);
    run_through(&mut cx, &mut events);
    assert_eq!(current(&setup, "docs"), "main");
    assert!(!ref_exists(&repo(&setup.made, "docs"), "refs/heads/topic"));

    // Create it from HEAD.
    setup.fixture.refresh(&setup.fixture.active());
    group_menu(&mut cx, &setup, &group, "Check out branch…");
    wait_for_element(&mut cx, "group-name".into());
    input(&mut cx, "topic");
    click(&mut cx, "group-name-missing-2".into());
    let mut events = setup.fixture.app().subscribe();
    submit(&mut cx);
    run_through(&mut cx, &mut events);
    assert_eq!(current(&setup, "docs"), "topic");
}

/// GRP-14: Create branch skips a repo where X exists and never resets it.
#[gpui::test]
fn grp_14_create_branch_skips_where_it_exists_and_never_resets(cx: &mut TestAppContext) {
    let (setup, group) = mixed(Remote::None);
    let api = repo(&setup.made, "api");
    let before = api.rev_parse("topic").unwrap();
    api.commit_file("main.txt", "main\n", "Main moves on")
        .unwrap();
    let (mut cx, _window) = open(cx, &setup, &group);

    click(&mut cx, "toolbar-3".into());
    wait_for_element(&mut cx, "group-name".into());
    assert_eq!(title(&mut cx).as_deref(), Some("New branch in Backend"));
    input(&mut cx, "topic");
    let mut events = setup.fixture.app().subscribe();
    submit(&mut cx);
    wait_for_element(&mut cx, "preflight".into());
    wait_preflight_ready(&mut cx);
    let rows = preflight_rows(&mut cx);
    assert!(
        rows.iter()
            .any(|row| row.contains("api") && row.contains("Will skip")),
        "{rows:?}"
    );
    let job = run_through(&mut cx, &mut events);
    assert!(matches!(
        outcome_of(&setup, &job, "api"),
        Outcome::Skipped { .. }
    ));
    assert_eq!(outcome_of(&setup, &job, "docs"), Outcome::Succeeded);
    assert_eq!(api.rev_parse("topic").unwrap(), before, "never reset");
    assert_eq!(current(&setup, "api"), "main");
    // Switch after create is on by default.
    assert_eq!(current(&setup, "docs"), "topic");
}

/// GRP-9: Force and Also delete on the remote are destructive: the
/// preflight asks first, and Cancel deletes nothing.
#[gpui::test]
fn grp_9_force_delete_and_delete_on_remote_ask_first(cx: &mut TestAppContext) {
    let (setup, group) = mixed(Remote::Ok);
    let api = repo(&setup.made, "api");
    api.git(["push", "--quiet", "-u", "origin", "topic"])
        .unwrap();
    let (mut cx, _window) = open(cx, &setup, &group);
    let row = branch_row(&mut cx, &setup, &group, "topic");

    for flag in ["group-name-force", "group-name-on-remote"] {
        right_click(&mut cx, row.clone());
        assert_eq!(menu_labels(&mut cx), ["Check out", "Delete…"]);
        click_menu(&mut cx, "Delete…");
        wait_for_element(&mut cx, "group-name".into());
        assert_eq!(
            title(&mut cx).as_deref(),
            Some("Delete a branch in Backend")
        );
        click(&mut cx, flag.into());
        submit(&mut cx);
        wait_for_element(&mut cx, "preflight".into());
        wait_preflight_ready(&mut cx);
        let actions = labels_in(&mut cx, "preflight-actions".into(), Role::ListItem);
        assert!(!actions.is_empty(), "{flag}: nothing named as destructive");
        let affected = labels_in(&mut cx, "preflight-affected".into(), Role::ListItem);
        assert!(affected.contains(&"api".to_owned()), "{affected:?}");
        click(&mut cx, "preflight-cancel".into());
        wait_until(&mut cx, |cx| !shown(cx, "preflight"));
    }
    assert!(ref_exists(&api, "refs/heads/topic"));
    assert!(ref_exists(&setup.origin("api"), "refs/heads/topic"));
}

/// GRP-13: Merge predicts conflicts and marks those repos *needs
/// attention*; a repo where git stops in conflict is *conflict*, not
/// *failed*.
#[gpui::test]
fn grp_13_merge_predicts_conflicts_and_marks_needs_attention(cx: &mut TestAppContext) {
    let (setup, group) = mixed(Remote::None);
    let api = repo(&setup.made, "api");
    api.switch("topic").unwrap();
    api.commit_file("README.md", "topic\n", "Topic").unwrap();
    api.switch("main").unwrap();
    api.commit_file("README.md", "main\n", "Main").unwrap();
    let web = repo(&setup.made, "web");
    web.commit_file("topic.txt", "topic\n", "Topic").unwrap();
    web.switch("main").unwrap();
    let (mut cx, _window) = open(cx, &setup, &group);

    click(&mut cx, "toolbar-4".into());
    wait_for_element(&mut cx, "group-name".into());
    assert_eq!(
        title(&mut cx).as_deref(),
        Some("Merge into the current branch in Backend")
    );
    assert!(shown(&mut cx, "group-name-predicts"));
    pick(&mut cx, "topic");
    click(&mut cx, "group-name-ff-2".into());
    let mut events = setup.fixture.app().subscribe();
    submit(&mut cx);
    wait_for_element(&mut cx, "preflight".into());
    wait_preflight_ready(&mut cx);
    let rows = preflight_rows(&mut cx);
    assert!(
        rows.iter()
            .any(|row| row.contains("api") && row.contains("Needs attention")),
        "{rows:?}"
    );
    let job = run_through(&mut cx, &mut events);
    assert_eq!(outcome_of(&setup, &job, "api"), Outcome::Conflict);
    assert_eq!(outcome_of(&setup, &job, "web"), Outcome::Succeeded);
    assert!(matches!(
        outcome_of(&setup, &job, "docs"),
        Outcome::Skipped { .. }
    ));
    // Always a merge commit: web's HEAD has two parents.
    assert!(web.rev_parse("HEAD^2").is_ok());
}

/// GRP-13: Rebase onto a ref, from the toolbar's Merge and from the group
/// row's Rebase…; a ref git would read as an option is never sent.
#[gpui::test]
fn grp_13_rebase_onto_a_ref_from_the_toolbar_and_the_menu(cx: &mut TestAppContext) {
    let (setup, group) = mixed(Remote::None);
    let web = repo(&setup.made, "web");
    web.commit_file("topic.txt", "topic\n", "Topic").unwrap();
    web.switch("main").unwrap();
    web.commit_file("main.txt", "main\n", "Main").unwrap();
    let (mut cx, _window) = open(cx, &setup, &group);
    let jobs = setup.jobs();

    click(&mut cx, "toolbar-4".into());
    wait_for_element(&mut cx, "group-name".into());
    click(&mut cx, "group-name-operation-1".into());
    assert_eq!(
        title(&mut cx).as_deref(),
        Some("Rebase the current branch in Backend")
    );
    assert_eq!(
        label_of(&mut cx, "group-name-submit".into()).as_deref(),
        Some("Rebase")
    );
    input(&mut cx, "topic");
    let mut events = setup.fixture.app().subscribe();
    submit(&mut cx);
    run_through(&mut cx, &mut events);
    assert!(
        web.git(["merge-base", "--is-ancestor", "topic", "main"])
            .is_ok(),
        "main was rebased onto topic"
    );

    // The menu's Rebase… opens on rebase; --root is refused as typed.
    group_menu(&mut cx, &setup, &group, "Rebase…");
    wait_for_element(&mut cx, "group-name".into());
    assert_eq!(
        title(&mut cx).as_deref(),
        Some("Rebase the current branch in Backend")
    );
    input(&mut cx, "--root");
    wait_for_element(&mut cx, "group-name-error".into());
    assert!(disabled(&mut cx, "group-name-submit"));
    press(&mut cx, "enter");
    assert!(!shown(&mut cx, "preflight"));
    press(&mut cx, "escape");
    wait_until(&mut cx, |cx| !shown(cx, "group-name"));
    assert_eq!(setup.jobs(), jobs + 1);
}

/// UI-7: the group row's menu and a group branch's menu offer the group
/// operations, enabled; Check out runs at once and Create tag… tags every
/// repo at its own HEAD.
#[gpui::test]
fn ui_7_the_group_menu_and_the_branches_node_offer_the_operations(cx: &mut TestAppContext) {
    let (setup, group) = mixed(Remote::None);
    let (mut cx, _window) = open(cx, &setup, &group);

    right_click(&mut cx, tree_row(&setup.group_node(&group)));
    assert_eq!(
        menu_labels(&mut cx),
        [
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
        ]
    );
    for entry in [
        "Check out branch…",
        "New branch…",
        "Delete branch…",
        "Merge…",
        "Rebase…",
        "Create tag…",
    ] {
        let off = cx.update(|window, cx| {
            window.render_frame(cx);
            window.within("menu").find_by_label(entry).disabled() == Some(true)
        });
        assert!(!off, "{entry} is disabled");
    }
    click_menu(&mut cx, "Create tag…");
    wait_for_element(&mut cx, "group-name".into());
    assert_eq!(title(&mut cx).as_deref(), Some("New tag in Backend"));
    input(&mut cx, "v1");
    // Annotated needs a message.
    wait_until(&mut cx, |cx| disabled(cx, "group-name-submit"));
    click(&mut cx, "group-name-tag-kind-1".into());
    let mut events = setup.fixture.app().subscribe();
    submit(&mut cx);
    let job = run_through(&mut cx, &mut events);
    for name in ["api", "web", "docs"] {
        assert_eq!(outcome_of(&setup, &job, name), Outcome::Succeeded);
        let made = repo(&setup.made, name);
        assert_eq!(
            made.rev_parse("v1").unwrap(),
            made.rev_parse("HEAD").unwrap()
        );
    }

    // A group branch's Check out runs at once, through the preflight.
    let row = branch_row(&mut cx, &setup, &group, "topic");
    right_click(&mut cx, row);
    click_menu(&mut cx, "Check out");
    wait_for_element(&mut cx, "preflight".into());
    wait_preflight_ready(&mut cx);
    assert!(title(&mut cx).unwrap().starts_with("Checkout topic ·"));
    click(&mut cx, "preflight-cancel".into());
    wait_until(&mut cx, |cx| !shown(cx, "preflight"));
    assert_eq!(current(&setup, "api"), "main");
}

/// SEC-4: branch and tag names are checked by the core as typed, and an
/// option-like ref is never sent.
#[gpui::test]
fn sec_4_branch_and_tag_names_are_checked_as_typed(cx: &mut TestAppContext) {
    let (setup, group) = mixed(Remote::None);
    let (mut cx, _window) = open(cx, &setup, &group);
    let jobs = setup.jobs();

    for (open_with, typed) in [
        ("New branch…", "bad..name"),
        ("Create tag…", "v 1"),
        ("Merge…", "-x"),
    ] {
        group_menu(&mut cx, &setup, &group, open_with);
        wait_for_element(&mut cx, "group-name".into());
        input(&mut cx, typed);
        wait_for_element(&mut cx, "group-name-error".into());
        assert!(disabled(&mut cx, "group-name-submit"), "{typed}");
        press(&mut cx, "enter");
        assert!(!shown(&mut cx, "preflight"), "{typed} was sent");
        press(&mut cx, "escape");
        wait_until(&mut cx, |cx| !shown(cx, "group-name"));
    }
    assert_eq!(setup.jobs(), jobs);
}

/// UI-20: the toolbar's Branch and Merge open the group dialogs for a group
/// and for a multi-selection; Stash stays single-repo.
#[gpui::test]
fn ui_20_branch_and_merge_open_the_group_dialogs(cx: &mut TestAppContext) {
    let (setup, group) = mixed(Remote::None);
    let (mut cx, window) = open(cx, &setup, &group);
    assert!(disabled(&mut cx, "toolbar-5"));

    click(&mut cx, "toolbar-3".into());
    wait_for_element(&mut cx, "group-name".into());
    assert_eq!(title(&mut cx).as_deref(), Some("New branch in Backend"));
    press(&mut cx, "escape");
    wait_until(&mut cx, |cx| !shown(cx, "group-name"));

    // A multi-selection: its name says how many, and it has no list of names.
    wait_for_element(&mut cx, member_row(&setup, &group, "api"));
    click(&mut cx, member_row(&setup, &group, "api"));
    click_with(
        &mut cx,
        member_row(&setup, &group, "web"),
        secondary_modifiers(),
    );
    assert!(matches!(
        selected(&mut cx, &window),
        TargetSet::Repos { .. }
    ));
    click(&mut cx, "toolbar-4".into());
    wait_for_element(&mut cx, "group-name".into());
    assert_eq!(
        title(&mut cx).as_deref(),
        Some("Merge into the current branch in 2 repositories")
    );
    assert!(!shown(&mut cx, "group-name-known"));
    press(&mut cx, "escape");
    wait_until(&mut cx, |cx| !shown(cx, "group-name"));
    assert!(disabled(&mut cx, "toolbar-5"));
}

/// SEC-3: branch names are text, in the list to pick from and in the
/// preflight that runs on them.
#[gpui::test]
fn sec_3_branch_names_render_as_text(cx: &mut TestAppContext) {
    // Markup that Windows can keep in a loose ref's file name: no < or >.
    let name = "&lt;i&gt;&amp;";
    let (setup, group) = mixed(Remote::None);
    repo(&setup.made, "api").branch(name).unwrap();
    let (mut cx, _window) = open(cx, &setup, &group);
    group_menu(&mut cx, &setup, &group, "Check out branch…");
    wait_for_element(&mut cx, "group-name".into());
    pick(&mut cx, name);
    submit(&mut cx);
    wait_for_element(&mut cx, "preflight".into());
    wait_preflight_ready(&mut cx);
    assert!(
        title(&mut cx)
            .unwrap()
            .starts_with(&format!("Checkout {name} ·"))
    );
    click(&mut cx, "preflight-cancel".into());
    wait_until(&mut cx, |cx| !shown(cx, "preflight"));
}
