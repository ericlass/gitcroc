//! What branch operations check before they run (M2-06): which branches and
//! remotes exist, what a branch tracks, the default branch and what a typed
//! start point names. Read with `gix`, so the checks start no git process.

use gix::bstr::ByteSlice;

use crate::GitRepo;

/// The branch a local branch tracks, as its config names it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrackedBranch {
    /// `branch.<name>.remote`.
    pub remote: String,
    /// `branch.<name>.merge` without `refs/heads/`: the branch's name on the
    /// remote.
    pub branch: String,
}

impl GitRepo {
    /// The short name of the branch `HEAD` points to, also when it has no
    /// commit yet; `None` when `HEAD` is detached.
    pub fn current_branch(&self) -> Option<String> {
        let name = self.thread_local().head_name().ok()??;
        name.as_bstr()
            .strip_prefix(b"refs/heads/")
            .map(|short| short.to_str_lossy().into_owned())
    }

    /// `refs/heads/<name>` exists.
    pub fn has_local_branch(&self, name: &str) -> bool {
        self.has_ref(&format!("refs/heads/{name}"))
    }

    /// `refs/remotes/<remote>/<branch>` exists.
    pub fn has_remote_branch(&self, remote: &str, branch: &str) -> bool {
        self.has_ref(&format!("refs/remotes/{remote}/{branch}"))
    }

    /// `refs/tags/<name>` exists.
    pub fn has_tag(&self, name: &str) -> bool {
        self.has_ref(&format!("refs/tags/{name}"))
    }

    fn has_ref(&self, full_name: &str) -> bool {
        self.thread_local()
            .try_find_reference(full_name)
            .is_ok_and(|found| found.is_some())
    }

    /// The configured remotes, sorted by name.
    pub fn remote_names(&self) -> Vec<String> {
        self.thread_local()
            .remote_names()
            .into_iter()
            .map(|name| name.to_str_lossy().into_owned())
            .collect()
    }

    /// What local branch `name` tracks on a remote; `None` when it tracks
    /// nothing, or a local branch (`branch.<name>.remote` is `.`).
    pub fn tracked_branch(&self, name: &str) -> Option<TrackedBranch> {
        let repo = self.thread_local();
        let remote = repo
            .branch_remote_name(name, gix::remote::Direction::Fetch)?
            .as_bstr()
            .to_str_lossy()
            .into_owned();
        if remote == "." {
            return None;
        }
        let full: gix::refs::FullName = format!("refs/heads/{name}").try_into().ok()?;
        let merge = repo
            .branch_remote_ref_name(full.as_ref(), gix::remote::Direction::Fetch)?
            .ok()?;
        let branch = merge.as_bstr().strip_prefix(b"refs/heads/")?;
        Some(TrackedBranch {
            remote,
            branch: branch.to_str_lossy().into_owned(),
        })
    }

    /// The remote-tracking branch that holds what local branch `name`
    /// tracks, as the remote's fetch refspec maps it, such as
    /// `refs/remotes/origin/main`; `None` when it tracks nothing or a local
    /// branch. The ref need not exist.
    pub fn tracking_ref(&self, name: &str) -> Option<String> {
        self.tracked_branch(name)?;
        let full: gix::refs::FullName = format!("refs/heads/{name}").try_into().ok()?;
        let tracking = self
            .thread_local()
            .branch_remote_tracking_ref_name(full.as_ref(), gix::remote::Direction::Fetch)?
            .ok()?;
        Some(tracking.as_bstr().to_str_lossy().into_owned())
    }

    /// The default branch (glossary): the branch `refs/remotes/origin/HEAD`
    /// points to, else local `main`, then `master`, whichever exists; as a
    /// full ref name. `None` when there is none.
    pub fn default_branch(&self) -> Option<String> {
        let repo = self.thread_local();
        if let Ok(Some(head)) = repo.try_find_reference("refs/remotes/origin/HEAD")
            && let gix::refs::TargetRef::Symbolic(target) = head.target()
        {
            let target = target.as_bstr().to_str_lossy().into_owned();
            if self.has_ref(&target) {
                return Some(target);
            }
        }
        ["refs/heads/main", "refs/heads/master"]
            .into_iter()
            .find(|name| self.has_ref(name))
            .map(str::to_owned)
    }

    /// The commit `revision` names, a ref or a hash, as a full hex id;
    /// `None` when it names no commit.
    pub fn resolve_commit(&self, revision: &str) -> Option<String> {
        let repo = self.thread_local();
        let id = repo.rev_parse_single(revision).ok()?;
        let commit = id.object().ok()?.peel_to_commit().ok()?;
        Some(commit.id.to_string())
    }
}
