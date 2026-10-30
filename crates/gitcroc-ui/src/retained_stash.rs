//! Session-only offers from rc.1 stores/stash.ts. Stable hashes let the
//! core refuse a stale stash entry. Early job ends are bounded.
use gitcroc_core::{JobId, Outcome, RepoId, StashRef, WorkspaceId};
use std::collections::HashMap;
#[derive(Default)]
pub struct RetainedStashes {
    pops: HashMap<JobId, (WorkspaceId, RepoId, StashRef)>,
    early: Vec<(JobId, Option<Outcome>)>,
    kept: HashMap<(WorkspaceId, RepoId), StashRef>,
}
impl RetainedStashes {
    pub fn pop(&mut self, job: JobId, workspace: WorkspaceId, repo: RepoId, stash: StashRef) {
        self.pops.insert(job, (workspace, repo, stash));
        if let Some(at) = self.early.iter().position(|(id, _)| *id == job) {
            let (_, outcome) = self.early.remove(at);
            self.ended(job, outcome.as_ref());
        }
    }
    pub fn ended(&mut self, job: JobId, outcome: Option<&Outcome>) {
        if let Some((workspace, repo, stash)) = self.pops.remove(&job) {
            if outcome == Some(&Outcome::Conflict) {
                self.kept.insert((workspace, repo), stash);
            }
        } else {
            self.early.push((job, outcome.cloned()));
            if self.early.len() > 16 {
                self.early.remove(0);
            }
        }
    }
    pub fn forget(&mut self, workspace: &WorkspaceId, repo: &RepoId) {
        self.kept.remove(&(workspace.clone(), repo.clone()));
    }
    pub fn offers<'a>(
        &'a self,
        workspace: &'a WorkspaceId,
    ) -> impl Iterator<Item = (&'a RepoId, &'a StashRef)> {
        self.kept
            .iter()
            .filter(move |((w, _), _)| w == workspace)
            .map(|((_, repo), stash)| (repo, stash))
    }
}
