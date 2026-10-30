//! The commit box of "Uncommitted changes" (UI-15): Commit and Amend, the
//! message kept per repo and tab (UI-29), a failed commit; and the
//! fsmonitor offer (PERF-7). Repos are real ones made by `repo-gen` and
//! added through the app (QA-2); each commit is a job, waited for by what
//! it changed, never by time.

use gitcroc_app::app::EVENT_CAPACITY;
use gitcroc_core::{ChangesContent, ChangesTarget, CommitMessage, RepoId};
use gitcroc_ui::changes::List;
use gitcroc_ui::commit_box::CommitBox;
use gitcroc_ui::window::GitcrocWindow;
use gpui_kit::test::TestWindowExt;
use gpui_kit::{ElementId, Entity, TestAppContext, VisualTestContext};

use crate::changes::{switch_to, wait_for_view, with_changes};
use crate::history::{add, click, id_of, label_of, press, repo, repos};
use crate::history_merged::present;
use crate::stage::{act_until, disabled, row_of, row_part, show_uncommitted, wait_for_lists};
use crate::support::{Fixture, Options, main_window};

/// The identity commits are made with: the user's own config is not the
/// test's.
fn identify(repo: &repo_gen::Repo) {
    repo.git(["config", "user.name", "Test User"]).unwrap();
    repo.git(["config", "user.email", "test@example.com"])
        .unwrap();
    repo.git(["config", "commit.gpgsign", "false"]).unwrap();
}

/// Runs `f` on the active tab's commit box.
fn with_box<R>(
    cx: &mut VisualTestContext,
    window: &Entity<GitcrocWindow>,
    f: impl FnOnce(&CommitBox) -> R,
) -> R {
    cx.update(|_, cx| {
        let pane = window.read(cx).changes(cx).expect("no changes pane");
        let commit_box = pane.read(cx).commit_box().read(cx);
        f(commit_box)
    })
}

fn draft(cx: &mut VisualTestContext, window: &Entity<GitcrocWindow>) -> CommitMessage {
    with_box(cx, window, |commit_box| commit_box.draft()).expect("no commit box")
}

fn message(subject: &str, body: &str) -> CommitMessage {
    CommitMessage {
        subject: subject.to_owned(),
        body: body.to_owned(),
    }
}

/// Types `text` into the field `id`.
fn type_in(cx: &mut VisualTestContext, id: &str, text: &str) {
    click(cx, ElementId::Name(id.to_owned().into()));
    cx.update(|window, cx| window.input(text, cx));
    cx.run_until_parked();
}

/// Ctrl or Cmd and Enter.
fn commit_keys(cx: &mut VisualTestContext) {
    let keys = if cfg!(target_os = "macos") {
        "cmd-enter"
    } else {
        "ctrl-enter"
    };
    press(cx, keys);
}

/// The subject of the repo's `HEAD`, and how many commits lead to it.
fn head(repo: &repo_gen::Repo) -> (String, usize) {
    let subject = repo.git(["log", "-1", "--format=%s"]).unwrap();
    let count = repo.git(["rev-list", "--count", "HEAD"]).unwrap();
    (subject.trim().to_owned(), count.trim().parse().unwrap())
}

#[track_caller]
fn wait_for(cx: &mut VisualTestContext, mut done: impl FnMut(&mut VisualTestContext) -> bool) {
    crate::history::wait_until(cx, |cx| done(cx));
}

