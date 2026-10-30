//! M6-18 real-git operation dialogs; tests precede the UI implementation.
use crate::history::{add, click, label_of, repo, wait_until};
use crate::support::{Fixture, open, settle};
use gitcroc_app::app::{AppEvent, EVENT_CAPACITY};
use gitcroc_core::{RepoId, ResetMode};
use gitcroc_ui::operation_dialog::{OperationDialog, OperationMode};
use gpui_kit::test::TestWindowExt;
use gpui_kit::{AppContext, Entity, TestAppContext, VisualTestContext};

// repo-gen gives only its own commands an identity through environment
// variables. App-started commands must also work on runners with no global
// git config, so give these fixtures a repo-local committer identity.
// Keep app and fixture commands consistent when the host enables autocrlf.
fn repos(names: &[&str]) -> repo_gen::Fixture {
    let made = crate::history::repos(names);
    for name in names {
        let repo = repo(&made, name);
        repo.git(["config", "core.autocrlf", "false"]).unwrap();
        repo.git(["config", "user.name", "Test User"]).unwrap();
        repo.git(["config", "user.email", "test@example.com"])
            .unwrap();
    }
    made
}

fn dialog(
    cx: &mut TestAppContext,
    f: &Fixture,
    id: RepoId,
    mode: OperationMode,
) -> (VisualTestContext, Entity<OperationDialog>) {
    let services = f.services.clone();
    let workspace = f.active();
    let (mut cx, view) = open(cx, move |window, cx| {
        cx.new(|cx| {
            OperationDialog::new(services, workspace, id, "<api>&".into(), mode, window, cx)
        })
    });
    cx.update(|window, cx| {
        use gitcroc_ui::dialog::Modal;
        view.update(cx, |view, cx| view.focus_first(window, cx));
    });
    settle(&mut cx, |_, _| true);
    (cx, view)
}
fn fill(cx: &mut VisualTestContext, id: &'static str, value: &str) {
    click(cx, id.into());
    cx.update(|window, cx| window.input(value, cx));
    cx.run_until_parked();
}
fn ready(cx: &mut VisualTestContext) {
    wait_until(cx, |cx| {
        cx.update(|window, _| window.find("dialog-submit").disabled() != Some(true))
    });
}
fn ended(cx: &mut VisualTestContext, events: &mut tokio::sync::broadcast::Receiver<AppEvent>) {
    wait_until(cx, |_| {
        while let Ok(event) = events.try_recv() {
            if let AppEvent::JobEnded(job) = event {
                assert!(
                    matches!(
                        job.outcome,
                        Some(gitcroc_core::Outcome::Succeeded | gitcroc_core::Outcome::NothingToDo)
                    ),
                    "{job:?}"
                );
                return true;
            }
        }
        false
    });
}
#[gpui::test]
fn sec_4_tag_and_remote_names_are_checked_as_typed(root: &mut TestAppContext) {
    let f = Fixture::start(EVENT_CAPACITY);
    let made = repos(&["api"]);
    let added = add(&f, &made);
    for mode in [
        OperationMode::TagCreate { commit: None },
        OperationMode::RemoteAdd,
    ] {
        let (mut cx, _) = dialog(root, &f, added[0].1.clone(), mode);
        fill(&mut cx, "op-name", "bad..name");
        wait_until(&mut cx, |cx| label_of(cx, "op-name-error".into()).is_some());
        cx.update(|window, _| assert_eq!(window.find("dialog-submit").disabled(), Some(true)));
    }
}
#[gpui::test]
fn ui_7_create_lightweight_and_annotated_tags(root: &mut TestAppContext) {
    let f = Fixture::start(EVENT_CAPACITY);
    let made = repos(&["api"]);
    let r = repo(&made, "api");
    let added = add(&f, &made);
    for annotated in [false, true] {
        let name = if annotated {
            "annotated"
        } else {
            "lightweight"
        };
        let (mut cx, _) = dialog(
            root,
            &f,
            added[0].1.clone(),
            OperationMode::TagCreate { commit: None },
        );
        fill(&mut cx, "op-name", name);
        if annotated {
            fill(&mut cx, "op-message", "<message>&");
        } else {
            click(&mut cx, "op-annotated".into());
        }
        ready(&mut cx);
        let mut events = f.app().subscribe();
        click(&mut cx, "dialog-submit".into());
        ended(&mut cx, &mut events);
        assert_eq!(
            r.git(["cat-file", "-t", name]).unwrap().trim(),
            if annotated { "tag" } else { "commit" }
        );
        assert_eq!(
            r.git(["rev-parse", &format!("{name}^{{}}")])
                .unwrap()
                .trim(),
            r.head().unwrap()
        );
    }
}
#[gpui::test]
fn grp_9_hard_reset_requires_explicit_confirmation(root: &mut TestAppContext) {
    let f = Fixture::start(EVENT_CAPACITY);
    let made = repos(&["api"]);
    let r = repo(&made, "api");
    r.git(["config", "core.autocrlf", "false"]).unwrap();
    let initial = r.head().unwrap();
    r.commit_file("new", "new\n", "Second").unwrap();
    let before = r.head().unwrap();
    let added = add(&f, &made);
    let (mut cx, _) = dialog(
        root,
        &f,
        added[0].1.clone(),
        OperationMode::Reset {
            commit: initial.clone(),
            mode: ResetMode::Mixed,
        },
    );
    click(&mut cx, "op-choice-2".into());
    click(&mut cx, "dialog-submit".into());
    wait_until(&mut cx, |cx| {
        label_of(cx, "op-confirm-actions".into()).is_some()
    });
    cx.update(|window, cx| {
        window.press("enter", cx);
    });
    assert_eq!(r.head().unwrap(), before);
    // Enter activates focused Cancel; reopen and explicitly confirm.
    let (mut cx, _) = dialog(
        root,
        &f,
        added[0].1.clone(),
        OperationMode::Reset {
            commit: initial.clone(),
            mode: ResetMode::Hard,
        },
    );
    click(&mut cx, "dialog-submit".into());
    wait_until(&mut cx, |cx| {
        label_of(cx, "op-confirm-actions".into()).is_some()
    });
    let mut events = f.app().subscribe();
    click(&mut cx, "dialog-submit".into());
    ended(&mut cx, &mut events);
    assert_eq!(r.head().unwrap(), initial);
}
#[gpui::test]
fn ui_7_remote_add_rename_url_and_confirmed_remove(root: &mut TestAppContext) {
    let f = Fixture::start(EVENT_CAPACITY);
    let made = repos(&["api"]);
    let r = repo(&made, "api");
    let added = add(&f, &made);
    let (mut cx, _) = dialog(root, &f, added[0].1.clone(), OperationMode::RemoteAdd);
    fill(&mut cx, "op-name", "backup");
    fill(&mut cx, "op-url", "https://example.invalid/repo.git");
    ready(&mut cx);
    let mut events = f.app().subscribe();
    click(&mut cx, "dialog-submit".into());
    ended(&mut cx, &mut events);
    let (mut cx, _) = dialog(
        root,
        &f,
        added[0].1.clone(),
        OperationMode::RemoteRename {
            name: "backup".into(),
        },
    );
    click(&mut cx, "op-name".into());
    cx.update(|window, cx| {
        window.press(
            if cfg!(target_os = "macos") {
                "cmd-a"
            } else {
                "ctrl-a"
            },
            cx,
        );
        window.input("archive", cx);
    });
    ready(&mut cx);
    let mut events = f.app().subscribe();
    click(&mut cx, "dialog-submit".into());
    ended(&mut cx, &mut events);
    r.git([
        "config",
        "remote.archive.pushurl",
        "https://example.invalid/push.git",
    ])
    .unwrap();
    let (mut cx, _) = dialog(
        root,
        &f,
        added[0].1.clone(),
        OperationMode::RemoteUrl {
            name: "archive".into(),
        },
    );
    ready(&mut cx);
    click(&mut cx, "op-url".into());
    cx.update(|window, cx| {
        window.press(
            if cfg!(target_os = "macos") {
                "cmd-a"
            } else {
                "ctrl-a"
            },
            cx,
        );
        window.input("https://example.invalid/new.git", cx);
    });
    click(&mut cx, "op-push-url".into());
    cx.update(|window, cx| {
        window.press(
            if cfg!(target_os = "macos") {
                "cmd-a"
            } else {
                "ctrl-a"
            },
            cx,
        );
        window.input("https://example.invalid/new-push.git", cx);
    });
    let mut events = f.app().subscribe();
    click(&mut cx, "dialog-submit".into());
    ended(&mut cx, &mut events);
    assert_eq!(
        r.git(["remote", "get-url", "--push", "archive"])
            .unwrap()
            .trim(),
        "https://example.invalid/new-push.git"
    );
    assert_eq!(
        r.git(["remote", "get-url", "archive"]).unwrap().trim(),
        "https://example.invalid/new.git"
    );
    let (mut cx, view) = dialog(
        root,
        &f,
        added[0].1.clone(),
        OperationMode::Run(gitcroc_ui::operation_dialog::Operation::Remote(
            gitcroc_core::RemoteOp::Remove {
                name: "archive".into(),
            },
        )),
    );
    cx.update(|window, cx| view.update(cx, |view, cx| view.submit(window, cx)));
    wait_until(&mut cx, |cx| {
        label_of(cx, "op-confirm-actions".into()).is_some()
    });
    let mut events = f.app().subscribe();
    click(&mut cx, "dialog-submit".into());
    ended(&mut cx, &mut events);
    assert!(r.git(["remote"]).unwrap().trim().is_empty());
}
#[gpui::test]
fn ui_20_stash_options_and_ui_7_apply_pop_drop(root: &mut TestAppContext) {
    let f = Fixture::start(EVENT_CAPACITY);
    let made = repos(&["api"]);
    let r = repo(&made, "api");
    r.git(["config", "core.autocrlf", "false"]).unwrap();
    std::fs::write(r.path().join("untracked"), "saved\n").unwrap();
    let added = add(&f, &made);
    let (mut cx, _) = dialog(root, &f, added[0].1.clone(), OperationMode::StashPush);
    fill(&mut cx, "op-message", "<stash>&");
    click(&mut cx, "op-untracked".into());
    let mut events = f.app().subscribe();
    click(&mut cx, "dialog-submit".into());
    ended(&mut cx, &mut events);
    assert!(!r.path().join("untracked").exists());
    let stash = gitcroc_core::StashRef {
        index: 0,
        hash: r.git(["rev-parse", "stash@{0}"]).unwrap().trim().into(),
    };
    for pop in [false, true] {
        let op = if pop {
            gitcroc_core::StashOp::Pop {
                stash: stash.clone(),
            }
        } else {
            gitcroc_core::StashOp::Apply {
                stash: stash.clone(),
            }
        };
        let (mut cx, _) = dialog(
            root,
            &f,
            added[0].1.clone(),
            OperationMode::Run(gitcroc_ui::operation_dialog::Operation::Stash(op)),
        );
        let mut events = f.app().subscribe();
        click(&mut cx, "dialog-submit".into());
        ended(&mut cx, &mut events);
        assert_eq!(
            std::fs::read_to_string(r.path().join("untracked")).unwrap(),
            "saved\n"
        );
        if !pop {
            std::fs::remove_file(r.path().join("untracked")).unwrap();
        }
    }
    assert!(r.git(["stash", "list"]).unwrap().is_empty());
}
#[gpui::test]
fn ui_20_merge_and_rebase_use_core_filtered_targets(root: &mut TestAppContext) {
    for rebase in [false, true] {
        let f = Fixture::start(EVENT_CAPACITY);
        let made = repos(&["api"]);
        let r = repo(&made, "api");
        r.branch("topic").unwrap();
        r.git(["checkout", "topic"]).unwrap();
        r.commit_file("topic", "topic\n", "Topic").unwrap();
        let tip = r.head().unwrap();
        r.git(["checkout", "main"]).unwrap();
        let added = add(&f, &made);
        let (mut cx, _) = dialog(root, &f, added[0].1.clone(), OperationMode::Merge);
        fill(&mut cx, "op-revision", "topic");
        wait_until(&mut cx, |cx| label_of(cx, "op-target-0".into()).is_some());
        if rebase {
            click(&mut cx, "op-rebase".into());
        }
        let mut events = f.app().subscribe();
        click(&mut cx, "dialog-submit".into());
        ended(&mut cx, &mut events);
        assert_eq!(r.head().unwrap(), tip);
    }
}
#[gpui::test]
fn ui_13_cherry_pick_multiple_and_revert_choose_mainline(root: &mut TestAppContext) {
    let f = Fixture::start(EVENT_CAPACITY);
    let made = repos(&["api"]);
    let r = repo(&made, "api");
    r.branch("topic").unwrap();
    r.git(["checkout", "topic"]).unwrap();
    r.commit_file("a", "a\n", "A").unwrap();
    let a = r.head().unwrap();
    r.commit_file("b", "b\n", "B").unwrap();
    let b = r.head().unwrap();
    r.git(["checkout", "main"]).unwrap();
    let added = add(&f, &made);
    let (mut cx, _) = dialog(
        root,
        &f,
        added[0].1.clone(),
        OperationMode::Run(gitcroc_ui::operation_dialog::Operation::Integrate(
            gitcroc_core::IntegrateOp::CherryPick {
                commits: vec![b, a],
            },
        )),
    );
    let mut events = f.app().subscribe();
    click(&mut cx, "dialog-submit".into());
    ended(&mut cx, &mut events);
    assert!(r.path().join("a").exists() && r.path().join("b").exists());
    r.git(["checkout", "topic"]).unwrap();
    r.commit_file("c", "c\n", "C").unwrap();
    r.git(["checkout", "main"]).unwrap();
    r.git(["merge", "--no-ff", "topic", "-m", "Merge"]).unwrap();
    let merge = r.head().unwrap();
    let (mut cx, _) = dialog(
        root,
        &f,
        added[0].1.clone(),
        OperationMode::Run(gitcroc_ui::operation_dialog::Operation::Integrate(
            gitcroc_core::IntegrateOp::Revert {
                commit: merge,
                mainline: None,
            },
        )),
    );
    click(&mut cx, "dialog-submit".into());
    wait_until(&mut cx, |cx| label_of(cx, "op-parent-0".into()).is_some());
    let mut events = f.app().subscribe();
    click(&mut cx, "dialog-submit".into());
    ended(&mut cx, &mut events);
}
#[gpui::test]
fn ui_20_sync_defaults_options_and_remote_choice(root: &mut TestAppContext) {
    let f = Fixture::start(EVENT_CAPACITY);
    let made = repos(&["api"]);
    let r = repo(&made, "api");
    let remote_dir = tempfile::tempdir().unwrap();
    let remote = remote_dir.path().join("remote.git");
    r.git(["init", "--bare", remote.to_str().unwrap()]).unwrap();
    r.git(["remote", "add", "backup", remote.to_str().unwrap()])
        .unwrap();
    r.git(["remote", "add", "other", remote.to_str().unwrap()])
        .unwrap();
    let added = add(&f, &made);
    let (mut cx, _) = dialog(root, &f, added[0].1.clone(), OperationMode::Push);
    wait_until(&mut cx, |cx| label_of(cx, "op-remote-0".into()).is_some());
    cx.update(|window, _| assert_eq!(window.find("dialog-submit").disabled(), Some(true)));
    click(&mut cx, "op-remote-0".into());
    ready(&mut cx);
    let mut events = f.app().subscribe();
    click(&mut cx, "dialog-submit".into());
    ended(&mut cx, &mut events);
    assert_eq!(
        r.git(["rev-parse", "--abbrev-ref", "@{upstream}"])
            .unwrap()
            .trim(),
        "backup/main"
    );
    for mode in [OperationMode::Fetch, OperationMode::Pull] {
        let (mut cx, _) = dialog(root, &f, added[0].1.clone(), mode);
        let mut events = f.app().subscribe();
        click(&mut cx, "dialog-submit".into());
        ended(&mut cx, &mut events);
    }
}
#[gpui::test]
fn arch_6_refusals_stay_inline_and_sec_3_names_are_text(root: &mut TestAppContext) {
    let f = Fixture::start(EVENT_CAPACITY);
    let made = repos(&["api"]);
    let added = add(&f, &made);
    let (mut cx, _) = dialog(root, &f, added[0].1.clone(), OperationMode::Merge);
    assert!(
        label_of(&mut cx, "dialog-title".into())
            .unwrap()
            .contains("<api>&")
    );
    fill(&mut cx, "op-revision", "missing-ref");
    click(&mut cx, "dialog-submit".into());
    wait_until(&mut cx, |cx| label_of(cx, "dialog-error".into()).is_some());
}
#[gpui::test]
fn ui_20_toolbar_uses_the_selected_repo_and_defaults(root: &mut TestAppContext) {
    use crate::history::{repo_node, tree_row, wait_for_element};
    let f = Fixture::start(EVENT_CAPACITY);
    let made = repos(&["api"]);
    let added = add(&f, &made);
    let (mut cx, _) = crate::support::main_window(root, &f);
    wait_for_element(&mut cx, tree_row(&repo_node(&added, "api")));
    click(&mut cx, tree_row(&repo_node(&added, "api")));
    for (at, text) in [(0, "Fetch"), (1, "Pull"), (2, "Push")] {
        let id = format!("toolbar-options-{at}");
        click(&mut cx, id.into());
        wait_for_element(&mut cx, "operation-dialog".into());
        assert!(
            label_of(&mut cx, "dialog-title".into())
                .unwrap()
                .contains(text)
        );
        crate::history::press(&mut cx, "escape");
    }
    click(&mut cx, "toolbar-3".into());
    wait_for_element(&mut cx, "branch-dialog".into());
    crate::history::press(&mut cx, "escape");
    // Ungrouped is a target set, whose operations wait for M6-22 preflight.
    click(&mut cx, tree_row("ungrouped"));
    cx.update(|window, _| {
        for at in 0..6 {
            assert_eq!(
                window
                    .find(gpui_kit::ElementId::Name(format!("toolbar-{at}").into()))
                    .disabled(),
                Some(true)
            );
        }
    });
}
#[gpui::test]
fn ui_13_soft_and_mixed_reset_keep_working_copy(root: &mut TestAppContext) {
    for mode in [ResetMode::Soft, ResetMode::Mixed] {
        let f = Fixture::start(EVENT_CAPACITY);
        let made = repos(&["api"]);
        let r = repo(&made, "api");
        r.git(["config", "core.autocrlf", "false"]).unwrap();
        let base = r.head().unwrap();
        r.commit_file("second", "kept\n", "Second").unwrap();
        let added = add(&f, &made);
        let (mut cx, _) = dialog(
            root,
            &f,
            added[0].1.clone(),
            OperationMode::Reset {
                commit: base.clone(),
                mode,
            },
        );
        let mut events = f.app().subscribe();
        click(&mut cx, "dialog-submit".into());
        ended(&mut cx, &mut events);
        assert_eq!(r.head().unwrap(), base);
        assert_eq!(
            std::fs::read_to_string(r.path().join("second")).unwrap(),
            "kept\n"
        );
        let staged = r.git(["diff", "--cached", "--name-only"]).unwrap();
        assert_eq!(staged.trim() == "second", mode == ResetMode::Soft);
    }
}
#[gpui::test]
fn grp_9_force_push_cancel_and_confirm(root: &mut TestAppContext) {
    let f = Fixture::start(EVENT_CAPACITY);
    let made = repos(&["api"]);
    let r = repo(&made, "api");
    let dir = tempfile::tempdir().unwrap();
    let remote = dir.path().join("remote.git");
    r.git(["init", "--bare", remote.to_str().unwrap()]).unwrap();
    r.git(["remote", "add", "origin", remote.to_str().unwrap()])
        .unwrap();
    r.git(["push", "-u", "origin", "main"]).unwrap();
    let old = r.head().unwrap();
    r.commit_file("new", "new\n", "New").unwrap();
    let added = add(&f, &made);
    let (mut cx, _) = dialog(root, &f, added[0].1.clone(), OperationMode::Push);
    ready(&mut cx);
    click(&mut cx, "op-force".into());
    click(&mut cx, "dialog-submit".into());
    wait_until(&mut cx, |cx| {
        label_of(cx, "op-confirm-actions".into()).is_some()
    });
    cx.update(|window, _| assert_eq!(window.find("dialog-cancel").focused(), Some(true)));
    click(&mut cx, "dialog-cancel".into());
    assert_eq!(r.git(["rev-parse", "origin/main"]).unwrap().trim(), old);
    let (mut cx, _) = dialog(root, &f, added[0].1.clone(), OperationMode::Push);
    ready(&mut cx);
    click(&mut cx, "op-force".into());
    click(&mut cx, "dialog-submit".into());
    wait_until(&mut cx, |cx| {
        label_of(cx, "op-confirm-actions".into()).is_some()
    });
    let mut events = f.app().subscribe();
    click(&mut cx, "dialog-submit".into());
    ended(&mut cx, &mut events);
    assert_eq!(
        r.git(["rev-parse", "origin/main"]).unwrap().trim(),
        r.head().unwrap()
    );
}
#[gpui::test]
fn ui_7_tag_remote_choice_preserves_create_and_delete_options(root: &mut TestAppContext) {
    let f = Fixture::start(EVENT_CAPACITY);
    let made = repos(&["api"]);
    let r = repo(&made, "api");
    let dir = tempfile::tempdir().unwrap();
    let remote = dir.path().join("remote.git");
    r.git(["init", "--bare", remote.to_str().unwrap()]).unwrap();
    for name in ["alpha", "beta"] {
        r.git(["remote", "add", name, remote.to_str().unwrap()])
            .unwrap();
    }
    let added = add(&f, &made);
    let (mut cx, _) = dialog(
        root,
        &f,
        added[0].1.clone(),
        OperationMode::TagCreate {
            commit: Some(r.head().unwrap()),
        },
    );
    fill(&mut cx, "op-name", "v1");
    click(&mut cx, "op-annotated".into());
    click(&mut cx, "op-push".into());
    ready(&mut cx);
    click(&mut cx, "dialog-submit".into());
    wait_until(&mut cx, |cx| label_of(cx, "op-remote-0".into()).is_some());
    click(&mut cx, "op-remote-1".into());
    let mut events = f.app().subscribe();
    click(&mut cx, "dialog-submit".into());
    ended(&mut cx, &mut events);
    assert!(
        r.git(["ls-remote", "beta", "refs/tags/v1"])
            .unwrap()
            .contains("refs/tags/v1")
    );
    let (mut cx, _) = dialog(
        root,
        &f,
        added[0].1.clone(),
        OperationMode::TagDelete { name: "v1".into() },
    );
    click(&mut cx, "op-on-remote".into());
    click(&mut cx, "dialog-submit".into());
    wait_until(&mut cx, |cx| label_of(cx, "op-remote-0".into()).is_some());
    click(&mut cx, "op-remote-1".into());
    click(&mut cx, "dialog-submit".into());
    wait_until(&mut cx, |cx| {
        label_of(cx, "op-confirm-actions".into()).is_some()
    });
    let mut events = f.app().subscribe();
    click(&mut cx, "dialog-submit".into());
    ended(&mut cx, &mut events);
    assert!(r.git(["tag", "--list"]).unwrap().is_empty());
    assert!(
        r.git(["ls-remote", "beta", "refs/tags/v1"])
            .unwrap()
            .is_empty()
    );
}
#[gpui::test]
fn ui_7_drop_stash_confirms_without_running_on_cancel(root: &mut TestAppContext) {
    let f = Fixture::start(EVENT_CAPACITY);
    let made = repos(&["api"]);
    let r = repo(&made, "api");
    std::fs::write(r.path().join("README.md"), "changed\n").unwrap();
    r.git(["stash", "push", "-m", "saved"]).unwrap();
    let hash = r.git(["rev-parse", "stash@{0}"]).unwrap().trim().to_owned();
    let added = add(&f, &made);
    let mode = OperationMode::Run(gitcroc_ui::operation_dialog::Operation::Stash(
        gitcroc_core::StashOp::Drop {
            stash: gitcroc_core::StashRef {
                index: 0,
                hash: hash.clone(),
            },
        },
    ));
    let (mut cx, _) = dialog(root, &f, added[0].1.clone(), mode.clone());
    click(&mut cx, "dialog-submit".into());
    wait_until(&mut cx, |cx| {
        label_of(cx, "op-confirm-actions".into()).is_some()
    });
    click(&mut cx, "dialog-cancel".into());
    assert_eq!(r.git(["rev-parse", "stash@{0}"]).unwrap().trim(), hash);
    let (mut cx, _) = dialog(root, &f, added[0].1.clone(), mode);
    click(&mut cx, "dialog-submit".into());
    wait_until(&mut cx, |cx| {
        label_of(cx, "op-confirm-actions".into()).is_some()
    });
    let mut events = f.app().subscribe();
    click(&mut cx, "dialog-submit".into());
    ended(&mut cx, &mut events);
    assert!(r.git(["stash", "list"]).unwrap().is_empty());
}

