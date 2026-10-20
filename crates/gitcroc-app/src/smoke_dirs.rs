//! Smoke-only directories: Windows Known Folders ignore APPDATA overrides.

use std::path::PathBuf;

use gitcroc_core::{CacheDir, ConfigDir, LogDir};

pub(crate) struct FixtureDirs(PathBuf);

#[cfg(feature = "e2e")]
impl FixtureDirs {
    pub(crate) fn from_env() -> Option<Self> {
        std::env::var_os("GITCROC_E2E_DIR").map(|path| Self(PathBuf::from(path)))
    }
}

impl ConfigDir for FixtureDirs {
    fn config_dir(&self) -> Option<PathBuf> {
        Some(self.0.join("config").join("gitcroc"))
    }
}

impl CacheDir for FixtureDirs {
    fn cache_dir(&self) -> Option<PathBuf> {
        Some(self.0.join("cache").join("gitcroc"))
    }
}

impl LogDir for FixtureDirs {
    fn log_dir(&self) -> Option<PathBuf> {
        Some(self.0.join("logs").join("gitcroc"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gitcroc_core::{CacheDir, ConfigDir, LogDir, SettingsStore, Workspaces};

    #[test]
    fn plat_2_smoke_reads_its_seeded_workspace_and_keeps_all_data_in_the_fixture() {
        let root = tempfile::tempdir().unwrap();
        let dirs = FixtureDirs(root.path().to_owned());
        assert_eq!(
            dirs.config_dir().unwrap(),
            root.path().join("config/gitcroc")
        );
        assert_eq!(dirs.cache_dir().unwrap(), root.path().join("cache/gitcroc"));
        assert_eq!(dirs.log_dir().unwrap(), root.path().join("logs/gitcroc"));
        let settings = std::sync::Arc::new(SettingsStore::load(&dirs));
        let workspaces = Workspaces::load_with(&dirs, settings.clone());
        let id = workspaces.create("Smoke").unwrap();
        let loaded = Workspaces::load_with(&dirs, settings);
        assert_eq!(loaded.view(&id).unwrap().name, "Smoke");
    }
}
