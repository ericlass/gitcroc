//! The status bar, the job list and the tabs' job marks (UI-21, UI-32).
//! The jobs are real, on `repo-gen` fixtures (QA-2): writing a
//! commit-graph file succeeds, a fetch from a missing folder fails, a merge
//! of a changed line conflicts. A fetch from a host that never answers
//! runs until it is cancelled, so a running job needs no timing. The
//! status bar's 10 s pass on the test executor's clock.

use std::collections::HashSet;
use std::time::Duration;

use gitcroc_app::app::EVENT_CAPACITY;
use gitcroc_core::{
    IntegrateOp, IntegrateRequest, IntegrateStart, JobId, JobState, JobView, MergeFastForward,
    Outcome, Prune, RepoId, SyncOp, SyncRequest, SyncStart, WorkspaceId,
};
use gitcroc_ui::strings;
use gitcroc_ui::window::GitcrocWindow;
use gpui_kit::test::TestWindowExt;
use gpui_kit::{ElementId, Entity, Role, TestAppContext, VisualTestContext, point, px};

use crate::history::{click, id_of, repo, repos, tab, wait_until};
use crate::support::{Fixture, main_window, settle};

/// Repos "api", whose origin is `origin`, and "web", whose branch `other`
/// changes the line `main` changed too. Returns the fixture repos.
pub(crate) fn made(origin: &str) -> repo_gen::Fixture {
    let made = repos(&["api", "web"]);
    let api = repo(&made, "api");
    api.git(["remote", "add", "origin", origin]).unwrap();
    // No credential helper or proxy of the runner's config answers or gets
    // in the way: git asks its askpass helper.
    api.git(["config", "credential.helper", ""]).unwrap();
    api.git(["config", "http.proxy", ""]).unwrap();
    let web = repo(&made, "web");
    // A merge needs a committer, even one that stops on a conflict; the
    // runners have no global identity.
    web.git(["config", "user.name", "Test"]).unwrap();
    web.git(["config", "user.email", "test@example.com"])
        .unwrap();
    web.branch("other").unwrap();
    web.git(["checkout", "-q", "other"]).unwrap();
    web.commit_file("README.md", "theirs\n", "Theirs").unwrap();
    web.git(["checkout", "-q", "-"]).unwrap();
    web.commit_file("README.md", "ours\n", "Ours").unwrap();
    made
}

/// Starts a fetch of `repo` and returns its job.
pub(crate) fn fetch(fixture: &Fixture, workspace: &WorkspaceId, repo: &RepoId) -> JobId {
    let app = fixture.app().clone();
    let request = SyncRequest {
        repo_id: repo.clone(),
        op: SyncOp::Fetch {
            prune: Prune::NoPrune,
        },
        confirmed: false,
    };
    let start = fixture
        .runtime
        .block_on(app.op_sync(workspace.clone(), request))
        .unwrap();
    let SyncStart::Started { job_id } = start else {
        panic!("{start:?}");
    };
    job_id
}

/// A host that takes each request and never answers, so a fetch from it
/// runs until it is cancelled. Returns a repo URL on it.
fn silent_host(fixture: &Fixture) -> String {
    let listener = fixture
        .runtime
        .block_on(tokio::net::TcpListener::bind("127.0.0.1:0"))
        .unwrap();
    let url = format!("http://{}/api.git", listener.local_addr().unwrap());
    fixture.runtime.spawn(async move {
        let mut open = Vec::new();
        while let Ok((stream, _)) = listener.accept().await {
            open.push(stream);
        }
    });
    url
}

