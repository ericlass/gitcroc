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
}

impl AppInfo {
    /// The build this code is part of.
    pub fn current() -> Self {
        Self {
            name: "gitcroc".to_owned(),
            version: env!("CARGO_PKG_VERSION").to_owned(),
            os: std::env::consts::OS.to_owned(),
            arch: std::env::consts::ARCH.to_owned(),
        }
    }
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
    }
}
