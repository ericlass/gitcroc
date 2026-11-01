//! M6-19c2: unmerged files open in the kept conflict editor in place of the
//! diff, and Mark resolved stages them through a job (OPS-3, OPS-5,
//! OPS-9). Real repos (QA-2), repo-local identity, no autocrlf.
use crate::changes::{show_repo, with_changes};
use crate::conflict_banner::{ended, repos};
use crate::history::{add, click, id_of, label_of, repo, wait_for_element, wait_until};
use crate::stage::{disabled, focused, row_of, row_part, show_uncommitted};
use crate::support::{Fixture, main_window};
use gitcroc_app::app::{AppEvent, EVENT_CAPACITY};
use gitcroc_core::{Outcome, RepoId};
use gitcroc_ui::changes::{FileRow, List};
use gitcroc_ui::conflict::{ConflictEditor, Take};
use gitcroc_ui::strings::conflict as text;
use gitcroc_ui::window::GitcrocWindow;
use gpui_kit::test::{TestQueryExt, TestWindowExt};
use gpui_kit::{Entity, TestAppContext, VisualTestContext};

/// Ten lines; line 5 differs on each side.
pub(crate) fn lines(middle: &str) -> String {
    (1..=10)
        .map(|n| {
            if n == 5 {
                format!("{middle}\n")
            } else {
                format!("line {n}\n")
            }
        })
        .collect()
}

/// `paths` conflict between `main` and `topic`, merged into `main` from a
/// terminal.
pub(crate) fn merge_conflict(r: &repo_gen::Repo, paths: &[&str]) {
    for path in paths {
        r.write(path, lines("base")).unwrap();
    }
    r.commit_all("Base").unwrap();
    r.branch("topic").unwrap();
    for path in paths {
        r.write(path, lines("main")).unwrap();
    }
    r.commit_all("Main edit").unwrap();
    r.git(["checkout", "topic"]).unwrap();
    for path in paths {
        r.write(path, lines("topic")).unwrap();
    }
    r.commit_all("Topic edit").unwrap();
    r.git(["checkout", "main"]).unwrap();
    assert!(r.git(["merge", "topic"]).is_err());
}

/// The conflict editor the changes pane shows.
pub(crate) fn shown(
    cx: &mut VisualTestContext,
    window: &Entity<GitcrocWindow>,
) -> Option<Entity<ConflictEditor>> {
    cx.update(|_, cx| {
        let pane = window.read(cx).changes(cx)?;
        pane.read(cx).shown_conflict(cx)
    })
}

/// Waits until the selected file is unmerged `path`, open in its editor.
#[track_caller]
pub(crate) fn wait_for_editor(
    cx: &mut VisualTestContext,
    window: &Entity<GitcrocWindow>,
    path: &str,
) -> Entity<ConflictEditor> {
    let label = text::resolution(path);
    wait_until(cx, |cx| {
        shown(cx, window).is_some()
            && cx.update(|window, cx| {
                window.render_frame(cx);
                window.try_find_by_label(&label).is_some()
            })
    });
    shown(cx, window).unwrap()
}

pub(crate) fn value(cx: &mut VisualTestContext, editor: &Entity<ConflictEditor>) -> String {
    cx.update(|_, cx| editor.read(cx).value(cx))
}

pub(crate) fn take(cx: &mut VisualTestContext, editor: &Entity<ConflictEditor>, side: Take) {
    cx.update(|window, cx| {
        editor.update(cx, |editor, cx| assert!(editor.take(0, side, window, cx)))
    });
    cx.run_until_parked();
}

/// Replaces the whole buffer with `text`, as typing would.
pub(crate) fn type_over(cx: &mut VisualTestContext, editor: &Entity<ConflictEditor>, text: &str) {
    cx.update(|window, cx| {
        let state = editor.read(cx).editor.clone();
        state.update(cx, |state, cx| {
            state.focus(window, cx);
            state.set_value(text.to_owned(), window, cx);
        });
    });
    cx.run_until_parked();
}

