# 5. Workspaces, groups and group operations

A group is only a list of repos: it has no branch and no state of its own, so any repo in it can be operated on alone at any time without "breaking" the group.

## Model

- **GRP-1** A repo is added to a workspace once, keyed by its canonical absolute path. Adding it again is a no-op.
- **GRP-2** Group names are unique per workspace (case-insensitive), 1 to 64 characters. Members keep a user-defined order.
- **GRP-3** Removing a repo from a group or workspace, or deleting a group, never touches anything on disk.
- **GRP-4** What the UI shows as a group's branch is derived on the fly: the branch name if every member is on the same branch, otherwise "mixed" with the count of distinct branches.
- **GRP-5** The target set is the selected repo, all members of the selected group, or the multi-selected repos. There is no other way to build one.

## Lifecycle of a group operation

1. **Preflight.** A read-only pass classifies every repo in the target set as *will run*, *will skip* (with reason), *blocked* (with reason) or *needs attention* (for example a predicted conflict or a missing upstream).
2. **Confirm.** The preflight dialog is shown if any repo is not *will run*, or if the operation is destructive (GRP-9). Otherwise the job starts immediately. A setting can force the dialog to always appear.
3. **Run.** Sub-jobs run in parallel, 8 at a time by default (setting: 1 to 32). ARCH-4 still applies per repo.
4. **Report.** When all sub-jobs end, the result report lists every repo with one outcome and git's own output.

Rules:

- **GRP-6** One repo failing never stops or reverts the others (D8).
- **GRP-7** Outcomes are exactly: *succeeded*, *nothing to do*, *skipped*, *failed*, *conflict*. *Conflict* means git left the repo in conflict state; it is not a failure.
- **GRP-8** A repo is *blocked* for any mutating operation when its path is missing, it is in conflict state, or another mutating job is running on it. Fetch is allowed in conflict state.
- **GRP-9** Destructive operations are: force push, force delete of a branch, delete of a remote branch, hard reset, discard changes. They always show a confirmation that names the affected repos.
- **GRP-10** Cancel stops all queued sub-jobs and kills running fetch and clone processes. Other running sub-jobs finish. Cancelled repos are reported as *skipped (cancelled)*.
- **GRP-11** The report offers "Retry failed", which starts a new job whose target set is the failed repos.
- **GRP-12** The app never stashes, commits or discards on the user's behalf to make an operation succeed. If git refuses because of local changes, the outcome is *failed* with git's message.

## Group-capable operations

Only these run on a target set of more than one repo in v1. Everything else in section 6 is single-repo.

| Operation | Per repo it does | Skipped when | Options |
| --- | --- | --- | --- |
| Fetch | Fetches all remotes | Repo has no remote | Prune (default: follow git config) |
| Pull | Updates the current branch from its upstream | Detached HEAD; no upstream | Strategy: git config (default), ff-only, merge, rebase. If git config is unset, ff-only |
| Push | Pushes the current branch to its upstream | Detached HEAD. No upstream is *needs attention* | Set upstream to `origin/<branch>` (default on); force-with-lease (destructive) |
| Checkout branch X | Switches to local X; if absent and exactly one remote has X, creates a tracking branch and switches | Per missing-branch policy | Missing-branch policy: skip (default), switch to default branch, create X from HEAD |
| Create branch X | Creates X and switches to it | X already exists (never reset) | Start point: current HEAD (default), default branch, or a named ref; switch after create (default on) |
| Delete branch X | `git branch -d X` | X does not exist; X is the current branch | Force (destructive); also delete on remote (destructive) |
| Merge X into current | Merges ref X | X does not resolve; detached HEAD | Fast-forward: git config (default), ff-only, no-ff |
| Rebase current onto X | Rebases onto ref X | X does not resolve; detached HEAD | None |
| Create tag X | Creates tag X at HEAD | X already exists | Annotated with message (default) or lightweight; push tag (default off) |
| Commit | Commits the repo's staged changes with the one shared message (GRP-15) | Nothing is staged | Stage all tracked changes first (default off); push after commit (default off) |

- **GRP-13** For merge and rebase, preflight SHOULD predict conflicts with `git merge-tree --write-tree` and mark those repos *needs attention*. The prediction is advisory; the run decides.
- **GRP-14** "X" is a single name typed or picked once. It is resolved separately in each repo. Repos where it resolves to different commits are normal and not flagged.

* **GRP-15** Group commit writes one commit per repo, each with the same message, in every repo of the target set that has staged changes. The commits are independent: they are not linked, and a commit rejected in one repo (for example by a hook) is *failed* while the others still commit (GRP-6). Amend is single-repo only. The confirmation lists each repo with its number of staged files before anything is committed.

## The mixed-branch scenario, step by step

1. Select the group, choose Checkout, pick `main`. All 5 repos switch. The group shows `main`.
2. Select one repo inside the group, check out `feature/x`. Only that repo switches.
3. The group now shows "mixed (2)". Each repo row shows its own branch.
4. Group Pull still works: each repo pulls whatever branch it is on.
5. Group Checkout `main` brings everything back in line.
