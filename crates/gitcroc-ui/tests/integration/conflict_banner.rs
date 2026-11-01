//! M6-19c1: the conflict banner above a repo's history (OPS-2, UI-10), for
//! conflicts started in a terminal and in the app, with Abort and Continue
//! through M6-18's operation dialog. Repos are real ones (QA-2), each with
//! a repo-local identity and `core.autocrlf=false`.
use crate::changes::show_repo;
use crate::history::{add, click, label_of, repo, wait_for_element, wait_until};
use crate::stage::{disabled, enlarge};
use crate::support::{Fixture, main_window};
use gitcroc_app::app::{AppEvent, EVENT_CAPACITY};
use gitcroc_core::{ConflictDetails, IntegrateOp, IntegrateRequest, Operation, RepoId};
use gitcroc_ui::strings::integrate as text;
use gpui_kit::test::TestWindowExt;
use gpui_kit::{TestAppContext, VisualTestContext};

/// Repos `names`, each with one commit, an identity and no line-ending
/// conversion.
pub(crate) fn repos(names: &[&str]) -> repo_gen::Fixture {
    let made = crate::history::repos(names);
    for name in names {
        let r = repo(&made, name);
        r.git(["config", "core.autocrlf", "false"]).unwrap();
        r.git(["config", "user.name", "Test User"]).unwrap();
        r.git(["config", "user.email", "test@example.com"]).unwrap();
    }
    made
}

/// `topic` and `main` change README.md differently; `main` is checked out.
pub(crate) fn diverge(r: &repo_gen::Repo, path: &str) {
    r.branch("topic").unwrap();
    r.commit_file(path, "main\n", "Main edit").unwrap();
    r.git(["checkout", "topic"]).unwrap();
    r.commit_file(path, "topic\n", "Topic edit").unwrap();
    r.git(["checkout", "main"]).unwrap();
}

pub(crate) fn details(f: &Fixture, repo: &RepoId) -> ConflictDetails {
    f.runtime
        .block_on(f.app().conflict_details(f.active(), repo.clone()))
        .unwrap()
}

/// What the banner says for the details the core reads now.
pub(crate) fn said(details: &ConflictDetails) -> String {
    let operation = details.operation.unwrap();
    text::in_progress(operation, details)
}

/// Waits until the banner says `line`.
#[track_caller]
pub(crate) fn wait_for_banner(cx: &mut VisualTestContext, line: &str) {
    wait_until(cx, |cx| {
        label_of(cx, "conflict-banner-text".into()).as_deref() == Some(line)
    });
}

fn refresh(f: &Fixture) {
    f.refresh(&f.active());
}

pub(crate) fn ended(
    events: &mut tokio::sync::broadcast::Receiver<AppEvent>,
    cx: &mut VisualTestContext,
) {
    wait_until(cx, |_| {
        while let Ok(event) = events.try_recv() {
            if let AppEvent::JobEnded(_) = event {
                return true;
            }
        }
        false
    });
}

