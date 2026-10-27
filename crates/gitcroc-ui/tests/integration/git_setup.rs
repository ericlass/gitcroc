//! PLAT-3: the setup screen while no usable git is found, and the way past
//! it without a restart. Detection itself is the core's, tested there; the
//! gits here are fake scripts chosen in the settings file.

use gitcroc_core::{GitStatus, SETTINGS_SCHEMA};
use gitcroc_ui::git_setup::GitSetup;
use gitcroc_ui::strings::{self, git_setup};
use gitcroc_ui::window::GitcrocWindow;
use gpui_kit::test::TestWindowExt;
use gpui_kit::{AppContext, ElementId, Entity, Role, TestAppContext, VisualTestContext};

use crate::support::{
    FakeGit, Fixture, Options, main_window, open, opened_main_window, settings_with_git, settle,
};

fn label(cx: &mut VisualTestContext, id: impl Into<ElementId>) -> Option<String> {
    let id = id.into();
    cx.update(|window, _| {
        window
            .try_find(id)
            .and_then(|found| found.label().map(str::to_owned))
    })
}

fn role(cx: &mut VisualTestContext, id: impl Into<ElementId>) -> Option<Role> {
    let id = id.into();
    cx.update(|window, _| window.find(id).role())
}

fn shown(cx: &mut VisualTestContext, id: impl Into<ElementId>) -> bool {
    let id = id.into();
    cx.update(|window, _| window.try_find(id).is_some())
}

fn disabled(cx: &mut VisualTestContext, id: &'static str) -> bool {
    cx.update(|window, _| window.find(id).disabled() == Some(true))
}

fn click(cx: &mut VisualTestContext, id: &'static str) {
    cx.update(|window, cx| window.click(id, cx));
}

/// An app whose settings choose `git`, and the main window on it once it
/// shows the setup screen.
fn setup_screen(
    cx: &mut TestAppContext,
    git: &FakeGit,
    options: Options,
) -> (Fixture, VisualTestContext, Entity<GitcrocWindow>) {
    let fixture = Fixture::start_with(Options {
        settings: Some(settings_with_git(git.path(), SETTINGS_SCHEMA)),
        ..options
    });
    let (mut cx, window) = main_window(cx, &fixture);
    assert!(shown(&mut cx, "git-setup"), "the setup screen is not shown");
    (fixture, cx, window)
}

/// The setup screen alone, over `problem`, on `os`.
fn screen(cx: &mut TestAppContext, problem: GitStatus, os: &str) -> (Fixture, VisualTestContext) {
    let fixture = Fixture::start(gitcroc_app::app::EVENT_CAPACITY);
    let services = fixture.services.clone();
    let os = os.to_owned();
    let (cx, _) = open(cx, move |_, cx| {
        cx.new(|cx| GitSetup::new(services, problem, os, cx))
    });
    (fixture, cx)
}

#[gpui::test]
fn plat_3_looking_for_git_until_the_first_result(cx: &mut TestAppContext) {
    let git = FakeGit::blocked("2.47.0");
    let fixture = Fixture::start_with(Options {
        settings: Some(settings_with_git(git.path(), SETTINGS_SCHEMA)),
        wait_for_git: false,
        ..Options::default()
    });
    let (mut cx, _) = opened_main_window(cx, &fixture);

    assert_eq!(
        label(&mut cx, "looking-for-git").as_deref(),
        Some(git_setup::LOOKING_FOR_GIT)
    );
    assert!(!shown(&mut cx, "toolbar"));
    assert!(!shown(&mut cx, "git-setup"));
    assert!(shown(&mut cx, "title-region"));

    git.release();
    settle(&mut cx, |window, _| window.try_find("toolbar").is_some());
    assert!(!shown(&mut cx, "looking-for-git"));
}

#[gpui::test]
fn plat_3_too_old_shows_both_versions_and_check_again_opens_the_app(cx: &mut TestAppContext) {
    let git = FakeGit::new("2.30.0");
    let (fixture, mut cx, _) = setup_screen(cx, &git, Options::default());
    let path = git.path().display().to_string();

    assert_eq!(
        label(&mut cx, "git-setup-heading").as_deref(),
        Some(git_setup::TITLE)
    );
    assert_eq!(role(&mut cx, "git-setup-heading"), Some(Role::Heading));
    assert_eq!(
        label(&mut cx, "git-setup-problem"),
        Some(git_setup::too_old("2.30.0", &path, "2.40.0"))
    );
    assert_eq!(role(&mut cx, "git-setup-problem"), Some(Role::Alert));
    assert!(!shown(&mut cx, "toolbar"));
    assert!(!shown(&mut cx, "git-setup-chosen-missing"));
    // The git was chosen, so it can be given up.
    assert!(shown(&mut cx, "git-setup-automatic"));

    // "Check again" after installing a newer git shows the app, no restart.
    git.set_version("2.47.0");
    click(&mut cx, "git-setup-check-again");
    settle(&mut cx, |window, _| window.try_find("toolbar").is_some());
    assert!(!shown(&mut cx, "git-setup"));
    drop(fixture);
}

