//! M6-26: the Settings dialog's Appearance, Git and Updates tabs (UI-23,
//! UI-24, UI-26, UI-35, PLAT-3, D12, section 5), the read-only warning
//! (DATA-1), and the palette's Check for updates; the port of rc.1's
//! `theme.test.tsx`, `TitleBar.test.tsx`, `GitSetup.test.tsx`,
//! `update.test.tsx` and `groupOps.test.tsx` settings tests. Against the
//! real core and its settings file in a temporary folder; no test waits on
//! time.

use std::path::Path;
use std::rc::Rc;
use std::sync::Arc;

use gitcroc_app::update::UpdateTest;
use gitcroc_core::{
    CodeTheme, GitStatus, Package, SETTINGS_SCHEMA, UpdateFound, UpdateFuture, UpdateProgress,
    UpdateSource,
};
use gitcroc_ui::settings_dialog::{SettingsDialog, SettingsTab};
use gitcroc_ui::strings;
use gitcroc_ui::theme::AppTheme;
use gitcroc_ui::theme::code_themes::CODE_THEMES;
use gitcroc_ui::title_bar::{SystemControls, WindowControls};
use gitcroc_ui::window::GitcrocWindow;
use gpui_kit::test::{TestQueryExt, TestWindowExt};
use gpui_kit::{ElementId, Entity, TestAppContext, VisualTestContext};

use crate::history::{label_of, press, wait_for_element, wait_until};
use crate::support::{
    Asked, FakeControls, Fixture, Options, main_window, opened_main_window_with, settings_with_git,
    settle,
};

const VERSION: &str = "9.9.9";

fn named(id: &str, at: usize) -> ElementId {
    ElementId::NamedInteger(id.to_owned().into(), at as u64)
}

fn secondary(key: &str) -> String {
    if cfg!(target_os = "macos") {
        format!("cmd-{key}")
    } else {
        format!("ctrl-{key}")
    }
}

/// Opens Settings from the toolbar; it opens at Appearance.
fn open_settings(cx: &mut VisualTestContext) {
    wait_for_element(cx, "toolbar-settings".into());
    click(cx, "toolbar-settings".into());
    wait_for_element(cx, "settings-dialog".into());
    wait_for_element(cx, "settings-appearance".into());
}

fn show_tab(cx: &mut VisualTestContext, tab: SettingsTab) {
    click(cx, tab.id().into());
}

/// Replaces the text of field `id` with `text` and presses Enter.
fn type_into(cx: &mut VisualTestContext, id: &'static str, text: &str) {
    click(cx, id.into());
    press(cx, &secondary("a"));
    if text.is_empty() {
        press(cx, "backspace");
    } else {
        cx.update(|window, cx| window.input(text, cx));
    }
    cx.run_until_parked();
    press(cx, "enter");
}

fn value(cx: &mut VisualTestContext, id: &'static str) -> Option<String> {
    cx.update(|window, cx| {
        window.render_frame(cx);
        window
            .try_find(id)
            .and_then(|found| found.value().map(str::to_owned))
    })
}

fn checked(cx: &mut VisualTestContext, id: &'static str) -> Option<bool> {
    cx.update(|window, cx| {
        window.render_frame(cx);
        window.try_find(id).and_then(|found| found.checked())
    })
}

fn chosen(cx: &mut VisualTestContext, id: ElementId) -> bool {
    cx.update(|window, cx| {
        window.render_frame(cx);
        window
            .try_find(id)
            .is_some_and(|found| found.selected() == Some(true) || found.checked() == Some(true))
    })
}

fn shown(cx: &mut VisualTestContext, id: impl Into<ElementId>) -> bool {
    let id = id.into();
    cx.update(|window, cx| {
        window.render_frame(cx);
        window.try_find(id).is_some()
    })
}

/// Whether `id`, or for a text field the editor in it, is disabled.
fn disabled(cx: &mut VisualTestContext, id: impl Into<ElementId>) -> bool {
    let id = id.into();
    cx.update(|window, cx| {
        window.render_frame(cx);
        window
            .elements()
            .iter()
            .filter(|found| found.path().contains(&id))
            .any(|found| found.disabled() == Some(true))
    })
}

