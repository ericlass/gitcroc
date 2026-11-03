//! The keyboard shortcuts (UI-22): GPUI actions, and the one table their
//! bindings come from, the palette's shortcut column (M6-24) and the user
//! documentation, which `keys::tests` checks. "Secondary" is Cmd on macOS
//! and Ctrl elsewhere. None shadows a key of gpui-component's text input or
//! code editor, which `keys::tests` checks against their keymaps. Keys that
//! belong to one place stay with it, in that view's own key context:
//! Shift+F10 and the context menu key for menus, the arrow keys in lists
//! and menus, Ctrl/Cmd+Enter in the commit box, the conflict editor's Alt
//! keys.

use std::rc::Rc;

use gpui_kit::{Action, App, KeyBinding, KeyBindingContextPredicate, actions};

use crate::strings;

actions!(
    gitcroc,
    [
        /// Opens the command palette, or closes it (UI-22).
        TogglePalette,
        /// Moves the focus to the next region of the window (UI-22).
        NextRegion,
        /// Moves the focus to the previous region.
        PreviousRegion,
        /// Opens the Settings dialog (UI-22).
        OpenSettings,
        /// Shows the app's name and version (the macOS App menu).
        About,
        /// Quits the app (the macOS App menu).
        Quit,
        /// Minimizes the window (the macOS Window menu).
        Minimize,
        /// Maximizes or restores the window (the macOS Window menu).
        Zoom,
        /// Activates the tab right of the active one (UI-22).
        NextTab,
        /// Activates the tab left of the active one.
        PreviousTab,
        /// Moves the active tab one place right.
        MoveTabRight,
        /// Moves the active tab one place left.
        MoveTabLeft,
        /// Goes to the next hunk of the diff (UI-22).
        NextHunk,
        /// Goes to the previous hunk of the diff.
        PreviousHunk,
    ]
);

/// The key context of the window's root view: the app-wide shortcuts work
/// wherever the focus is in it.
pub const WINDOW_CONTEXT: &str = "GitcrocWindow";

/// A key with the modifiers held. `key` is GPUI's name for it, such as `f6`,
/// `k` or `pagedown`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Keys {
    pub key: &'static str,
    /// Cmd on macOS, Ctrl elsewhere.
    pub secondary: bool,
    pub shift: bool,
    pub alt: bool,
}

impl Keys {
    const fn plain(key: &'static str) -> Self {
        Self {
            key,
            secondary: false,
            shift: false,
            alt: false,
        }
    }

    const fn secondary(self) -> Self {
        Self {
            secondary: true,
            ..self
        }
    }

    const fn shift(self) -> Self {
        Self {
            shift: true,
            ..self
        }
    }

    /// The keystroke as GPUI parses it, such as `secondary-shift-pagedown`.
    pub fn keystroke(&self) -> String {
        let mut parts = Vec::new();
        if self.secondary {
            parts.push("secondary");
        }
        if self.alt {
            parts.push("alt");
        }
        if self.shift {
            parts.push("shift");
        }
        parts.push(self.key);
        parts.join("-")
    }

    /// How the keys are written on the platform, as "Ctrl+Shift+Page Down".
    pub fn label(&self, mac: bool) -> String {
        let key = strings::keys::name(self.key)
            .map(str::to_owned)
            .unwrap_or_else(|| self.key.to_uppercase());
        let mut parts = Vec::new();
        if self.secondary {
            parts.push(if mac {
                strings::keys::CMD
            } else {
                strings::keys::CTRL
            });
        }
        if self.shift {
            parts.push(strings::keys::SHIFT);
        }
        if self.alt {
            parts.push(if mac {
                strings::keys::OPTION
            } else {
                strings::keys::ALT
            });
        }
        parts.push(&key);
        strings::keys::join(&parts)
    }
}

/// An app-wide shortcut.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Shortcut {
    Palette,
    NextRegion,
    PreviousRegion,
    Settings,
    NextTab,
    PreviousTab,
    MoveTabRight,
    MoveTabLeft,
    NextHunk,
    PreviousHunk,
}

