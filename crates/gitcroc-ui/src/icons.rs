//! The UI's own icons, built into the binary. gpui-kit's `assets` feature
//! is off (M6-06): gitcroc draws only the icons it needs.

use std::borrow::Cow;

use gpui_kit::{AssetSource, Result, SharedString};

pub const WINDOW_MINIMIZE: &str = "icons/window-minimize.svg";
pub const WINDOW_MAXIMIZE: &str = "icons/window-maximize.svg";
pub const WINDOW_RESTORE: &str = "icons/window-restore.svg";
pub const WINDOW_CLOSE: &str = "icons/window-close.svg";
pub const CHEVRON_RIGHT: &str = "icons/chevron-right.svg";
pub const CHEVRON_DOWN: &str = "icons/chevron-down.svg";
pub const CHECK: &str = "icons/check.svg";
pub const WARNING: &str = "icons/warning.svg";
pub const CONFLICT: &str = "icons/conflict.svg";
pub const SPINNER: &str = "icons/spinner.svg";

const ICONS: [(&str, &[u8]); 10] = [
    (
        WINDOW_MINIMIZE,
        include_bytes!("../assets/icons/window-minimize.svg"),
    ),
    (
        WINDOW_MAXIMIZE,
        include_bytes!("../assets/icons/window-maximize.svg"),
    ),
    (
        WINDOW_RESTORE,
        include_bytes!("../assets/icons/window-restore.svg"),
    ),
    (
        WINDOW_CLOSE,
        include_bytes!("../assets/icons/window-close.svg"),
    ),
    (
        CHEVRON_RIGHT,
        include_bytes!("../assets/icons/chevron-right.svg"),
    ),
    (
        CHEVRON_DOWN,
        include_bytes!("../assets/icons/chevron-down.svg"),
    ),
    (CHECK, include_bytes!("../assets/icons/check.svg")),
    (WARNING, include_bytes!("../assets/icons/warning.svg")),
    (CONFLICT, include_bytes!("../assets/icons/conflict.svg")),
    (SPINNER, include_bytes!("../assets/icons/spinner.svg")),
];

/// The icons, as GPUI's asset source.
pub struct Icons;

impl AssetSource for Icons {
    fn load(&self, path: &str) -> Result<Option<Cow<'static, [u8]>>> {
        Ok(ICONS
            .iter()
            .find(|(icon, _)| *icon == path)
            .map(|(_, bytes)| Cow::Borrowed(*bytes)))
    }

    fn list(&self, path: &str) -> Result<Vec<SharedString>> {
        Ok(ICONS
            .iter()
            .filter(|(icon, _)| icon.starts_with(path))
            .map(|(icon, _)| SharedString::from(*icon))
            .collect())
    }
}
