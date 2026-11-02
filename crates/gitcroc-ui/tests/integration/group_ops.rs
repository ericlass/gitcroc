//! M6-22a: the preflight dialog, the result report and group Fetch, Pull and
//! Push (GRP-5 to GRP-12, UI-20, UI-21, UI-22, SEC-3). Repos are real ones
//! made by `repo-gen` and added through the app (QA-2); remotes are local
//! bare repos or a port that never answers, so nothing waits on the network.
//! A group job's end is the barrier before a fixture changes.

use std::time::Duration;

use gitcroc_app::app::{AppEvent, EVENT_CAPACITY};
use gitcroc_core::{GroupId, JobState, JobView, Outcome, RepoId, TargetSet};
use gitcroc_ui::strings;
use gitcroc_ui::window::GitcrocWindow;
use gpui_kit::test::TestWindowExt;
use gpui_kit::{ElementId, Entity, Role, TestAppContext, VisualTestContext};
use tokio::sync::broadcast;

use crate::history::{
    add, click, id_of, label_of, labels_in, press, repo, repos, tree_row, wait_for_element,
    wait_until,
};
use crate::support::{Fixture, main_window};

/// How a repo's remote is set up.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Remote {
    /// A bare origin with `main` pushed and tracked.
    Ok,
    /// A bare origin with `main` pushed, but no upstream set.
    NoUpstream,
    /// A remote whose folder does not exist: the run fails.
    Missing,
    /// No remote at all.
    None,
}

/// How a repo's remote and local `main` differ.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Diverge {
    /// Both change `README.md`: a pull conflicts.
    Conflict,
    /// Each adds a file: a pull needs a merge commit.
    Merge,
    /// The remote changes `README.md`; the local repo changes it in the
    /// working tree only: a pull is refused.
    Overwrite,
}

pub(crate) struct Setup {
    pub(crate) fixture: Fixture,
    pub(crate) made: repo_gen::Fixture,
    remotes: tempfile::TempDir,
    added: Vec<(String, RepoId)>,
}