impl Shortcut {
    /// What it does, as the documentation and the palette say it.
    pub fn label(self) -> &'static str {
        match self {
            Self::Palette => strings::keys::PALETTE,
            Self::NextRegion => strings::keys::NEXT_REGION,
            Self::PreviousRegion => strings::keys::PREVIOUS_REGION,
            Self::Settings => strings::keys::SETTINGS,
            Self::NextTab => strings::keys::NEXT_TAB,
            Self::PreviousTab => strings::keys::PREVIOUS_TAB,
            Self::MoveTabRight => strings::keys::MOVE_TAB_RIGHT,
            Self::MoveTabLeft => strings::keys::MOVE_TAB_LEFT,
            Self::NextHunk => strings::keys::NEXT_HUNK,
            Self::PreviousHunk => strings::keys::PREVIOUS_HUNK,
        }
    }

    /// The action it dispatches.
    pub fn action(self) -> Box<dyn Action> {
        match self {
            Self::Palette => Box::new(TogglePalette),
            Self::NextRegion => Box::new(NextRegion),
            Self::PreviousRegion => Box::new(PreviousRegion),
            Self::Settings => Box::new(OpenSettings),
            Self::NextTab => Box::new(NextTab),
            Self::PreviousTab => Box::new(PreviousTab),
            Self::MoveTabRight => Box::new(MoveTabRight),
            Self::MoveTabLeft => Box::new(MoveTabLeft),
            Self::NextHunk => Box::new(NextHunk),
            Self::PreviousHunk => Box::new(PreviousHunk),
        }
    }
}

/// The shortcut of a palette operation, shown in its row (UI-22).
pub fn palette_shortcut(op: gitcroc_core::PaletteOp) -> Option<Keys> {
    use gitcroc_core::PaletteOp;
    let shortcut = match op {
        PaletteOp::Settings => Shortcut::Settings,
        PaletteOp::NextTab => Shortcut::NextTab,
        PaletteOp::PreviousTab => Shortcut::PreviousTab,
        _ => return None,
    };
    KEYMAP
        .iter()
        .find(|(known, _)| *known == shortcut)
        .map(|(_, keys)| *keys)
}

/// The app-wide shortcuts and their keys, in the order `keyboard.md`
/// lists them.
pub const KEYMAP: [(Shortcut, Keys); 10] = [
    (Shortcut::Palette, Keys::plain("k").secondary()),
    (Shortcut::NextRegion, Keys::plain("f6")),
    (Shortcut::PreviousRegion, Keys::plain("f6").shift()),
    (Shortcut::Settings, Keys::plain(",").secondary()),
    (Shortcut::NextTab, Keys::plain("pagedown").secondary()),
    (Shortcut::PreviousTab, Keys::plain("pageup").secondary()),
    (
        Shortcut::MoveTabRight,
        Keys::plain("pagedown").secondary().shift(),
    ),
    (
        Shortcut::MoveTabLeft,
        Keys::plain("pageup").secondary().shift(),
    ),
    (Shortcut::NextHunk, Keys::plain("f7")),
    (Shortcut::PreviousHunk, Keys::plain("f7").shift()),
];

