//! M6-17: real-git branch dialogs (QA-2, QA-3), with JobEnded as the barrier.
use crate::history::{add, click, label_of, repo, repos, wait_until};
use crate::support::{Fixture, open, settle};
use gitcroc_app::app::{AppEvent, EVENT_CAPACITY};
use gitcroc_core::{RepoId, StartPoint};
use gitcroc_ui::branch_dialog::{BranchDialog, BranchMode};
use gpui_kit::test::TestWindowExt;
use gpui_kit::{AppContext, Entity, TestAppContext, VisualTestContext};

fn dialog(
    cx: &mut TestAppContext,
    f: &Fixture,
    id: RepoId,
    mode: BranchMode,
) -> (VisualTestContext, Entity<BranchDialog>) {
    let services = f.services.clone();
    let workspace = f.active();
    let (mut cx, view) = open(cx, move |window, cx| {
        cx.new(|cx| BranchDialog::new(services, workspace, id, "<api>&".into(), mode, window, cx))
    });
    cx.update(|window, cx| {
        use gitcroc_ui::dialog::Modal;
        view.update(cx, |view, cx| view.focus_first(window, cx));
    });
    settle(&mut cx, |_, _| true);
    (cx, view)
}
fn input(cx: &mut VisualTestContext, text: &str) {
    cx.update(|window, cx| window.input(text, cx));
    cx.run_until_parked();
}
fn ended(cx: &mut VisualTestContext, events: &mut tokio::sync::broadcast::Receiver<AppEvent>) {
    wait_until(cx, |_| {
        while let Ok(event) = events.try_recv() {
            if let AppEvent::JobEnded(job) = event {
                assert_eq!(
                    job.outcome,
                    Some(gitcroc_core::Outcome::Succeeded),
                    "{job:?}"
                );
                return true;
            }
        }
        false
    });
}

#[gpui::test]
fn sec_4_branch_name_is_checked_as_typed(cx: &mut TestAppContext) {
    let f = Fixture::start(EVENT_CAPACITY);
    let made = repos(&["api"]);
    let added = add(&f, &made);
    let (mut cx, _) = dialog(
        cx,
        &f,
        added[0].1.clone(),
        BranchMode::Create {
            start: StartPoint::Head,
        },
    );
    input(&mut cx, "bad..name");
    wait_until(&mut cx, |cx| {
        label_of(cx, "branch-name-error".into()).is_some()
    });
    cx.update(|window, _| assert!(window.find("dialog-submit").disabled() == Some(true)));
}

#[gpui::test]
fn ui_7_create_a_branch_from_each_start_point(cx: &mut TestAppContext) {
    let f = Fixture::start(EVENT_CAPACITY);
    let made = repos(&["api"]);
    let r = repo(&made, "api");
    r.git(["config", "core.autocrlf", "false"]).unwrap();
    r.tag("v1").unwrap();
    let tag = r.head().unwrap();
    r.commit_file("second", "second\n", "Second").unwrap();
    let default = r.head().unwrap();
    r.git(["update-ref", "refs/remotes/origin/main", &default])
        .unwrap();
    r.git([
        "symbolic-ref",
        "refs/remotes/origin/HEAD",
        "refs/remotes/origin/main",
    ])
    .unwrap();
    r.commit_file("third", "third\n", "Third").unwrap();
    let head = r.head().unwrap();
    let added = add(&f, &made);
    for (at, (start, expected)) in [
        (StartPoint::Head, head),
        (StartPoint::DefaultBranch, default.clone()),
        (
            StartPoint::Revision {
                revision: default.clone(),
            },
            default,
        ),
        (
            StartPoint::Revision {
                revision: "v1".into(),
            },
            tag,
        ),
    ]
    .into_iter()
    .enumerate()
    {
        for switch in [false, true] {
            let before = r.head().unwrap();
            let name = format!("created-{at}-{switch}");
            let (mut cx, _) = dialog(
                cx,
                &f,
                added[0].1.clone(),
                BranchMode::Create {
                    start: start.clone(),
                },
            );
            input(&mut cx, &name);
            if !switch {
                click(&mut cx, "branch-switch".into());
            }
            wait_until(&mut cx, |cx| {
                cx.update(|window, _| window.find("dialog-submit").disabled() != Some(true))
            });
            let mut events = f.app().subscribe();
            click(&mut cx, "dialog-submit".into());
            ended(&mut cx, &mut events);
            // HEAD is deliberately changed by the preceding checked-out branch.
            let expected = if matches!(start, StartPoint::Head) {
                before.clone()
            } else {
                expected.clone()
            };
            assert_eq!(r.git(["rev-parse", &name]).unwrap().trim(), expected);
            if switch {
                assert_eq!(r.git(["branch", "--show-current"]).unwrap().trim(), name);
            } else {
                assert_eq!(r.head().unwrap(), before);
            }
        }
    }
}

