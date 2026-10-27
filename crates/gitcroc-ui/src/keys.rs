//! The keyboard shortcuts (UI-22): GPUI actions, and the one table their
//! bindings come from, the palette's shortcut column (M6-24) and the user
//! documentation, which `keys::tests` checks. "Secondary" is Cmd on macOS
//! and Ctrl elsewhere. The table holds the shortcuts `gitcroc-ui` handles;
//! each task that ports one from `ui/src/keys.ts` adds its row. Keys that
//! belong to one place stay with it, in that view's own key context.

use std::rc::Rc;

use gpui_kit::{Action, App, KeyBinding, KeyBindingContextPredicate, actions};

use crate::strings;

actions!(
    gitcroc,
    [
        /// Moves the focus to the next region of the window (UI-22).
        NextRegion,
        /// Moves the focus to the previous region.
        PreviousRegion,
        /// Shows the app's name and version (the macOS App menu).
        About,
        /// Quits the app (the macOS App menu).
        Quit,
        /// Minimizes the window (the macOS Window menu).
        Minimize,
        /// Maximizes or restores the window (the macOS Window menu).
        Zoom,
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
    NextRegion,
    PreviousRegion,
}

impl Shortcut {
    /// What it does, as the documentation and the palette say it.
    pub fn label(self) -> &'static str {
        match self {
            Self::NextRegion => strings::keys::NEXT_REGION,
            Self::PreviousRegion => strings::keys::PREVIOUS_REGION,
        }
    }

    /// The action it dispatches.
    pub fn action(self) -> Box<dyn Action> {
        match self {
            Self::NextRegion => Box::new(NextRegion),
            Self::PreviousRegion => Box::new(PreviousRegion),
        }
    }
}

/// The app-wide shortcuts and their keys.
pub const KEYMAP: [(Shortcut, Keys); 2] = [
    (Shortcut::NextRegion, Keys::plain("f6")),
    (Shortcut::PreviousRegion, Keys::plain("f6").shift()),
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

    /// M5-18: the keyboard page of the user documentation lists every
    /// shortcut of [`KEYMAP`], as a table row with what it does and its keys
    /// on Windows and Linux and on macOS, written as the app writes them.
    /// Until M6-30 it checks the shortcuts `gitcroc-ui` has.
    #[test]
    fn ui_22_keyboard_md_lists_every_shortcut_with_its_keys() {
        let page = concat!(env!("CARGO_MANIFEST_DIR"), "/../../docs/user/keyboard.md");
        let text = std::fs::read_to_string(page).unwrap();
        let rows = table_rows(&text);
        for (shortcut, keys) in KEYMAP {
            let label = shortcut.label();
            let row = rows
                .iter()
                .find(|cells| cells[0] == label)
                .unwrap_or_else(|| panic!("no row for {label:?}"));
            assert_eq!(row[1..3], [keys.label(false), keys.label(true)], "{label}");
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