/// The settings file's text, empty until it is written.
trait Saved {
    fn saved(&self) -> String;
}

impl Saved for Fixture {
    fn saved(&self) -> String {
        self.settings_file_if_any().unwrap_or_default()
    }
}

/// Scrolls the settings panel until the middle of `id` is inside it, then
/// clicks it.
fn click(cx: &mut VisualTestContext, id: ElementId) {
    use gpui_kit::{ScrollDelta, point, px};
    for _ in 0..40 {
        let step = cx.update(|window, cx| {
            window.render_frame(cx);
            let (Some(found), Some(panel)) = (
                window.try_find(id.clone()),
                window.try_find("settings-panel"),
            ) else {
                return 0.;
            };
            let (middle, panel) = (found.bounds().center().y, panel.bounds());
            if middle > panel.bottom() - px(8.) {
                -80.
            } else if middle < panel.top() + px(8.) {
                80.
            } else {
                0.
            }
        });
        if step == 0. {
            break;
        }
        cx.update(|window, cx| {
            window.scroll(
                "settings-panel",
                ScrollDelta::Pixels(point(px(0.), px(step))),
                cx,
            );
        });
        cx.run_until_parked();
    }
    crate::history::click(cx, id);
}

fn focused(cx: &mut VisualTestContext, id: impl Into<ElementId>) -> bool {
    crate::stage::focused(cx, id.into())
}

fn shown_tab(cx: &mut VisualTestContext, window: &Entity<GitcrocWindow>) -> Option<SettingsTab> {
    cx.update(|_, cx| {
        window
            .read(cx)
            .dialog_of::<SettingsDialog>()
            .map(|dialog| dialog.read(cx).tab())
    })
}

/// The index of `theme` among the themes the tab lists.
fn theme_at(theme: CodeTheme) -> usize {
    CODE_THEMES
        .iter()
        .position(|known| *known == theme)
        .unwrap()
}

/// A settings file of a newer schema, which is read-only (DATA-1).
fn newer_schema() -> String {
    "schema = 99\nupdate_check = false\n".to_owned()
}

/// UI-23: Dark applies the dark tokens and is saved; Follow the system
/// follows the system's mode live.
#[gpui::test]
fn ui_23_the_theme_choice_applies_and_is_saved(root: &mut TestAppContext) {
    let f = Fixture::start(gitcroc_app::app::EVENT_CAPACITY);
    let (mut cx, _) = main_window(root, &f);
    open_settings(&mut cx);
    assert!(chosen(&mut cx, named("settings-theme", 0)));

    click(&mut cx, named("settings-theme", 2));
    wait_until(&mut cx, |cx| cx.update(|_, cx| AppTheme::global(cx).dark()));
    assert!(chosen(&mut cx, named("settings-theme", 2)));
    assert!(
        f.saved().contains("theme = \"dark\""),
        "{}",
        f.settings_file()
    );

    click(&mut cx, named("settings-theme", 1));
    wait_until(&mut cx, |cx| {
        cx.update(|_, cx| !AppTheme::global(cx).dark())
    });
    assert!(f.saved().contains("theme = \"light\""));

    click(&mut cx, named("settings-theme", 0));
    wait_until(&mut cx, |_| f.saved().contains("theme = \"system\""));
    cx.update(|_, cx| AppTheme::set_system_dark(true, cx));
    assert!(cx.update(|_, cx| AppTheme::global(cx).dark()));
    cx.update(|_, cx| AppTheme::set_system_dark(false, cx));
    assert!(!cx.update(|_, cx| AppTheme::global(cx).dark()));
}

