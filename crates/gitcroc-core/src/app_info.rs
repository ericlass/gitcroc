use serde::{Deserialize, Serialize};

/// What the running build is and where it runs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
pub struct AppInfo {
    pub name: String,
    pub version: String,
    /// `std::env::consts::OS`, e.g. `linux`, `windows`, `macos`.
    pub os: String,
    /// `std::env::consts::ARCH`, e.g. `x86_64`, `aarch64`.
    pub arch: String,
    /// The desktop's UI font family, when the platform tells it (GTK on
    /// Linux). The UI puts it before its own list of system fonts (UI-26).
    pub ui_font: Option<String>,
    /// The desktop's monospace font family, when the platform tells it
    /// (GNOME's settings on Linux). The UI puts it before its own list of
    /// monospace fonts (UI-26).
    pub code_font: Option<String>,
}

impl AppInfo {
    /// The build this code is part of.
    pub fn current() -> Self {
        Self {
            name: "gitcroc".to_owned(),
            version: env!("CARGO_PKG_VERSION").to_owned(),
            os: std::env::consts::OS.to_owned(),
            arch: std::env::consts::ARCH.to_owned(),
            ui_font: None,
            code_font: None,
        }
    }

    /// This build, on a desktop whose UI font is `ui_font`.
    pub fn with_ui_font(self, ui_font: Option<String>) -> Self {
        Self {
            ui_font: family(ui_font),
            ..self
        }
    }

    /// This build, on a desktop whose monospace font is `code_font`.
    pub fn with_code_font(self, code_font: Option<String>) -> Self {
        Self {
            code_font: family(code_font),
            ..self
        }
    }
}

fn family(name: Option<String>) -> Option<String> {
    name.filter(|family| !family.trim().is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn current_reports_this_build() {
        let info = AppInfo::current();
        assert_eq!(info.name, "gitcroc");
        assert_eq!(info.version, env!("CARGO_PKG_VERSION"));
        assert_eq!(info.os, std::env::consts::OS);
        assert_eq!(info.arch, std::env::consts::ARCH);
        assert_eq!(info.ui_font, None);
        assert_eq!(info.code_font, None);
    }

    #[test]
    fn ui_26_a_blank_desktop_font_is_none() {
        let info = AppInfo::current();
        assert_eq!(
            info.clone().with_ui_font(Some("Ubuntu".into())).ui_font,
            Some("Ubuntu".to_owned())
        );
        assert_eq!(info.clone().with_ui_font(Some(" ".into())).ui_font, None);
        assert_eq!(
            info.clone()
                .with_code_font(Some("JetBrains Mono".into()))
                .code_font,
            Some("JetBrains Mono".to_owned())
        );
        assert_eq!(info.with_code_font(Some(String::new())).code_font, None);
    }
}
