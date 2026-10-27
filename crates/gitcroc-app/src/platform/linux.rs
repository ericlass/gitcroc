//! Linux: the file manager, the browser, the terminal and the package.

use std::path::Path;

use gitcroc_core::Package;

use super::{FileManager, Installation, Launch, Terminal, WebBrowser};

// gix's temporary Rust allocations must not remain in glibc's per-thread
// arenas after a refresh. Purge them asynchronously while the app is idle
// (section 9, M5-17).
#[global_allocator]
static ALLOCATOR: tikv_jemallocator::Jemalloc = tikv_jemallocator::Jemalloc;

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
    #[test]
    fn d2_app_allocator_purges_freed_arenas_in_the_background() {
        assert!(tikv_jemalloc_ctl::background_thread::read().unwrap());
    }
}