#[gpui::test]
fn plat_3_not_found_lists_the_searched_paths_and_an_install_hint(cx: &mut TestAppContext) {
    let searched = [
        r"C:\Program Files\Git\cmd\git.exe",
        r"C:\Users\me\AppData\Local\Programs\Git\cmd\git.exe",
    ];
    let problem = GitStatus::NotFound {
        searched: searched.map(str::to_owned).to_vec(),
        chosen: None,
        chosen_missing: false,
    };
    let (_fixture, mut cx) = screen(cx, problem, "windows");

    assert_eq!(
        label(&mut cx, "git-setup-problem").as_deref(),
        Some(git_setup::NOT_FOUND)
    );
    assert_eq!(role(&mut cx, "git-setup-problem"), Some(Role::Alert));
    assert_eq!(
        label(&mut cx, "git-setup-searched").as_deref(),
        Some(git_setup::LOOKED_AT)
    );
    for (at, path) in searched.into_iter().enumerate() {
        assert_eq!(
            label(&mut cx, ("git-setup-path", at)).as_deref(),
            Some(path)
        );
        assert_eq!(role(&mut cx, ("git-setup-path", at)), Some(Role::ListItem));
    }
    assert!(!shown(&mut cx, ("git-setup-path", searched.len())));
    let hint = label(&mut cx, "git-setup-hint").unwrap();
    assert!(hint.contains("winget install --id Git.Git -e"), "{hint}");
    // Nothing was chosen.
    assert!(!shown(&mut cx, "git-setup-automatic"));
    assert!(!shown(&mut cx, "git-setup-chosen-missing"));
    assert!(shown(&mut cx, "git-setup-check-again"));
    assert!(shown(&mut cx, "git-setup-choose"));
}

#[gpui::test]
fn plat_3_not_found_with_nowhere_to_look(cx: &mut TestAppContext) {
    let problem = GitStatus::NotFound {
        searched: Vec::new(),
        chosen: None,
        chosen_missing: false,
    };
    let (_fixture, mut cx) = screen(cx, problem, "haiku");

    assert_eq!(
        label(&mut cx, "git-setup-searched").as_deref(),
        Some(git_setup::NOWHERE_TO_LOOK)
    );
    assert!(!shown(&mut cx, ("git-setup-path", 0_usize)));
    assert_eq!(
        label(&mut cx, "git-setup-hint").as_deref(),
        Some(git_setup::DEFAULT_HINT)
    );
}

#[gpui::test]
fn plat_3_choose_git_gets_past_the_setup_screen(cx: &mut TestAppContext) {
    let old = FakeGit::new("2.30.0");
    let new = FakeGit::new("2.51.0");
    let (fixture, mut cx, _) = setup_screen(
        cx,
        &old,
        Options {
            picked: Some(new.path().to_owned()),
            ..Options::default()
        },
    );

    assert_eq!(
        label(&mut cx, "git-setup-choose").as_deref(),
        Some(git_setup::CHOOSE_GIT)
    );
    click(&mut cx, "git-setup-choose");
    settle(&mut cx, |window, _| window.try_find("toolbar").is_some());
    let saved = fixture.settings_file();
    assert!(saved.contains("git_path"), "{saved}");
    assert!(
        saved.contains(&*new.path().to_string_lossy().replace('\\', "\\\\")),
        "{saved}"
    );
}

#[gpui::test]
fn plat_3_a_missing_chosen_git_is_named_and_can_be_given_up(cx: &mut TestAppContext) {
    let problem = GitStatus::NotFound {
        searched: vec!["/opt/gone/git".to_owned(), "/usr/bin/git".to_owned()],
        chosen: Some("/opt/gone/git".to_owned()),
        chosen_missing: true,
    };
    let (screen_fixture, mut screen_cx) = screen(cx, problem, "linux");
    assert_eq!(
        label(&mut screen_cx, "git-setup-chosen-missing"),
        Some(git_setup::chosen_missing("/opt/gone/git"))
    );
    assert!(shown(&mut screen_cx, "git-setup-automatic"));
    drop((screen_cx, screen_fixture));

    // Given up, detection finds the git on PATH and the app opens.
    let old = FakeGit::new("2.30.0");
    let (fixture, mut cx, _) = setup_screen(cx, &old, Options::default());
    assert_eq!(
        label(&mut cx, "git-setup-automatic").as_deref(),
        Some(git_setup::USE_AUTOMATIC)
    );
    click(&mut cx, "git-setup-automatic");
    settle(&mut cx, |window, _| window.try_find("toolbar").is_some());
    let saved = fixture.settings_file();
    assert!(!saved.contains("git_path"), "{saved}");
}