#[gpui::test]
fn ui_7_rename_and_delete_a_branch(cx: &mut TestAppContext) {
    let f = Fixture::start(EVENT_CAPACITY);
    let made = repos(&["api"]);
    let r = repo(&made, "api");
    r.branch("old").unwrap();
    let added = add(&f, &made);
    let (mut cx, _) = dialog(
        cx,
        &f,
        added[0].1.clone(),
        BranchMode::Rename { name: "old".into() },
    );
    // Rename starts with the original name selected by the input's SelectAll action.
    crate::history::press(
        &mut cx,
        if cfg!(target_os = "macos") {
            "cmd-a"
        } else {
            "ctrl-a"
        },
    );
    input(&mut cx, "renamed");
    wait_until(&mut cx, |cx| {
        cx.update(|window, _| window.find("dialog-submit").disabled() != Some(true))
    });
    let mut events = f.app().subscribe();
    click(&mut cx, "dialog-submit".into());
    ended(&mut cx, &mut events);
    let (mut cx, _) = dialog(
        &mut cx,
        &f,
        added[0].1.clone(),
        BranchMode::Delete {
            name: "renamed".into(),
            tracked: false,
        },
    );
    let mut events = f.app().subscribe();
    click(&mut cx, "dialog-submit".into());
    ended(&mut cx, &mut events);
    assert!(
        !r.git(["branch", "--list", "renamed"])
            .unwrap()
            .contains("renamed")
    );
}

#[gpui::test]
fn grp_9_force_delete_asks_first(cx: &mut TestAppContext) {
    let f = Fixture::start(EVENT_CAPACITY);
    let made = repos(&["api"]);
    let r = repo(&made, "api");
    r.branch("topic").unwrap();
    let added = add(&f, &made);
    let (mut cx, _) = dialog(
        cx,
        &f,
        added[0].1.clone(),
        BranchMode::Delete {
            name: "topic".into(),
            tracked: false,
        },
    );
    click(&mut cx, "branch-force".into());
    click(&mut cx, "dialog-submit".into());
    crate::history::wait_for_element(&mut cx, "branch-confirm-actions".into());
    let title = label_of(&mut cx, "dialog-title".into()).unwrap();
    assert!(title.contains("api"));
    assert!(
        label_of(&mut cx, "branch-confirm-actions".into())
            .unwrap()
            .contains("topic")
    );
    assert!(
        r.git(["branch", "--list", "topic"])
            .unwrap()
            .contains("topic")
    );
    cx.update(|window, _| assert_eq!(window.find("dialog-cancel").focused(), Some(true)));
    let jobs = f.app().jobs_get().len();
    crate::history::press(&mut cx, "enter");
    click(&mut cx, "dialog-cancel".into());
    assert_eq!(f.app().jobs_get().len(), jobs);
    assert!(
        r.git(["branch", "--list", "topic"])
            .unwrap()
            .contains("topic")
    );
    let (mut cx, _) = dialog(
        &mut cx,
        &f,
        added[0].1.clone(),
        BranchMode::Delete {
            name: "topic".into(),
            tracked: false,
        },
    );
    click(&mut cx, "branch-force".into());
    click(&mut cx, "dialog-submit".into());
    crate::history::wait_for_element(&mut cx, "branch-confirm-actions".into());
    let mut events = f.app().subscribe();
    click(&mut cx, "dialog-submit".into());
    ended(&mut cx, &mut events);
    assert!(
        !r.git(["branch", "--list", "topic"])
            .unwrap()
            .contains("topic")
    );
}

