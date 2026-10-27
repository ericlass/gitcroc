//! macOS: the file manager, the browser and the terminal, all through
//! `open`, and the package.

use std::path::Path;

use gitcroc_core::Package;

use super::{FileManager, Installation, Launch, Terminal, WebBrowser};

/// `open`, which shows a folder in Finder.
#[derive(Debug, Clone, Copy, Default)]
pub struct Finder;

impl FileManager for Finder {
    fn show(path: &Path) -> Launch {
        Launch::new("open", [path.into()], path.to_owned())
    }
}

/// Terminal.app, opened in the folder.
#[derive(Debug, Clone, Copy, Default)]
pub struct TerminalApp;

impl Terminal for TerminalApp {
    fn open_in(path: &Path) -> Vec<Launch> {
        let args = ["-a".into(), "Terminal".into(), path.into()];
        vec![Launch::new("open", args, path.to_owned())]
    }
}

/// `open`, which opens a URL in the default browser.
#[derive(Debug, Clone, Copy, Default)]
pub struct Open;

impl WebBrowser for Open {
    fn open(url: &str) -> Launch {
        Launch::new("open", [url.into()], std::env::temp_dir())
    }
}

/// The `.app` (see [`super::package`]).
#[derive(Debug, Clone, Copy, Default)]
pub struct MacInstallation;

impl Installation for MacInstallation {
    fn package() -> Package {
        super::package::macos_package(super::package::bundle_kind())
    }
}
