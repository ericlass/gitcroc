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
    pub const NEXT_HUNK: &str = "Next hunk in the diff or the conflict editor";
    pub const PREVIOUS_HUNK: &str = "Previous hunk in the diff or the conflict editor";
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

/// `count` with `one` or `many` after it.
fn plural(count: u32, one: &str, many: &str) -> String {
    format!("{count} {}", if count == 1 { one } else { many })
}

/// The tree pane (UI-1 to UI-8).
pub mod tree {
    use super::plural;

    pub const LABEL: &str = "Repositories";
    pub const FILTER: &str = "Filter repos and branches";
    pub const EMPTY: &str = "This workspace has no repositories yet.";
    pub fn cannot_read(error: &str) -> String {
        format!("Cannot read the tree: {error}")
    }
    pub const LOADING: &str = "Loading…";
    pub fn in_members(present: u32, members: u32) -> String {
        format!("in {present} of {members} members")
    }
    pub const CURRENT_BRANCH: &str = "Current branch";
    pub const UPSTREAM_GONE: &str = "The upstream is gone";
    pub const GONE: &str = "gone";
    pub fn ahead(count: u32) -> String {
        format!("{count} ahead")
    }
    pub fn behind(count: u32) -> String {
        format!("{count} behind")
    }
    /// Ahead and behind as the row shows them: `↑1`, `↓2`.
    pub fn ahead_mark(count: u32) -> String {
        format!("↑{count}")
    }
    pub fn behind_mark(count: u32) -> String {
        format!("↓{count}")
    }
    /// A group's branch name in `present` of its `members`: `(3/5)`.
    pub fn present_mark(present: u32, members: u32) -> String {
        format!("({present}/{members})")
    }
    pub const UNCOMMITTED: &str = "Uncommitted changes";
    pub const CONFLICT: &str = "Conflict state";
    pub const FOLDER_MISSING: &str = "Folder is missing";
    pub fn cannot_read_repo(error: &str) -> String {
        format!("Cannot read: {error}")
    }
    pub const REFRESHING: &str = "Refreshing";
    /// A repo row whose summary comes from the cache (PERF-3).
    pub const STALE: &str = "Last known state, not read again yet";
    pub fn members(count: u32) -> String {
        plural(count, "member", "members")
    }
    pub const DETACHED: &str = "detached";
    pub fn mixed(count: u32) -> String {
        format!("mixed ({count})")
    }
    pub const MIXED_TITLE: &str = "The members are on different branches";
    pub fn members_with_changes(count: u32) -> String {
        plural(count, "member with changes", "members with changes")
    }
    pub fn members_in_conflict(count: u32) -> String {
        plural(
            count,
            "member in conflict state",
            "members in conflict state",
        )
    }
}

/// The history pane (UI-9, UI-10, UI-12, UI-36).
pub mod history {
    use super::plural;