/// UI-24: each choice shows a preview of its own theme, whatever the app
/// theme is, and changing one leaves the other.
#[gpui::test]
fn ui_24_each_code_theme_choice_shows_its_own_preview(root: &mut TestAppContext) {
    let f = Fixture::start(gitcroc_app::app::EVENT_CAPACITY);
    let (mut cx, _) = main_window(root, &f);
    open_settings(&mut cx);
    // The app is light; both previews still show their own theme.
    let light = theme_at(CodeTheme::GithubLight);
    let dark = theme_at(CodeTheme::GithubDark);
    assert!(shown(&mut cx, named("settings-preview-light", light)));
    assert!(shown(&mut cx, named("settings-preview-dark", dark)));

    let dracula = theme_at(CodeTheme::Dracula);
    click(&mut cx, named("settings-code-theme-dark", dracula));
    wait_for_element(&mut cx, named("settings-preview-dark", dracula));
    assert!(chosen(&mut cx, named("settings-code-theme-dark", dracula)));
    assert!(shown(&mut cx, named("settings-preview-light", light)));
    assert!(chosen(&mut cx, named("settings-code-theme-light", light)));
    assert!(f.saved().contains("code_theme_dark = \"dracula\""));
    assert!(f.saved().contains("code_theme_light = \"github_light\""));
}

/// UI-24: the code theme of the app's mode is the one the diff draws in.
#[gpui::test]
fn ui_24_the_chosen_code_theme_applies_to_the_diff(root: &mut TestAppContext) {
    let f = Fixture::start(gitcroc_app::app::EVENT_CAPACITY);
    let (mut cx, _) = main_window(root, &f);
    open_settings(&mut cx);
    click(&mut cx, named("settings-theme", 2));
    let nord = theme_at(CodeTheme::Nord);
    click(&mut cx, named("settings-code-theme-dark", nord));
    wait_until(&mut cx, |cx| {
        cx.update(|_, cx| AppTheme::global(cx).code_theme().label == "Nord")
    });
    let monokai = theme_at(CodeTheme::Monokai);
    click(&mut cx, named("settings-code-theme-light", monokai));
    click(&mut cx, named("settings-theme", 1));
    wait_until(&mut cx, |cx| {
        cx.update(|_, cx| AppTheme::global(cx).code_theme().label == "Monokai")
    });
}

/// UI-26: a font and a size apply and are saved; a size of 40 is saved as
/// 32, and text that is not a number puts the saved size back.
#[gpui::test]
fn ui_26_fonts_and_sizes_apply_and_are_saved(root: &mut TestAppContext) {
    let f = Fixture::start(gitcroc_app::app::EVENT_CAPACITY);
    let (mut cx, _) = main_window(root, &f);
    open_settings(&mut cx);
    assert_eq!(
        value(&mut cx, "settings-ui-font-size").as_deref(),
        Some("13")
    );

    type_into(&mut cx, "settings-ui-font-size", "15");
    wait_until(&mut cx, |_| f.saved().contains("ui_font_size = 15"));
    assert_eq!(
        cx.update(|_, cx| gpui_component::Theme::global(cx).font_size),
        gpui_kit::px(15.)
    );

    type_into(&mut cx, "settings-ui-font-size", "40");
    wait_until(&mut cx, |_| f.saved().contains("ui_font_size = 32"));
    wait_until(&mut cx, |cx| {
        value(cx, "settings-ui-font-size").as_deref() == Some("32")
    });

    type_into(&mut cx, "settings-code-font-size", "large");
    wait_until(&mut cx, |cx| {
        value(cx, "settings-code-font-size").as_deref() == Some("13")
    });

    type_into(&mut cx, "settings-code-font", "Fira Code");
    wait_until(&mut cx, |_| f.saved().contains("code_font = \"Fira Code\""));
    // An empty name is the system font again.
    type_into(&mut cx, "settings-code-font", "");
    wait_until(&mut cx, |_| !f.saved().contains("code_font ="));
}

/// The main window on `f` with `controls`, once it shows the toolbar.
fn window_with(
    root: &mut TestAppContext,
    f: &Fixture,
    controls: Rc<FakeControls>,
) -> (VisualTestContext, Entity<GitcrocWindow>) {
    let (mut cx, window) = opened_main_window_with(root, f, controls);
    settle(&mut cx, |window, _| window.try_find("toolbar").is_some());
    (cx, window)
}

