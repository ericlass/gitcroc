//! App info, git setup, settings and the job list.

use std::path::PathBuf;

use gitcroc_core::{
    AppInfo, Appearance, CancelResult, ChangesSettings, GitStatus, GroupSettings, JobId, JobView,
    SettingsView, Theme,
};

use super::{App, AppError};

impl App {
    /// The name, version, OS and architecture of the running build, and the
    /// desktop's fonts.
    pub fn app_info(&self) -> AppInfo {
        let fonts = &self.inner.fonts;
        AppInfo::current()
            .with_ui_font(fonts.ui.clone())
            .with_code_font(fonts.code.clone())
    }

    /// The latest git detection result; `Checking` until the first one is
    /// done.
    pub async fn git_status(&self) -> GitStatus {
        self.inner.git.git_status().await
    }

    /// Starts detecting git again and returns at once; the result arrives as
    /// `GitStatus` events.
    pub fn git_recheck(&self) {
        self.spawn_git_detection();
    }

    /// Chooses the git executable, or with `None` goes back to automatic
    /// detection (PLAT-3), saves the setting, and starts detecting git
    /// again; the result arrives as `GitStatus` events.
    pub async fn settings_set_git_path(&self, path: Option<PathBuf>) -> Result<(), AppError> {
        let settings = self.inner.settings.clone();
        self.blocking(move || settings.set_git_path(path)).await??;
        self.spawn_git_detection();
        Ok(())
    }

    /// The settings, and why they are read-only if they are (DATA-1).
    pub fn settings_get(&self) -> SettingsView {
        self.inner.settings.view()
    }

    /// Changes the app theme (UI-23) and saves the settings.
    pub async fn settings_set_theme(&self, theme: Theme) -> Result<SettingsView, AppError> {
        let settings = self.inner.settings.clone();
        Ok(self.blocking(move || settings.set_theme(theme)).await??)
    }

    /// Turns the automatic update check on or off (D12, SEC-7) and saves the
    /// settings.
    pub async fn settings_set_update_check(&self, enabled: bool) -> Result<SettingsView, AppError> {
        let settings = self.inner.settings.clone();
        Ok(self
            .blocking(move || settings.set_update_check(enabled))
            .await??)
    }

    /// Saves the changes pane's view: diff mode and options and the file
    /// list toggle, the same for every workspace (UI-14, UI-17).
    pub async fn settings_set_changes(
        &self,
        changes: ChangesSettings,
    ) -> Result<SettingsView, AppError> {
        let settings = self.inner.settings.clone();
        Ok(self
            .blocking(move || settings.set_changes(changes))
            .await??)
    }

    /// Changes the group operations' settings (section 5, steps 2 and 3)
    /// and saves them. A parallelism outside 1 to 32 is refused.
    pub async fn settings_set_groups(
        &self,
        groups: GroupSettings,
    ) -> Result<SettingsView, AppError> {
        let settings = self.inner.settings.clone();
        Ok(self.blocking(move || settings.set_groups(groups)).await??)
    }

    /// Changes the code themes, fonts and title bar (UI-24, UI-26, UI-35)
    /// and saves the settings. Font sizes outside 8 to 32 are moved to the
    /// nearest limit. The window applies the title bar itself.
    pub async fn settings_set_appearance(
        &self,
        appearance: Appearance,
    ) -> Result<SettingsView, AppError> {
        let settings = self.inner.settings.clone();
        Ok(self
            .blocking(move || settings.set_appearance(appearance))
            .await??)
    }

    /// This run's jobs of every workspace, newest first, with their outcome
    /// and git's output, for the job list (UI-21).
    pub fn jobs_get(&self) -> Vec<JobView> {
        self.inner.jobs.all_views()
    }

    /// Cancels a job: one that has not started never runs, a running fetch
    /// or clone is killed, any other running job finishes (GRP-10). The end
    /// arrives as `JobEnded`.
    pub fn job_cancel(&self, job_id: JobId) -> CancelResult {
        self.inner.jobs.cancel(job_id)
    }
}
