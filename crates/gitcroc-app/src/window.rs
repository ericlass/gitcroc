//! The window's title bar (UI-35): the tab bar stands in for the system's,
//! unless the user chose the system's. The tab bar's buttons and dragging
//! reach the window through the app's own commands, so the window needs no
//! Tauri window permission (SEC-2).

use serde::{Deserialize, Serialize};
use tauri::{Runtime, WebviewWindow};

use crate::platform::{PlatformWindowFrame, WindowFrame};

/// The label of the app's one window (`tauri.conf.json`).
pub const MAIN_WINDOW: &str = "main";

/// What the system draws above the page.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TitleBar {
    /// The system's title bar and buttons.
    System,
    /// No system decorations; the tab bar has the buttons (Windows, Linux).
    Hidden,
    /// The system's title bar is transparent over the page, its title hidden
    /// (`tauri.macos.conf.json`), and the system's buttons stay (macOS).
    Overlay,
}

impl TitleBar {
    /// The title bar on `os` (`std::env::consts::OS`), when the user did or
    /// did not choose the system's.
    pub fn new(os: &str, system: bool) -> Self {
        match (system, os) {
            (true, _) => Self::System,
            (false, "macos") => Self::Overlay,
            (false, _) => Self::Hidden,
        }
    }

    /// The title bar on this platform.
    pub fn here(system: bool) -> Self {
        Self::new(std::env::consts::OS, system)
    }

    /// Gives `window` this title bar. The window keeps the system's resizing.
    pub fn apply<R: Runtime>(self, window: &WebviewWindow<R>) -> tauri::Result<()> {
        PlatformWindowFrame::apply(window, self)
    }
}

/// What a window button or the tab bar asks of the window.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "snake_case")]
pub enum WindowAction {
    Minimize,
    /// Maximizes the window, or restores it when it is maximized.
    ToggleMaximize,
    Close,
    /// Moves the window with the mouse, from a press on the tab bar's empty
    /// space until the button is released.
    StartDragging,
}

/// Does `action` to `window`.
pub fn control<R: Runtime>(window: &WebviewWindow<R>, action: WindowAction) -> tauri::Result<()> {
    match action {
        WindowAction::Minimize => window.minimize(),
        WindowAction::ToggleMaximize => {
            if window.is_maximized()? {
                window.unmaximize()
            } else {
                window.maximize()
            }
        }
        WindowAction::Close => window.close(),
        WindowAction::StartDragging => window.start_dragging(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ui_35_title_bar_style_per_platform() {
        assert_eq!(TitleBar::new("linux", false), TitleBar::Hidden);
        assert_eq!(TitleBar::new("windows", false), TitleBar::Hidden);
        assert_eq!(TitleBar::new("macos", false), TitleBar::Overlay);
        for os in ["linux", "windows", "macos"] {
            assert_eq!(TitleBar::new(os, true), TitleBar::System, "{os}");
        }
    }
}