/// UI-35: on Linux the open window switches to the system's title bar and
/// back, and its own buttons go and come back; where the window cannot
/// switch, it reopens.
#[gpui::test]
fn ui_35_the_setting_switches_the_title_bar_at_once(root: &mut TestAppContext) {
    let f = Fixture::start(gitcroc_app::app::EVENT_CAPACITY);
    let controls = Rc::new(FakeControls::default());
    let (mut cx, _) = window_with(root, &f, controls.clone());
    assert!(shown(&mut cx, "window-close"));
    open_settings(&mut cx);
    assert_eq!(checked(&mut cx, "settings-system-title-bar"), Some(false));

    click(&mut cx, "settings-system-title-bar".into());
    wait_until(&mut cx, |_| {
        controls.asked.borrow().contains(&Asked::TitleBar(true))
    });
    wait_until(&mut cx, |cx| !shown(cx, "window-close"));
    assert!(f.saved().contains("system_title_bar = true"));

    click(&mut cx, "settings-system-title-bar".into());
    wait_until(&mut cx, |_| {
        controls.asked.borrow().contains(&Asked::TitleBar(false))
    });
    wait_until(&mut cx, |cx| shown(cx, "window-close"));
    assert!(
        !controls
            .asked
            .borrow()
            .iter()
            .any(|asked| matches!(asked, Asked::Reopen(_)))
    );

    // Windows and macOS: the window reopens instead.
    controls.reopens.set(true);
    click(&mut cx, "settings-system-title-bar".into());
    wait_until(&mut cx, |_| {
        controls.asked.borrow().contains(&Asked::Reopen(true))
    });
}

/// UI-35: reopening opens the window again with the same tabs and bounds,
/// and closes the old one.
#[gpui::test]
fn ui_35_reopening_keeps_the_tabs_and_the_bounds(root: &mut TestAppContext) {
    let f = Fixture::start(gitcroc_app::app::EVENT_CAPACITY);
    let made = crate::history::repos(&["api"]);
    crate::history::add(&f, &made);
    let (mut cx, window) = main_window(root, &f);
    let old = cx.update(|win, _| win.window_handle().window_id());
    let (tabs, bounds) = cx.update(|win, cx| (window.read(cx).tabs(cx), win.window_bounds()));
    assert!(!tabs.open.is_empty());
    let services = f.services.clone();
    cx.update(|win, cx| SystemControls.reopen(services, true, win, cx));
    cx.run_until_parked();
    let windows = root.update(|cx| cx.windows());
    assert_eq!(windows.len(), 1);
    assert_ne!(windows[0].window_id(), old);
    // The new window is the app's main window on the same tabs.
    let services = f.services.clone();
    let (handle, reopened) = root
        .update(|cx| gitcroc_ui::open_window(services, true, Some(bounds), cx))
        .unwrap();
    let mut reopened_cx = VisualTestContext::from_window(handle, root);
    reopened_cx.run_until_parked();
    let (again, at) =
        reopened_cx.update(|win, cx| (reopened.read(cx).tabs(cx), win.window_bounds()));
    assert_eq!(again.open, tabs.open);
    assert_eq!(again.active, tabs.active);
    assert_eq!(at, bounds);
}

/// UI-35, OPS-9: where the window reopens, unsaved conflict edits are
/// asked about first: Cancel keeps the setting, and confirming saves it
/// and reopens.
#[gpui::test]
fn ui_35_reopening_asks_first_with_unsaved_conflict_edits(root: &mut TestAppContext) {
    use crate::conflict_flow::{kept_unsaved, merge_conflict, take, wait_for_editor};
    use gitcroc_ui::conflict::Take;
    let f = Fixture::start(gitcroc_app::app::EVENT_CAPACITY);
    let made = crate::history::repos(&["api"]);
    merge_conflict(&crate::history::repo(&made, "api"), &["a.txt"]);
    let added = crate::history::add(&f, &made);
    let api = crate::history::id_of(&added, "api");
    let controls = Rc::new(FakeControls::default());
    controls.reopens.set(true);
    let (mut cx, window) = window_with(root, &f, controls.clone());
    crate::stage::show_uncommitted(&mut cx, &window, &added, "api");
    let editor = wait_for_editor(&mut cx, &window, "a.txt");
    take(&mut cx, &editor, Take::Theirs);
    wait_until(&mut cx, |cx| {
        kept_unsaved(cx, &window, &api, "a.txt") == Some(true)
    });

    open_settings(&mut cx);
    click(&mut cx, "settings-system-title-bar".into());
    wait_for_element(&mut cx, "settings-reopen-question".into());
    let file = label_of(&mut cx, named("settings-reopen-file", 0)).unwrap();
    assert!(file.ends_with("api: a.txt"), "{file}");
    click(&mut cx, "settings-reopen-cancel".into());
    assert!(!shown(&mut cx, "settings-reopen-question"));
    assert_eq!(checked(&mut cx, "settings-system-title-bar"), Some(false));
    assert!(controls.asked.borrow().is_empty());

    click(&mut cx, "settings-system-title-bar".into());
    wait_for_element(&mut cx, "settings-reopen-confirm".into());
    click(&mut cx, "settings-reopen-confirm".into());
    wait_until(&mut cx, |_| {
        controls.asked.borrow().contains(&Asked::Reopen(true))
    });
    assert!(f.saved().contains("system_title_bar = true"));
}

