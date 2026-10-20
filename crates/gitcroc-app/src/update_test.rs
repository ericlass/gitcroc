//! CI-only update exercise (PLAT-4, SEC-7, D12).

use gitcroc_core::{Package, UpdateSource};

async fn attempt(
    source: &dyn UpdateSource,
    package: Package,
    before_install: impl FnOnce(&str) -> Result<(), String>,
) -> Result<Option<(String, bool)>, String> {
    let Some(found) = source.check().await? else {
        return Ok(None);
    };
    if package.installs() {
        before_install(&found.version)?;
        source.install(Box::new(|_, _| {})).await?;
    }
    Ok(Some((found.version, package.installs())))
}

#[cfg(feature = "update-test")]
pub fn resumed(app: &tauri::AppHandle, log: &gitcroc_core::AppLog) {
    use gitcroc_core::{ConfigDir, PlatformConfigDir};
    if let Some(dir) = PlatformConfigDir.config_dir() {
        let pending = dir.join("update-test.pending");
        let version = app.package_info().version.to_string();
        if std::fs::read_to_string(&pending).is_ok_and(|text| text == version) {
            // Windows exits from the plugin. Only the newly installed app
            // can acknowledge success; a failed installer cannot produce it.
            log.warn(&format!("update: installed {version}"));
            let _ = std::fs::remove_file(pending);
        }
    }
}

#[cfg(feature = "update-test")]
pub fn start(app: tauri::AppHandle) {
    use crate::platform::{Installation, PlatformInstallation};
    use gitcroc_core::{AppLog, ConfigDir, PlatformConfigDir};
    use std::sync::Arc;
    use tauri::Manager;
    tauri::async_runtime::spawn(async move {
        let log = app.state::<Arc<AppLog>>();
        let Some(dir) = PlatformConfigDir.config_dir() else {
            log.warn("update: refused (config directory unavailable)");
            return;
        };
        let pending = dir.join("update-test.pending");
        let result = attempt(
            &crate::update::source(&app),
            PlatformInstallation::package(),
            |version| std::fs::write(&pending, version).map_err(|error| error.to_string()),
        )
        .await;
        match result {
            Ok(Some((version, true))) => {
                log.warn(&format!("update: installed {version}"));
                let _ = std::fs::remove_file(pending);
                app.restart();
            }
            Ok(Some((version, false))) => log.warn(&format!("update: notify only ({version})")),
            Ok(None) => {}
            Err(reason) => {
                let _ = std::fs::remove_file(pending);
                log.warn(&format!("update: refused ({reason})"));
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use gitcroc_core::{Package, UpdateFound, UpdateFuture, UpdateProgress, UpdateSource};
    use std::sync::atomic::{AtomicUsize, Ordering};

    struct Source {
        installs: AtomicUsize,
        bad: bool,
    }
    impl UpdateSource for Source {
        fn check(&self) -> UpdateFuture<'_, Option<UpdateFound>> {
            Box::pin(async {
                Ok(Some(UpdateFound {
                    version: "0.1.1".into(),
                    date: None,
                    notes: String::new(),
                }))
            })
        }
        fn install(&self, _: UpdateProgress) -> UpdateFuture<'_, ()> {
            self.installs.fetch_add(1, Ordering::SeqCst);
            Box::pin(async {
                if self.bad {
                    Err("signature mismatch".into())
                } else {
                    Ok(())
                }
            })
        }
    }

    #[tokio::test]
    async fn plat_4_installs_each_updatable_package_through_the_source() {
        for package in [
            Package::WindowsInstaller,
            Package::MacApp,
            Package::AppImage,
        ] {
            let source = Source {
                installs: AtomicUsize::new(0),
                bad: false,
            };
            assert_eq!(
                attempt(&source, package, |_| Ok(())).await,
                Ok(Some(("0.1.1".into(), true)))
            );
            assert_eq!(source.installs.load(Ordering::SeqCst), 1);
        }
    }

    #[tokio::test]
    async fn sec_7_failed_verification_is_refused_and_never_reported_installed() {
        let source = Source {
            installs: AtomicUsize::new(0),
            bad: true,
        };
        assert_eq!(
            attempt(&source, Package::AppImage, |_| Ok(())).await,
            Err("signature mismatch".into())
        );
    }

    #[tokio::test]
    async fn plat_4_notify_packages_never_download_or_install() {
        for package in [Package::Deb, Package::Rpm, Package::WindowsPortable] {
            let source = Source {
                installs: AtomicUsize::new(0),
                bad: true,
            };
            assert_eq!(
                attempt(&source, package, |_| panic!("install attempted")).await,
                Ok(Some(("0.1.1".into(), false)))
            );
            assert_eq!(source.installs.load(Ordering::SeqCst), 0);
        }
    }
}
