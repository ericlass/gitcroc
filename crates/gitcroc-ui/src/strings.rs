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
    pub const NEXT_TAB: &str = "Next tab";
    pub const PREVIOUS_TAB: &str = "Previous tab";
    pub const MOVE_TAB_RIGHT: &str = "Move the tab to the right";
    pub const MOVE_TAB_LEFT: &str = "Move the tab to the left";
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

/// Words many places share.
pub mod common {
    pub const CANCEL: &str = "Cancel";
    pub const DISMISS: &str = "Dismiss";
}

/// The window's own messages.
pub mod app {
    pub fn missing_workspaces(files: &str) -> String {
        format!("These workspace files were not found, so their tabs were not restored: {files}")
    }
    pub const NO_WORKSPACE: &str = "No workspace is open. Open one or create a new one.";
}

/// What the dialogs share.
pub mod dialogs {
    pub const NAME: &str = "Name";
}

/// The workspace tabs (UI-27, UI-28).
pub mod tabs {
    pub const LABEL: &str = "Workspaces";
    pub fn close(workspace: &str) -> String {
        format!("Close {workspace}")
    }
    pub const OPEN: &str = "Open workspace";
    pub const OPEN_TITLE: &str = "Open or create a workspace";
    pub const ACTIONS: &str = "Tab actions";
    pub const RENAME: &str = "Rename…";
    pub const EXPORT: &str = "Export workspace…";
    pub const CLOSE_TAB: &str = "Close";
}

/// The workspace files, their menu and their dialogs (UI-28).
pub mod workspaces {
    pub const LABEL: &str = "Workspaces";
    pub const MENU: &str = "Open workspace";
    pub const OPEN: &str = "(open)";
    pub const READ_ONLY: &str = "(read-only)";
    /// A workspace's name with a mark after it, as "Java (open)".
    pub fn marked(name: &str, mark: &str) -> String {
        format!("{name} {mark}")
    }
    pub fn newer_schema(found: u32) -> String {
        format!("From a newer gitcroc (schema {found}); it opens read-only.")
    }
    pub fn unreadable(reason: &str) -> String {
        format!("Cannot be read: {reason}. It opens read-only.")
    }
    pub const NO_CONFIG_DIR: &str = "There is no config directory; changes are not saved.";
    pub const DELETE_ENTRY: &str = "Delete…";
    pub fn delete_label(workspace: &str) -> String {
        format!("Delete {workspace}…")
    }
    pub const NEW_ENTRY: &str = "New workspace…";
    pub const IMPORT_ENTRY: &str = "Import workspace…";
    pub fn cannot_list(error: &str) -> String {
        format!("Cannot list the workspaces: {error}")
    }
    pub fn cannot_open(workspace: &str, error: &str) -> String {
        format!("Cannot open {workspace}: {error}")
    }
    pub const NEW_DIALOG: &str = "New workspace";
    pub const CREATE: &str = "Create";
    pub fn rename_dialog(workspace: &str) -> String {
        format!("Rename {workspace}")
    }
    pub const RENAME: &str = "Rename";
    pub fn delete_dialog(workspace: &str) -> String {
        format!("Delete the workspace {workspace}?")
    }
    pub const DELETE: &str = "Delete";
    pub const DELETE_WHAT: &str =
        "Its workspace file is deleted. The repositories on disk are not touched.";
    pub fn cannot_export(error: &str) -> String {
        format!("Cannot export the workspace: {error}")
    }
    pub fn cannot_import(error: &str) -> String {
        format!("Cannot import a workspace: {error}")
    }
}

/// The git setup screen (PLAT-3, D13), and the message before it.
pub mod git_setup {
    pub const LOOKING_FOR_GIT: &str = "Looking for git…";
    pub const TITLE: &str = "gitcroc needs git";
    pub const NOT_FOUND: &str = "git was not found.";
    pub const NOWHERE_TO_LOOK: &str = "There was nowhere to look: PATH is empty.";
    pub const LOOKED_AT: &str = "Looked for it at:";
    pub fn too_old(found: &str, path: &str, required: &str) -> String {
        format!("Found git {found} at {path}, but gitcroc needs git {required} or newer.")
    }
    pub fn unparseable(path: &str) -> String {
        format!("Found git at {path}, but could not read its version. It reported:")
    }
    pub const CHECKING: &str = "Checking…";
    pub const CHECK_AGAIN: &str = "Check again";
    pub const CHOOSE_GIT: &str = "Choose git…";
    pub const USE_AUTOMATIC: &str = "Use automatic detection";
    pub fn chosen_missing(chosen: &str) -> String {
        format!("The chosen git was not found at {chosen}.")
    }
    /// One line per platform, keyed by `AppInfo::os`.
    pub fn install_hint(os: &str) -> &'static str {
        match os {
            "windows" => {
                "Install Git for Windows from git-scm.com, or run: winget install --id Git.Git -e"
            }
            "macos" => "Install a current git with Homebrew: brew install git",
            "linux" => {
                "Install a current git with your package manager; on older Ubuntu releases, use \
                 the git-core PPA."
            }
            _ => DEFAULT_HINT,
        }
    }
    pub const DEFAULT_HINT: &str = "Install a current git from git-scm.com.";
}

/// The code themes (UI-24), as the Settings dialog lists them.
pub mod code_themes {
    pub const GITHUB_LIGHT: &str = "GitHub Light";
    pub const GITHUB_DARK: &str = "GitHub Dark";
    pub const SOLARIZED_LIGHT: &str = "Solarized Light";
    pub const SOLARIZED_DARK: &str = "Solarized Dark";
    pub const ONE_DARK: &str = "One Dark";
    pub const DRACULA: &str = "Dracula";
    pub const NORD: &str = "Nord";
    pub const MONOKAI: &str = "Monokai";
}

/// Messages from the start, before any window.
pub mod startup {
    /// No font is installed, so no text can be drawn (UI-26).
    pub const NO_FONTS: &str = "gitcroc cannot start: no font is installed. Install a font \
        package, such as fonts-dejavu-core or dejavu-sans-fonts, and start gitcroc again.";
    /// A font from the settings or the desktop that is not installed.
    pub fn missing_font(family: &str) -> String {
        format!("the font {family:?} is not installed; using the default instead")
    }
}
