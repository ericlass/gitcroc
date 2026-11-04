//! M5-10's views, each reached in the real window by the app's own
//! controls and keys, and each tree checked as it is reached. The repos
//! are real ones made by `repo-gen` and added through the app (QA-2): api
//! with an origin and uncommitted changes, web whose origin is gone so
//! that its fetch fails, and docs in a merge conflict. Backend groups api
//! and web; Docs groups api and docs, whose missing origin makes a group
//! fetch ask first.

use std::cell::RefCell;
use std::collections::HashMap;
use std::path::PathBuf;
use std::process::ExitCode;
use std::rc::Rc;
use std::time::Duration;

use gitcroc_ai::ProviderReply;
use gitcroc_core::{GroupId, ProviderDraft, ProviderKind, RepoId, SETTINGS_SCHEMA};
use gitcroc_ui::icons::Icons;
use gitcroc_ui::services::Services;
use gitcroc_ui::title_bar::{Platform, WindowButton};
use gitcroc_ui::window::GitcrocWindow;
use gpui_kit::{App, AppContext, AsyncApp};
use serde_json::Value;

use crate::atspi::{self, Announcements};
use crate::checker::{self, Gap};
use crate::driver::Driver;
use crate::support::{
    AiBackend, AiScript, FakeControls, FakeGit, Fixture, Options, settings_with_git,
};

/// Elements allowed to break one rule, each with its reason.
const GAPS: &[Gap] = &[];

/// Where each view's tree is written, for a failure's reader.
fn out_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("a11y_tree")
}

/// The repos, groups and app every view is reached in.
struct Scene {
    fixture: Fixture,
    _made: repo_gen::Fixture,
    _remotes: tempfile::TempDir,
    repos: HashMap<&'static str, RepoId>,
    backend: GroupId,
    docs: GroupId,
}

/// 120 lines whose fifth says `middle`, so a conflict has one hunk.
fn lines(middle: &str) -> String {
    (1..=120)
        .map(|n| {
            if n == 5 {
                format!("{middle}\n")
            } else {
                format!("line {n}\n")
            }
        })
        .collect()
}

fn scene() -> Scene {
    // The proposal resolves the conflict's one hunk.
    let reply = serde_json::json!({ "hunks": [{
        "id": 1, "status": "resolved", "resolution": "both\n",
        "explanation": "Kept both sides.", "confidence": "high",
    }] });
    let fixture = Fixture::start_with(Options {
        ai: AiBackend::Script(AiScript {
            mock: true,
            replies: vec![ProviderReply {
                text: reply.to_string(),
                usage: None,
            }],
            ..AiScript::default()
        }),
        ..Options::default()
    });
    let made = repo_gen::Fixture::new().unwrap();
    let remotes = tempfile::tempdir().unwrap();
    for name in ["api", "web", "docs"] {
        let repo = made.repo(name).unwrap();
        repo.git(["config", "core.autocrlf", "false"]).unwrap();
        repo.write("a.txt", lines("base")).unwrap();
        repo.commit_all("Initial").unwrap();
    }
    let repo = |name: &str| repo_gen::Repo::open(made.path().join(name));
    // api: an origin it is up to date with, and uncommitted changes.
    let origin = remotes.path().join("api.git");
    std::fs::create_dir_all(&origin).unwrap();
    repo_gen::Repo::open(&origin)
        .git(["init", "--bare", "--quiet", "--initial-branch=main"])
        .unwrap();
    let api = repo("api");
    api.git(["remote", "add", "origin", origin.to_str().unwrap()])
        .unwrap();
    api.git(["push", "--quiet", "-u", "origin", "main"])
        .unwrap();
    api.write("a.txt", lines("edited")).unwrap();
    api.write("notes.txt", "new\n").unwrap();
    // web: an origin that is gone, so a fetch fails.
    let gone = remotes.path().join("web-gone.git");
    repo("web")
        .git(["remote", "add", "origin", gone.to_str().unwrap()])
        .unwrap();
    // docs: no origin, and a merge conflict in a.txt.
    let docs = repo("docs");
    docs.branch("topic").unwrap();
    docs.write("a.txt", lines("main")).unwrap();
    docs.commit_all("Main edit").unwrap();
    docs.switch("topic").unwrap();
    docs.write("a.txt", lines("topic")).unwrap();
    docs.commit_all("Topic edit").unwrap();
    docs.switch("main").unwrap();
    docs.merge_with_conflicts("topic").unwrap();

    let added = fixture.add_repos(&fixture.active(), made.path());
    let repos: HashMap<&'static str, RepoId> = ["api", "web", "docs"]
        .into_iter()
        .map(|name| {
            let id = added
                .iter()
                .find(|(added, _)| added == name)
                .unwrap()
                .1
                .clone();
            (name, id)
        })
        .collect();
    let group = |name: &str, members: [&str; 2]| {
        let app = fixture.app().clone();
        let members = members.iter().map(|name| repos[name].clone()).collect();
        fixture
            .runtime
            .block_on(app.group_create(fixture.active(), name.to_owned(), members))
            .unwrap()
    };
    let backend = group("Backend", ["api", "web"]);
    let docs_group = group("Docs", ["api", "docs"]);
    mock_provider(&fixture);
    Scene {
        fixture,
        _made: made,
        _remotes: remotes,
        repos,
        backend,
        docs: docs_group,
    }
}

