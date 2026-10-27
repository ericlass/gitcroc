//! Linux: the file manager, the browser, the terminal, the package and the
//! desktop's fonts.

use std::path::Path;
use std::time::Duration;

use ashpd::desktop::settings::Settings;
use gitcroc_core::Package;

use super::{
    DesktopFontSource, DesktopFonts, FileManager, Installation, Launch, Terminal, WebBrowser,
};

// gix's temporary Rust allocations must not remain in glibc's per-thread
// arenas after a refresh. Purge them asynchronously while the app is idle
// (section 9, M5-17).
#[global_allocator]
static ALLOCATOR: tikv_jemallocator::Jemalloc = tikv_jemallocator::Jemalloc;

/// How long the start waits for the settings portal.
const PORTAL_TIMEOUT: Duration = Duration::from_millis(500);

/// Reads the fonts through the settings portal (UI-26): GNOME's settings,
/// else KDE's. `None` for what the desktop does not tell, or when no portal
/// answers within [`PORTAL_TIMEOUT`].
#[derive(Debug, Clone, Copy, Default)]
pub struct PortalFonts;

impl DesktopFontSource for PortalFonts {
    fn read(&self) -> DesktopFonts {
        async_io::block_on(futures_lite::future::or(read_portal(), async {
            async_io::Timer::after(PORTAL_TIMEOUT).await;
            DesktopFonts::default()
        }))
    }
}

async fn read_portal() -> DesktopFonts {
    let Ok(settings) = Settings::new().await else {
        return DesktopFonts::default();
    };
    let read =
        async |namespace: &str, key: &str| settings.read::<String>(namespace, key).await.ok();
    const GNOME: &str = "org.gnome.desktop.interface";
    const KDE: &str = "org.kde.kdeglobals.General";
    let ui = match read(GNOME, "font-name")
        .await
        .as_deref()
        .and_then(font_family)
    {
        Some(family) => Some(family),
        None => read(KDE, "font").await.as_deref().and_then(kde_font_family),
    };
    let code = match read(GNOME, "monospace-font-name")
        .await
        .as_deref()
        .and_then(font_family)
    {
        Some(family) => Some(family),
        None => read(KDE, "fixed")
            .await
            .as_deref()
            .and_then(kde_font_family),
    };
    DesktopFonts { ui, code }
}

/// The words of a Pango font description after its family: style, variant,
/// weight, stretch and gravity, as Pango parses them.
const PANGO_OPTIONS: [&str; 52] = [
    "normal",
    "roman",
    "oblique",
    "italic",
    "small-caps",
    "all-small-caps",
    "petite-caps",
    "all-petite-caps",
    "unicase",
    "title-caps",
    "thin",
    "ultra-light",
    "ultralight",
    "extra-light",
    "extralight",
    "light",
    "semi-light",
    "semilight",
    "demi-light",
    "demilight",
    "book",
    "regular",
    "medium",
    "semi-bold",
    "semibold",
    "demi-bold",
    "demibold",
    "bold",
    "ultra-bold",
    "ultrabold",
    "extra-bold",
    "extrabold",
    "heavy",
    "black",
    "ultra-heavy",
    "ultraheavy",
    "extra-heavy",
    "extraheavy",
    "ultra-condensed",
    "extra-condensed",
    "condensed",
    "semi-condensed",
    "semi-expanded",
    "expanded",
    "extra-expanded",
    "ultra-expanded",
    "not-rotated",
    "south",
    "upside-down",
    "north",
    "rotated-left",
    "rotated-right",
];

/// The family of a Pango font description such as GNOME's `Noto Sans Bold
/// 10`: its first family, without the size, variations and options after it.
pub fn font_family(description: &str) -> Option<String> {
    let mut words: Vec<&str> = description.split_whitespace().collect();
    // Variations, then the size, then the options, from the end.
    if words.last().is_some_and(|word| word.starts_with('@')) {
        words.pop();
    }
    if words.last().is_some_and(|word| {
        let number = word.strip_suffix("px").unwrap_or(word);
        !number.is_empty() && number.parse::<f64>().is_ok()
    }) {
        words.pop();
    }
    while words
        .last()
        .is_some_and(|word| PANGO_OPTIONS.contains(&word.to_ascii_lowercase().as_str()))
    {
        words.pop();
    }
    let families = words.join(" ");
    let family = families.split(',').next().unwrap_or_default().trim();
    (!family.is_empty()).then(|| family.to_owned())
}

/// The family of a Qt font string such as KDE's `Noto Sans,10,-1,5,50,0,0,0,0,0`.
pub fn kde_font_family(description: &str) -> Option<String> {
    let family = description.split(',').next().unwrap_or_default().trim();
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
            super::package::bundle_kind(),
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

#[cfg(test)]
mod tests {
    use super::{font_family, kde_font_family};

    /// GNOME names a font with its options and size, KDE with Qt's fields
    /// after commas; the UI needs the family alone.
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
        assert_eq!(
            font_family("Source Code Pro Semi-Bold Italic 10.5").as_deref(),
            Some("Source Code Pro")
        );
        assert_eq!(
            font_family("Adwaita Sans 11px").as_deref(),
            Some("Adwaita Sans")
        );
        assert_eq!(font_family("Inter, Cantarell 11").as_deref(), Some("Inter"));
        assert_eq!(font_family("11"), None);
        assert_eq!(font_family(""), None);

        assert_eq!(
            kde_font_family("Noto Sans,10,-1,5,50,0,0,0,0,0").as_deref(),
            Some("Noto Sans")
        );
        assert_eq!(
            kde_font_family("Hack,10,-1,5,400,0,0,0,0,0,0,0,0,0,0,1").as_deref(),
            Some("Hack")
        );
        assert_eq!(kde_font_family(",10"), None);
    }

    #[test]
    fn d2_app_allocator_purges_freed_arenas_in_the_background() {
        assert!(tikv_jemalloc_ctl::background_thread::read().unwrap());
    }
}