#[gpui::test]
fn sec_3_branch_dialogs_render_names_as_text(cx: &mut TestAppContext) {
    let f = Fixture::start(EVENT_CAPACITY);
    let made = repos(&["api"]);
    let added = add(&f, &made);
    let (mut cx, _) = dialog(
        cx,
        &f,
        added[0].1.clone(),
        BranchMode::Delete {
            name: "<topic>&".into(),
            tracked: false,
        },
    );
    assert_eq!(
        label_of(&mut cx, "dialog-title".into()).unwrap(),
        "Delete branch <topic>& in <api>&?"
    );
}

#[gpui::test]
fn ui_7_set_and_unset_the_upstream(cx: &mut TestAppContext) {
    let f = Fixture::start(EVENT_CAPACITY);
    let made = repos(&["api"]);
    let r = repo(&made, "api");
    let bare = tempfile::tempdir().unwrap();
    r.git([
        "clone",
        "--bare",
        r.path().to_str().unwrap(),
        bare.path().join("remote.git").to_str().unwrap(),
    ])
    .unwrap();
    r.git([
        "remote",
        "add",
        "origin",
        bare.path().join("remote.git").to_str().unwrap(),
    ])
    .unwrap();
    r.git(["fetch", "origin"]).unwrap();
    let added = add(&f, &made);
    let (mut cx, _) = dialog(
        cx,
        &f,
        added[0].1.clone(),
        BranchMode::Upstream {
            name: "main".into(),
            upstream: None,
        },
    );
    crate::history::wait_for_element(&mut cx, "branch-upstream-0".into());
    click(&mut cx, "branch-upstream-0".into());
    let mut events = f.app().subscribe();
    click(&mut cx, "dialog-submit".into());
    ended(&mut cx, &mut events);
    assert_eq!(
        r.git(["rev-parse", "--abbrev-ref", "@{upstream}"])
            .unwrap()
            .trim(),
        "origin/main"
    );
    let (mut cx, _) = dialog(
        &mut cx,
        &f,
        added[0].1.clone(),
        BranchMode::Run {
            op: gitcroc_core::BranchOp::SetUpstream {
                name: "main".into(),
                upstream: None,
            },
        },
    );
    let mut events = f.app().subscribe(); // Run requests start when rendered, before any click.
    click(&mut cx, "dialog-submit".into());
    ended(&mut cx, &mut events);
    assert!(
        !r.git_output(["rev-parse", "@{upstream}"])
            .unwrap()
            .status
            .success()
    );
}