#[gpui::test]
fn ui_15_commit_and_amend(cx: &mut TestAppContext) {
    let fixture = Fixture::start(EVENT_CAPACITY);
    let made = repos(&["api"]);
    let api = repo(&made, "api");
    identify(&api);
    api.write("README.md", "changed\n").unwrap();
    // Left untracked, so the repo keeps uncommitted changes to show.
    api.write("keep.txt", "k\n").unwrap();
    let added = add(&fixture, &made);
    let (mut cx, window) = main_window(cx, &fixture);
    show_uncommitted(&mut cx, &window, &added, "api");
    wait_for_lists(&mut cx, &window, &[], &["README.md", "keep.txt"]);

    // Nothing staged: Commit is disabled and says why.
    assert!(disabled(&mut cx, "commit-commit".into()));
    assert_eq!(
        label_of(&mut cx, "commit-why".into()).as_deref(),
        Some("Stage changes first")
    );
    let readme = row_of(&mut cx, &window, List::Unstaged, "README.md");
    act_until(
        &mut cx,
        &window,
        |cx| click(cx, row_part(readme, "stage")),
        &["README.md"],
        &["keep.txt"],
    );
    // No subject.
    assert!(disabled(&mut cx, "commit-commit".into()));
    assert_eq!(
        label_of(&mut cx, "commit-why".into()).as_deref(),
        Some("Write the commit message first")
    );

    // Ctrl or Cmd and Enter commits, from the body too, and the box is
    // cleared.
    type_in(&mut cx, "commit-subject", "First change");
    assert!(!disabled(&mut cx, "commit-commit".into()));
    assert!(!present(&mut cx, "commit-why".into()));
    type_in(&mut cx, "commit-body", "Why it changed.");
    act_until(&mut cx, &window, commit_keys, &[], &["keep.txt"]);
    wait_for(&mut cx, |cx| draft(cx, &window) == CommitMessage::default());
    assert_eq!(field(&mut cx, "commit-subject").as_deref(), Some(""));
    assert_eq!(head(&api), ("First change".to_owned(), 2));
    assert_eq!(
        api.git(["log", "-1", "--format=%B"]).unwrap(),
        "First change\n\nWhy it changed.\n\n"
    );

    // Amend with an empty box fills it with the last message, and the
    // second Amend amends with the staged change.
    api.write("b.txt", "b\n").unwrap();
    api.git(["add", "b.txt"]).unwrap();
    fixture.refresh(&fixture.active());
    wait_for_lists(&mut cx, &window, &["b.txt"], &["keep.txt"]);
    let before = api.head().unwrap();
    click(&mut cx, "commit-amend".into());
    assert_eq!(
        draft(&mut cx, &window),
        message("First change", "Why it changed.")
    );
    assert_eq!(api.head().unwrap(), before);
    assert_eq!(
        field(&mut cx, "commit-subject").as_deref(),
        Some("First change")
    );
    act_until(
        &mut cx,
        &window,
        |cx| click(cx, "commit-amend".into()),
        &[],
        &["keep.txt"],
    );
    wait_for(&mut cx, |cx| draft(cx, &window) == CommitMessage::default());
    assert_ne!(api.head().unwrap(), before);
    assert_eq!(head(&api), ("First change".to_owned(), 2));
    assert!(
        api.git(["show", "--stat", "HEAD"])
            .unwrap()
            .contains("b.txt")
    );
}

#[gpui::test]
fn ui_15_commit_message_is_kept_per_repo(cx: &mut TestAppContext) {
    let fixture = Fixture::start(EVENT_CAPACITY);
    let mine = fixture.active();
    let made = repos(&["api", "web"]);
    repo(&made, "api").write("README.md", "api\n").unwrap();
    repo(&made, "web").write("README.md", "web\n").unwrap();
    let added = add(&fixture, &made);
    // A second workspace with a dirty repo of its own.
    let app = fixture.app().clone();
    let java = fixture.runtime.block_on(async {
        let tabs = app.workspace_create("Java".to_owned()).await.unwrap();
        tabs.active.unwrap()
    });
    let other = repos(&["core"]);
    repo(&other, "core").write("README.md", "core\n").unwrap();
    let other_added = fixture.add_repos(&java, other.path());
    fixture
        .runtime
        .block_on(app.workspace_activate(mine.clone()))
        .unwrap();
    let (mut cx, window) = main_window(cx, &fixture);

    show_uncommitted(&mut cx, &window, &added, "api");
    type_in(&mut cx, "commit-subject", "For api");
    show_uncommitted(&mut cx, &window, &added, "web");
    assert_eq!(draft(&mut cx, &window), CommitMessage::default());
    type_in(&mut cx, "commit-subject", "For web");
    show_uncommitted(&mut cx, &window, &added, "api");
    assert_eq!(draft(&mut cx, &window), message("For api", ""));
    // The field shows it too.
    assert_eq!(field(&mut cx, "commit-subject").as_deref(), Some("For api"));

    switch_to(&mut cx, &fixture, &window, &java);
    show_uncommitted(&mut cx, &window, &other_added, "core");
    assert_eq!(draft(&mut cx, &window), CommitMessage::default());
    type_in(&mut cx, "commit-subject", "For core");
    switch_to(&mut cx, &fixture, &window, &mine);
    wait_for_view(&mut cx, &window);
    assert_eq!(draft(&mut cx, &window), message("For api", ""));
    show_uncommitted(&mut cx, &window, &added, "web");
    assert_eq!(draft(&mut cx, &window), message("For web", ""));
    switch_to(&mut cx, &fixture, &window, &java);
    wait_for_view(&mut cx, &window);
    assert_eq!(draft(&mut cx, &window), message("For core", ""));
}