#[gpui::test]
fn ops_7_conflicting_pop_offers_to_drop_only_after_resolution(root: &mut TestAppContext) {
    use crate::history::{tree_part, tree_row, wait_for_element};
    let f = Fixture::start(EVENT_CAPACITY);
    let made = repos(&["api"]);
    let r = repo(&made, "api");
    r.git(["config", "core.autocrlf", "false"]).unwrap();
    std::fs::write(r.path().join("README.md"), "stashed\n").unwrap();
    r.git(["stash", "push", "-m", "saved"]).unwrap();
    r.commit_file("README.md", "conflicting\n", "Conflict with stash")
        .unwrap();
    let added = add(&f, &made);
    let (mut cx, window) = crate::support::main_window(root, &f);
    crate::stage::enlarge(&mut cx);
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
    wait_until(&mut cx, |_| {
        while let Ok(event) = events.try_recv() {
            if let AppEvent::JobEnded(job) = event {
                assert_eq!(job.outcome, Some(gitcroc_core::Outcome::Conflict));
                return true;
            }
        }
        false
    });
    let offer: gpui_kit::ElementId = format!("stash-offer-{}", added[0].1.0).into();
    cx.update(|window, _| assert!(window.try_find(offer.clone()).is_none()));
    std::fs::write(r.path().join("README.md"), "resolved\n").unwrap();
    r.git(["add", "README.md"]).unwrap();
    f.runtime
        .block_on(f.app().workspace_refresh(f.active()))
        .unwrap();
    wait_for_element(&mut cx, offer.clone());
    click(&mut cx, format!("stash-offer-keep-{}", added[0].1.0).into());
    cx.update(|window, _| assert!(window.try_find(offer).is_none()));
    assert!(!r.git(["stash", "list"]).unwrap().is_empty());
}