fn git_status(f: &Fixture) -> GitStatus {
    let app = f.app().clone();
    f.runtime.block_on(async move { app.git_status().await })
}

/// PLAT-3: the Git tab shows the git that detection found.
#[gpui::test]
fn plat_3_settings_shows_the_git_in_use(root: &mut TestAppContext) {
    let f = Fixture::start(gitcroc_app::app::EVENT_CAPACITY);
    let (mut cx, _) = main_window(root, &f);
    open_settings(&mut cx);
    show_tab(&mut cx, SettingsTab::Git);
    let GitStatus::Usable { path, version, .. } = git_status(&f) else {
        panic!("no usable git");
    };
    wait_for_element(&mut cx, "settings-git-in-use".into());
    assert_eq!(
        label_of(&mut cx, "settings-git-in-use".into()),
        Some(strings::settings::git(&version, &path))
    );
    // Without a chosen git there is nothing to go back from.
    assert!(!shown(&mut cx, "settings-git-automatic"));
    assert!(!shown(&mut cx, "settings-git-missing"));
}

/// PLAT-3: a chosen git that is missing is named with the git used
/// instead, and Use automatic detection gives it up.
#[gpui::test]
fn plat_3_settings_names_a_missing_chosen_git_and_can_go_back(root: &mut TestAppContext) {
    let missing = Path::new("/nowhere/<b>git</b>");
    let f = Fixture::start_with(Options {
        settings: Some(settings_with_git(missing, SETTINGS_SCHEMA)),
        ..Options::default()
    });
    let (mut cx, _) = main_window(root, &f);
    open_settings(&mut cx);
    show_tab(&mut cx, SettingsTab::Git);
    let GitStatus::Usable { path, .. } = git_status(&f) else {
        panic!("no usable git");
    };
    wait_for_element(&mut cx, "settings-git-missing".into());
    // SEC-3: the path is text.
    assert_eq!(
        label_of(&mut cx, "settings-git-missing".into()),
        Some(strings::settings::chosen_missing(
            missing.to_str().unwrap(),
            &path
        ))
    );
    click(&mut cx, "settings-git-automatic".into());
    wait_until(&mut cx, |cx| !shown(cx, "settings-git-missing"));
    assert!(!f.saved().contains("git_path"));
    assert!(!shown(&mut cx, "settings-git-automatic"));
}

/// PLAT-3: a cancelled file dialog changes nothing.
#[gpui::test]
fn plat_3_a_cancelled_git_dialog_changes_nothing(root: &mut TestAppContext) {
    let f = Fixture::start(gitcroc_app::app::EVENT_CAPACITY);
    let (mut cx, _) = main_window(root, &f);
    open_settings(&mut cx);
    show_tab(&mut cx, SettingsTab::Git);
    wait_for_element(&mut cx, "settings-git-in-use".into());
    let before = label_of(&mut cx, "settings-git-in-use".into());
    f.answer(None);
    click(&mut cx, "settings-choose-git".into());
    settle(&mut cx, |_, _| true);
    assert_eq!(label_of(&mut cx, "settings-git-in-use".into()), before);
    assert!(!disabled(&mut cx, "settings-choose-git"));
    assert!(
        f.settings_file_if_any()
            .is_none_or(|file| !file.contains("git_path"))
    );
}