impl Setup {
    pub(crate) fn new(specs: &[(&'static str, Remote)]) -> Self {
        let names: Vec<&str> = specs.iter().map(|(name, _)| *name).collect();
        let made = repos(&names);
        let remotes = tempfile::tempdir().unwrap();
        for (name, remote) in specs {
            let local = repo(&made, name);
            local.git(["config", "core.autocrlf", "false"]).unwrap();
            local.git(["config", "user.name", "Test User"]).unwrap();
            local
                .git(["config", "user.email", "test@example.com"])
                .unwrap();
            match remote {
                Remote::Ok | Remote::NoUpstream => {
                    let origin = origin_of(&remotes, name);
                    local
                        .git(["remote", "add", "origin", origin.path().to_str().unwrap()])
                        .unwrap();
                    if *remote == Remote::Ok {
                        local
                            .git(["push", "--quiet", "-u", "origin", "main"])
                            .unwrap();
                    } else {
                        local.git(["push", "--quiet", "origin", "main"]).unwrap();
                    }
                }
                Remote::Missing => {
                    let gone = remotes.path().join(format!("{name}-gone.git"));
                    local
                        .git(["remote", "add", "origin", gone.to_str().unwrap()])
                        .unwrap();
                }
                Remote::None => {}
            }
        }
        Self {
            fixture: Fixture::start(EVENT_CAPACITY),
            made,
            remotes,
            added: Vec::new(),
        }
    }

    pub(crate) fn add(&mut self) {
        self.added = add(&self.fixture, &self.made);
    }

    pub(crate) fn id(&self, name: &str) -> RepoId {
        id_of(&self.added, name)
    }

    pub(crate) fn origin(&self, name: &str) -> repo_gen::Repo {
        origin_of(&self.remotes, name)
    }

    /// Starts a merge that conflicts, leaving the repo in conflict state
    /// (GRP-8).
    fn conflict(&self, name: &str) {
        let local = repo(&self.made, name);
        local.branch("other").unwrap();
        local.switch("other").unwrap();
        local
            .commit_file("README.md", "theirs\n", "Theirs")
            .unwrap();
        local.switch("main").unwrap();
        local.commit_file("README.md", "ours\n", "Ours").unwrap();
        local.merge_with_conflicts("other").unwrap();
    }

    /// Makes the repo's upstream commit something the local `main` does not
    /// have, and the local commit `kind` asks for.
    fn diverge(&self, name: &str, kind: Diverge) {
        let local = repo(&self.made, name);
        let origin = self.origin(name);
        let peer = origin
            .clone_to(self.remotes.path().join(format!("{name}-peer")))
            .unwrap();
        peer.git(["config", "user.name", "Test User"]).unwrap();
        peer.git(["config", "user.email", "test@example.com"])
            .unwrap();
        match kind {
            Diverge::Conflict | Diverge::Overwrite => {
                peer.commit_file("README.md", "theirs\n", "Theirs").unwrap();
            }
            Diverge::Merge => {
                peer.commit_file("remote.txt", "theirs\n", "Theirs")
                    .unwrap();
            }
        }
        peer.git(["push", "--quiet", "origin", "main"]).unwrap();
        match kind {
            Diverge::Conflict => {
                local.commit_file("README.md", "ours\n", "Ours").unwrap();
            }
            Diverge::Merge => {
                local.commit_file("local.txt", "ours\n", "Ours").unwrap();
            }
            Diverge::Overwrite => {}
        }
    }

    /// Points the repo's only remote at `url`.
    fn set_remote_url(&self, name: &str, url: &str) {
        repo(&self.made, name)
            .git(["remote", "set-url", "origin", url])
            .unwrap();
    }

    pub(crate) fn group(&self, name: &str, members: &[&str]) -> GroupId {
        crate::history_merged::group(
            &self.fixture,
            name,
            members.iter().map(|name| self.id(name)).collect(),
        )
    }

    pub(crate) fn group_node(&self, group: &GroupId) -> String {
        crate::history_merged::group_node(group)
    }

    pub(crate) fn jobs(&self) -> usize {
        self.fixture.app().jobs_get().len()
    }
}

/// A bare origin for `name`, under the setup's remote folder.
fn origin_of(remotes: &tempfile::TempDir, name: &str) -> repo_gen::Repo {
    let path = remotes.path().join(format!("{name}.git"));
    std::fs::create_dir_all(&path).unwrap();
    let origin = repo_gen::Repo::open(&path);
    origin
        .git(["init", "--bare", "--quiet", "--initial-branch=main"])
        .unwrap();
    origin
}

/// Opens the main window and selects `group`'s row.
pub(crate) fn open(
    cx: &mut TestAppContext,
    setup: &Setup,
    group: &GroupId,
) -> (VisualTestContext, Entity<GitcrocWindow>) {
    let (mut cx, window) = main_window(cx, &setup.fixture);
    crate::stage::enlarge(&mut cx);
    wait_for_element(&mut cx, tree_row(&setup.group_node(group)));
    click(&mut cx, tree_row(&setup.group_node(group)));
    (cx, window)
}

fn selected_group(cx: &mut VisualTestContext, window: &Entity<GitcrocWindow>) -> TargetSet {
    cx.update(|_, cx| window.read(cx).tree(cx).unwrap().read(cx).target().unwrap())
}

fn prior_jobs(setup: &Setup) -> usize {
    setup.jobs()
}

#[track_caller]
pub(crate) fn wait_group_end(
    cx: &mut VisualTestContext,
    events: &mut broadcast::Receiver<AppEvent>,
) -> JobView {
    for _ in 0..3000 {
        cx.run_until_parked();
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

#[track_caller]
pub(crate) fn wait_preflight(
    cx: &mut VisualTestContext,
    events: &mut broadcast::Receiver<AppEvent>,
) -> gitcroc_core::PreflightReady {
    for _ in 0..3000 {
        cx.run_until_parked();
        while let Ok(event) = events.try_recv() {
            if let AppEvent::PreflightReady(ready) = event {
                return ready;
            }
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    panic!("no preflight finished");
}

pub(crate) fn shown(cx: &mut VisualTestContext, id: &str) -> bool {
    label_of(cx, ElementId::Name(id.to_owned().into())).is_some()
}

pub(crate) fn focused(cx: &mut VisualTestContext, id: &str) -> bool {
    cx.update(|window, cx| {
        window.render_frame(cx);
        window.find(ElementId::Name(id.to_owned().into())).focused() == Some(true)
    })
}

pub(crate) fn preflight_rows(cx: &mut VisualTestContext) -> Vec<String> {
    labels_in(cx, "preflight-repos".into(), Role::ListItem)
}

/// Waits until the preflight's result is in its dialog, not just the
/// checking line.
pub(crate) fn wait_preflight_ready(cx: &mut VisualTestContext) {
    wait_until(cx, |cx| !preflight_rows(cx).is_empty());
}

fn retry_disabled(cx: &mut VisualTestContext) -> bool {
    cx.update(|window, cx| {
        window.render_frame(cx);
        window.find("report-retry").disabled() == Some(true)
    })
}

fn job_list_open(cx: &mut VisualTestContext) -> bool {
    shown(cx, "job-list")
}

fn open_job_list(cx: &mut VisualTestContext) {
    click(cx, "status-jobs".into());
    wait_for_element(cx, "job-list".into());
}

/// A host that takes each request and never answers, so a fetch from it
/// runs until it is cancelled (GRP-10).
fn silent_host(setup: &Setup) -> String {
    let listener = setup
        .fixture
        .runtime
        .block_on(tokio::net::TcpListener::bind("127.0.0.1:0"))
        .unwrap();
    let url = format!("http://{}/api.git", listener.local_addr().unwrap());
    setup.fixture.runtime.spawn(async move {
        let mut open = Vec::new();
        while let Ok((stream, _)) = listener.accept().await {
            open.push(stream);
        }
    });
    url
}

pub(crate) fn head(repo: &repo_gen::Repo) -> String {
    repo.git(["rev-parse", "HEAD"]).unwrap().trim().to_owned()
}

fn file(repo: &repo_gen::Repo, path: &str) -> String {
    std::fs::read_to_string(repo.path().join(path)).unwrap()
}

pub(crate) fn ref_exists(repo: &repo_gen::Repo, reference: &str) -> bool {
    repo.git(["rev-parse", "--verify", "--quiet", reference])
        .is_ok()
}

/// GRP-9: a preflight with every repo *will run* starts at once; one with a
/// skip shows the dialog, and Run starts it confirmed.
#[gpui::test]
fn grp_9_the_preflight_dialog_appears_only_when_the_core_asks(cx: &mut TestAppContext) {
    let mut setup = Setup::new(&[
        ("api", Remote::Ok),
        ("web", Remote::Ok),
        ("docs", Remote::None),
    ]);
    setup.add();
    let all = setup.group("Backend", &["api", "web"]);
    let skip = setup.group("Skipping", &["api", "docs"]);
    let (mut cx, window) = open(cx, &setup, &all);

    let mut events = setup.fixture.app().subscribe();
    click(&mut cx, "toolbar-0".into());
    let job = wait_group_end(&mut cx, &mut events);
    assert_eq!(job.report.as_ref().unwrap().len(), 2);
    assert!(!shown(&mut cx, "preflight"));
    assert!(!job_list_open(&mut cx), "a success does not open the list");

    // A repo that will skip asks first; Run starts it confirmed.
    click(&mut cx, tree_row(&setup.group_node(&skip)));
    assert_eq!(
        selected_group(&mut cx, &window),
        TargetSet::Group {
            group_id: skip.clone()
        }
    );
    let mut events = setup.fixture.app().subscribe();
    click(&mut cx, "toolbar-0".into());
    wait_for_element(&mut cx, "preflight".into());
    wait_preflight_ready(&mut cx);
    assert_eq!(
        label_of(&mut cx, "preflight-counts".into()).as_deref(),
        Some("1 will run, 1 will skip")
    );
    click(&mut cx, "preflight-run".into());
    let job = wait_group_end(&mut cx, &mut events);
    let report = job.report.unwrap();
    assert_eq!(report.len(), 2);
    assert_eq!(
        report[1].outcome,
        Outcome::Skipped {
            reason: gitcroc_core::SkipReason::Preflight {
                reason: "no remote".to_owned()
            }
        }
    );
    assert!(!shown(&mut cx, "preflight"));
}

/// GRP-9: a destructive group push names what cannot be undone and every
/// repo it runs in, with the action and count on the run button, and Cancel
/// focused.
#[gpui::test]
fn grp_9_destructive_group_operation_names_every_affected_repo(cx: &mut TestAppContext) {
    let mut setup = Setup::new(&[("api", Remote::Ok), ("web", Remote::Ok)]);
    setup.add();
    let group = setup.group("Backend", &["api", "web"]);
    let (mut cx, _window) = open(cx, &setup, &group);
    let before = prior_jobs(&setup);

    click(&mut cx, "toolbar-options-2".into());
    wait_for_element(&mut cx, "group-sync".into());
    assert_eq!(
        label_of(&mut cx, "dialog-title".into()).as_deref(),
        Some("Push Backend")
    );
    click(&mut cx, "group-sync-force".into());
    click(&mut cx, "group-sync-submit".into());
    wait_for_element(&mut cx, "preflight".into());
    wait_preflight_ready(&mut cx);

    let rows = preflight_rows(&mut cx);
    assert_eq!(rows.len(), 2, "{rows:?}");
    assert!(
        rows[0].contains("api") && rows[0].contains("Will run"),
        "{rows:?}"
    );
    let actions = labels_in(&mut cx, "preflight-actions".into(), Role::ListItem);
    assert_eq!(actions.len(), 1, "{actions:?}");
    assert!(actions[0].contains("Force push"), "{actions:?}");
    let affected = labels_in(&mut cx, "preflight-affected".into(), Role::ListItem);
    assert_eq!(affected, ["api", "web"]);
    assert_eq!(
        label_of(&mut cx, "preflight-run".into()).as_deref(),
        Some("Force push in 2 repos")
    );
    assert!(focused(&mut cx, "preflight-cancel"));

    // Escape drops the preflight, and nothing runs.
    press(&mut cx, "escape");
    wait_until(&mut cx, |cx| !shown(cx, "preflight"));
    assert_eq!(prior_jobs(&setup), before);
}

/// GRP-8: a repo in conflict state is blocked for a pull but fetches.
#[gpui::test]
fn grp_8_a_repo_in_conflict_is_blocked_for_pull_but_fetches(cx: &mut TestAppContext) {
    let mut setup = Setup::new(&[("api", Remote::Ok), ("web", Remote::Ok)]);
    setup.conflict("web");
    setup.add();
    let group = setup.group("Backend", &["api", "web"]);
    let (mut cx, _window) = open(cx, &setup, &group);

    let mut events = setup.fixture.app().subscribe();
    click(&mut cx, "toolbar-1".into());
    wait_for_element(&mut cx, "preflight".into());
    wait_preflight_ready(&mut cx);
    let rows = preflight_rows(&mut cx);
    let web = rows
        .iter()
        .find(|row| row.contains("web"))
        .unwrap_or_else(|| panic!("{rows:?}"));
    assert!(web.contains("Blocked"), "{rows:?}");
    assert!(web.contains("conflict state"), "{rows:?}");
    click(&mut cx, "preflight-run".into());
    let job = wait_group_end(&mut cx, &mut events);
    let report = job.report.unwrap();
    assert!(
        matches!(
            &report[1].outcome,
            Outcome::Skipped {
                reason: gitcroc_core::SkipReason::Blocked { .. }
            }
        ),
        "{report:?}"
    );

    // Fetch runs in conflict state, so web is not blocked.
    let mut events = setup.fixture.app().subscribe();
    click(&mut cx, "toolbar-0".into());
    let job = wait_group_end(&mut cx, &mut events);
    assert!(
        matches!(
            job.report.unwrap()[1].outcome,
            Outcome::Succeeded | Outcome::NothingToDo
        ),
        "web must fetch in conflict state"
    );
}

/// GRP-5: the preflight dialog only shows the target set; it never edits
/// it. Changing the selection is the tree's business.
#[gpui::test]
fn grp_5_the_dialog_never_changes_the_target_set(cx: &mut TestAppContext) {
    let mut setup = Setup::new(&[("api", Remote::Ok), ("docs", Remote::None)]);
    setup.add();
    let group = setup.group("Backend", &["api", "docs"]);
    let (mut cx, window) = open(cx, &setup, &group);
    let target = TargetSet::Group {
        group_id: group.clone(),
    };
    let members = |cx: &mut VisualTestContext| {
        cx.update(|_, _cx| {
            setup
                .fixture
                .app()
                .tree_get(&setup.fixture.active())
                .unwrap()
                .into_iter()
                .find_map(|node| match node.item {
                    gitcroc_core::TreeItem::Group { group_id, .. } if group_id == group => Some(
                        node.children
                            .unwrap_or_default()
                            .into_iter()
                            .filter_map(|child| match child.item {
                                gitcroc_core::TreeItem::Repo { repo_id, .. } => Some(repo_id),
                                _ => None,
                            })
                            .collect::<Vec<_>>(),
                    ),
                    _ => None,
                })
                .unwrap()
        })
    };
    let before = members(&mut cx);

    click(&mut cx, "toolbar-0".into());
    wait_for_element(&mut cx, "preflight".into());
    wait_preflight_ready(&mut cx);
    assert_eq!(selected_group(&mut cx, &window), target);
    assert_eq!(members(&mut cx), before);
    assert!(preflight_rows(&mut cx).len() == 2);
    assert_eq!(
        label_of(&mut cx, "preflight-hint".into()).as_deref(),
        Some(strings::group_ops::preflight::LEAVE_OUT)
    );
    press(&mut cx, "escape");
    wait_until(&mut cx, |cx| !shown(cx, "preflight"));
    assert_eq!(selected_group(&mut cx, &window), target);
    assert_eq!(members(&mut cx), before);
}

/// GRP-9: Cancel drops the preflight before and after its result.
#[gpui::test]
fn grp_9_cancel_drops_the_preflight_before_and_after_the_result(cx: &mut TestAppContext) {
    let mut setup = Setup::new(&[("api", Remote::Ok), ("docs", Remote::None)]);
    setup.add();
    let group = setup.group("Backend", &["api", "docs"]);
    let (mut cx, window) = open(cx, &setup, &group);
    let before = prior_jobs(&setup);
    let app = setup.fixture.app().clone();

    // Cancel before the core answered: its answer is dropped, so the
    // preflight cannot start.
    let mut events = app.subscribe();
    cx.update(|window, cx| {
        window.render_frame(cx);
        window.click("toolbar-0", cx);
    });
    cx.update(|_, cx| {
        let ops = window.read(cx).group_ops().clone();
        ops.update(cx, |ops, cx| ops.cancel(cx));
    });
    let ready = wait_preflight(&mut cx, &mut events);
    if let Ok(job) = setup
        .fixture
        .runtime
        .block_on(app.group_start(ready.preflight_id, false))
    {
        // A preflight that still starts is the failure; stop its job.
        app.job_cancel(job);
        panic!("the cancelled preflight still started");
    }
    cx.run_until_parked();

    // Cancel once the result is in the dialog.
    let mut events = app.subscribe();
    click(&mut cx, "toolbar-0".into());
    wait_for_element(&mut cx, "preflight".into());
    let ready = wait_preflight(&mut cx, &mut events);
    assert!(ready.needs_confirmation);
    click(&mut cx, "preflight-cancel".into());
    wait_until(&mut cx, |cx| !shown(cx, "preflight"));
    assert!(
        setup
            .fixture
            .runtime
            .block_on(app.group_start(ready.preflight_id, true))
            .is_err()
    );
    assert_eq!(prior_jobs(&setup), before);
}

/// GRP-7: the report lists every repo of the target set in target-set
/// order with one outcome, the counts per outcome, git's output on expand,
/// and a conflict shown as such.
#[gpui::test]
fn grp_7_the_report_lists_every_repo_with_one_outcome(cx: &mut TestAppContext) {
    let mut setup = Setup::new(&[
        ("api", Remote::Ok),
        ("web", Remote::Ok),
        ("cli", Remote::Missing),
        ("docs", Remote::None),
        ("pulling", Remote::Ok),
    ]);
    setup.conflict("web");
    setup.diverge("pulling", Diverge::Conflict);
    setup.add();
    let group = setup.group("Backend", &["api", "web", "cli", "docs"]);
    let pulling = setup.group("Pulling", &["pulling"]);
    let (mut cx, window) = open(cx, &setup, &group);
    // A branch only the remotes have, so the fetching repos succeed.
    setup
        .origin("api")
        .git(["branch", "topic", "main"])
        .unwrap();
    setup
        .origin("web")
        .git(["branch", "topic", "main"])
        .unwrap();

    let mut events = setup.fixture.app().subscribe();
    click(&mut cx, "toolbar-0".into());
    wait_for_element(&mut cx, "preflight".into());
    wait_preflight_ready(&mut cx);
    click(&mut cx, "preflight-run".into());
    let job = wait_group_end(&mut cx, &mut events);
    let report = job.report.clone().unwrap();
    assert_eq!(
        report
            .iter()
            .map(|line| line.repo_name.as_str())
            .collect::<Vec<_>>(),
        ["api", "web", "cli", "docs"]
    );
    assert_eq!(
        report
            .iter()
            .map(|line| line.outcome.clone())
            .collect::<Vec<_>>(),
        [
            Outcome::Succeeded,
            Outcome::Succeeded,
            Outcome::Failed,
            Outcome::Skipped {
                reason: gitcroc_core::SkipReason::Preflight {
                    reason: "no remote".to_owned()
                }
            }
        ]
    );
    // A failed repo opens the list by itself.
    wait_for_element(&mut cx, "job-list".into());
    assert_eq!(
        label_of(&mut cx, "report-counts".into()).as_deref(),
        Some("1 failed, 2 succeeded, 1 skipped")
    );
    assert!(focused(&mut cx, "report-toggle-0"));
    press(&mut cx, "down");
    press(&mut cx, "down");
    assert!(focused(&mut cx, "report-toggle-2"));
    press(&mut cx, "enter");
    cx.run_until_parked();
    let output = report[2].output.clone();
    assert!(!output.is_empty());
    assert_eq!(label_of(&mut cx, "report-output-2".into()), Some(output));
    // The repo's name selects it in its workspace's tree and closes the
    // list.
    press(&mut cx, "right");
    assert!(focused(&mut cx, "report-name-2"));
    press(&mut cx, "enter");
    wait_until(&mut cx, |cx| !job_list_open(cx));
    assert_eq!(
        selected_group(&mut cx, &window),
        TargetSet::Repo {
            repo_id: setup.id("cli")
        }
    );

    // A pull conflict is not a failure; the report shows it as a conflict.
    click(&mut cx, tree_row(&setup.group_node(&pulling)));
    assert_eq!(
        selected_group(&mut cx, &window),
        TargetSet::Group { group_id: pulling }
    );
    // Merge explicitly: with git config unset the default pull is
    // fast-forward only, which fails rather than conflicts.
    click(&mut cx, "toolbar-options-1".into());
    wait_for_element(&mut cx, "group-sync".into());
    click(&mut cx, "group-sync-choice-2".into());
    let mut events = setup.fixture.app().subscribe();
    click(&mut cx, "group-sync-submit".into());
    let job = wait_group_end(&mut cx, &mut events);
    assert_eq!(job.outcome, Some(Outcome::Conflict));
    assert_eq!(job.report.as_ref().unwrap()[0].outcome, Outcome::Conflict);
    wait_for_element(&mut cx, "job-list".into());
    assert_eq!(
        label_of(&mut cx, "report-counts".into()).as_deref(),
        Some("1 conflict")
    );
    assert_eq!(
        label_of(&mut cx, "report-outcome-0".into()).as_deref(),
        Some("Conflict")
    );
    // Only a failed repo earns Retry failed.
    assert!(retry_disabled(&mut cx));
}

/// GRP-6: one repo failing never stops the others.
#[gpui::test]
fn grp_6_one_repo_failing_does_not_stop_the_others(cx: &mut TestAppContext) {
    let mut setup = Setup::new(&[("api", Remote::Ok), ("cli", Remote::Missing)]);
    setup.add();
    let group = setup.group("Backend", &["api", "cli"]);
    let (mut cx, _window) = open(cx, &setup, &group);
    // A branch only the remote has, which a successful fetch brings in.
    let origin = setup.origin("api");
    origin.git(["branch", "topic", "main"]).unwrap();
    let api = repo(&setup.made, "api");
    assert!(!ref_exists(&api, "refs/remotes/origin/topic"));

    let mut events = setup.fixture.app().subscribe();
    click(&mut cx, "toolbar-0".into());
    let job = wait_group_end(&mut cx, &mut events);
    let report = job.report.unwrap();
    assert_eq!(report[0].outcome, Outcome::Succeeded);
    assert_eq!(report[1].outcome, Outcome::Failed);
    assert!(
        ref_exists(&api, "refs/remotes/origin/topic"),
        "the successful fetch did not run"
    );
}

/// GRP-10: cancelling a group job reports its repos as *skipped
/// (cancelled)*.
#[gpui::test]
fn grp_10_cancel_reports_skipped_cancelled(cx: &mut TestAppContext) {
    let mut setup = Setup::new(&[("api", Remote::Ok), ("web", Remote::Ok)]);
    setup.add();
    let group = setup.group("Backend", &["api", "web"]);
    let (mut cx, window) = open(cx, &setup, &group);
    // A method that sets both remotes after the setup help their upstream.
    {
        let host = silent_host(&setup);
        setup.set_remote_url("api", &host);
        setup.set_remote_url("web", &host);
        setup.fixture.refresh(&setup.fixture.active());
    }

    let mut events = setup.fixture.app().subscribe();
    click(&mut cx, "toolbar-0".into());
    // Wait until the window has the job running, for the status bar and the
    // list, and the checking dialog is gone, then cancel in the list (UI-21).
    wait_until(&mut cx, |cx| {
        cx.update(|_, cx| {
            window.read(cx).jobs().read(cx).jobs().any(|job| {
                job.label.starts_with("Fetch · Backend") && job.state == JobState::Running
            })
        })
    });
    wait_until(&mut cx, |cx| !shown(cx, "preflight"));
    open_job_list(&mut cx);
    click(&mut cx, "job-cancel".into());
    let job = wait_group_end(&mut cx, &mut events);
    let cancelled = Outcome::Skipped {
        reason: gitcroc_core::SkipReason::Cancelled,
    };
    let report = job.report.unwrap();
    assert_eq!(report.len(), 2, "{report:?}");
    assert!(
        report.iter().all(|line| line.outcome == cancelled),
        "{report:?}"
    );
    // The list is open on the job; its lines say so.
    assert_eq!(
        label_of(&mut cx, "report-outcome-0".into()).as_deref(),
        Some("Skipped (cancelled)")
    );
}

/// GRP-11: Retry failed starts a new job on the report's failed repos only.
#[gpui::test]
fn grp_11_retry_failed_targets_only_the_failed_repos(cx: &mut TestAppContext) {
    let mut setup = Setup::new(&[("api", Remote::Ok), ("cli", Remote::Missing)]);
    setup.add();
    let group = setup.group("Backend", &["api", "cli"]);
    let (mut cx, _window) = open(cx, &setup, &group);

    let mut events = setup.fixture.app().subscribe();
    click(&mut cx, "toolbar-0".into());
    let first = wait_group_end(&mut cx, &mut events);
    let report = first.report.clone().unwrap();
    assert_eq!(report.len(), 2);
    assert_eq!(report[1].outcome, Outcome::Failed);
    wait_for_element(&mut cx, "job-list".into());
    assert!(!retry_disabled(&mut cx));

    let mut events = setup.fixture.app().subscribe();
    click(&mut cx, "report-retry".into());
    // The retry's own failed report opens by itself too (UI-21).
    let second = wait_group_end(&mut cx, &mut events);
    let report = second.report.unwrap();
    assert_eq!(report.len(), 1, "{report:?}");
    assert_eq!(report[0].repo_name, "cli");
    assert_eq!(second.repo_names, ["cli"]);
}

/// GRP-12: a pull git refuses because of local changes is failed with git's
/// message; the app stashes nothing and the changes stay.
#[gpui::test]
fn grp_12_a_refused_pull_is_failed_with_gits_message(cx: &mut TestAppContext) {
    let mut setup = Setup::new(&[("api", Remote::Ok)]);
    setup.diverge("api", Diverge::Overwrite);
    setup.add();
    let group = setup.group("Backend", &["api"]);
    let local = repo(&setup.made, "api");
    local.write("README.md", "local\n").unwrap();
    let before = head(&local);
    let (mut cx, _window) = open(cx, &setup, &group);

    let mut events = setup.fixture.app().subscribe();
    click(&mut cx, "toolbar-1".into());
    let job = wait_group_end(&mut cx, &mut events);
    let report = job.report.unwrap();
    assert_eq!(report[0].outcome, Outcome::Failed);
    let message = report[0].message.clone().unwrap_or_default();
    assert!(
        message.contains("local changes") || message.contains("would be overwritten"),
        "{message}"
    );
    // Nothing was stashed, committed or discarded.
    assert_eq!(local.git(["stash", "list"]).unwrap().trim(), "");
    assert_eq!(file(&local, "README.md"), "local\n");
    assert_eq!(head(&local), before);
}

/// UI-20: the toolbar's Fetch, Pull and Push run on a group with section
/// 5's defaults, and each options dialog's choice reaches the core.
#[gpui::test]
fn ui_20_the_toolbar_runs_fetch_pull_and_push_on_a_group(cx: &mut TestAppContext) {
    let mut setup = Setup::new(&[
        ("api", Remote::Ok),
        ("pulling", Remote::Ok),
        ("fresh", Remote::NoUpstream),
    ]);
    setup.diverge("pulling", Diverge::Merge);
    setup.add();
    let fetching = setup.group("Fetching", &["api"]);
    let pulling = setup.group("Pulling", &["pulling"]);
    let fresh = setup.group("Fresh", &["fresh"]);
    let (mut cx, _window) = open(cx, &setup, &fetching);
    let api = repo(&setup.made, "api");
    let head = head(&api);
    api.git(["update-ref", "refs/remotes/origin/gone", &head])
        .unwrap();

    // The default follows git config, which does not prune.
    let mut events = setup.fixture.app().subscribe();
    click(&mut cx, "toolbar-0".into());
    wait_group_end(&mut cx, &mut events);
    assert!(ref_exists(&api, "refs/remotes/origin/gone"));

    // The options dialog's Prune reaches the core.
    click(&mut cx, "toolbar-options-0".into());
    wait_for_element(&mut cx, "group-sync".into());
    assert_eq!(
        label_of(&mut cx, "dialog-title".into()).as_deref(),
        Some("Fetch Fetching")
    );
    click(&mut cx, "group-sync-choice-1".into());
    let mut events = setup.fixture.app().subscribe();
    click(&mut cx, "group-sync-submit".into());
    wait_group_end(&mut cx, &mut events);
    assert!(!ref_exists(&api, "refs/remotes/origin/gone"));

    // The pull strategy reaches the core: Merge makes a merge commit where
    // git config would be fast-forward only.
    click(&mut cx, tree_row(&setup.group_node(&pulling)));
    click(&mut cx, "toolbar-options-1".into());
    wait_for_element(&mut cx, "group-sync".into());
    assert_eq!(
        label_of(&mut cx, "dialog-title".into()).as_deref(),
        Some("Pull Pulling")
    );
    click(&mut cx, "group-sync-choice-2".into());
    let mut events = setup.fixture.app().subscribe();
    click(&mut cx, "group-sync-submit".into());
    wait_group_end(&mut cx, &mut events);
    let parents = repo(&setup.made, "pulling")
        .git(["rev-list", "--parents", "-n1", "HEAD"])
        .unwrap();
    assert_eq!(parents.split_whitespace().count(), 3, "{parents}");

    // The push default sets the upstream where a branch has none.
    click(&mut cx, tree_row(&setup.group_node(&fresh)));
    let mut events = setup.fixture.app().subscribe();
    click(&mut cx, "toolbar-2".into());
    // No upstream is *needs attention*, so the dialog asks first.
    wait_for_element(&mut cx, "preflight".into());
    wait_preflight_ready(&mut cx);
    click(&mut cx, "preflight-run".into());
    wait_group_end(&mut cx, &mut events);
    assert_eq!(
        repo(&setup.made, "fresh")
            .git(["rev-parse", "--abbrev-ref", "main@{upstream}"])
            .unwrap()
            .trim(),
        "origin/main"
    );

    // Force with lease in the options is destructive, so the preflight asks.
    click(&mut cx, tree_row(&setup.group_node(&fetching)));
    click(&mut cx, "toolbar-options-2".into());
    wait_for_element(&mut cx, "group-sync".into());
    click(&mut cx, "group-sync-force".into());
    click(&mut cx, "group-sync-submit".into());
    wait_for_element(&mut cx, "preflight".into());
    wait_preflight_ready(&mut cx);
    assert_eq!(
        label_of(&mut cx, "preflight-run".into()).as_deref(),
        Some("Force push in 1 repo")
    );
    assert!(shown(&mut cx, "preflight-destructive"));
    press(&mut cx, "escape");
}

/// UI-21: a report with a failed or conflict repo opens by itself; others
/// do not.
#[gpui::test]
fn ui_21_a_report_with_a_failure_opens_by_itself(cx: &mut TestAppContext) {
    let mut setup = Setup::new(&[("api", Remote::Ok), ("cli", Remote::Missing)]);
    setup.add();
    let failing = setup.group("Failing", &["api", "cli"]);
    let good = setup.group("Good", &["api"]);
    let (mut cx, _window) = open(cx, &setup, &failing);

    let mut events = setup.fixture.app().subscribe();
    click(&mut cx, "toolbar-0".into());
    wait_group_end(&mut cx, &mut events);
    wait_for_element(&mut cx, "job-list".into());
    assert!(shown(&mut cx, "report-counts"));

    press(&mut cx, "escape");
    wait_until(&mut cx, |cx| !job_list_open(cx));
    click(&mut cx, tree_row(&setup.group_node(&good)));
    let mut events = setup.fixture.app().subscribe();
    click(&mut cx, "toolbar-0".into());
    let job = wait_group_end(&mut cx, &mut events);
    assert!(
        job.report
            .as_ref()
            .unwrap()
            .iter()
            .all(|line| matches!(line.outcome, Outcome::Succeeded | Outcome::NothingToDo))
    );
    assert!(!job_list_open(&mut cx), "a success opens no report");
}

/// UI-22: the preflight dialog and the report work from the keyboard.
#[gpui::test]
fn ui_22_the_report_and_the_preflight_dialog_from_the_keyboard(cx: &mut TestAppContext) {
    let mut setup = Setup::new(&[("api", Remote::Ok), ("docs", Remote::None)]);
    setup.add();
    let group = setup.group("Backend", &["api", "docs"]);
    let (mut cx, _window) = open(cx, &setup, &group);

    let mut events = setup.fixture.app().subscribe();
    click(&mut cx, "toolbar-0".into());
    wait_for_element(&mut cx, "preflight".into());
    wait_preflight_ready(&mut cx);
    assert!(focused(&mut cx, "preflight-run"));
    press(&mut cx, "enter");
    wait_group_end(&mut cx, &mut events);

    open_job_list(&mut cx);
    assert!(focused(&mut cx, "report-toggle-0"));
    press(&mut cx, "down");
    assert!(focused(&mut cx, "report-toggle-1"));
    press(&mut cx, "right");
    assert!(focused(&mut cx, "report-name-1"));
    press(&mut cx, "down");
    assert!(focused(&mut cx, "report-name-1"), "the last line");
    press(&mut cx, "home");
    assert!(focused(&mut cx, "report-name-0"));
    press(&mut cx, "end");
    assert!(focused(&mut cx, "report-name-1"));
    press(&mut cx, "left");
    assert!(focused(&mut cx, "report-toggle-1"));
    press(&mut cx, "enter");
    cx.run_until_parked();
    assert!(shown(&mut cx, "report-output-1"));
    press(&mut cx, "home");
    assert!(focused(&mut cx, "report-toggle-0"));
    press(&mut cx, "escape");
    assert!(!job_list_open(&mut cx));
}

/// SEC-3: repo names and git's output are drawn as text, never as markup.
#[gpui::test]
fn sec_3_repo_names_and_git_output_render_as_text(cx: &mut TestAppContext) {
    let mut setup = Setup::new(&[("a&b", Remote::Missing)]);
    setup.add();
    let group = setup.group("Backend", &["a&b"]);
    // git fails naming the URL, markup and all.
    setup.set_remote_url("a&b", "http://127.0.0.1:1/<i>gone</i>.git");
    setup.fixture.refresh(&setup.fixture.active());
    let (mut cx, _window) = open(cx, &setup, &group);

    let mut events = setup.fixture.app().subscribe();
    click(&mut cx, "toolbar-0".into());
    let job = wait_group_end(&mut cx, &mut events);
    let report = job.report.unwrap();
    assert!(
        report[0].output.contains("<i>gone</i>"),
        "{:?}",
        report[0].output
    );
    wait_for_element(&mut cx, "job-list".into());
    assert_eq!(
        label_of(&mut cx, "report-name-0".into()).as_deref(),
        Some("a&b")
    );
    assert!(focused(&mut cx, "report-toggle-0"));
    press(&mut cx, "enter");
    cx.run_until_parked();
    let output = label_of(&mut cx, "report-output-0".into()).unwrap();
    assert_eq!(output, report[0].output);
    assert!(output.contains("<i>gone</i>"), "{output}");
}
