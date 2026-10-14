//! macOS: the file manager and the terminal, both through `open`.

use std::path::Path;

use super::{FileManager, Launch, Terminal};

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