/// Section 5: the group settings round-trip through the core; 12 is
/// saved, 40 is refused with the core's message and the field shows 12
/// again; Always show the preflight dialog is saved.
#[gpui::test]
fn the_group_settings_round_trip_and_outside_1_to_32_is_refused(root: &mut TestAppContext) {
    let f = Fixture::start(gitcroc_app::app::EVENT_CAPACITY);
    let (mut cx, _) = main_window(root, &f);
    open_settings(&mut cx);
    show_tab(&mut cx, SettingsTab::Git);
    wait_for_element(&mut cx, "settings-repos-at-once".into());
    assert_eq!(
        value(&mut cx, "settings-repos-at-once").as_deref(),
        Some("8")
    );

    type_into(&mut cx, "settings-repos-at-once", "12");
    wait_until(&mut cx, |_| f.saved().contains("group_parallelism = 12"));

    type_into(&mut cx, "settings-repos-at-once", "40");
    wait_for_element(&mut cx, "settings-groups-error".into());
    assert_eq!(
        label_of(&mut cx, "settings-groups-error".into()).as_deref(),
        Some("group operations run 1 to 32 repos at once")
    );
    wait_until(&mut cx, |cx| {
        value(cx, "settings-repos-at-once").as_deref() == Some("12")
    });
    assert!(f.saved().contains("group_parallelism = 12"));

    click(&mut cx, "settings-always-preflight".into());
    wait_until(&mut cx, |_| {
        f.saved().contains("always_show_preflight = true")
    });
    assert!(!shown(&mut cx, "settings-groups-error"));
}

/// Offers `VERSION`, finds nothing, or fails, as a test asks.
enum Source {
    Offers,
    UpToDate,
    Fails(&'static str),
}

impl UpdateSource for Source {
    fn check(&self) -> UpdateFuture<'_, Option<UpdateFound>> {
        let result = match self {
            Self::Offers => Ok(Some(UpdateFound {
                version: VERSION.to_owned(),
                date: None,
                notes: String::new(),
            })),
            Self::UpToDate => Ok(None),
            Self::Fails(reason) => Err((*reason).to_owned()),
        };
        Box::pin(async move { result })
    }

    fn install(&self, _progress: UpdateProgress) -> UpdateFuture<'_, ()> {
        Box::pin(async { Ok(()) })
    }
}

fn checking(source: Source, settings: Option<String>) -> Fixture {
    Fixture::start_with(Options {
        settings,
        updates: Some((
            Arc::new(source),
            UpdateTest {
                package: Package::AppImage,
                checks: true,
            },
        )),
        ..Options::default()
    })
}

fn status(cx: &mut VisualTestContext) -> String {
    label_of(cx, "settings-update-status".into()).unwrap_or_default()
}

/// D12: the automatic check can be turned off, and Check now still checks.
#[gpui::test]
fn d12_settings_turns_the_automatic_check_off_and_checks_now(root: &mut TestAppContext) {
    let f = checking(Source::Offers, None);
    let (mut cx, _) = main_window(root, &f);
    open_settings(&mut cx);
    show_tab(&mut cx, SettingsTab::Updates);
    wait_for_element(&mut cx, "settings-update-check".into());
    assert_eq!(checked(&mut cx, "settings-update-check"), Some(true));

    click(&mut cx, "settings-update-check".into());
    wait_until(&mut cx, |_| f.saved().contains("update_check = false"));
    // The file is written on the app's runtime; the checkbox follows when
    // the settings event reaches the UI, which may come later.
    wait_until(&mut cx, |cx| {
        checked(cx, "settings-update-check") == Some(false)
    });

    wait_until(&mut cx, |cx| !disabled(cx, "settings-check-now"));
    click(&mut cx, "settings-check-now".into());
    let available = format!("gitcroc {VERSION} is available (checked ");
    wait_until(&mut cx, |cx| status(cx).starts_with(&available));
}