/// Binds the shortcuts of [`KEYMAP`], and on macOS the keys of the App and
/// Window menus, which every Mac app has.
pub fn bind(cx: &mut App) {
    let context = KeyBindingContextPredicate::parse(WINDOW_CONTEXT)
        .ok()
        .map(Rc::new);
    // `ui_22_every_shortcut_is_bound` checks that none is left out.
    let mut bindings: Vec<KeyBinding> = KEYMAP
        .iter()
        .filter_map(|(shortcut, keys)| {
            let keystroke = keys.keystroke();
            KeyBinding::load(
                &keystroke,
                shortcut.action(),
                context.clone(),
                false,
                None,
                cx,
            )
            .ok()
        })
        .collect();
    if cfg!(target_os = "macos") {
        bindings.push(KeyBinding::new("cmd-q", Quit, None));
        bindings.push(KeyBinding::new("cmd-m", Minimize, None));
    }
    cx.bind_keys(bindings);
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The cells of each table row of `markdown`, code spans unwrapped.
    fn table_rows(markdown: &str) -> Vec<Vec<String>> {
        markdown
            .lines()
            .map(str::trim)
            .filter(|line| line.starts_with('|'))
            .map(|line| {
                line.trim_matches('|')
                    .split('|')
                    .map(|cell| cell.trim().replace('`', ""))
                    .collect()
            })
            .collect()
    }

    /// The rows of the table under heading `heading` of `markdown`, less its
    /// header and rule.
    fn section_rows(markdown: &str, heading: &str) -> Vec<Vec<String>> {
        let section = markdown
            .split("\n## ")
            .find(|section| section.starts_with(heading))
            .unwrap_or_else(|| panic!("no section {heading:?}"));
        table_rows(section).into_iter().skip(2).collect()
    }

    /// M5-18: the keyboard page of the user documentation lists the
    /// shortcuts of [`KEYMAP`] and no other, in its order, each with what it
    /// does and its keys on Windows and Linux and on macOS, written as the
    /// app writes them.
    #[test]
    fn ui_22_keyboard_md_and_the_keymap_list_the_same_shortcuts() {
        let page = concat!(env!("CARGO_MANIFEST_DIR"), "/../../docs/user/keyboard.md");
        let text = std::fs::read_to_string(page).unwrap();
        let documented = section_rows(&text, "App-wide shortcuts");
        let bound: Vec<Vec<String>> = KEYMAP
            .iter()
            .map(|(shortcut, keys)| {
                vec![
                    shortcut.label().to_owned(),
                    keys.label(false),
                    keys.label(true),
                ]
            })
            .collect();
        assert_eq!(documented, bound);
    }

    #[test]
    fn ui_22_no_two_shortcuts_share_keys() {
        for (at, (shortcut, keys)) in KEYMAP.iter().enumerate() {
            for (other, other_keys) in &KEYMAP[at + 1..] {
                assert_ne!(keys, other_keys, "{shortcut:?} and {other:?}");
            }
        }
    }

    /// The key contexts of gpui-component's text input and code editor: its
    /// keymaps are `input/base/state.rs` ("Input") and the code editor's
    /// search bar, `input/search.rs` ("SearchPanel").
    const EDITOR_CONTEXTS: [&str; 2] = ["Input", "SearchPanel"];

    /// A focused field or editor takes the keys its keymaps bind before the
    /// window sees them: no app-wide shortcut may be one of them, or the
    /// shortcut would stop working there.
    #[gpui::test]
    fn ui_22_no_shortcut_shadows_an_editor_key(cx: &mut gpui_kit::TestAppContext) {
        cx.update(crate::init);
        let keymap = cx.update(|cx| cx.key_bindings());
        let keymap = keymap.borrow();
        let editor: Vec<_> = keymap
            .bindings()
            .filter(|binding| {
                binding.predicate().is_some_and(|predicate| {
                    let predicate = predicate.to_string();
                    EDITOR_CONTEXTS.iter().any(|context| {
                        predicate
                            .split(|c: char| !c.is_alphanumeric())
                            .any(|word| word == *context)
                    })
                })
            })
            .collect();
        assert!(!editor.is_empty(), "the editors' keymaps are not bound");
        for (shortcut, _) in KEYMAP {
            let action = shortcut.action();
            for ours in keymap.bindings_for_action(action.as_ref()) {
                let shadowed: Vec<String> = editor
                    .iter()
                    .filter(|theirs| theirs.match_keystrokes(ours.keystrokes()).is_some())
                    .map(|theirs| format!("{} in {:?}", theirs.action().name(), theirs.predicate()))
                    .collect();
                assert!(shadowed.is_empty(), "{shortcut:?} is {shadowed:?}");
            }
        }
    }

    #[test]
    fn ui_22_keys_are_written_as_the_platform_writes_them() {
        let keys = Keys {
            key: "pagedown",
            secondary: true,
            shift: true,
            alt: false,
        };
        assert_eq!(keys.label(false), "Ctrl+Shift+Page Down");
        assert_eq!(keys.label(true), "Cmd+Shift+Page Down");
        assert_eq!(keys.keystroke(), "secondary-shift-pagedown");
        assert_eq!(Keys::plain("f6").shift().label(false), "Shift+F6");
        assert_eq!(Keys::plain("k").label(true), "K");
    }
}
