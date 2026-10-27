//! Every piece of text the UI shows (D15): English only in v1, kept here so
//! no view writes text of its own. `d15_ui_text_comes_from_the_strings_module`
//! fails on text written anywhere else in `src`. Each task that ports an area
//! from `ui/src/strings/en.ts` moves its strings here.

/// The product name, in the title bar of screens without tabs (UI-35).
pub const APP_NAME: &str = "gitcroc";

/// The window's buttons (UI-35), as screen readers name them.
pub mod window {
    pub const MINIMIZE: &str = "Minimize";
    pub const MAXIMIZE: &str = "Maximize";
    pub const RESTORE: &str = "Restore";
    pub const CLOSE: &str = "Close";
}

/// The regions of the window, as screen readers name them.
pub mod regions {
    pub const TITLE_BAR: &str = "Tabs";
    pub const TOOLBAR: &str = "Toolbar";
    pub const TREE: &str = "Repositories";
    pub const HISTORY: &str = "History";
    pub const CHANGES: &str = "Changes";
    pub const STATUS_BAR: &str = "Status";
}

/// The macOS menus.
pub mod menus {
    pub const ABOUT: &str = "About gitcroc";
    pub const QUIT: &str = "Quit gitcroc";
    pub const EDIT: &str = "Edit";
    pub const UNDO: &str = "Undo";
    pub const REDO: &str = "Redo";
    pub const CUT: &str = "Cut";
    pub const COPY: &str = "Copy";
    pub const PASTE: &str = "Paste";
    pub const SELECT_ALL: &str = "Select All";
    pub const WINDOW: &str = "Window";
    pub const MINIMIZE: &str = "Minimize";
    pub const ZOOM: &str = "Zoom";
    /// The About dialog's text.
    pub fn version(version: &str) -> String {
        format!("Version {version}")
    }
    pub const OK: &str = "OK";
}

/// The app-wide shortcuts of `keys.rs` (UI-22): what each does, and how its
/// keys are written.
pub mod keys {
    pub const NEXT_REGION: &str = "Move the focus to the next region";
    pub const PREVIOUS_REGION: &str = "Move the focus to the previous region";
    /// Modifiers, as each platform writes them.
    pub const CTRL: &str = "Ctrl";
    pub const CMD: &str = "Cmd";
    pub const SHIFT: &str = "Shift";
    pub const ALT: &str = "Alt";
    pub const OPTION: &str = "Option";
    /// Keys whose name is not the character they type.
    pub fn name(key: &str) -> Option<&'static str> {
        match key {
            "pagedown" => Some("Page Down"),
            "pageup" => Some("Page Up"),
            _ => None,
        }
    }
    /// Keys and modifiers joined, as "Ctrl+Shift+Page Down".
    pub fn join(parts: &[&str]) -> String {
        parts.join("+")
    }
}