/// OPS-2, UI-10, OPS-1: merge, rebase (with its step), cherry-pick,
/// revert, mailbox and stash apply each show their banner, from the
/// repo's state whoever started them; another repo shows none.
#[gpui::test]
fn ops_2_banner_tracks_terminal_and_app_conflicts(root: &mut TestAppContext) {
    let f = Fixture::start(EVENT_CAPACITY);
    let made = repos(&["api", "web"]);
    let r = repo(&made, "api");
    diverge(&r, "README.md");
    // Topic gets a second commit, so a rebase stops at step 1 of 2.
    r.git(["checkout", "topic"]).unwrap();
    r.commit_file("other.txt", "other\n", "Topic other")
        .unwrap();
    r.git(["checkout", "main"]).unwrap();
    let added = add(&f, &made);
    let api = crate::history::id_of(&added, "api");
    let (mut cx, window) = main_window(root, &f);
    enlarge(&mut cx);
    show_repo(&mut cx, &window, &added, "api");
    cx.update(|window, cx| {
        window.render_frame(cx);
        assert!(window.try_find("conflict-banner").is_none());
    });

    // A merge started in a terminal.
    assert!(r.git(["merge", "topic"]).is_err());
    refresh(&f);
    let merge = details(&f, &api);
    assert_eq!(merge.operation, Some(Operation::Merge));
    wait_for_banner(&mut cx, &said(&merge));
    assert_eq!(
        label_of(&mut cx, "conflict-banner-unmerged".into()),
        Some(text::resolve_and_stage(1))
    );
    assert!(disabled(&mut cx, "conflict-banner-continue".into()));
    assert!(!disabled(&mut cx, "conflict-banner-abort".into()));
    // Another repo has none.
    show_repo(&mut cx, &window, &added, "web");
    cx.update(|window, cx| {
        window.render_frame(cx);
        assert!(window.try_find("conflict-banner").is_none());
    });
    show_repo(&mut cx, &window, &added, "api");
    wait_for_banner(&mut cx, &said(&merge));
    r.git(["merge", "--abort"]).unwrap();
    refresh(&f);
    wait_until(&mut cx, |cx| {
        label_of(cx, "conflict-banner-text".into()).is_none()
    });

    // A rebase started in the app, at its step.
    let mut events = f.app().subscribe();
    r.git(["checkout", "topic"]).unwrap();
    refresh(&f);
    f.runtime
        .block_on(f.app().op_integrate(
            f.active(),
            IntegrateRequest {
                repo_id: api.clone(),
                op: IntegrateOp::Rebase {
                    onto: "main".into(),
                },
                confirmed: false,
            },
        ))
        .unwrap();
    ended(&mut events, &mut cx);
    let rebase = details(&f, &api);
    assert_eq!(rebase.operation, Some(Operation::Rebase));
    let step = rebase.step.unwrap();
    assert_eq!((step.current, step.total), (1, 2));
    wait_for_banner(&mut cx, &said(&rebase));
    assert!(said(&rebase).contains("step 1 of 2"));
    r.git(["rebase", "--abort"]).unwrap();
    r.git(["checkout", "main"]).unwrap();

    // Cherry-pick and revert from a terminal.
    let topic = r.rev_parse("topic~1").unwrap();
    assert!(r.git(["cherry-pick", &topic]).is_err());
    refresh(&f);
    let pick = details(&f, &api);
    assert_eq!(pick.operation, Some(Operation::CherryPick));
    wait_for_banner(&mut cx, &said(&pick));
    r.git(["cherry-pick", "--abort"]).unwrap();

    r.commit_file("README.md", "later\n", "Later edit").unwrap();
    assert!(r.git(["revert", "--no-edit", "HEAD~1"]).is_err());
    refresh(&f);
    let revert = details(&f, &api);
    assert_eq!(revert.operation, Some(Operation::Revert));
    wait_for_banner(&mut cx, &said(&revert));
    r.git(["revert", "--abort"]).unwrap();

    // A mailbox, with its patch number.
    let patches = tempfile::tempdir().unwrap();
    let out = patches.path().to_string_lossy().into_owned();
    r.git(["format-patch", "-o", &out, "main..topic"]).unwrap();
    let mut files: Vec<_> = std::fs::read_dir(patches.path())
        .unwrap()
        .map(|entry| entry.unwrap().path().to_string_lossy().into_owned())
        .collect();
    files.sort();
    let mut am = vec!["am".to_owned()];
    am.extend(files);
    assert!(r.git(am).is_err());
    refresh(&f);
    let mailbox = details(&f, &api);
    assert_eq!(mailbox.operation, Some(Operation::ApplyMailbox));
    wait_for_banner(&mut cx, &said(&mailbox));
    assert!(said(&mailbox).contains("patch 1 of 2"));
    r.git(["am", "--abort"]).unwrap();

    // A stash apply has nothing to continue.
    r.write("README.md", "stashed\n").unwrap();
    r.git(["stash", "push", "-m", "saved"]).unwrap();
    r.commit_file("README.md", "conflicting\n", "Conflicting")
        .unwrap();
    assert!(r.git(["stash", "apply"]).is_err());
    refresh(&f);
    let stash = details(&f, &api);
    assert_eq!(stash.operation, Some(Operation::StashApply));
    wait_for_banner(&mut cx, &said(&stash));
    assert_eq!(
        label_of(&mut cx, "conflict-banner-unmerged".into()),
        Some(text::mark_resolved(1))
    );
    assert!(disabled(&mut cx, "conflict-banner-continue".into()));
}

/// GRP-9, OPS-2: Abort of a stash apply shows the core's confirmation,
/// Cancel first; Cancel runs nothing, and only the confirmed Abort writes.
#[gpui::test]
fn grp_9_stash_abort_requires_confirmation(root: &mut TestAppContext) {
    let f = Fixture::start(EVENT_CAPACITY);
    let made = repos(&["api"]);
    let r = repo(&made, "api");
    r.write("README.md", "stashed\n").unwrap();
    r.git(["stash", "push", "-m", "saved"]).unwrap();
    r.commit_file("README.md", "conflicting\n", "Conflicting")
        .unwrap();
    assert!(r.git(["stash", "apply"]).is_err());
    let added = add(&f, &made);
    let (mut cx, window) = main_window(root, &f);
    enlarge(&mut cx);
    show_repo(&mut cx, &window, &added, "api");
    wait_for_element(&mut cx, "conflict-banner-abort".into());
    let before = std::fs::read_to_string(r.path().join("README.md")).unwrap();

    click(&mut cx, "conflict-banner-abort".into());
    wait_for_element(&mut cx, "op-confirm-actions".into());
    // Cancel has the focus, so Enter cannot confirm by accident.
    assert!(crate::stage::focused(&mut cx, "dialog-cancel".into()));
    click(&mut cx, "dialog-cancel".into());
    cx.update(|window, cx| {
        window.render_frame(cx);
        assert!(window.try_find("op-confirm-actions").is_none());
    });
    assert_eq!(
        std::fs::read_to_string(r.path().join("README.md")).unwrap(),
        before
    );
    assert!(r.git(["ls-files", "-u"]).unwrap().contains("README.md"));

    let mut events = f.app().subscribe();
    click(&mut cx, "conflict-banner-abort".into());
    wait_for_element(&mut cx, "op-confirm-actions".into());
    click(&mut cx, "dialog-submit".into());
    ended(&mut events, &mut cx);
    assert!(r.git(["ls-files", "-u"]).unwrap().is_empty());
    wait_until(&mut cx, |cx| {
        label_of(cx, "conflict-banner-text".into()).is_none()
    });
    // The stash is kept.
    assert!(!r.git(["stash", "list"]).unwrap().is_empty());
}