#[gpui::test]
fn grp_8_toolbar_allows_fetch_during_conflict(root: &mut TestAppContext) {
    let f = Fixture::start(EVENT_CAPACITY);
    let made = repos(&["api"]);
    let r = repo(&made, "api");
    r.branch("topic").unwrap();
    r.commit_file("README.md", "main\n", "Main edit").unwrap();
    r.git(["checkout", "topic"]).unwrap();
    r.commit_file("README.md", "topic\n", "Topic edit").unwrap();
    r.git(["checkout", "main"]).unwrap();
    assert!(r.git(["merge", "topic"]).is_err());
    let added = add(&f, &made);
    let (mut cx, window) = crate::support::main_window(root, &f);
    crate::changes::show_repo(&mut cx, &window, &added, "api");
    wait_until(&mut cx, |cx| {
        cx.update(|window, _| window.find("toolbar-1").disabled() == Some(true))
    });
    cx.update(|window, _| {
        assert_ne!(window.find("toolbar-0").disabled(), Some(true));
        for at in 1..6 {
            assert_eq!(
                window
                    .find(gpui_kit::ElementId::Name(format!("toolbar-{at}").into()))
                    .disabled(),
                Some(true)
            );
        }
    });
}
#[gpui::test]
fn ui_20_detached_toolbar_keeps_branch_and_stash_available(root: &mut TestAppContext) {
    let f = Fixture::start(EVENT_CAPACITY);
    let made = repos(&["api"]);
    let r = repo(&made, "api");
    r.git(["checkout", "--detach"]).unwrap();
    let added = add(&f, &made);
    let (mut cx, window) = crate::support::main_window(root, &f);
    crate::changes::show_repo(&mut cx, &window, &added, "api");
    wait_until(&mut cx, |cx| {
        cx.update(|window, _| window.find("toolbar-1").disabled() == Some(true))
    });
    cx.update(|window, _| {
        for at in 0..6 {
            assert_eq!(
                window
                    .find(gpui_kit::ElementId::Name(format!("toolbar-{at}").into()))
                    .disabled()
                    == Some(true),
                [1, 2, 4].contains(&at)
            );
        }
    });
}