pub(crate) fn unmerged(r: &repo_gen::Repo) -> String {
    r.git(["ls-files", "-u"]).unwrap()
}

pub(crate) fn on_disk(r: &repo_gen::Repo, path: &str) -> String {
    std::fs::read_to_string(r.path().join(path)).unwrap()
}

/// Waits for the end of a job and returns its outcome.
pub(crate) fn job_end(
    cx: &mut VisualTestContext,
    events: &mut tokio::sync::broadcast::Receiver<AppEvent>,
) -> Option<Outcome> {
    let mut outcome = None;
    wait_until(cx, |_| {
        while let Ok(event) = events.try_recv() {
            if let AppEvent::JobEnded(job) = event {
                outcome = job.outcome;
                return true;
            }
        }
        false
    });
    outcome
}

/// Clicks Mark resolved and waits for the resolution's job to end.
pub(crate) fn mark_resolved(f: &Fixture, cx: &mut VisualTestContext) -> Option<Outcome> {
    let mut events = f.app().subscribe();
    click(cx, "conflict-mark-resolved".into());
    job_end(cx, &mut events)
}

/// Gives the focus to Mark resolved, or to Mark resolved anyway.
pub(crate) fn focus_mark(cx: &mut VisualTestContext, window: &Entity<GitcrocWindow>, anyway: bool) {
    cx.update(|window_, cx| {
        window_.render_frame(cx);
        let pane = window.read(cx).changes(cx).unwrap();
        let handle = pane.read(cx).mark_focus(anyway);
        window_.focus(&handle, cx);
    });
    cx.run_until_parked();
}

pub(crate) fn kept_unsaved(
    cx: &mut VisualTestContext,
    window: &Entity<GitcrocWindow>,
    repo: &RepoId,
    path: &str,
) -> Option<bool> {
    cx.update(|_, cx| {
        let pane = window.read(cx).changes(cx)?;
        let editor = pane.read(cx).kept_conflict(repo, path)?;
        Some(editor.read(cx).unsaved(cx))
    })
}

/// OPS-3: the Unmerged list comes first and its first file opens in the
/// conflict editor in place of the diff; an edited file's row is marked
/// unsaved; staged files keep their diff and controls.
#[gpui::test]
fn ops_3_unmerged_files_open_first_and_show_unsaved_marks(root: &mut TestAppContext) {
    let f = Fixture::start(EVENT_CAPACITY);
    let made = repos(&["api"]);
    let r = repo(&made, "api");
    r.git(["checkout", "-b", "side"]).unwrap();
    r.commit_file("clean.txt", "clean\n", "Clean").unwrap();
    r.git(["checkout", "main"]).unwrap();
    merge_conflict(&r, &["a.txt", "b.txt"]);
    // A staged change beside the conflicts.
    r.write("staged.txt", "staged\n").unwrap();
    r.git(["add", "staged.txt"]).unwrap();
    let added = add(&f, &made);
    let api = id_of(&added, "api");
    let (mut cx, window) = main_window(root, &f);
    show_uncommitted(&mut cx, &window, &added, "api");

    let sections = with_changes(&mut cx, &window, |changes| {
        changes
            .rows()
            .iter()
            .filter_map(|row| match row {
                FileRow::Section { list, .. } => Some(*list),
                _ => None,
            })
            .collect::<Vec<_>>()
    });
    assert_eq!(sections.first(), Some(&List::Unmerged));
    let editor = wait_for_editor(&mut cx, &window, "a.txt");
    assert!(value(&mut cx, &editor).contains("<<<<<<< "));
    let row = row_of(&mut cx, &window, List::Unmerged, "a.txt");
    cx.update(|window, cx| {
        window.render_frame(cx);
        assert!(window.try_find(row_part(row, "unsaved")).is_none());
    });

    take(&mut cx, &editor, Take::Theirs);
    wait_until(&mut cx, |cx| {
        kept_unsaved(cx, &window, &api, "a.txt") == Some(true)
    });
    wait_for_element(&mut cx, row_part(row, "unsaved"));
    assert_eq!(
        label_of(&mut cx, row_part(row, "unsaved")).as_deref(),
        Some(gitcroc_ui::strings::changes::UNSAVED)
    );

    // Another unmerged file has its own editor, and a staged file its diff.
    let b = row_of(&mut cx, &window, List::Unmerged, "b.txt");
    click(&mut cx, crate::changes::row(b));
    let other = wait_for_editor(&mut cx, &window, "b.txt");
    assert_ne!(other, editor);
    let staged = row_of(&mut cx, &window, List::Staged, "staged.txt");
    click(&mut cx, crate::changes::row(staged));
    wait_until(&mut cx, |cx| shown(cx, &window).is_none());
    assert_eq!(
        crate::changes::shown_file(&mut cx).as_deref(),
        Some("staged.txt")
    );
    wait_for_element(&mut cx, row_part(staged, "unstage"));
    // Back on a.txt, its edit is there.
    let a = row_of(&mut cx, &window, List::Unmerged, "a.txt");
    click(&mut cx, crate::changes::row(a));
    let again = wait_for_editor(&mut cx, &window, "a.txt");
    assert_eq!(again, editor);
    assert!(!value(&mut cx, &again).contains("<<<<<<< "));
    assert!(unmerged(&r).contains("a.txt"));
}