#[gpui::test]
fn ui_7_check_out_a_remote_branch_and_delete_it_on_the_remote(cx: &mut TestAppContext) {
    let f = Fixture::start(EVENT_CAPACITY);
    let made = repos(&["api"]);
    let r = repo(&made, "api");
    r.branch("topic").unwrap();
    let bare = tempfile::tempdir().unwrap();
    let remote = bare.path().join("remote.git");
    r.git([
        "clone",
        "--bare",
        r.path().to_str().unwrap(),
        remote.to_str().unwrap(),
    ])
    .unwrap();
    r.git(["remote", "add", "origin", remote.to_str().unwrap()])
        .unwrap();
    r.git(["fetch", "origin"]).unwrap();
    r.git(["branch", "-d", "topic"]).unwrap();
    let added = add(&f, &made);
    let branch = gitcroc_core::RemoteBranchRef {
        remote: "origin".into(),
        branch: "topic".into(),
    };
    let (mut cx, _) = dialog(
        cx,
        &f,
        added[0].1.clone(),
        BranchMode::Run {
            op: gitcroc_core::BranchOp::CheckoutRemote {
                branch: branch.clone(),
            },
        },
    );
    let mut events = f.app().subscribe();
    click(&mut cx, "dialog-submit".into());
    ended(&mut cx, &mut events);
    assert_eq!(r.git(["branch", "--show-current"]).unwrap().trim(), "topic");
    let (mut cx, _) = dialog(
        &mut cx,
        &f,
        added[0].1.clone(),
        BranchMode::Run {
            op: gitcroc_core::BranchOp::DeleteRemote { branch },
        },
    );
    click(&mut cx, "dialog-submit".into());
    crate::history::wait_for_element(&mut cx, "branch-confirm-actions".into());
    let mut events = f.app().subscribe();
    click(&mut cx, "dialog-submit".into());
    ended(&mut cx, &mut events);
    assert!(
        r.git(["ls-remote", "--heads", "origin", "topic"])
            .unwrap()
            .is_empty()
    );
}

#[gpui::test]
fn ui_7_menus_open_from_the_keyboard(cx: &mut TestAppContext) {
    let f = Fixture::start(EVENT_CAPACITY);
    let made = repos(&["api"]);
    let added = add(&f, &made);
    let (mut cx, window) = crate::support::main_window(cx, &f);
    crate::stage::enlarge(&mut cx);
    crate::changes::show_repo(&mut cx, &window, &added, "api");
    crate::history::press(&mut cx, "shift-f10");
    crate::history::wait_for_element(&mut cx, "menu".into());
    crate::history::press(&mut cx, "escape");
    cx.update(|window, _| {
        assert!(window.try_find("menu").is_none());
        assert_eq!(window.find("tree-list").focused(), Some(true));
    });
    crate::history::wait_for_rows(&mut cx, &window, 1);
    click(&mut cx, "history-row-0".into());
    crate::history::press(&mut cx, "shift-f10");
    crate::history::wait_for_element(&mut cx, "menu".into());
    crate::history::press(&mut cx, "escape");
    cx.update(|window, _| {
        assert!(window.try_find("menu").is_none());
        assert_eq!(window.find("history").focused(), Some(true));
    });
    crate::history::press(&mut cx, "menu");
    crate::history::wait_for_element(&mut cx, "menu".into());
    crate::history::press(&mut cx, "escape");
}

#[gpui::test]
fn ui_7_double_click_checks_out_a_local_branch(cx: &mut TestAppContext) {
    let f = Fixture::start(EVENT_CAPACITY);
    let made = repos(&["api"]);
    let r = repo(&made, "api");
    r.branch("topic").unwrap();
    let added = add(&f, &made);
    let (mut cx, window) = crate::support::main_window(cx, &f);
    crate::stage::enlarge(&mut cx);
    crate::changes::show_repo(&mut cx, &window, &added, "api");
    click(
        &mut cx,
        crate::history::tree_part(&format!("repo:{}", added[0].1.0), "chevron"),
    );
    let node = format!("repo:{}:local", added[0].1.0);
    click(&mut cx, crate::history::tree_part(&node, "chevron"));
    let row = crate::history::tree_row(&format!("repo:{}:local:ref:topic", added[0].1.0));
    crate::history::wait_for_element(&mut cx, row.clone());
    let mut events = f.app().subscribe();
    cx.update(|window, cx| window.double_click(row, cx));
    ended(&mut cx, &mut events);
    assert_eq!(r.git(["branch", "--show-current"]).unwrap().trim(), "topic");
}