    pub const LABEL: &str = "History";
    pub const COLUMNS_LABEL: &str = "History columns";
    pub const GRAPH: &str = "Graph";
    pub const SUBJECT: &str = "Subject";
    pub const AUTHOR: &str = "Author";
    pub const DATE: &str = "Date";
    pub const HASH: &str = "Hash";
    /// The graph column's name in a merged history (UI-9).
    pub const REPOSITORY: &str = "Repository";
    pub const MOVE_LEFT: &str = "Move left";
    pub const MOVE_RIGHT: &str = "Move right";
    pub const RESET_COLUMNS: &str = "Reset columns";
    /// A column's menu, named after the column: "Graph column".
    pub fn column_menu(column: &str) -> String {
        format!("{column} column")
    }
    /// A column's resize edge: "Resize Graph".
    pub fn resize_column(column: &str) -> String {
        format!("Resize {column}")
    }
    pub const NO_SOURCE: &str = "Select a repo, branch, tag or stash to see its history.";
    pub const NO_COMMITS: &str = "No commits yet.";
    pub const NO_MATCH: &str = "No commits match.";
    /// The search box (UI-12).
    pub const SEARCH: &str = "Search subject, author or hash";
    pub const SEARCH_LABEL: &str = "Search history";
    /// The row that leads a dirty repo's history, with its changed paths
    /// (UI-10).
    pub fn uncommitted(changes: u32) -> String {
        format!("Uncommitted changes ({changes})")
    }
    /// The row that leads a merged history, with its dirty repos (UI-34).
    pub fn uncommitted_repos(repos: u32) -> String {
        format!("Uncommitted changes ({})", plural(repos, "repo", "repos"))
    }
    /// The summary strip of a group or several repos (UI-11).
    pub const STRIP: &str = "Repositories of the selection";
    pub fn changed_files(count: u32) -> String {
        plural(count, "changed file", "changed files")
    }
    /// A strip line's dirty count as it shows it: `●2`.
    pub fn changed_mark(count: u32) -> String {
        format!("●{count}")
    }
    pub fn cannot_show(error: &str) -> String {
        format!("Cannot show the selection: {error}")
    }
    pub const NO_REPOS: &str = "This group has no repositories.";
    /// The offer to write a commit-graph file (PERF-2).
    pub const COMMIT_GRAPH_OFFER: &str = "Commit-graph offer";
    pub const COMMIT_GRAPH_WHY: &str = "This repo has no commit-graph file, so its history loads \
        slowly. Git can write one now in the background; nothing else changes.";
    pub const WRITE_COMMIT_GRAPH: &str = "Write commit-graph file";
    pub const DONT_ASK_AGAIN: &str = "Don't ask again";
    pub fn cannot_write_commit_graph(error: &str) -> String {
        format!("Cannot write the commit-graph file: {error}")
    }
    pub fn cannot_keep_answer(error: &str) -> String {
        format!("Cannot keep the answer: {error}")
    }
    pub const LOADING: &str = "Loading";
    pub fn cannot_read(error: &str) -> String {
        format!("Cannot read the history: {error}")
    }
    /// What a ref label is, by kind.
    pub const HEAD: &str = "Detached HEAD";
    pub const CURRENT_BRANCH: &str = "Current branch";
    pub const LOCAL_BRANCH: &str = "Local branch";
    pub const REMOTE_BRANCH: &str = "Remote branch";
    pub const TAG: &str = "Tag";
    /// A ref label as a screen reader reads it: "Current branch main".
    pub fn ref_label(kind: &str, name: &str) -> String {
        format!("{kind} {name}")
    }
    /// The current branch's label, marked with a check: "✓ main".
    pub fn current_ref(name: &str) -> String {
        format!("✓ {name}")
    }
    /// The full author and committer dates of a row.
    pub fn dates(authored: &str, committed: &str) -> String {
        format!("Authored {authored}\nCommitted {committed}")
    }
}

/// The changes pane (UI-14 to UI-16, UI-34).
pub mod changes {
    use super::plural;

