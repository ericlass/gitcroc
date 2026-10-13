//! The desktop's UI and monospace fonts (UI-26): GTK names it with style and size, the UI
//! needs the family alone.
#![cfg(target_os = "linux")]

use gitcroc_app::ipc::font_family;

#[test]
fn ui_26_the_desktop_font_is_read_as_its_family() {
    assert_eq!(font_family("Ubuntu 11").as_deref(), Some("Ubuntu"));
    assert_eq!(
        font_family("Noto Sans Bold 10").as_deref(),
        Some("Noto Sans")
    );
    assert_eq!(font_family("Cantarell").as_deref(), Some("Cantarell"));
    assert_eq!(
        font_family("JetBrains Mono NL 11").as_deref(),
        Some("JetBrains Mono NL")
    );
    assert_eq!(font_family("11"), None);
    assert_eq!(font_family(""), None);
}

/// Where GNOME's schema is installed this reads the setting; where it is
/// not, it must return `None` instead of letting GIO abort the process.
#[test]
fn ui_26_the_monospace_font_is_read_with_or_without_gnome() {
    if let Some(name) = gitcroc_app::ipc::gnome_monospace_font() {
        assert!(font_family(&name).is_some(), "{name:?} has no family");
    }
}