/// The value of field `id`, as a screen reader reads it.
fn field(cx: &mut VisualTestContext, id: &str) -> Option<String> {
    cx.update(|window, cx| {
        window.render_frame(cx);
        window
            .try_find(ElementId::Name(id.to_owned().into()))
            .and_then(|found| found.value().map(str::to_owned))
    })
}

#[gpui::test]
fn ui_15_a_failed_commit_keeps_the_message(cx: &mut TestAppContext) {
    let fixture = Fixture::start(EVENT_CAPACITY);
    let made = repos(&["api"]);
    let api = repo(&made, "api");
    // An empty name in the repo's own config wins over the user's, and
    // git refuses it.
    api.git(["config", "user.name", ""]).unwrap();
    api.git(["config", "user.email", ""]).unwrap();
    api.git(["config", "user.useConfigOnly", "true"]).unwrap();
    api.git(["config", "commit.gpgsign", "false"]).unwrap();
    api.write("README.md", "changed\n").unwrap();
    api.git(["add", "README.md"]).unwrap();
    let head_before = api.head().unwrap();
    let added = add(&fixture, &made);
    let (mut cx, window) = main_window(cx, &fixture);
    show_uncommitted(&mut cx, &window, &added, "api");
    wait_for_lists(&mut cx, &window, &["README.md"], &[]);

    type_in(&mut cx, "commit-subject", "Nobody's commit");
    click(&mut cx, "commit-commit".into());
    wait_for(&mut cx, |cx| {
        label_of(cx, "commit-error".into()).as_deref()
            == Some("The commit failed. The job list shows what git said.")
    });
    assert_eq!(draft(&mut cx, &window), message("Nobody's commit", ""));
    assert_eq!(api.head().unwrap(), head_before);
    // It can be sent again.
    assert!(!disabled(&mut cx, "commit-commit".into()));
}

/// A repo `name` of `made` with `files` tracked files besides its README,
/// and a change, for "Uncommitted changes".
fn tracked(made: &repo_gen::Fixture, name: &str, files: usize) {
    let repo = repo(made, name);
    for n in 0..files {
        repo.write(&format!("f{n}.txt"), "x\n").unwrap();
    }
    if files > 0 {
        repo.commit_all("Files").unwrap();
    }
    repo.write("README.md", "changed\n").unwrap();
}

/// What the app offers for repo `id` now, as an app started anew reads it.
fn offered(fixture: &Fixture, id: &RepoId) -> Option<Vec<String>> {
    let (app, workspace, repo) = (fixture.app().clone(), fixture.active(), id.clone());
    let view = fixture
        .runtime
        .block_on(app.changes_get(workspace, repo, ChangesTarget::Uncommitted))
        .unwrap()
        .unwrap();
    match view.content {
        ChangesContent::Uncommitted {
            offer_fsmonitor, ..
        } => offer_fsmonitor,
        _ => None,
    }
}