/// OPS-5, OPS-9: nothing is written before Mark resolved; Mark resolved
/// writes the buffer and stages it; a failed job keeps the buffer and says
/// why.
#[gpui::test]
fn ops_5_mark_resolved_stages_only_on_request(root: &mut TestAppContext) {
    let f = Fixture::start(EVENT_CAPACITY);
    let made = repos(&["api"]);
    let r = repo(&made, "api");
    merge_conflict(&r, &["a.txt"]);
    let disk = on_disk(&r, "a.txt");
    let added = add(&f, &made);
    let (mut cx, window) = main_window(root, &f);
    show_uncommitted(&mut cx, &window, &added, "api");
    let editor = wait_for_editor(&mut cx, &window, "a.txt");
    take(&mut cx, &editor, Take::Ours);
    let resolved = value(&mut cx, &editor);
    assert_eq!(resolved, lines("main"));
    assert_eq!(on_disk(&r, "a.txt"), disk);
    assert!(unmerged(&r).contains("a.txt"));

    // The index is locked: the job fails, and the buffer stays.
    let lock = r.path().join(".git").join("index.lock");
    std::fs::write(&lock, "").unwrap();
    let outcome = mark_resolved(&f, &mut cx);
    assert_eq!(outcome, Some(Outcome::Failed));
    wait_for_element(&mut cx, "conflict-error".into());
    assert_eq!(value(&mut cx, &editor), resolved);
    assert!(unmerged(&r).contains("a.txt"));
    assert!(!disabled(&mut cx, "conflict-mark-resolved".into()));

    std::fs::remove_file(&lock).unwrap();
    assert_eq!(mark_resolved(&f, &mut cx), Some(Outcome::Succeeded));
    assert_eq!(on_disk(&r, "a.txt"), lines("main"));
    assert!(unmerged(&r).is_empty());
    assert_eq!(r.git(["show", ":a.txt"]).unwrap(), lines("main"));
    // The resolved file leaves the Unmerged list, and its editor goes.
    wait_until(&mut cx, |cx| shown(cx, &window).is_none());
}