#[gpui::test]
fn plat_3_buttons_are_disabled_while_checking(cx: &mut TestAppContext) {
    let git = FakeGit::new("2.30.0");
    let (_fixture, mut cx, _) = setup_screen(cx, &git, Options::default());
    let buttons = [
        "git-setup-check-again",
        "git-setup-choose",
        "git-setup-automatic",
    ];
    assert_eq!(
        label(&mut cx, "git-setup-check-again").as_deref(),
        Some(git_setup::CHECK_AGAIN)
    );
    assert!(buttons.iter().all(|button| !disabled(&mut cx, button)));

    git.block();
    click(&mut cx, "git-setup-check-again");
    settle(&mut cx, |window, _| {
        window.find("git-setup-check-again").disabled() == Some(true)
    });
    assert!(buttons.iter().all(|button| disabled(&mut cx, button)));
    assert_eq!(
        label(&mut cx, "git-setup-check-again").as_deref(),
        Some(git_setup::CHECKING)
    );
    assert!(shown(&mut cx, "git-setup-problem"));

    // Still too old: the screen stays, ready again.
    git.release();
    settle(&mut cx, |window, _| {
        window.find("git-setup-check-again").disabled() != Some(true)
    });
    assert!(buttons.iter().all(|button| !disabled(&mut cx, button)));
    assert_eq!(
        label(&mut cx, "git-setup-check-again").as_deref(),
        Some(git_setup::CHECK_AGAIN)
    );
}

#[gpui::test]
fn plat_3_read_only_settings_refuse_automatic_detection(cx: &mut TestAppContext) {
    let git = FakeGit::new("2.30.0");
    // A newer schema is read, but never written (DATA-1).
    let fixture = Fixture::start_with(Options {
        settings: Some(settings_with_git(git.path(), SETTINGS_SCHEMA + 1)),
        ..Options::default()
    });
    let before = fixture.settings_file();
    let (mut cx, _) = main_window(cx, &fixture);
    assert!(!shown(&mut cx, "git-setup-error"));

    click(&mut cx, "git-setup-automatic");
    settle(&mut cx, |window, _| {
        window.try_find("git-setup-error").is_some()
    });
    assert_eq!(role(&mut cx, "git-setup-error"), Some(Role::Alert));
    let error = label(&mut cx, "git-setup-error").unwrap();
    assert!(error.contains("read-only"), "{error}");
    assert!(shown(&mut cx, "git-setup"));
    assert_eq!(fixture.settings_file(), before);
}

#[gpui::test]
fn plat_3_setup_screen_regions_and_focus(cx: &mut TestAppContext) {
    let git = FakeGit::new("2.30.0");
    let (_fixture, mut cx, window) = setup_screen(cx, &git, Options::default());
    let focused = |cx: &mut VisualTestContext, id: &'static str| {
        cx.update(|window, cx| {
            window.render_frame(cx);
            window.find(id).focused() == Some(true)
        })
    };

    // The first button has the focus when the screen appears.
    assert!(focused(&mut cx, "git-setup-check-again"));
    cx.update(|window, cx| window.press("f6", cx));
    assert!(focused(&mut cx, "title-region"));
    cx.update(|window, cx| window.press("f6", cx));
    assert!(focused(&mut cx, "git-setup"));
    cx.update(|window, cx| window.press("shift-f6", cx));
    assert!(focused(&mut cx, "title-region"));
    // The title bar shows the app's name, not a workspace's (UI-35).
    let title = cx.update(|_, cx| {
        let title_bar = window.read(cx).title_bar().clone();
        title_bar.read(cx).title().to_string()
    });
    assert_eq!(title, strings::APP_NAME);
}

#[gpui::test]
fn sec_3_git_path_and_output_render_as_text(cx: &mut TestAppContext) {
    let path = "/opt/<b>evil</b>/git";
    let output = "<img src=x onerror=alert(1)>\nsecond line";
    let problem = GitStatus::Unparseable {
        path: path.to_owned(),
        output: output.to_owned(),
        chosen: None,
        chosen_missing: false,
    };
    let (_fixture, mut cx) = screen(cx, problem, "linux");

    assert_eq!(
        label(&mut cx, "git-setup-problem"),
        Some(git_setup::unparseable(path))
    );
    assert_eq!(label(&mut cx, "git-setup-output").as_deref(), Some(output));
}