/// The mock provider as the default, consented to.
fn mock_provider(fixture: &Fixture) {
    let app = fixture.app().clone();
    let draft = ProviderDraft {
        id: None,
        name: "Mock".to_owned(),
        kind: ProviderKind::Mock,
        model: None,
        base_url: None,
        path: None,
        context_budget: None,
    };
    fixture.runtime.block_on(async move {
        let view = app.ai_provider_save(draft, None).await.unwrap();
        let id = view.providers.last().unwrap().id.clone();
        app.ai_provider_set_default(id.clone()).await.unwrap();
        app.ai_provider_consent(id, "mock".to_owned())
            .await
            .unwrap();
    });
}

/// An app whose chosen git is too old, for the setup screen (PLAT-3).
fn too_old_git() -> (Fixture, FakeGit) {
    let git = FakeGit::new("2.30.0");
    let fixture = Fixture::start_with(Options {
        settings: Some(settings_with_git(git.path(), SETTINGS_SCHEMA)),
        ..Options::default()
    });
    (fixture, git)
}

pub fn run() -> ExitCode {
    if let Err(error) = atspi::enable() {
        eprintln!("a11y_tree: {error}");
        return ExitCode::FAILURE;
    }
    let scene = scene();
    let (setup, _git) = too_old_git();
    let services = scene.fixture.services.clone();
    let setup_services = setup.services.clone();
    let groups = (
        scene.backend.clone(),
        scene.docs.clone(),
        scene.repos.clone(),
    );
    let failed: Rc<RefCell<Option<String>>> = Rc::default();
    let result = failed.clone();
    gpui_kit::application().with_assets(Icons).run(move |cx| {
        gitcroc_ui::init(cx);
        cx.spawn(async move |cx| {
            let outcome = every_view(services, setup_services, groups, cx).await;
            *result.borrow_mut() = outcome.err();
            cx.update(|cx| cx.quit());
        })
        .detach();
    });
    drop(scene);
    drop(setup);
    for gap in GAPS {
        println!(
            "a11y_tree: allowed: {} may break {:?}: {}",
            gap.element, gap.rule, gap.reason
        );
    }
    match failed.borrow().as_ref() {
        None => {
            println!(
                "a11y_tree: every view passed; trees in {}",
                out_dir().display()
            );
            ExitCode::SUCCESS
        }
        Some(error) => {
            eprintln!("a11y_tree: {error}\n(trees in {})", out_dir().display());
            ExitCode::FAILURE
        }
    }
}

/// Opens the main window on `services`. Under Xvfb, with no compositor,
/// the window cannot draw its own frame, so its title bar would draw no
/// window buttons (UI-35). These controls say it draws its frame, as it
/// does on a desktop, so the buttons are drawn and checked.
fn open(services: Services, cx: &mut AsyncApp) -> Result<Driver, String> {
    let opened = cx.update(|cx: &mut App| {
        let options = gitcroc_ui::window_options(false, cx);
        gpui_kit::open_window(options, cx, move |window, cx| {
            cx.new(|cx| {
                GitcrocWindow::new(
                    services,
                    Rc::new(FakeControls::default()),
                    Platform::Linux,
                    false,
                    window,
                    cx,
                )
            })
        })
    });
    let (handle, _) = opened.map_err(|error| format!("cannot open the window: {error:#}"))?;
    Ok(Driver {
        handle,
        cx: cx.clone(),
    })
}