#[gpui::test]
fn ui_13_commit_context_menu(cx: &mut TestAppContext) {
    let f = Fixture::start(EVENT_CAPACITY);
    let made = repos(&["api"]);
    let r = repo(&made, "api");
    let hash = r.head().unwrap();
    let added = add(&f, &made);
    let (mut cx, window) = crate::support::main_window(cx, &f);
    crate::stage::enlarge(&mut cx);
    crate::changes::show_repo(&mut cx, &window, &added, "api");
    crate::history::wait_for_rows(&mut cx, &window, 1);
    click(&mut cx, "history-row-0".into());
    crate::history::press(&mut cx, "shift-f10");
    crate::history::wait_for_element(&mut cx, "menu".into());
    click(
        &mut cx,
        gpui_kit::ElementId::NamedInteger("menu-item".into(), 6),
    );
    cx.update(|_, cx| assert_eq!(cx.read_from_clipboard().unwrap().text().unwrap(), hash));
    crate::history::press(&mut cx, "shift-f10");
    click(
        &mut cx,
        gpui_kit::ElementId::NamedInteger("menu-item".into(), 1),
    );
    crate::history::wait_for_element(&mut cx, "branch-dialog".into());
    input(&mut cx, "from-commit");
    wait_until(&mut cx, |cx| {
        cx.update(|window, _| window.find("dialog-submit").disabled() != Some(true))
    });
    let mut events = f.app().subscribe();
    click(&mut cx, "dialog-submit".into());
    ended(&mut cx, &mut events);
    assert_eq!(r.git(["rev-parse", "from-commit"]).unwrap().trim(), hash);
    crate::history::wait_for_element(&mut cx, "history-row-0".into());
    click(&mut cx, "history-row-0".into());
    crate::history::press(&mut cx, "shift-f10");
    let mut events = f.app().subscribe();
    click(
        &mut cx,
        gpui_kit::ElementId::NamedInteger("menu-item".into(), 0),
    );
    ended(&mut cx, &mut events);
    assert!(
        !r.git_output(["symbolic-ref", "HEAD"])
            .unwrap()
            .status
            .success()
    );
}

#[gpui::test]
fn ui_13_cherry_pick_needs_rows_of_one_repo(cx: &mut TestAppContext) {
    let f = Fixture::start(EVENT_CAPACITY);
    let made = repos(&["api"]);
    let r = repo(&made, "api");
    r.commit_file("two", "two\n", "Second").unwrap();
    r.commit_file("three", "three\n", "Third").unwrap();
    let added = add(&f, &made);
    let (mut cx, window) = crate::support::main_window(cx, &f);
    crate::stage::enlarge(&mut cx);
    crate::changes::show_repo(&mut cx, &window, &added, "api");
    crate::history::wait_for_rows(&mut cx, &window, 3);
    click(&mut cx, "history-row-0".into());
    cx.update(|window, cx| {
        window.click_with_modifiers("history-row-1", gpui_kit::Modifiers::shift(), cx)
    });
    cx.run_until_parked();
    cx.update(|_, cx| {
        assert_eq!(
            window.read(cx).history(cx).unwrap().read(cx).marked().len(),
            2
        )
    });
    crate::history::press(&mut cx, "shift-f10");
    crate::history::wait_for_element(&mut cx, "menu".into());
    let labels = crate::history::labels_in(&mut cx, "menu".into(), gpui_kit::Role::MenuItem);
    assert!(labels.iter().any(|label| label == "Cherry-pick 2 commits"));
}