/// OPS-5: marker lines refuse Mark resolved, naming the first; nothing is
/// written; Enter on Mark resolved asks again, and only Mark resolved
/// anyway, by Space on its focus, stages the text as it is.
#[gpui::test]
fn ops_5_marker_refusal_requires_explicit_override(root: &mut TestAppContext) {
    let f = Fixture::start(EVENT_CAPACITY);
    let made = repos(&["api"]);
    let r = repo(&made, "api");
    merge_conflict(&r, &["a.txt"]);
    let disk = on_disk(&r, "a.txt");
    let added = add(&f, &made);
    let (mut cx, window) = main_window(root, &f);
    show_uncommitted(&mut cx, &window, &added, "api");
    let editor = wait_for_editor(&mut cx, &window, "a.txt");
    let marked = format!("keep\n{}\n=======\nliteral\n", "<".repeat(7));
    type_over(&mut cx, &editor, &marked);

    click(&mut cx, "conflict-mark-resolved".into());
    wait_for_element(&mut cx, "conflict-markers".into());
    assert_eq!(
        label_of(&mut cx, "conflict-markers".into()),
        Some(text::marker_line(2))
    );
    assert_eq!(on_disk(&r, "a.txt"), disk);
    assert!(unmerged(&r).contains("a.txt"));

    // Enter on the focused Mark resolved asks again; it never overrides.
    focus_mark(&mut cx, &window, false);
    crate::jobs::press(&mut cx, "enter");
    wait_for_element(&mut cx, "conflict-markers".into());
    assert!(unmerged(&r).contains("a.txt"));
    assert!(!focused(&mut cx, "conflict-mark-anyway".into()));

    let mut events = f.app().subscribe();
    focus_mark(&mut cx, &window, true);
    crate::jobs::press(&mut cx, "space");
    assert_eq!(
        crate::conflict_flow::job_end(&mut cx, &mut events),
        Some(Outcome::Succeeded)
    );
    assert!(unmerged(&r).is_empty());
    assert_eq!(r.git(["show", ":a.txt"]).unwrap(), marked);
}

/// ARCH-9: identical paths in two repos keep two editors; after the view
/// is read anew, Mark resolved uses the new view's handle and stages the
/// right repo's file only.
#[gpui::test]
fn arch_9_stale_conflict_reads_and_resolve_handles_cannot_cross_targets(root: &mut TestAppContext) {
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
    show_uncommitted(&mut cx, &window, &added, "api");
    let api_editor = wait_for_editor(&mut cx, &window, "a.txt");
    take(&mut cx, &api_editor, Take::Theirs);

    show_repo(&mut cx, &window, &added, "web");
    wait_for_element(&mut cx, "history-uncommitted".into());
    click(&mut cx, "history-uncommitted".into());
    let web_editor = wait_for_editor(&mut cx, &window, "a.txt");
    assert_ne!(web_editor, api_editor);
    assert!(value(&mut cx, &web_editor).contains("<<<<<<< "));
    assert_eq!(
        kept_unsaved(&mut cx, &window, &web_id, "a.txt"),
        Some(false)
    );
    assert_eq!(kept_unsaved(&mut cx, &window, &api_id, "a.txt"), Some(true));

    // The view is read anew under a new handle; the kept buffer stays.
    show_repo(&mut cx, &window, &added, "api");
    click(&mut cx, "history-uncommitted".into());
    let editor = wait_for_editor(&mut cx, &window, "a.txt");
    assert_eq!(editor, api_editor);
    let handle = with_changes(&mut cx, &window, |changes| changes.view().unwrap().handle);
    api.write("unrelated.txt", "x\n").unwrap();
    f.refresh(&f.active());
    wait_until(&mut cx, |cx| {
        with_changes(cx, &window, |changes| {
            !changes.loading() && changes.view().is_some_and(|view| view.handle != handle)
        })
    });
    assert_eq!(mark_resolved(&f, &mut cx), Some(Outcome::Succeeded));
    assert!(unmerged(&api).is_empty());
    assert_eq!(on_disk(&api, "a.txt"), lines("topic"));
    assert!(unmerged(&web).contains("a.txt"));
}