#[gpui::test]
fn perf_7_fsmonitor_offer_asks_once(cx: &mut TestAppContext) {
    // Over 3 tracked files earns the offer.
    let fixture = Fixture::start_with(Options {
        fsmonitor_threshold: 3,
        ..Options::default()
    });
    let made = repos(&["api", "small", "web"]);
    tracked(&made, "api", 4);
    tracked(&made, "web", 4);
    tracked(&made, "small", 0);
    let added = add(&fixture, &made);
    let (mut cx, window) = main_window(cx, &fixture);

    // Under the threshold: no offer.
    show_uncommitted(&mut cx, &window, &added, "small");
    assert!(!present(&mut cx, "changes-offer".into()));

    // Over it: the offer names the keys; "Turn on" sets them by a job.
    show_uncommitted(&mut cx, &window, &added, "api");
    let keys = with_changes(&mut cx, &window, |changes| changes.offer())
        .expect("no offer")
        .keys;
    assert!(keys.contains(&"core.untrackedCache".to_owned()), "{keys:?}");
    let text = label_of(&mut cx, "changes-offer-text".into()).unwrap();
    assert!(text.contains("core.untrackedCache"), "{text}");
    let jobs = fixture.app().jobs_get().len();
    click(&mut cx, "changes-offer-enable".into());
    assert!(!present(&mut cx, "changes-offer".into()));
    let api = repo(&made, "api");
    wait_for(&mut cx, |_| {
        api.git_output(["config", "core.untrackedCache"])
            .map(|output| String::from_utf8_lossy(&output.stdout).trim() == "true")
            .unwrap_or(false)
    });
    assert_eq!(fixture.app().jobs_get().len(), jobs + 1);

    // "Don't ask again" hides it, after a restart too, and changes no
    // config.
    show_uncommitted(&mut cx, &window, &added, "web");
    assert!(present(&mut cx, "changes-offer".into()));
    let web_id = id_of(&added, "web");
    click(&mut cx, "changes-offer-decline".into());
    assert!(!present(&mut cx, "changes-offer".into()));
    wait_for(&mut cx, |_| offered(&fixture, &web_id).is_none());
    assert!(
        fixture
            .workspace_file(&fixture.active())
            .contains(&web_id.0)
    );
    let web = repo(&made, "web");
    let config = web.git_output(["config", "core.untrackedCache"]).unwrap();
    assert!(!config.status.success(), "{config:?}");
    // The window shows it no more, though the view is read again.
    show_uncommitted(&mut cx, &window, &added, "small");
    show_uncommitted(&mut cx, &window, &added, "web");
    assert!(!present(&mut cx, "changes-offer".into()));
}

/// The bounds element `id` was drawn in.
fn bounds(cx: &mut VisualTestContext, id: &str) -> gpui_kit::Bounds<gpui_kit::Pixels> {
    cx.update(|window, cx| {
        window.render_frame(cx);
        window.find(ElementId::Name(id.to_owned().into())).bounds()
    })
}

fn shown_height(cx: &mut VisualTestContext, window: &Entity<GitcrocWindow>) -> f32 {
    cx.update(|_, cx| {
        let pane = window.read(cx).changes(cx).expect("no changes pane");
        pane.read(cx).commit_shown()
    })
}

#[gpui::test]
fn ui_15_the_commit_box_splitter_resizes_it(cx: &mut TestAppContext) {
    let fixture = Fixture::start(EVENT_CAPACITY);
    let made = repos(&["api"]);
    let api = repo(&made, "api");
    for n in 0..12 {
        api.write(&format!("f{n:02}.txt"), "x\n").unwrap();
    }
    let added = add(&fixture, &made);
    // At the default size, where the box once squeezed the lists.
    let (mut cx, window) = main_window(cx, &fixture);
    crate::changes::show_repo(&mut cx, &window, &added, "api");
    crate::history::wait_for_element(&mut cx, "history-uncommitted".into());
    click(&mut cx, "history-uncommitted".into());
    wait_for_view(&mut cx, &window);
    let list = bounds(&mut cx, "changes-files").size.height;
    assert!(f32::from(list) >= 100., "the lists are {list:?} high");

    // Dragged up, the box gets taller and the lists shorter.
    let before = shown_height(&mut cx, &window);
    let from = bounds(&mut cx, "commit-box-splitter").center();
    let to = from - gpui_kit::point(gpui_kit::px(0.), gpui_kit::px(20.));
    cx.update(|window, cx| window.drag(from, to, cx));
    cx.run_until_parked();
    let after = shown_height(&mut cx, &window);
    assert!((after - before - 20.).abs() < 1., "{before} to {after}");
    assert!(bounds(&mut cx, "changes-files").size.height < list);

    // Focused, Down makes it shorter, never under its least.
    click(&mut cx, "commit-box-splitter".into());
    press(&mut cx, "down");
    assert!((shown_height(&mut cx, &window) - (after - 16.)).abs() < 1.);
    for _ in 0..20 {
        press(&mut cx, "down");
    }
    let least = shown_height(&mut cx, &window);
    assert!(least >= 100., "{least}");
    // Dragged far up, it leaves the lists room.
    let from = bounds(&mut cx, "commit-box-splitter").center();
    let to = from - gpui_kit::point(gpui_kit::px(0.), gpui_kit::px(1000.));
    cx.update(|window, cx| window.drag(from, to, cx));
    cx.run_until_parked();
    assert!(f32::from(bounds(&mut cx, "changes-files").size.height) >= 80.);
}
