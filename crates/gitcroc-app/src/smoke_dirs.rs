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

/// Pass the debug port to WebView2 itself; Wry sets explicit browser args.
pub(crate) fn configure(config: &mut tauri::utils::config::Config, dirs: Option<&FixtureDirs>) {
    for window in &mut config.app.windows {
        window.additional_browser_args =
            Some("--remote-debugging-port=9222 --remote-debugging-address=127.0.0.1".to_owned());
        if let Some(dirs) = dirs {
            window.data_directory = Some(dirs.0.join("webview"));
        }
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

    #[test]
    fn plat_2_smoke_configures_the_native_webview_with_its_fixture_and_loopback_port() {
        let root = tempfile::tempdir().unwrap();
        let dirs = FixtureDirs(root.path().to_owned());
        let mut config = tauri::utils::config::Config::default();
        config.app.windows.push(Default::default());
        configure(&mut config, Some(&dirs));
        let window = &config.app.windows[0];
        let args = window.additional_browser_args.as_ref().unwrap();
        assert!(args.contains("--remote-debugging-port=9222"));
        assert!(args.contains("--remote-debugging-address=127.0.0.1"));
        assert_eq!(window.data_directory, Some(root.path().join("webview")));
    }
}