#[gpui::test]
fn ui_7_open_in_file_manager_terminal_and_remove(cx: &mut TestAppContext) {
    use gitcroc_app::platform::{FileManager, Launch, Terminal};
    use std::sync::atomic::{AtomicUsize, Ordering};
    static FILES: AtomicUsize = AtomicUsize::new(0);
    static TERMS: AtomicUsize = AtomicUsize::new(0);
    struct Files;
    struct Term;
    impl FileManager for Files {
        fn show(path: &std::path::Path) -> Launch {
            FILES.fetch_add(1, Ordering::SeqCst);
            Launch::new("git", ["--version".into()], path.to_owned())
        }
    }
    impl Terminal for Term {
        fn open_in(path: &std::path::Path) -> Vec<Launch> {
            TERMS.fetch_add(1, Ordering::SeqCst);
            vec![Launch::new("git", ["--version".into()], path.to_owned())]
        }
    }
    let mut f = Fixture::start(EVENT_CAPACITY);
    f.services = f.services.clone().with_repo_openers::<Files, Term>();
    let made = repos(&["api"]);
    let added = add(&f, &made);
    let (mut cx, window) = crate::support::main_window(cx, &f);
    crate::stage::enlarge(&mut cx);
    crate::changes::show_repo(&mut cx, &window, &added, "api");
    let row = crate::history::tree_row(&format!("repo:{}", added[0].1.0));
    for (at, counter) in [(3, &FILES), (4, &TERMS)] {
        cx.update(|window, cx| window.right_click(row.clone(), cx));
        cx.run_until_parked();
        click(
            &mut cx,
            gpui_kit::ElementId::NamedInteger("menu-item".into(), at),
        );
        wait_until(&mut cx, |_| counter.load(Ordering::SeqCst) > 0);
    }
    cx.update(|window, cx| window.right_click(row.clone(), cx));
    cx.run_until_parked();
    click(
        &mut cx,
        gpui_kit::ElementId::NamedInteger("menu-item".into(), 5),
    );
    wait_until(&mut cx, |_| {
        f.app()
            .workspace_get()
            .open
            .iter()
            .find(|view| view.id == f.active())
            .unwrap()
            .repos
            .is_empty()
    });
    assert!(repo(&made, "api").path().exists());
}

#[gpui::test]
fn arch_6_branch_refusals_and_choices_failures_stay_in_the_dialog(cx: &mut TestAppContext) {
    let f = Fixture::start(EVENT_CAPACITY);
    let (mut cx, _) = dialog(
        cx,
        &f,
        RepoId("missing".into()),
        BranchMode::Create {
            start: StartPoint::Head,
        },
    );
    crate::history::wait_for_element(&mut cx, "branch-choices-error".into());
    input(&mut cx, "valid-name");
    wait_until(&mut cx, |cx| {
        cx.update(|window, _| window.find("dialog-submit").disabled() != Some(true))
    });
    click(&mut cx, "dialog-submit".into());
    wait_until(&mut cx, |cx| label_of(cx, "dialog-error".into()).is_some());
    cx.update(|window, _| assert!(window.try_find("branch-dialog").is_some()));
}

