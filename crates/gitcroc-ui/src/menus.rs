//! The app's actions, and on macOS its menus: App (About, Quit), Edit and
//! Window. The Edit menu's items are the text inputs' own actions, so Cmd+C,
//! Cmd+V and the rest work in every input.

use gitcroc_core::AppInfo;
use gpui_component::input::{Copy, Cut, Paste, Redo, SelectAll, Undo};
use gpui_kit::{App, Menu, MenuItem, OsAction, PromptLevel};

use crate::keys::{About, Minimize, Quit, Zoom};
use crate::strings::{self, menus};

/// Handles the app-wide actions and, on macOS, sets the menus.
pub fn init(cx: &mut App) {
    cx.on_action(|_: &Quit, cx| cx.quit());
    cx.on_action(|_: &About, cx| about(cx));
    if cfg!(target_os = "macos") {
        cx.set_menus(app_menus());
    }
}

/// The menus of the macOS menu bar.
pub fn app_menus() -> Vec<Menu> {
    vec![
        Menu::new(strings::APP_NAME).items([
            MenuItem::action(menus::ABOUT, About),
            MenuItem::separator(),
            MenuItem::action(menus::QUIT, Quit),
        ]),
        Menu::new(menus::EDIT).items([
            MenuItem::os_action(menus::UNDO, Undo, OsAction::Undo),
            MenuItem::os_action(menus::REDO, Redo, OsAction::Redo),
            MenuItem::separator(),
            MenuItem::os_action(menus::CUT, Cut, OsAction::Cut),
            MenuItem::os_action(menus::COPY, Copy, OsAction::Copy),
            MenuItem::os_action(menus::PASTE, Paste, OsAction::Paste),
            MenuItem::os_action(menus::SELECT_ALL, SelectAll, OsAction::SelectAll),
        ]),
        Menu::new(menus::WINDOW).items([
            MenuItem::action(menus::MINIMIZE, Minimize),
            MenuItem::action(menus::ZOOM, Zoom),
        ]),
    ]
}

/// The app's name and version, over the active window.
fn about(cx: &mut App) {
    let Some(window) = cx.active_window() else {
        return;
    };
    let version = menus::version(&AppInfo::current().version);
    let _ = window.update(cx, |_, window, cx| {
        // The answer only closes it.
        let _closed = window.prompt(
            PromptLevel::Info,
            strings::APP_NAME,
            Some(&version),
            &[menus::OK],
            cx,
        );
    });
}
