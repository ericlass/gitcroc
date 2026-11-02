//! M6-22c: the commit box of a group or several repos (UI-34) and the group
//! Commit it runs through the preflight (GRP-15, GRP-6). Repos are real
//! ones made by `repo-gen` and added through the app (QA-2); a group job's
//! end is the barrier before anything is checked.

use gitcroc_core::{CommitMessage, GroupId, Outcome};
use gitcroc_ui::changes::FileRow;
use gitcroc_ui::commit_box::CommitBox;
use gitcroc_ui::strings;
use gitcroc_ui::window::GitcrocWindow;
use gpui_kit::test::TestWindowExt;
use gpui_kit::{ElementId, Entity, TestAppContext, VisualTestContext};

use crate::changes::{wait_for_view, with_changes};
use crate::group_ops::{
    Remote, Setup, head, open, preflight_rows, wait_group_end, wait_preflight_ready,
};
use crate::history::{click, label_of, repo, tree_row, wait_for_element, wait_until};
use crate::history_merged::present;

/// The group's repos: two with a staged file, one with only a changed
/// tracked file, and one with a staged file whose `pre-commit` hook fails.
const FOUR: [&str; 4] = ["api", "web", "docs", "hooked"];

fn setup(names: &[&'static str]) -> Setup {
    let specs: Vec<(&'static str, Remote)> = names.iter().map(|name| (*name, Remote::Ok)).collect();
    let mut setup = Setup::new(&specs);
    for name in names {
        let local = repo(&setup.made, name);
        local.git(["config", "commit.gpgsign", "false"]).unwrap();
        match *name {
            "docs" => local.write("README.md", "changed\n").unwrap(),
            "hooked" => {
                local.write("h.txt", "h\n").unwrap();
                local.git(["add", "h.txt"]).unwrap();
                let hook = local.path().join(".git").join("hooks").join("pre-commit");
                std::fs::write(&hook, "#!/bin/sh\necho refused by the hook >&2\nexit 1\n").unwrap();
                #[cfg(unix)]
                {
                    use std::os::unix::fs::PermissionsExt;
                    std::fs::set_permissions(&hook, std::fs::Permissions::from_mode(0o755))
                        .unwrap();
                }
            }
            _ => {
                let file = format!("{name}.txt");
                local.write(&file, "new\n").unwrap();
                local.git(["add", file.as_str()]).unwrap();
            }
        }
    }
    setup.add();
    setup
}

/// Shows "Uncommitted changes (n repos)" of `group`.
fn show(
    cx: &mut VisualTestContext,
    window: &Entity<GitcrocWindow>,
    setup: &Setup,
    group: &GroupId,
) {
    click(cx, tree_row(&setup.group_node(group)));
    wait_for_element(cx, "history-uncommitted".into());
    click(cx, "history-uncommitted".into());
    wait_for_view(cx, window);
    wait_for_element(cx, "commit-commit".into());
}

fn with_box<R>(
    cx: &mut VisualTestContext,
    window: &Entity<GitcrocWindow>,
    f: impl FnOnce(&CommitBox) -> R,
) -> R {
    cx.update(|_, cx| {
        let pane = window.read(cx).changes(cx).expect("no changes pane");
        f(pane.read(cx).commit_box().read(cx))
    })
}

fn draft(cx: &mut VisualTestContext, window: &Entity<GitcrocWindow>) -> CommitMessage {
    with_box(cx, window, |commit_box| commit_box.draft()).expect("no commit box")
}

fn subject(text: &str) -> CommitMessage {
    CommitMessage {
        subject: text.to_owned(),
        body: String::new(),
    }
}

fn type_in(cx: &mut VisualTestContext, id: &str, text: &str) {
    click(cx, ElementId::Name(id.to_owned().into()));
    cx.update(|window, cx| window.input(text, cx));
    cx.run_until_parked();
}

fn field(cx: &mut VisualTestContext, id: &str) -> Option<String> {
    cx.update(|window, cx| {
        window.render_frame(cx);
        window
            .try_find(ElementId::Name(id.to_owned().into()))
            .and_then(|found| found.value().map(str::to_owned))
    })
}

fn commit_label(cx: &mut VisualTestContext) -> Option<String> {
    label_of(cx, "commit-commit".into())
}

/// The subject of `HEAD` and how many commits lead to it.
fn log(setup: &Setup, name: &str) -> (String, usize) {
    let local = repo(&setup.made, name);
    let subject = local.git(["log", "-1", "--format=%s"]).unwrap();
    let count = local.git(["rev-list", "--count", "HEAD"]).unwrap();
    (subject.trim().to_owned(), count.trim().parse().unwrap())
}

/// Writes `message` and presses Commit; the preflight always asks
/// (GRP-15), and its Run starts the job, which this waits for.
fn commit(
    cx: &mut VisualTestContext,
    setup: &Setup,
    message: &str,
) -> Vec<gitcroc_core::RepoResult> {
    type_in(cx, "commit-subject", message);
    let mut events = setup.fixture.app().subscribe();
    click(cx, "commit-commit".into());
    wait_for_element(cx, "preflight".into());
    wait_preflight_ready(cx);
    click(cx, "preflight-run".into());
    let job = wait_group_end(cx, &mut events);
    assert!(job.label.starts_with("Commit"), "{}", job.label);
    job.report.unwrap()
}

fn outcome(report: &[gitcroc_core::RepoResult], name: &str) -> Outcome {
    report
        .iter()
        .find(|line| line.repo_name == name)
        .map(|line| line.outcome.clone())
        .unwrap_or_else(|| panic!("no line for {name}"))
}

#[gpui::test]
fn ui_34_uncommitted_row_sections_lists_by_repo_and_labels_commit_with_the_repo_count(
    cx: &mut TestAppContext,
) {
    let setup = setup(&FOUR);
    let group = setup.group("Backend", &FOUR);
    let (mut cx, window) = open(cx, &setup, &group);
    wait_for_element(&mut cx, "history-uncommitted".into());
    assert_eq!(
        label_of(&mut cx, "history-uncommitted".into()).as_deref(),
        Some("Uncommitted changes (4 repos)")
    );
    show(&mut cx, &window, &setup, &group);

    // The lists are sectioned by repo.
    let parts = with_changes(&mut cx, &window, |changes| {
        let mut names: Vec<String> = changes
            .rows()
            .iter()
            .filter_map(|row| match row {
                FileRow::Repo { name, .. } => Some(name.clone()),
                _ => None,
            })
            .collect();
        names.sort();
        names.dedup();
        names
    });
    assert_eq!(parts, ["api", "docs", "hooked", "web"]);

    // Three repos have something staged; Stage all adds docs.
    assert_eq!(commit_label(&mut cx).as_deref(), Some("Commit 3 repos"));
    click(&mut cx, "commit-stage-all".into());
    assert_eq!(commit_label(&mut cx).as_deref(), Some("Commit 4 repos"));
    click(&mut cx, "commit-stage-all".into());
    assert_eq!(commit_label(&mut cx).as_deref(), Some("Commit 3 repos"));
    // Disabled with no subject, and says why.
    assert!(crate::stage::disabled(&mut cx, "commit-commit".into()));
    assert_eq!(
        label_of(&mut cx, "commit-why".into()).as_deref(),
        Some(strings::changes::NO_SUBJECT)
    );
    type_in(&mut cx, "commit-subject", "Shared");
    assert!(!crate::stage::disabled(&mut cx, "commit-commit".into()));
}

#[gpui::test]
fn ui_34_no_amend_in_the_group_view(cx: &mut TestAppContext) {
    let setup = setup(&["api", "web"]);
    let group = setup.group("Backend", &["api", "web"]);
    let (mut cx, window) = open(cx, &setup, &group);
    show(&mut cx, &window, &setup, &group);
    assert!(present(&mut cx, "commit-commit".into()));
    assert!(!present(&mut cx, "commit-amend".into()));
    assert!(present(&mut cx, "commit-stage-all".into()));
    assert!(present(&mut cx, "commit-push".into()));
}

#[gpui::test]
fn grp_15_the_confirmation_lists_each_repo_with_its_staged_files(cx: &mut TestAppContext) {
    let setup = setup(&FOUR);
    let group = setup.group("Backend", &FOUR);
    let (mut cx, window) = open(cx, &setup, &group);
    show(&mut cx, &window, &setup, &group);
    let before: Vec<String> = FOUR
        .iter()
        .map(|name| head(&repo(&setup.made, name)))
        .collect();

    type_in(&mut cx, "commit-subject", "Shared");
    click(&mut cx, "commit-commit".into());
    wait_for_element(&mut cx, "preflight".into());
    wait_preflight_ready(&mut cx);
    assert_eq!(
        preflight_rows(&mut cx),
        [
            "docs: Will skip: nothing staged",
            "api: Will run: 1 staged file",
            "web: Will run: 1 staged file",
            "hooked: Will run: 1 staged file",
        ]
    );
    // Nothing is committed before the user confirms.
    click(&mut cx, "preflight-cancel".into());
    wait_until(&mut cx, |cx| !present(cx, "preflight".into()));
    let after: Vec<String> = FOUR
        .iter()
        .map(|name| head(&repo(&setup.made, name)))
        .collect();
    assert_eq!(after, before);
    // Cancelled, the box keeps the message and is not busy.
    assert_eq!(draft(&mut cx, &window), subject("Shared"));
    assert_eq!(commit_label(&mut cx).as_deref(), Some("Commit 3 repos"));
}

#[gpui::test]
fn grp_15_one_commit_per_repo_with_the_same_message(cx: &mut TestAppContext) {
    let setup = setup(&FOUR);
    let group = setup.group("Backend", &FOUR);
    let (mut cx, window) = open(cx, &setup, &group);
    show(&mut cx, &window, &setup, &group);
    let before: Vec<String> = FOUR
        .iter()
        .map(|name| head(&repo(&setup.made, name)))
        .collect();

    let report = commit(&mut cx, &setup, "Shared change");
    assert_eq!(outcome(&report, "api"), Outcome::Succeeded);
    assert_eq!(outcome(&report, "web"), Outcome::Succeeded);
    assert!(matches!(outcome(&report, "docs"), Outcome::Skipped { .. }));
    for name in ["api", "web"] {
        assert_eq!(log(&setup, name), ("Shared change".to_owned(), 2));
    }
    // Nothing staged: skipped, and nothing committed.
    assert_eq!(log(&setup, "docs"), ("Initial".to_owned(), 1));
    // Each commit stands alone on its own repo's last commit.
    let (api, web) = (repo(&setup.made, "api"), repo(&setup.made, "web"));
    let parents = |local: &repo_gen::Repo| {
        local
            .git(["rev-list", "--parents", "-n", "1", "HEAD"])
            .unwrap()
            .split_whitespace()
            .skip(1)
            .map(str::to_owned)
            .collect::<Vec<_>>()
    };
    assert_eq!(parents(&api), [before[0].clone()]);
    assert_eq!(parents(&web), [before[1].clone()]);
    assert_ne!(head(&api), head(&web));
}

#[gpui::test]
fn grp_6_a_hook_rejecting_one_commit_does_not_stop_the_others(cx: &mut TestAppContext) {
    let setup = setup(&FOUR);
    let group = setup.group("Backend", &FOUR);
    let (mut cx, window) = open(cx, &setup, &group);
    show(&mut cx, &window, &setup, &group);

    let report = commit(&mut cx, &setup, "Shared change");
    assert_eq!(outcome(&report, "hooked"), Outcome::Failed);
    let hooked = report
        .iter()
        .find(|line| line.repo_name == "hooked")
        .unwrap();
    assert!(hooked.output.contains("refused by the hook"), "{hooked:?}");
    assert_eq!(log(&setup, "hooked"), ("Initial".to_owned(), 1));
    assert_eq!(outcome(&report, "api"), Outcome::Succeeded);
    assert_eq!(outcome(&report, "web"), Outcome::Succeeded);
    // The box keeps the message and says a repo did not commit.
    wait_until(&mut cx, |cx| {
        label_of(cx, "commit-error".into()).as_deref()
            == Some(strings::changes::GROUP_COMMIT_FAILED)
    });
    assert_eq!(draft(&mut cx, &window), subject("Shared change"));
    assert_eq!(
        field(&mut cx, "commit-subject").as_deref(),
        Some("Shared change")
    );
}

#[gpui::test]
fn ui_34_the_draft_is_kept_per_target_set(cx: &mut TestAppContext) {
    let setup = setup(&FOUR);
    let backend = setup.group("Backend", &FOUR);
    let pair = setup.group("Pair", &["api", "web"]);
    let (mut cx, window) = open(cx, &setup, &backend);
    show(&mut cx, &window, &setup, &backend);
    type_in(&mut cx, "commit-subject", "For all four");
    click(&mut cx, "commit-stage-all".into());
    assert_eq!(commit_label(&mut cx).as_deref(), Some("Commit 4 repos"));

    // Another target set has its own message and options.
    show(&mut cx, &window, &setup, &pair);
    assert_eq!(draft(&mut cx, &window), CommitMessage::default());
    assert_eq!(field(&mut cx, "commit-subject").as_deref(), Some(""));
    assert_eq!(commit_label(&mut cx).as_deref(), Some("Commit 2 repos"));
    type_in(&mut cx, "commit-subject", "For two");

    // Back, each is as it was left.
    show(&mut cx, &window, &setup, &backend);
    assert_eq!(draft(&mut cx, &window), subject("For all four"));
    assert_eq!(
        field(&mut cx, "commit-subject").as_deref(),
        Some("For all four")
    );
    assert_eq!(commit_label(&mut cx).as_deref(), Some("Commit 4 repos"));
    show(&mut cx, &window, &setup, &pair);
    assert_eq!(draft(&mut cx, &window), subject("For two"));
}

#[gpui::test]
fn grp_15_push_after_commit_pushes_the_repos_that_committed(cx: &mut TestAppContext) {
    let names = ["api", "web", "docs"];
    let setup = setup(&names);
    let group = setup.group("Backend", &names);
    let (mut cx, window) = open(cx, &setup, &group);
    show(&mut cx, &window, &setup, &group);
    let docs_origin = head(&setup.origin("docs"));

    click(&mut cx, "commit-push".into());
    let report = commit(&mut cx, &setup, "Shared and pushed");
    assert_eq!(outcome(&report, "api"), Outcome::Succeeded);
    assert_eq!(outcome(&report, "web"), Outcome::Succeeded);
    for name in ["api", "web"] {
        assert_eq!(
            head(&setup.origin(name)),
            head(&repo(&setup.made, name)),
            "{name} is pushed"
        );
    }
    // Skipped, docs commits nothing and pushes nothing.
    assert_eq!(head(&setup.origin("docs")), docs_origin);
    // Every repo that ran committed: the box is cleared.
    wait_until(&mut cx, |cx| draft(cx, &window) == CommitMessage::default());
    assert!(!present(&mut cx, "commit-error".into()));
}

#[gpui::test]
fn sec_3_the_message_and_repo_names_render_as_text(cx: &mut TestAppContext) {
    // Markup that a folder name can hold on every platform: no < or >.
    let name = "&lt;i&gt;&amp;";
    let setup = setup(&["api", name]);
    let group = setup.group("Backend", &["api", name]);
    let (mut cx, window) = open(cx, &setup, &group);
    show(&mut cx, &window, &setup, &group);
    let message = "<b>bold</b> &amp;";
    type_in(&mut cx, "commit-subject", message);
    assert_eq!(field(&mut cx, "commit-subject").as_deref(), Some(message));
    click(&mut cx, "commit-commit".into());
    wait_for_element(&mut cx, "preflight".into());
    wait_preflight_ready(&mut cx);
    let rows = preflight_rows(&mut cx);
    assert_eq!(rows[1], format!("{name}: Will run: 1 staged file"));
    let mut events = setup.fixture.app().subscribe();
    click(&mut cx, "preflight-run".into());
    wait_group_end(&mut cx, &mut events);
    assert_eq!(log(&setup, name), (message.to_owned(), 2));
}
