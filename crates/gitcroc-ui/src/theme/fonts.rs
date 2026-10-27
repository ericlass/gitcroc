//! The UI and code fonts (UI-26): the settings' families, else the
//! desktop's, else GPUI's and gpui-component's defaults. A family that is
//! not installed never reaches GPUI, which panics when neither it nor any
//! of its fallbacks is installed.

use gpui_kit::SharedString;

use crate::strings;

/// The family to use and the families passed over because they are not
/// installed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Family {
    pub name: SharedString,
    pub missing: Vec<String>,
}

/// The first of `chosen` (the setting) and `desktop` that is installed,
/// else `default`. With an empty `installed`, which only a text system
/// that lists no fonts gives, every name is taken as it is.
pub fn family(
    chosen: Option<&str>,
    desktop: Option<&str>,
    installed: &[String],
    default: &SharedString,
) -> Family {
    let mut missing = Vec::new();
    for name in [chosen, desktop].into_iter().flatten() {
        if installed.is_empty() || installed.iter().any(|family| family == name) {
            return Family {
                name: SharedString::from(name.to_owned()),
                missing,
            };
        }
        missing.push(name.to_owned());
    }
    Family {
        name: default.clone(),
        missing,
    }
}

/// No font is installed: GPUI would panic at the first text (M6-03).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NoFonts;

impl std::fmt::Display for NoFonts {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(strings::startup::NO_FONTS)
    }
}

/// Fails when `installed`, the text system's family names, is empty.
pub fn check_installed(installed: &[String]) -> Result<(), NoFonts> {
    if installed.is_empty() {
        Err(NoFonts)
    } else {
        Ok(())
    }
}