#[gpui::test]
fn ui_13_merged_selection_and_context_act_on_the_rows_repo(cx: &mut TestAppContext) {
    let f = Fixture::start(EVENT_CAPACITY);
    let made = repos(&["api", "web"]);
    let added = add(&f, &made);
    let group =
        crate::history_merged::group(&f, "Both", added.iter().map(|(_, id)| id.clone()).collect());
    let (mut cx, window) = crate::support::main_window(cx, &f);
    crate::stage::enlarge(&mut cx);
    let row = crate::history::tree_row(&crate::history_merged::group_node(&group));
    crate::history::wait_for_element(&mut cx, row.clone());
    click(&mut cx, row);
    crate::history::wait_for_rows(&mut cx, &window, 2);
    click(&mut cx, "history-row-0".into());
    let modifiers = if cfg!(target_os = "macos") {
        gpui_kit::Modifiers::command()
    } else {
        gpui_kit::Modifiers::control()
    };
    cx.update(|window, cx| window.click_with_modifiers("history-row-1", modifiers, cx));
    cx.run_until_parked();
    cx.update(|_, cx| {
        assert_eq!(
            window.read(cx).history(cx).unwrap().read(cx).marked().len(),
            2
        )
    });
    crate::history::press(&mut cx, "shift-f10");
    crate::history::wait_for_element(&mut cx, "menu".into());
    cx.update(|window, _| {
        assert_eq!(
            window
                .find(gpui_kit::ElementId::NamedInteger("menu-item".into(), 3))
                .description(),
            Some(gitcroc_ui::strings::tree_menu::ONE_REPO)
        )
    });
    crate::history::press(&mut cx, "escape");
    click(&mut cx, "history-row-1".into());
    let selected = cx.update(|_, cx| {
        window
            .read(cx)
            .history(cx)
            .unwrap()
            .read(cx)
            .selection()
            .unwrap()
    });
    let gitcroc_ui::history::Selection::Commit { repo: target, .. } = selected else {
        panic!("not a commit")
    };
    crate::history::press(&mut cx, "shift-f10");
    let mut events = f.app().subscribe();
    click(
        &mut cx,
        gpui_kit::ElementId::NamedInteger("menu-item".into(), 0),
    );
    ended(&mut cx, &mut events);
    for (name, id) in &added {
        assert_eq!(
            !repo(&made, name)
                .git_output(["symbolic-ref", "HEAD"])
                .unwrap()
                .status
                .success(),
            *id == target
        );
    }
}

#[gpui::test]
fn ui_7_tag_menu_creates_a_branch_at_the_tag(cx: &mut TestAppContext) {
    let f = Fixture::start(EVENT_CAPACITY);
    let made = repos(&["api"]);
    let r = repo(&made, "api");
    r.tag("v1").unwrap();
    let tag = r.head().unwrap();
    r.commit_file("second", "next\n", "Next").unwrap();
    let added = add(&f, &made);
    let (mut cx, window) = crate::support::main_window(cx, &f);
    crate::stage::enlarge(&mut cx);
    crate::changes::show_repo(&mut cx, &window, &added, "api");
    click(
        &mut cx,
        crate::history::tree_part(&format!("repo:{}", added[0].1.0), "chevron"),
    );
    click(
        &mut cx,
        crate::history::tree_part(&format!("repo:{}:tags", added[0].1.0), "chevron"),
    );
    let row = crate::history::tree_row(&format!("repo:{}:tags:ref:v1", added[0].1.0));
    crate::history::wait_for_element(&mut cx, row.clone());
    cx.update(|window, cx| window.right_click(row, cx));
    cx.run_until_parked();
    click(
        &mut cx,
        gpui_kit::ElementId::NamedInteger("menu-item".into(), 1),
    );
    crate::history::wait_for_element(&mut cx, "branch-dialog".into());
    input(&mut cx, "from-tag");
    wait_until(&mut cx, |cx| {
        cx.update(|window, _| window.find("dialog-submit").disabled() != Some(true))
    });
    let mut events = f.app().subscribe();
    click(&mut cx, "dialog-submit".into());
    ended(&mut cx, &mut events);
    assert_eq!(r.head().unwrap(), tag);
}

#[gpui::test]
fn ui_7_a_menu_with_only_later_actions_still_closes_from_the_keyboard(cx: &mut TestAppContext) {
    let f = Fixture::start(EVENT_CAPACITY);
    let made = repos(&["api"]);
    add(&f, &made);
    let (mut cx, _) = crate::support::main_window(cx, &f);
    let row = crate::history::tree_row("ungrouped");
    crate::history::wait_for_element(&mut cx, row.clone());
    cx.update(|window, cx| window.right_click(row, cx));
    cx.run_until_parked();
    crate::history::wait_for_element(&mut cx, "menu".into());
    crate::history::press(&mut cx, "enter");
    cx.update(|window, _| assert!(window.try_find("menu").is_some()));
    crate::history::press(&mut cx, "escape");
    cx.update(|window, _| assert!(window.try_find("menu").is_none()));
}
