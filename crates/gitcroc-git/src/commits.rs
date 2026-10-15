//! What cherry-pick and revert check before they run (M2-08): a commit's
//! parents, how it reads in a label, and the order a series applies in;
//! and whether one commit descends from another (GRP-13).
//! Read with `gix`, so the checks start no git process.

use gix::hash::ObjectId;

use crate::{GitRepo, RefsError};

impl GitRepo {
    /// The parents of commit `hash`, a full hex id, as full hex ids; `None`
    /// when it names no commit.
    pub fn commit_parents(&self, hash: &str) -> Option<Vec<String>> {
        let repo = self.thread_local();
        let id = ObjectId::from_hex(hash.as_bytes()).ok()?;
        let commit = repo.find_commit(id).ok()?;
        Some(commit.parent_ids().map(|id| id.to_string()).collect())
    }

    /// Commit `hash` as `1a2b3c4 Subject`; `None` when it names no commit.
    pub fn commit_text(&self, hash: &str) -> Option<String> {
        let repo = self.thread_local();
        let id = ObjectId::from_hex(hash.as_bytes()).ok()?;
        repo.find_commit(id).ok()?;
        Some(crate::status::conflict::commit_text(&repo, id))
    }

    /// Whether commit `ancestor` is `descendant` or one of its ancestors,
    /// both full hex ids: a merge of `ancestor` into `descendant` is up to
    /// date, and the other way round a fast-forward (GRP-13). `false` when
    /// either is not an id.
    pub fn is_ancestor(&self, ancestor: &str, descendant: &str) -> Result<bool, RefsError> {
        let repo = self.thread_local();
        let (Ok(ancestor), Ok(descendant)) = (
            ObjectId::from_hex(ancestor.as_bytes()),
            ObjectId::from_hex(descendant.as_bytes()),
        ) else {
            return Ok(false);
        };
        if ancestor == descendant {
            return Ok(true);
        }
        Ok(repo
            .merge_base(ancestor, descendant)?
            .is_some_and(|base| base == ancestor))
    }

    /// `commits`, full hex ids, oldest first: each after every other one it
    /// descends from, and otherwise by commit time. A commit that descends
    /// from `k` of the others has exactly those `k` before it.
    pub fn oldest_first(&self, commits: &[String]) -> Result<Vec<String>, RefsError> {
        let repo = self.thread_local();
        let mut ranked = Vec::with_capacity(commits.len());
        for hash in commits {
            let Ok(id) = ObjectId::from_hex(hash.as_bytes()) else {
                // Not an id: no commit to order it by.
                ranked.push((0, 0, hash.clone()));
                continue;
            };
            let time = repo.find_commit(id)?.time()?.seconds;
            let mut ancestors = 0usize;
            for other in commits {
                let Ok(other) = ObjectId::from_hex(other.as_bytes()) else {
                    continue;
                };
                if other != id
                    && repo
                        .merge_base(other, id)?
                        .is_some_and(|base| base == other)
                {
                    ancestors += 1;
                }
            }
            ranked.push((ancestors, time, hash.clone()));
        }
        ranked.sort();
        Ok(ranked.into_iter().map(|(_, _, hash)| hash).collect())
    }
}
