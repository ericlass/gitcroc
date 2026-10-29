//! gitcroc's native UI (section 8) on GPUI: views over the app's API in
//! `gitcroc-app`, with no source of truth and no business logic of their
//! own (ARCH-6). `gitcroc-next` runs it until M6-30.

pub mod changes;
pub mod changes_pane;
pub mod dialog;
pub mod dialogs;
pub mod diff;
pub mod diff_view;
pub mod git_setup;
pub mod highlight;
pub mod history;
pub mod history_columns;
pub mod history_pane;
pub mod icons;
pub mod job_list;
pub mod jobs;
pub mod keys;
pub mod menu;
pub mod menus;
pub mod panes;
#[cfg(feature = "perf-trace")]
pub mod perf;
pub mod services;
pub mod special_diff;
pub mod status_bar;
pub mod strings;
pub mod tab_bar;
pub mod theme;
pub mod title_bar;
pub mod tree;
pub mod tree_pane;
pub mod window;
pub mod workspace_menu;
pub mod workspaces;

use std::rc::Rc;

use gpui_component::TitleBar as ComponentTitleBar;
use gpui_kit::{
    AnyWindowHandle, App, AppContext, Bounds, Entity, Result, TitlebarOptions, WindowBounds,
    WindowDecorations, WindowOptions, px, size,
};

use crate::services::Services;
use crate::title_bar::{Platform, SystemControls};
use crate::window::GitcrocWindow;

/// Sets GPUI up for the app: the component library and its theme, the
/// keymap and the keys of menus, dialogs, tabs, the tree, the history and
/// the changes, and the app's actions and menus.
pub fn init(cx: &mut App) {
    gpui_kit::init(cx);
    theme::AppTheme::init(cx);
    keys::bind(cx);
    menu::bind(cx);
    dialog::bind(cx);
    tab_bar::bind(cx);
    status_bar::bind(cx);
    job_list::bind(cx);
    tree_pane::bind(cx);
    history_pane::bind(cx);
    changes_pane::bind(cx);
    panes::bind(cx);
    menus::init(cx);
}

/// The main window's options. With `system_title_bar` off, the window draws
/// its own title bar (UI-35); on macOS the traffic lights stay, over it.
pub fn window_options(system_title_bar: bool, cx: &App) -> WindowOptions {
    let bounds = WindowBounds::Windowed(Bounds::centered(None, size(px(1280.), px(800.)), cx));
    let title = Some(strings::APP_NAME.into());
    if system_title_bar {
        WindowOptions {
            window_bounds: Some(bounds),
            titlebar: Some(TitlebarOptions {
                title,
                ..Default::default()
            }),
            window_decorations: Some(WindowDecorations::Server),
            app_id: Some(strings::APP_NAME.to_owned()),
            ..Default::default()
        }
    } else {
        // The component library's options: a transparent title bar with the
        // traffic lights centred in it, moved by the app (macOS).
        let options = ComponentTitleBar::window_options();
        WindowOptions {
            window_bounds: Some(bounds),
            titlebar: options
                .titlebar
                .map(|titlebar| TitlebarOptions { title, ..titlebar }),
            window_decorations: Some(WindowDecorations::Client),
            app_id: Some(strings::APP_NAME.to_owned()),
            ..options
        }
    }
}

/// Opens the main window on `services`.
pub fn open_window(
    services: Services,
    system_title_bar: bool,
    cx: &mut App,
) -> Result<(AnyWindowHandle, Entity<GitcrocWindow>)> {
    let options = window_options(system_title_bar, cx);
    // Inside the component library's root, which moves the focus with Tab
    // and Shift+Tab, keeps it inside a dialog, and draws the window's
    // frame where the window draws its own.
    gpui_kit::open_window(options, cx, move |window, cx| {
        cx.new(|cx| {
            GitcrocWindow::new(
                services,
                Rc::new(SystemControls),
                Platform::current(),
                system_title_bar,
                window,
                cx,
            )
        })
    })
}