    pub const NO_SELECTION: &str =
        "Select a commit or the uncommitted changes to see what changed.";
    pub fn cannot_read(error: &str) -> String {
        format!("Cannot read the changes: {error}")
    }
    pub const UNCOMMITTED: &str = "Uncommitted changes";
    /// The commit header (UI-14).
    pub const COMMIT: &str = "Commit";
    pub const AUTHOR: &str = "Author";
    pub const COMMITTER: &str = "Committer";
    /// A person as the header shows them: "Ana <ana@example.com>".
    pub fn person(name: &str, email: &str) -> String {
        format!("{name} <{email}>")
    }
    /// A header line as a screen reader reads it: "Author: Ana …".
    pub fn field(name: &str, value: &str) -> String {
        format!("{name}: {value}")
    }
    pub const MESSAGE: &str = "Message";
    pub fn parents(count: usize) -> &'static str {
        if count == 1 { "Parent" } else { "Parents" }
    }
    pub const NO_PARENTS: &str = "none (the first commit)";
    pub fn select(hash: &str) -> String {
        format!("Select {hash}")
    }
    /// The parent selector of a merge commit (UI-16).
    pub const COMPARE_WITH: &str = "Compare with";
    pub const COMPARE_WITH_PARENT: &str = "Compare with parent";
    pub const FIRST_PARENT: &str = "First parent";
    pub fn parent(number: usize) -> String {
        format!("Parent {number}")
    }
    /// A parent choice with its short hash: "First parent (1a2b3c4d5e)".
    pub fn parent_choice(parent: &str, hash: &str) -> String {
        format!("{parent} ({hash})")
    }
    /// The files header and its toggle (UI-14).
    pub fn files(count: usize) -> String {
        plural(u32::try_from(count).unwrap_or(u32::MAX), "file", "files")
    }
    pub const FILE_LIST: &str = "File list";
    pub const FLAT: &str = "Flat";
    pub const TREE: &str = "Tree";
    pub const CHANGED_FILES: &str = "Changed files";
    /// The lists of uncommitted changes (UI-15, OPS-3).
    pub const UNMERGED: &str = "Unmerged";
    pub const STAGED: &str = "Staged";
    pub const UNSTAGED: &str = "Unstaged";
    /// A section or a repo's part of it, with its file count: "Staged (2)".
    pub fn counted(name: &str, count: usize) -> String {
        format!("{name} ({count})")
    }
    /// Why a repo's files cannot be changed in a merged view (GRP-8).
    pub const BLOCKED: &str = "blocked";
    /// A file's status, by kind; the list shows git's letter for it.
    pub const ADDED: &str = "Added";
    pub const MODIFIED: &str = "Modified";
    pub const DELETED: &str = "Deleted";
    pub const RENAMED: &str = "Renamed";
    pub const TYPE_CHANGED: &str = "Type changed";
    pub const UNTRACKED: &str = "Untracked";
    pub const CONFLICT: &str = "Conflict";
    /// The letters the list shows for each status, as git writes them.
    pub const ADDED_LETTER: &str = "A";
    pub const MODIFIED_LETTER: &str = "M";
    pub const DELETED_LETTER: &str = "D";
    pub const RENAMED_LETTER: &str = "R";
    pub const TYPE_CHANGED_LETTER: &str = "T";
    pub const UNTRACKED_LETTER: &str = "U";
    pub const CONFLICT_LETTER: &str = "!";
    /// How each side changed an unmerged file.
    pub const BOTH_DELETED: &str = "both deleted";
    pub const ADDED_BY_US: &str = "added by us";
    pub const DELETED_BY_THEM: &str = "deleted by them";
    pub const ADDED_BY_THEM: &str = "added by them";
    pub const DELETED_BY_US: &str = "deleted by us";
    pub const BOTH_ADDED: &str = "both added";
    pub const BOTH_MODIFIED: &str = "both modified";
    pub fn renamed_from(path: &str) -> String {
        format!("Renamed from {path}")
    }
    /// The old path after a renamed file's name: "← old.txt".
    pub fn old_path(path: &str) -> String {
        format!("← {path}")
    }
    pub fn conflict_kind(kind: &str) -> String {
        format!("Conflict: {kind}")
    }
    /// The conflict kind after an unmerged file's name: "(both modified)".
    pub fn conflict_mark(kind: &str) -> String {
        format!("({kind})")
    }
    /// A file's line counts as it shows them: "+3 −1".
    pub fn counts_mark(added: u32, removed: u32) -> String {
        format!("+{added} −{removed}")
    }
    /// A file's line counts as a screen reader reads them.
    pub fn counts(added: u32, removed: u32) -> String {
        format!(
            "{} added, {} removed",
            plural(added, "line", "lines"),
            removed
        )
    }
    /// The diff area (M6-13 fills it).
    pub const DIFF: &str = "Diff";
    pub const NO_FILE: &str = "No file changed.";
    /// A renamed file in the diff's header: "old.txt → new.txt".
    pub fn renamed(old: &str, new: &str) -> String {
        format!("{old} → {new}")
    }
    pub fn mode(old: &str, new: &str) -> String {
        format!("mode {old} → {new}")
    }
    /// The pane header's diff options (UI-17).
    pub const DIFF_OPTIONS: &str = "Diff options";
    pub const DIFF_MODE: &str = "Diff mode";
    pub const INLINE: &str = "Inline";
    pub const SIDE_BY_SIDE: &str = "Side by side";
    pub const IGNORE_WHITESPACE: &str = "Ignore whitespace";
    pub const WRAP_LINES: &str = "Wrap lines";
    pub const CONTEXT: &str = "Context";
    pub const CONTEXT_THREE: &str = "3 lines";
    pub const CONTEXT_TEN: &str = "10 lines";
    pub const WHOLE_FILE: &str = "Whole file";
    /// The diff's lines, as a screen reader names them.
    pub fn diff_of(path: &str) -> String {
        format!("Diff of {path}")
    }
    pub fn cannot_read_diff(error: &str) -> String {
        format!("Cannot read the diff: {error}")
    }
    pub const HUNKS_IGNORE_WHITESPACE: &str = "Hunks can be staged with \"Ignore whitespace\" off.";
    pub const NO_LINES: &str = "No lines changed.";
    /// That the old file, the new one or both end without a newline.
    pub fn no_newline(old: bool, new: bool) -> String {
        let sides = match (old, new) {
            (true, true) => "old and new",
            (true, false) => "old",
            _ => "new",
        };
        format!("No newline at the end of the {sides} file.")
    }
    /// A hunk's header as a screen reader reads it: "Hunk 1: @@ -1,2 +1,3 @@".
    pub fn hunk(number: usize, header: &str) -> String {
        format!("Hunk {number}: {header}")
    }
    /// Folded unchanged lines in side-by-side mode, and their button.
    pub fn folded(lines: usize) -> String {
        format!(
            "⋯ {} ⋯",
            plural(
                u32::try_from(lines).unwrap_or(u32::MAX),
                "unchanged line",
                "unchanged lines"
            )
        )
    }
    pub fn unfold(lines: usize) -> String {
        format!(
            "Show {}",
            plural(
                u32::try_from(lines).unwrap_or(u32::MAX),
                "unchanged line",
                "unchanged lines"
            )
        )
    }
    /// What a diff line is, as a screen reader reads it before the line.
    pub const LINE_ADDED: &str = "Added";
    pub const LINE_REMOVED: &str = "Removed";
    pub const LINE_UNCHANGED: &str = "Unchanged";
    /// The signs before an added and a removed line.
    pub const SIGN_ADDED: &str = "+";
    pub const SIGN_REMOVED: &str = "−";
    /// A line as a screen reader reads it: "Added, new 3: text".
    pub fn line(kind: &str, old: Option<u32>, new: Option<u32>, text: &str) -> String {
        let numbers: Vec<String> = [("old", old), ("new", new)]
            .into_iter()
            .filter_map(|(side, number)| number.map(|number| format!("{side} {number}")))
            .collect();
        format!("{kind}, {}: {text}", numbers.join(", "))
    }
    /// Both sides of a side-by-side row, as a screen reader reads them.
    pub fn pair(old: Option<&str>, new: Option<&str>) -> String {
        [old, new]
            .into_iter()
            .flatten()
            .collect::<Vec<_>>()
            .join("; ")
    }
    /// The diffs that are not text (UI-18). A size under 1 KB, and the
    /// units of larger ones.
    pub fn bytes(count: u64) -> String {
        format!("{count} B")
    }
    pub const SIZE_UNITS: [&str; 3] = ["KB", "MB", "GB"];
    /// A size of `value` in `unit`, with one decimal: "1.2 KB".
    pub fn size(value: f64, unit: &str) -> String {
        format!("{value:.1} {unit}")
    }
    /// A side that does not exist.
    pub const NONE: &str = "none";
    pub fn binary(before: &str, after: &str) -> String {
        format!("Binary file changed. Before: {before}, after: {after}.")
    }
    pub const BEFORE: &str = "Before";
    pub const AFTER: &str = "After";
    /// An image side's caption: "Before: PNG, 1.2 KB".
    pub fn image_caption(side: &str, what: &str) -> String {
        format!("{side}: {what}")
    }
    pub fn image_kind(format: &str, size: &str) -> String {
        format!("{format}, {size}")
    }
    /// The image itself, as a screen reader names it.
    pub fn image(side: &str) -> String {
        format!("{side} image")
    }
    pub const PNG: &str = "PNG";
    pub const JPEG: &str = "JPEG";
    pub const GIF: &str = "GIF";
    pub const WEBP: &str = "WEBP";
    pub const BMP: &str = "BMP";
    pub fn large(added: u32, removed: u32) -> String {
        format!(
            "This diff is large: {} lines added and {} removed. Showing it may take a while.",
            grouped(added),
            grouped(removed)
        )
    }
    pub const LOAD_DIFF: &str = "Load diff";
    pub const UNTRACKED_FOLDER: &str = "An untracked folder. Its files are not read.";
    pub const SUBMODULE_BEFORE: &str = "Submodule commit before";
    pub const SUBMODULE_AFTER: &str = "Submodule commit after";
    /// A submodule's commit as a screen reader reads it.
    pub fn submodule(side: &str, commit: &str) -> String {
        format!("{side}: {commit}")
    }

    /// `count` with its thousands apart, as rc.1's `toLocaleString`
    /// showed them in English: "20,001".
    fn grouped(count: u32) -> String {
        let digits = count.to_string();
        let mut out = String::with_capacity(digits.len() + digits.len() / 3);
        for (at, digit) in digits.chars().enumerate() {
            if at > 0 && (digits.len() - at).is_multiple_of(3) {
                out.push(',');
            }
            out.push(digit);
        }
        out
    }
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
