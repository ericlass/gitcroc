//! What a workspace lets go of when its tab goes to the background, and
//! what it reads again when the tab comes back (UI-30, UI-31).

use std::sync::Arc;

use gitcroc_git::GitRunner;

use crate::summary::{Summaries, SummaryView, refresh_workspace};
use crate::{Changes, Histories, Watchers, WorkspaceError, WorkspaceId, Workspaces};

/// The per-workspace state a tab switch acts on (ARCH-9).
#[derive(Debug, Clone)]
pub struct TabSwitch {
    pub workspaces: Arc<Workspaces>,
    pub summaries: Arc<Summaries>,
    pub watchers: Arc<Watchers>,
    pub histories: Arc<Histories>,
    pub changes: Arc<Changes>,
}

impl TabSwitch {
    /// Workspace `id`'s tab went to the background. It is no longer
    /// watched, its repos leave the refresh queue (UI-30), and its history
    /// walk and changes view end, so their handles give `None` (UI-31). Its
    /// summaries, tree and loaded refs stay. Returns the views of the repos
    /// taken out of the queue, no longer refreshing. Does nothing when the
    /// workspace is the active tab again, after a quick switch back.
    pub fn background(&self, id: &WorkspaceId) -> Vec<SummaryView> {
        if self.workspaces.active().as_ref() == Some(id) {
            return Vec::new();
        }
        self.watchers.stop(id);
        self.histories.release(id);
        self.changes.release(id);
        self.summaries.deactivate(id)
    }

    /// Workspace `id`'s tab became active: its repos are watched again and
    /// all of them are read again, the selected one first (PERF-4), each
    /// view sent to `emit`. Without a usable `git` nothing is read; the
    /// refresh after git detection reads them. Does nothing when another
    /// tab became active meanwhile.
    pub async fn foreground(
        &self,
        id: WorkspaceId,
        git: Option<GitRunner>,
        emit: impl Fn(SummaryView) + Send + Sync + 'static,
    ) -> Result<(), WorkspaceError> {
        let (watchers, workspaces, watched) =
            (self.watchers.clone(), self.workspaces.clone(), id.clone());
        // Stopping the watch forgot the selection; the summaries kept it.
        self.watchers.select(&id, self.summaries.selected(&id));
        // Opens repos and adds watches (ARCH-7).
        tokio::task::spawn_blocking(move || {
            if workspaces.active().as_ref() == Some(&watched) {
                watchers.sync(&workspaces, &watched);
            }
        })
        .await
        .unwrap_or_default();
        let Some(git) = git else {
            return Ok(());
        };
        refresh_workspace(
            self.workspaces.clone(),
            self.summaries.clone(),
            id,
            git,
            emit,
        )
        .await
    }
}