/// D12: a failed check shows in Settings with its time, and nowhere else.
#[gpui::test]
fn d12_a_failed_check_shows_in_settings_with_its_time_and_nowhere_else(root: &mut TestAppContext) {
    let f = checking(Source::Fails("offline"), None);
    let (mut cx, _) = main_window(root, &f);
    f.app().update_check_now();
    open_settings(&mut cx);
    show_tab(&mut cx, SettingsTab::Updates);
    wait_until(&mut cx, |cx| {
        status(cx).starts_with("The last check failed at ")
    });
    assert!(
        status(&mut cx).ends_with(": offline"),
        "{}",
        status(&mut cx)
    );
    assert!(!shown(&mut cx, "update-notice"));
    assert!(!shown(&mut cx, "settings-updates-error"));
}

/// D12: a build that does not check says so, and cannot check now.
#[gpui::test]
fn d12_a_build_that_does_not_check_says_so_and_cannot_check_now(root: &mut TestAppContext) {
    let f = Fixture::start(gitcroc_app::app::EVENT_CAPACITY);
    let (mut cx, _) = main_window(root, &f);
    open_settings(&mut cx);
    show_tab(&mut cx, SettingsTab::Updates);
    wait_until(&mut cx, |cx| status(cx) == strings::settings::NO_CHECKS);
    assert!(disabled(&mut cx, "settings-check-now"));
}

/// D12: Check for updates in the palette, with nothing on offer, checks
/// and opens Settings at its Updates tab, where the result shows.
#[gpui::test]
fn d12_check_for_updates_in_the_palette_opens_the_updates_tab(root: &mut TestAppContext) {
    let f = checking(Source::UpToDate, None);
    let (mut cx, window) = main_window(root, &f);
    crate::palette::open_palette(&mut cx, &window);
    crate::palette::search(&mut cx, &window, "Check for updates", "Check for updates");
    press(&mut cx, "enter");
    wait_for_element(&mut cx, "settings-dialog".into());
    assert_eq!(shown_tab(&mut cx, &window), Some(SettingsTab::Updates));
    wait_until(&mut cx, |cx| status(cx).contains("is up to date (checked "));
}

/// SEC-7: read-only settings keep the automatic check as it is.
#[gpui::test]
fn sec_7_read_only_settings_keep_the_automatic_check_as_it_is(root: &mut TestAppContext) {
    let f = checking(Source::UpToDate, Some(newer_schema()));
    let (mut cx, _) = main_window(root, &f);
    open_settings(&mut cx);
    show_tab(&mut cx, SettingsTab::Updates);
    wait_for_element(&mut cx, "settings-update-check".into());
    assert_eq!(checked(&mut cx, "settings-update-check"), Some(false));
    assert!(disabled(&mut cx, "settings-update-check"));
    click(&mut cx, "settings-update-check".into());
    settle(&mut cx, |_, _| true);
    assert_eq!(checked(&mut cx, "settings-update-check"), Some(false));
    assert_eq!(f.settings_file(), newer_schema());
}

/// DATA-1: settings of a newer schema show a banner naming the file and
/// its schema, and no control of any tab is enabled.
#[gpui::test]
fn data_1_read_only_settings_show_a_banner_and_disable_every_tab(root: &mut TestAppContext) {
    let f = Fixture::start_with(Options {
        settings: Some(newer_schema()),
        ..Options::default()
    });
    let (mut cx, _) = main_window(root, &f);
    wait_for_element(&mut cx, "settings-banner".into());
    let banner = label_of(&mut cx, "settings-banner".into()).unwrap();
    assert!(banner.contains(gitcroc_core::SETTINGS_FILE), "{banner}");
    assert!(banner.contains("schema 99"), "{banner}");
    assert!(banner.contains("read-only"), "{banner}");

    open_settings(&mut cx);
    for id in [
        named("settings-theme", 0),
        named("settings-theme", 2),
        named("settings-code-theme-light", 1),
        named("settings-code-theme-dark", 1),
        "settings-system-title-bar".into(),
    ] {
        assert!(disabled(&mut cx, id.clone()), "{id:?}");
    }
    // A text field takes no typing.
    for (id, saved) in [("settings-ui-font", ""), ("settings-ui-font-size", "13")] {
        type_into(&mut cx, id, "20");
        assert_eq!(value(&mut cx, id).as_deref(), Some(saved), "{id}");
    }
    show_tab(&mut cx, SettingsTab::Git);
    wait_for_element(&mut cx, "settings-choose-git".into());
    for id in ["settings-choose-git", "settings-always-preflight"] {
        assert!(disabled(&mut cx, id), "{id}");
    }
    type_into(&mut cx, "settings-repos-at-once", "20");
    assert_eq!(
        value(&mut cx, "settings-repos-at-once").as_deref(),
        Some("8")
    );
    show_tab(&mut cx, SettingsTab::Updates);
    wait_for_element(&mut cx, "settings-update-check".into());
    assert!(disabled(&mut cx, "settings-update-check"));
    show_tab(&mut cx, SettingsTab::Ai);
    wait_for_element(&mut cx, "ai-features".into());
    assert!(disabled(&mut cx, "ai-features"));
    // A click changes nothing.
    show_tab(&mut cx, SettingsTab::Appearance);
    click(&mut cx, named("settings-theme", 2));
    settle(&mut cx, |_, _| true);
    assert!(!cx.update(|_, cx| AppTheme::global(cx).dark()));
    assert_eq!(f.settings_file(), newer_schema());
}