async fn every_view(
    services: Services,
    setup: Services,
    (backend, docs, repos): (GroupId, GroupId, HashMap<&'static str, RepoId>),
    cx: &mut AsyncApp,
) -> Result<(), String> {
    let announcements = Announcements::listen()?;
    let app = services.app().clone();
    let mut driver = open(services, cx)?;
    let d = &mut driver;
    d.shown("toolbar").await?;

    // The main window: a group in the tree, api's uncommitted changes.
    let api = format!("tree-row-group:{}:repo:{}", backend.0, repos["api"].0);
    d.click(&api).await?;
    d.click("history-uncommitted").await?;
    d.click_labelled("changes-row-", "notes.txt").await?;
    d.shown("diff-lines").await?;
    let main = check(d, "main").await?;
    report(
        "main: lists are list boxes and the repos a tree",
        checker::roles(
            &main,
            &[
                ("tree-list", "Tree"),
                ("history-list", "ListBox"),
                ("changes-files", "ListBox"),
                ("live-status", "Status"),
            ],
        ),
    )?;
    report(
        "ui_22_window_buttons_are_named_and_not_tab_stops",
        checker::window_buttons(
            &main,
            &[
                WindowButton::Minimize.id(),
                WindowButton::Maximize.id(),
                WindowButton::Close.id(),
            ],
        ),
    )?;

    // The commit message reads back by character: its text and the caret
    // (M6-25c).
    d.click("commit-subject").await?;
    d.input("ab").await?;
    d.press("left").await?;
    d.press("left").await?;
    let subject = gitcroc_ui::strings::changes::SUBJECT;
    let connection = atspi::connect()?;
    let mut field = None;
    // AT-SPI hears of the edit after the frame that drew it.
    for _ in 0..50 {
        field = atspi::text_field(&connection, subject)?;
        if field.as_ref().is_some_and(|field| field.caret == 0) {
            break;
        }
        d.frames(2).await?;
    }
    let expected = atspi::TextField {
        text: "ab".to_owned(),
        caret: 0,
    };
    report(
        "ui_22_the_commit_message_exposes_its_text_and_caret",
        if field.as_ref() == Some(&expected) {
            Vec::new()
        } else {
            vec![format!("expected {expected:?}; AT-SPI has {field:?}")]
        },
    )?;
    d.press("ctrl-a").await?;
    d.press("backspace").await?;

    // A destructive confirmation: discarding a file.
    d.click_labelled("changes-row-", "a.txt").await?;
    let row = d
        .labelled_id("changes-row-", "a.txt")?
        .ok_or("api's a.txt row is gone")?;
    d.click(&format!("{row}-discard")).await?;
    d.shown("discard-dialog").await?;
    check_dialog(d, "discard").await?;
    d.press("escape").await?;

    // The command palette.
    d.press("ctrl-k").await?;
    d.shown("palette").await?;
    let palette = check_dialog(d, "palette").await?;
    report(
        "palette: its results are a list box",
        checker::roles(&palette, &[("palette-list", "ListBox")]),
    )?;
    d.press("escape").await?;

    // Settings, each of its tabs (M6-26 adds its own).
    d.click("toolbar-settings").await?;
    d.shown("settings-dialog").await?;
    for tab in ["settings-tab-ai"] {
        d.click(tab).await?;
        check_dialog(d, &tab.replace("settings-tab-", "settings-")).await?;
    }
    d.press("escape").await?;

    // The preflight dialog: docs has no origin, so a fetch of Docs asks.
    d.click(&format!("tree-row-group:{}", docs.0)).await?;
    d.click("toolbar-0").await?;
    d.shown("preflight").await?;
    d.until("the preflight's repos", |window, _| {
        use gpui_kit::test::TestWindowExt;
        window.try_find("preflight-repos").is_some() && window.try_find("preflight-run").is_some()
    })
    .await?;
    check_dialog(d, "preflight").await?;
    d.press("escape").await?;

    // The job list with a result report: a fetch of Backend, where web's
    // origin is gone, opens its report by itself (UI-21), and the failure
    // is announced once, its progress never (M5-10).
    d.click(&format!("tree-row-group:{}", backend.0)).await?;
    // Nothing was said so far: the live status starts empty, and no other
    // node is live (M6-04 finding 4).
    let before = announcements.all();
    report(
        "the live status starts empty and is the only live node",
        before
            .iter()
            .map(|(text, _)| format!("heard {text:?} before any job ended"))
            .collect(),
    )?;
    let before = before.len();
    d.click("toolbar-0").await?;
    d.shown("job-list").await?;
    let jobs = check_dialog(d, "job-report").await?;
    report(
        "job-report: the jobs are a list box",
        checker::roles(&jobs, &[("job-rows", "ListBox")]),
    )?;
    // AccessKit sends an announcement after the frame that changed the
    // name; give it time to arrive.
    d.frames(25).await?;
    let heard: Vec<String> = announcements.all()[before..]
        .iter()
        .map(|(text, politeness)| format!("{text} (politeness {politeness})"))
        .collect();
    let fetch = app
        .jobs_get()
        .into_iter()
        .rfind(|job| job.report.is_some())
        .ok_or("Backend's fetch is not in the job list")?;
    let failed = gitcroc_ui::strings::jobs::announced(
        &fetch.label,
        &fetch.workspace_name,
        &gitcroc_ui::strings::jobs::outcome(&gitcroc_core::Outcome::Failed),
    );
    report(
        "ui_21_a_failed_job_is_announced_once_and_progress_is_not",
        if heard == [format!("{failed} (politeness 1)")] {
            Vec::new()
        } else {
            vec![format!(
                "expected only \"{failed}\", politely; heard {heard:?}"
            )]
        },
    )?;
    d.press("escape").await?;

    // The conflict editor with an AI proposal.
    d.click(&format!(
        "tree-row-group:{}:repo:{}",
        docs.0, repos["docs"].0
    ))
    .await?;
    d.click("history-uncommitted").await?;
    d.click_labelled("changes-row-", "a.txt").await?;
    d.until("Resolve with AI", |window, _| {
        use gpui_kit::test::TestWindowExt;
        window
            .try_find("conflict-resolve-ai")
            .is_some_and(|found| found.disabled() != Some(true))
    })
    .await?;
    d.click("conflict-resolve-ai").await?;
    d.shown("conflict-proposal-0").await?;
    check(d, "conflict-ai").await?;

    // The empty state, with no tab open (UI-27).
    d.press("ctrl-k").await?;
    d.shown("palette").await?;
    d.input("Close tab").await?;
    d.frames(10).await?;
    d.press("enter").await?;
    d.shown("workspace-start").await?;
    check(d, "empty").await?;

    // The git setup screen (PLAT-3), in a window of its own app.
    let mut setup = open(setup, cx)?;
    setup.shown("git-setup").await?;
    check(&mut setup, "git-setup").await?;
    Ok(())
}

/// Prints how a named test went; fails the run when it found problems.
fn report(test: &str, problems: Vec<String>) -> Result<(), String> {
    if problems.is_empty() {
        println!("a11y_tree: {test} ... ok");
        return Ok(());
    }
    Err(format!("{test} failed:\n  {}", problems.join("\n  ")))
}

/// Writes the tree of view `name` and checks it.
async fn check(driver: &mut Driver, name: &str) -> Result<Value, String> {
    let tree = driver.tree().await?;
    write(name, &tree);
    let problems = checker::check(&tree, GAPS);
    if problems.is_empty() {
        println!("a11y_tree: {name} ... ok");
        return Ok(tree);
    }
    let list: Vec<String> = problems.iter().map(ToString::to_string).collect();
    Err(format!("view {name}:\n  {}", list.join("\n  ")))
}

/// [`check`], and every dialog on screen is modal, as AT-SPI exposes it:
/// the tree dump does not hold the modal state.
async fn check_dialog(driver: &mut Driver, name: &str) -> Result<Value, String> {
    let tree = check(driver, name).await?;
    let connection = atspi::connect()?;
    let mut dialogs = Vec::new();
    // AT-SPI hears of the dialog after the frame that drew it.
    for _ in 0..50 {
        dialogs = atspi::dialogs(&connection)?;
        if !dialogs.is_empty() {
            break;
        }
        driver
            .cx
            .background_executor()
            .timer(Duration::from_millis(50))
            .await;
    }
    let problems: Vec<String> = if dialogs.is_empty() {
        vec!["AT-SPI shows no dialog".to_owned()]
    } else {
        dialogs
            .iter()
            .filter(|dialog| !dialog.modal)
            .map(|dialog| format!("dialog {:?} is not modal", dialog.name))
            .collect()
    };
    report(&format!("{name}: dialogs are modal"), problems)?;
    Ok(tree)
}

fn write(name: &str, tree: &Value) {
    let dir = out_dir();
    std::fs::create_dir_all(&dir).unwrap();
    let text = serde_json::to_string_pretty(tree).unwrap();
    std::fs::write(dir.join(format!("{name}.json")), text).unwrap();
}
