//! Updates (PLAT-4): the check, Later, the install and the download page.

use gitcroc_core::{JobId, UpdateView};

use super::{App, AppError};
use crate::platform::{PlatformBrowser, WebBrowser};

impl App {
    /// Where the update check and install are (PLAT-4); each change also
    /// arrives as `UpdateStatusChanged`.
    pub fn update_status(&self) -> UpdateView {
        let view = self.inner.updates.view();
        #[cfg(feature = "update-test")]
        if let Some(version) = self.inner.installed_at_start.get() {
            return UpdateView {
                status: gitcroc_core::UpdateStatus::ReadyToRestart {
                    version: version.clone(),
                },
                ..view
            };
        }
        view
    }

    /// The `update-test` build's update at start (M6-28c): checks at once
    /// and installs without asking, then says `ReadyToRestart`, by event
    /// and in [`Self::update_status`], so the UI restarts into it whether
    /// it already listened or reads the status first. Only this build does
    /// it (D12).
    #[cfg(feature = "update-test")]
    pub(super) fn update_at_start(
        &self,
        source: std::sync::Arc<dyn gitcroc_core::UpdateSource>,
        config: Option<std::path::PathBuf>,
        version: String,
    ) {
        use crate::platform::{Installation, PlatformInstallation};
        use crate::update::at_start::{self, Outcome};

        let app = self.clone();
        self.inner.runtime.spawn(async move {
            let outcome = at_start::run(
                source.as_ref(),
                PlatformInstallation::package(),
                &version,
                config.as_deref(),
                &app.inner.log,
            )
            .await;
            if let Outcome::Installed(installed) = outcome {
                let _ = app.inner.installed_at_start.set(installed);
                app.inner
                    .hub
                    .send(super::AppEvent::UpdateStatusChanged(app.update_status()));
            }
        });
    }

    /// Checks for an update at once, also while automatic checks are off,
    /// and returns at once; the result arrives as `UpdateStatusChanged`. A
    /// build that does not check does nothing (D12).
    pub fn update_check_now(&self) {
        let updates = self.inner.updates.clone();
        self.inner
            .runtime
            .spawn(async move { updates.check_now().await });
    }

    /// Puts off the available update's notice until the next start or a
    /// newer version.
    pub fn update_later(&self) {
        self.inner.updates.later();
    }

    /// Installs the available update as a job of the active workspace,
    /// refused while another job runs (PLAT-4, SEC-7). Once it is verified
    /// and installed, `UpdateStatusChanged` says `ReadyToRestart`, and the
    /// UI restarts into it. The UI asks first when a tab holds unsaved
    /// conflict edits (OPS-9).
    pub fn update_install(&self) -> Result<JobId, AppError> {
        let workspace = self
            .workspaces()
            .active()
            .ok_or_else(|| AppError::refused("open a workspace to update from"))?;
        // The install runs on the app's runtime, whichever thread calls.
        let _entered = self.inner.runtime.enter();
        let install = self.inner.updates.install(&self.inner.jobs, &workspace)?;
        Ok(install.job_id)
    }

    /// Opens the releases page in the browser, for the packages that do not
    /// install updates (PLAT-4). The page is fixed in the app.
    pub async fn update_open_download_page(&self) -> Result<(), AppError> {
        let launch = PlatformBrowser::open(crate::update::RELEASES_PAGE);
        self.blocking(move || crate::platform::start(&[launch]))
            .await??;
        Ok(())
    }
}