/// UI-22: the tab keys reach all four tabs, and Tab reaches each tab's
/// controls in order.
#[gpui::test]
fn ui_22_every_settings_tab_is_keyboard_reachable(root: &mut TestAppContext) {
    let f = Fixture::start(gitcroc_app::app::EVENT_CAPACITY);
    let (mut cx, window) = main_window(root, &f);
    wait_for_element(&mut cx, "toolbar-settings".into());
    press(&mut cx, &secondary(","));
    wait_for_element(&mut cx, "settings-dialog".into());
    // It opens at Appearance, with the focus on its first control.
    assert_eq!(shown_tab(&mut cx, &window), Some(SettingsTab::Appearance));
    wait_until(&mut cx, |cx| focused(cx, named("settings-theme", 0)));

    press(&mut cx, "shift-tab");
    assert!(focused(&mut cx, "settings-tab-appearance"));
    for (key, tab) in [
        ("right", SettingsTab::Git),
        ("right", SettingsTab::Updates),
        ("right", SettingsTab::Ai),
        ("right", SettingsTab::Appearance),
        ("end", SettingsTab::Ai),
        ("home", SettingsTab::Appearance),
        ("left", SettingsTab::Ai),
    ] {
        press(&mut cx, key);
        assert!(focused(&mut cx, tab.id()), "{key} {tab:?}");
        assert_eq!(shown_tab(&mut cx, &window), Some(tab), "{key}");
    }

    // Each tab's controls are reached with Tab, in order.
    let reach = |cx: &mut VisualTestContext, tab: SettingsTab, ids: &[ElementId]| {
        press(cx, "home");
        for _ in 0..SettingsTab::ALL
            .iter()
            .position(|known| *known == tab)
            .unwrap()
        {
            press(cx, "right");
        }
        assert!(focused(cx, tab.id()));
        let mut at = 0;
        for _ in 0..40 {
            press(cx, "tab");
            if ids.get(at).is_some_and(|id| focused(cx, id.clone())) {
                at += 1;
                if at == ids.len() {
                    break;
                }
            }
        }
        assert_eq!(at, ids.len(), "{tab:?} reached {:?}", &ids[..at]);
        // Back to the tab strip.
        click(cx, tab.id().into());
    };
    reach(
        &mut cx,
        SettingsTab::Appearance,
        &[
            named("settings-theme", 0),
            named("settings-code-theme-light", 0),
            named("settings-code-theme-dark", 0),
            "settings-ui-font".into(),
            "settings-ui-font-size".into(),
            "settings-code-font".into(),
            "settings-code-font-size".into(),
            "settings-system-title-bar".into(),
        ],
    );
    reach(
        &mut cx,
        SettingsTab::Git,
        &[
            "settings-choose-git".into(),
            "settings-repos-at-once".into(),
            "settings-always-preflight".into(),
        ],
    );
    reach(
        &mut cx,
        SettingsTab::Updates,
        &["settings-update-check".into()],
    );
}