/// OPS-2: Continue stays disabled until every path is resolved and staged;
/// two files are resolved with the keyboard, then Continue commits the
/// merge. A rebase stops again at its next step, whose banner says so.
#[gpui::test]
fn ops_2_continue_waits_for_all_unmerged_paths(root: &mut TestAppContext) {
    let f = Fixture::start(EVENT_CAPACITY);
    let made = repos(&["api"]);
    let r = repo(&made, "api");
    merge_conflict(&r, &["a.txt", "b.txt"]);
    let added = add(&f, &made);
    let (mut cx, window) = main_window(root, &f);
    show_uncommitted(&mut cx, &window, &added, "api");
    wait_for_element(&mut cx, "conflict-banner-continue".into());
    assert!(disabled(&mut cx, "conflict-banner-continue".into()));

    for path in ["a.txt", "b.txt"] {
        let editor = wait_for_editor(&mut cx, &window, path);
        cx.update(|window, cx| {
            let state = editor.read(cx).editor.clone();
            state.update(cx, |state, cx| state.focus(window, cx));
        });
        let mut events = f.app().subscribe();
        // Alt+Down to the hunk, Alt+1 takes ours, Ctrl+Enter marks it resolved.
        crate::jobs::press(&mut cx, "alt-down");
        crate::jobs::press(&mut cx, "alt-1");
        let mark = if cfg!(target_os = "macos") {
            "cmd-enter"
        } else {
            "ctrl-enter"
        };
        crate::jobs::press(&mut cx, mark);
        assert_eq!(job_end(&mut cx, &mut events), Some(Outcome::Succeeded));
        if path == "a.txt" {
            wait_until(&mut cx, |cx| {
                label_of(cx, "conflict-banner-unmerged".into())
                    == Some(gitcroc_ui::strings::integrate::resolve_and_stage(1))
            });
            assert!(disabled(&mut cx, "conflict-banner-continue".into()));
        }
    }
    assert!(unmerged(&r).is_empty());
    wait_until(&mut cx, |cx| {
        !cx.update(|window, cx| {
            window.render_frame(cx);
            window.find("conflict-banner-continue").disabled() == Some(true)
        })
    });
    let mut events = f.app().subscribe();
    click(&mut cx, "conflict-banner-continue".into());
    ended(&mut events, &mut cx);
    wait_until(&mut cx, |cx| {
        label_of(cx, "conflict-banner-text".into()).is_none()
    });
    assert!(
        r.git(["rev-parse", "-q", "--verify", "MERGE_HEAD"])
            .is_err()
    );
    assert_eq!(on_disk(&r, "a.txt"), lines("main"));

    // A rebase of two conflicting commits stops twice.
    diverge_steps(&r);
    f.refresh(&f.active());
    assert!(r.git(["rebase", "main"]).is_err());
    f.refresh(&f.active());
    wait_until(&mut cx, |cx| {
        label_of(cx, "conflict-banner-text".into()).is_some_and(|line| line.contains("step 1 of 2"))
    });
    click(&mut cx, "history-uncommitted".into());
    let editor = wait_for_editor(&mut cx, &window, "a.txt");
    take(&mut cx, &editor, Take::Theirs);
    assert_eq!(mark_resolved(&f, &mut cx), Some(Outcome::Succeeded));
    let mut events = f.app().subscribe();
    wait_until(&mut cx, |cx| {
        !cx.update(|window, cx| {
            window.render_frame(cx);
            window.find("conflict-banner-continue").disabled() == Some(true)
        })
    });
    click(&mut cx, "conflict-banner-continue".into());
    ended(&mut events, &mut cx);
    wait_until(&mut cx, |cx| {
        label_of(cx, "conflict-banner-text".into()).is_some_and(|line| line.contains("step 2 of 2"))
    });
}

/// Branch `two`, checked out, with two commits from `main`: the first
/// conflicts with `main`'s next a.txt, the second with its b.txt.
fn diverge_steps(r: &repo_gen::Repo) {
    r.branch("two").unwrap();
    r.write("a.txt", lines("main 2")).unwrap();
    r.write("b.txt", lines("main 2")).unwrap();
    r.commit_all("Main again").unwrap();
    r.git(["checkout", "two"]).unwrap();
    r.write("a.txt", lines("first")).unwrap();
    r.commit_all("First").unwrap();
    r.write("b.txt", lines("second")).unwrap();
    r.commit_all("Second").unwrap();
}
