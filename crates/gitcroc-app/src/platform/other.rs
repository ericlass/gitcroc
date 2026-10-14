//! macOS and Windows: the webview's system fonts already are the
//! desktop's, so none are read (UI-26).

use super::DesktopFontSource;
use crate::ipc::DesktopFonts;

#[derive(Debug, Clone, Copy, Default)]
pub struct NoDesktopFonts;

impl DesktopFontSource for NoDesktopFonts {
    fn read(&self) -> DesktopFonts {
        DesktopFonts::default()
    }
}