/// Starts a fetch of `repo`, whose origin is the silent host, and waits
/// until it runs.
fn held_fetch(fixture: &Fixture, workspace: &WorkspaceId, repo: &RepoId) -> JobId {
    let id = fetch(fixture, workspace, repo);
    fixture.runtime.block_on(async {
        while job(fixture, id).state != JobState::Running {
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    });
    id
}

/// Writes `repo`'s commit-graph file, which succeeds.
fn write_commit_graph(fixture: &Fixture, workspace: &WorkspaceId, repo: &RepoId) -> JobId {
    let app = fixture.app().clone();
    fixture
        .runtime
        .block_on(app.commit_graph_write(workspace.clone(), repo.clone()))
        .unwrap()
}

/// Merges `other` into `repo`'s `main`, which conflicts.
fn merge(fixture: &Fixture, workspace: &WorkspaceId, repo: &RepoId) -> JobId {
    let app = fixture.app().clone();
    let request = IntegrateRequest {
        repo_id: repo.clone(),
        op: IntegrateOp::Merge {
            revision: "other".to_owned(),
            fast_forward: MergeFastForward::Config,
        },
        confirmed: false,
    };
    let start = fixture
        .runtime
        .block_on(app.op_integrate(workspace.clone(), request))
        .unwrap();
    let IntegrateStart::Started { job_id } = start else {
        panic!("{start:?}");
    };
    job_id
}

/// Job `id` as the app has it.
pub(crate) fn job(fixture: &Fixture, id: JobId) -> JobView {
    fixture
        .app()
        .jobs_get()
        .into_iter()
        .find(|job| job.job_id == id)
        .unwrap()
}

/// Waits until job `id` has ended, and checks that it ended with
/// `outcome`. The deadline only ends a hung test: git on a busy runner can
/// be slow, so it is generous.
#[track_caller]
pub(crate) fn wait_for_end(
    cx: &mut VisualTestContext,
    fixture: &Fixture,
    id: JobId,
    outcome: &Outcome,
) {
    for _ in 0..3000 {
        cx.run_until_parked();
        let view = job(fixture, id);
        if view.state == JobState::Ended {
            assert_eq!(
                view.outcome.as_ref(),
                Some(outcome),
                "{}: {:?}\n{}",
                view.label,
                view.message,
                view.output
            );
            return;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    panic!("job {id:?} never ended: {:?}", job(fixture, id));
}

pub(crate) fn label_of(cx: &mut VisualTestContext, id: &str) -> Option<String> {
    cx.update(|window, cx| {
        window.render_frame(cx);
        window
            .try_find(ElementId::Name(id.to_owned().into()))
            .and_then(|found| found.label().map(str::to_owned))
    })
}

/// Waits until the window has heard that job `id` ended.
fn wait_heard(cx: &mut VisualTestContext, window: &Entity<GitcrocWindow>, id: JobId) {
    wait_until(cx, |cx| {
        cx.update(|_, cx| {
            window
                .read(cx)
                .jobs()
                .read(cx)
                .get(id)
                .is_some_and(|job| job.state == JobState::Ended)
        })
    });
}

/// What the status bar says.
pub(crate) fn status(cx: &mut VisualTestContext) -> String {
    label_of(cx, "status-jobs").expect("no status bar")
}

#[track_caller]
fn wait_for_status(cx: &mut VisualTestContext, wanted: &str) {
    wait_until(cx, |cx| status(cx) == wanted);
}

/// The 10 s an outcome stays pass.
fn outcome_time_passes(cx: &mut VisualTestContext) {
    cx.executor().advance_clock(Duration::from_secs(10));
    cx.run_until_parked();
}

pub(crate) fn press(cx: &mut VisualTestContext, keys: &str) {
    cx.update(|window, cx| window.press(keys, cx));
    cx.run_until_parked();
}

fn job_list_open(cx: &mut VisualTestContext) -> bool {
    label_of(cx, "job-list").is_some()
}

fn open_job_list(cx: &mut VisualTestContext) {
    click(cx, "status-jobs".into());
    assert!(job_list_open(cx), "the job list did not open");
}

/// The job list's rows, top to bottom, and which is selected.
fn job_rows(cx: &mut VisualTestContext) -> Vec<(String, bool)> {
    cx.update(|window, cx| {
        window.render_frame(cx);
        window
            .within("job-list")
            .find_all_by_role(Role::ListBoxOption)
            .iter()
            .map(|row| {
                (
                    row.label().unwrap_or_default().to_owned(),
                    row.selected() == Some(true),
                )
            })
            .collect()
    })
}

fn selected_row(cx: &mut VisualTestContext) -> usize {
    job_rows(cx)
        .iter()
        .position(|(_, selected)| *selected)
        .expect("no job is selected")
}

fn outcome_text(outcome: &Outcome) -> String {
    strings::jobs::outcome(outcome)
}

/// UI-21: the active tab's running job shows with its label and progress,
/// then its outcome for 10 s. Jobs that ended before the window opened,
/// such as adding the repos, are only in the job list.
#[gpui::test]
fn ui_21_status_bar_shows_running_jobs_with_progress(cx: &mut TestAppContext) {
    let fixture = Fixture::start(EVENT_CAPACITY);
    let made = made(&silent_host(&fixture));
    let workspace = fixture.active();
    let added = fixture.add_repos(&workspace, made.path());
    let (mut cx, _window) = main_window(cx, &fixture);
    wait_for_status(&mut cx, strings::status_bar::NO_JOBS);

    let id = held_fetch(&fixture, &workspace, &id_of(&added, "api"));
    let label = job(&fixture, id).label;
    // The fetch waits on the host, and its total is not known.
    wait_for_status(&mut cx, &format!("{label} {}", strings::jobs::RUNNING));

    let cancelled = Outcome::Skipped {
        reason: gitcroc_core::SkipReason::Cancelled,
    };
    fixture.app().job_cancel(id);
    wait_for_end(&mut cx, &fixture, id, &cancelled);
    wait_for_status(&mut cx, &format!("{label}: {}", outcome_text(&cancelled)));
    outcome_time_passes(&mut cx);
    assert_eq!(status(&mut cx), strings::status_bar::NO_JOBS);
}

/// UI-21: a failed job stays in the status bar after the 10 s, until the
/// job list has shown it.
#[gpui::test]
fn ui_21_a_failed_job_stays_in_the_status_bar_until_seen(cx: &mut TestAppContext) {
    let fixture = Fixture::start(EVENT_CAPACITY);
    let missing = tempfile::tempdir().unwrap();
    let made = made(missing.path().join("gone").to_str().unwrap());
    let workspace = fixture.active();
    let added = fixture.add_repos(&workspace, made.path());
    let (mut cx, _window) = main_window(cx, &fixture);

    let id = fetch(&fixture, &workspace, &id_of(&added, "api"));
    wait_for_end(&mut cx, &fixture, id, &Outcome::Failed);
    let failed = format!(
        "{}: {}",
        job(&fixture, id).label,
        outcome_text(&Outcome::Failed)
    );
    wait_for_status(&mut cx, &failed);
    outcome_time_passes(&mut cx);
    assert_eq!(status(&mut cx), failed);

    open_job_list(&mut cx);
    press(&mut cx, "escape");
    assert!(!job_list_open(&mut cx));
    assert_eq!(status(&mut cx), strings::status_bar::NO_JOBS);
}

/// UI-21: the job list has every job of this run, from every tab, newest
/// first, each with its workspace and outcome; the selected one shows
/// git's output. The keys move the selection; Escape or a click beside it
/// closes it.
#[gpui::test]
fn ui_21_job_list_shows_outcome_and_raw_output_of_every_job(cx: &mut TestAppContext) {
    let fixture = Fixture::start(EVENT_CAPACITY);
    let missing = tempfile::tempdir().unwrap();
    let made = made(missing.path().join("gone").to_str().unwrap());
    let main = fixture.active();
    let added = fixture.add_repos(&main, made.path());
    let app = fixture.app().clone();
    let second = fixture
        .runtime
        .block_on(app.workspace_create("Second".to_owned()))
        .unwrap()
        .active
        .unwrap();
    // Repos of its own: no other tab reads the folder its merge writes.
    let theirs = self::made(missing.path().join("gone").to_str().unwrap());
    let in_second = fixture.add_repos(&second, theirs.path());
    fixture
        .runtime
        .block_on(app.workspace_activate(main.clone()))
        .unwrap();
    let (mut cx, window) = main_window(cx, &fixture);

    let succeeded = write_commit_graph(&fixture, &main, &id_of(&added, "web"));
    wait_for_end(&mut cx, &fixture, succeeded, &Outcome::Succeeded);
    let failed = fetch(&fixture, &main, &id_of(&added, "api"));
    wait_for_end(&mut cx, &fixture, failed, &Outcome::Failed);
    let conflict = merge(&fixture, &second, &id_of(&in_second, "web"));
    wait_for_end(&mut cx, &fixture, conflict, &Outcome::Conflict);
    // Ends arrive in order: the window has heard all three.
    wait_heard(&mut cx, &window, conflict);

    open_job_list(&mut cx);
    let rows = job_rows(&mut cx);
    let expected = [
        (conflict, "Second", Outcome::Conflict),
        (failed, "My workspace", Outcome::Failed),
        (succeeded, "My workspace", Outcome::Succeeded),
    ];
    // Under them, the two jobs that added the repos.
    assert_eq!(rows.len(), expected.len() + 2, "{rows:?}");
    for ((row, _), (id, workspace, outcome)) in rows.iter().zip(&expected) {
        let view = job(&fixture, *id);
        assert!(row.starts_with(&view.label), "{row}");
        assert!(row.contains(workspace), "{row}");
        assert!(row.contains(&outcome_text(outcome)), "{row}");
    }
    // The newest is selected and shows its output.
    assert_eq!(selected_row(&mut cx), 0);

    press(&mut cx, "down");
    assert_eq!(selected_row(&mut cx), 1);
    let output = job(&fixture, failed).output;
    assert!(!output.is_empty());
    assert_eq!(label_of(&mut cx, "job-output"), Some(output));
    press(&mut cx, "end");
    assert_eq!(selected_row(&mut cx), rows.len() - 1);
    press(&mut cx, "up");
    assert_eq!(selected_row(&mut cx), rows.len() - 2);
    press(&mut cx, "home");
    assert_eq!(selected_row(&mut cx), 0);
    // A click selects.
    click(&mut cx, ElementId::Name("job-row-2".into()));
    assert_eq!(selected_row(&mut cx), 2);

    press(&mut cx, "escape");
    assert!(!job_list_open(&mut cx));
    // A click beside it closes it too.
    open_job_list(&mut cx);
    cx.update(|window, cx| window.click_at("dialog-layer", point(px(5.), px(5.)), cx));
    cx.run_until_parked();
    assert!(!job_list_open(&mut cx));
}

/// UI-21, GRP-10: Cancel in the job list stops a running job.
#[gpui::test]
fn ui_21_cancel_stops_a_running_job(cx: &mut TestAppContext) {
    let fixture = Fixture::start(EVENT_CAPACITY);
    let made = made(&silent_host(&fixture));
    let workspace = fixture.active();
    let added = fixture.add_repos(&workspace, made.path());
    let (mut cx, _window) = main_window(cx, &fixture);
    let id = held_fetch(&fixture, &workspace, &id_of(&added, "api"));
    // The window has heard of it, so the list opens on it.
    wait_until(&mut cx, |cx| status(cx).ends_with(strings::jobs::RUNNING));

    open_job_list(&mut cx);
    assert!(label_of(&mut cx, "job-cancel").is_some());
    click(&mut cx, "job-cancel".into());
    let cancelled = Outcome::Skipped {
        reason: gitcroc_core::SkipReason::Cancelled,
    };
    wait_for_end(&mut cx, &fixture, id, &cancelled);
    wait_until(&mut cx, |cx| {
        job_rows(cx)[0].0.contains(&outcome_text(&cancelled))
    });
    assert_eq!(label_of(&mut cx, "job-cancel"), None);
}

/// UI-21, UI-22: F6 reaches the status bar, and Enter opens the job list.
#[gpui::test]
fn ui_21_the_job_list_opens_from_the_keyboard(cx: &mut TestAppContext) {
    let fixture = Fixture::start(EVENT_CAPACITY);
    let made = repos(&["api"]);
    fixture.add_repos(&fixture.active(), made.path());
    let (mut cx, _window) = main_window(cx, &fixture);

    for _ in 0..6 {
        let focused = cx.update(|window, cx| {
            window.render_frame(cx);
            window.find("status-jobs").focused() == Some(true)
        });
        if focused {
            break;
        }
        press(&mut cx, "f6");
    }
    press(&mut cx, "enter");
    assert!(job_list_open(&mut cx));
    // The job that added the repo.
    assert_eq!(job_rows(&mut cx).len(), 1);
    press(&mut cx, "escape");
    assert!(!job_list_open(&mut cx));
}

fn badge(cx: &mut VisualTestContext, workspace: &WorkspaceId) -> Option<String> {
    label_of(cx, &format!("tab-badge-{}", workspace.0))
}

fn marker(cx: &mut VisualTestContext, workspace: &WorkspaceId) -> bool {
    label_of(cx, &format!("tab-marker-{}", workspace.0)).is_some()
}

/// UI-32: a tab has a badge while it has jobs, and a marker when one ended
/// failed and was not seen: activating the tab or the job list sees it.
/// "api" fetches from the silent host, "web" from a missing folder.
#[gpui::test]
fn ui_32_tab_shows_a_badge_and_an_unseen_failure(cx: &mut TestAppContext) {
    let fixture = Fixture::start(EVENT_CAPACITY);
    let made = made(&silent_host(&fixture));
    let missing = tempfile::tempdir().unwrap();
    let gone = missing.path().join("gone");
    repo(&made, "web")
        .git(["remote", "add", "origin", gone.to_str().unwrap()])
        .unwrap();
    let main = fixture.active();
    let app = fixture.app().clone();
    let second = fixture
        .runtime
        .block_on(app.workspace_create("Second".to_owned()))
        .unwrap()
        .active
        .unwrap();
    let in_second = fixture.add_repos(&second, made.path());
    fixture
        .runtime
        .block_on(app.workspace_activate(main.clone()))
        .unwrap();
    let (mut cx, _window) = main_window(cx, &fixture);
    assert_eq!(badge(&mut cx, &second), None);

    let id = held_fetch(&fixture, &second, &id_of(&in_second, "api"));
    wait_until(&mut cx, |cx| {
        badge(cx, &second).as_deref() == Some(strings::tabs::jobs_title(1).as_str())
    });
    assert!(!marker(&mut cx, &second));
    // A cancelled job leaves no marker.
    fixture.app().job_cancel(id);
    wait_until(&mut cx, |cx| badge(cx, &second).is_none());
    assert!(!marker(&mut cx, &second));

    let id = fetch(&fixture, &second, &id_of(&in_second, "web"));
    wait_for_end(&mut cx, &fixture, id, &Outcome::Failed);
    wait_until(&mut cx, |cx| marker(cx, &second));
    assert_eq!(badge(&mut cx, &second), None);
    assert!(!marker(&mut cx, &main));

    // Its own tab's status bar shows it.
    click(&mut cx, tab(&second));
    wait_until(&mut cx, |cx| !marker(cx, &second));

    // Seen in the job list, from another tab.
    click(&mut cx, tab(&main));
    let id = fetch(&fixture, &second, &id_of(&in_second, "web"));
    wait_for_end(&mut cx, &fixture, id, &Outcome::Failed);
    wait_until(&mut cx, |cx| marker(cx, &second));
    open_job_list(&mut cx);
    press(&mut cx, "escape");
    assert!(!marker(&mut cx, &second));
}

/// PERF-1: the job list lays out only the rows on screen.
#[gpui::test]
fn perf_1_the_job_list_lays_out_only_the_rows_on_screen(cx: &mut TestAppContext) {
    let fixture = Fixture::start(EVENT_CAPACITY);
    let made = repos(&["api"]);
    let workspace = fixture.active();
    let added = fixture.add_repos(&workspace, made.path());
    let (mut cx, window) = main_window(cx, &fixture);
    let api = id_of(&added, "api");
    // One write at a time: a write already running is joined. The core
    // lets go of a write a little after it has ended, so a new write that
    // came back with the last one's job is asked for again.
    const JOBS: usize = 60;
    let mut started = HashSet::new();
    while started.len() < JOBS {
        let id = write_commit_graph(&fixture, &workspace, &api);
        started.insert(id);
        fixture.runtime.block_on(async {
            while job(&fixture, id).state != JobState::Ended {
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
        });
    }
    // The window has every job, the one that added the repo too.
    let listed = fixture.app().jobs_get().len();
    assert_eq!(listed, JOBS + 1);
    for _ in 0..3000 {
        settle(&mut cx, |_, _| true);
        if cx.update(|_, cx| window.read(cx).jobs().read(cx).len()) == listed {
            break;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    let heard = cx.update(|_, cx| window.read(cx).jobs().read(cx).len());
    assert_eq!(heard, listed, "the window has {heard} of {listed} jobs");

    open_job_list(&mut cx);
    let drawn = job_rows(&mut cx).len();
    assert!(
        drawn > 0 && drawn < JOBS / 2,
        "{drawn} of {JOBS} rows drawn"
    );
}

/// SEC-3: git's output and a workspace's name show as written, never as
/// markup.
#[gpui::test]
fn sec_3_job_output_and_labels_render_as_text(cx: &mut TestAppContext) {
    let fixture = Fixture::start(EVENT_CAPACITY);
    // Nothing listens on port 1: git fails at once, naming the URL.
    let made = made("http://127.0.0.1:1/<i>gone</i>.git");
    let name = "<b>Mine</b>";
    let app = fixture.app().clone();
    let workspace = fixture
        .runtime
        .block_on(app.workspace_create(name.to_owned()))
        .unwrap()
        .active
        .unwrap();
    let added = fixture.add_repos(&workspace, made.path());
    let (mut cx, window) = main_window(cx, &fixture);

    let id = fetch(&fixture, &workspace, &id_of(&added, "api"));
    wait_for_end(&mut cx, &fixture, id, &Outcome::Failed);
    wait_heard(&mut cx, &window, id);
    open_job_list(&mut cx);
    let view = job(&fixture, id);
    assert!(view.output.contains("<i>gone</i>"), "{}", view.output);
    assert_eq!(label_of(&mut cx, "job-output"), Some(view.output));
    assert!(job_rows(&mut cx)[0].0.contains(name));
}
