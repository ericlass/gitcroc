//! Linux: the desktop's fonts from GTK and GNOME (UI-26), the file manager,
//! the browser, the terminal and the package.

use std::path::Path;

use gtk::gio;
use gtk::gio::prelude::SettingsExtManual;
use gtk::prelude::GtkSettingsExt;

use gitcroc_core::Package;

use super::{DesktopFontSource, FileManager, Installation, Launch, Terminal, WebBrowser};
use crate::ipc::DesktopFonts;

/// Reads the fonts from GTK's settings and GNOME's schema.
#[derive(Debug, Clone, Copy, Default)]
pub struct GtkFonts;

impl DesktopFontSource for GtkFonts {
    /// E.g. `Ubuntu` from GTK's `Ubuntu 11`; `None` for what the desktop
    /// does not tell.
    fn read(&self) -> DesktopFonts {
        let ui = gtk::Settings::default().and_then(|settings| settings.gtk_font_name());
        DesktopFonts {
            ui: ui.and_then(|name| font_family(&name)),
            code: gnome_monospace_font().and_then(|name| font_family(&name)),
        }
    }
}

/// GNOME's monospace font, e.g. `JetBrains Mono NL 11`. GTK has no setting
/// for it. `None` where the GNOME schema is not installed: GIO aborts the
/// process on a schema it does not know, so it is looked up first.
pub fn gnome_monospace_font() -> Option<String> {
    const SCHEMA: &str = "org.gnome.desktop.interface";
    const KEY: &str = "monospace-font-name";
    let schema = gio::SettingsSchemaSource::default()?.lookup(SCHEMA, true)?;
    if !schema.has_key(KEY) {
        return None;
    }
    let settings = gio::Settings::new_full(&schema, None::<&gio::SettingsBackend>, None);
    Some(settings.get::<String>(KEY))
}

/// The family of a Pango font description such as `Noto Sans Bold 10`.
pub fn font_family(description: &str) -> Option<String> {
    let family = gtk::pango::FontDescription::from_string(description).family()?;
    let family = family.trim();
    (!family.is_empty()).then(|| family.to_owned())
}

/// `xdg-open`, which opens a folder in the desktop's file manager.
#[derive(Debug, Clone, Copy, Default)]
pub struct XdgOpen;

impl FileManager for XdgOpen {
    fn show(path: &Path) -> Launch {
        Launch::new("xdg-open", [path.into()], path.to_owned())
    }
}

impl WebBrowser for XdgOpen {
    fn open(url: &str) -> Launch {
        Launch::new("xdg-open", [url.into()], std::env::temp_dir())
    }
}

/// The `.deb`, the `.rpm` or the AppImage (see [`super::package`]).
#[derive(Debug, Clone, Copy, Default)]
pub struct LinuxInstallation;

impl Installation for LinuxInstallation {
    fn package() -> Package {
        super::package::linux_package(
            tauri::utils::platform::bundle_type(),
            std::env::var_os("APPIMAGE").as_deref(),
        )
    }
}

/// The Debian alternative for the desktop's terminal, then the terminals of
/// GNOME and KDE, then xterm, each started in the folder.
#[derive(Debug, Clone, Copy, Default)]
pub struct Terminals;

impl Terminal for Terminals {
    fn open_in(path: &Path) -> Vec<Launch> {
        ["x-terminal-emulator", "gnome-terminal", "konsole", "xterm"]
            .into_iter()
            .map(|program| Launch::new(program, [], path.to_owned()))
            .collect()
    }
}
