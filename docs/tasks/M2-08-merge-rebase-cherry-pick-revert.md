# M2-08 Merge, rebase, cherry-pick, revert and the conflict banner

## Goal
The user merges, rebases, cherry-picks and reverts from the toolbar and the history's context menu, and when git stops in conflict state, a banner names the operation with Abort and Continue.

## Depends on
M2-06

## Spec
Implements: OPS-2, UI-13
Respects: OPS-1, OPS-7, ARCH-1, ARCH-3, ARCH-6, ARCH-7, ARCH-9, GRP-7, GRP-8, GRP-12, UI-10, UI-20, SEC-3, section 5 "Merge X into current" and "Rebase current onto X" rows, section 6 "Integrate" row

## In scope
- `gitcroc-git::ops`, single repo:
  - Merge ref X into the current branch: fast-forward as git config says (default), ff-only, or no-ff; `--no-edit`, so git's prepared message is used. Detached `HEAD` or X not resolving: refused with the reason.
  - Rebase the current branch onto ref X, non-interactive (`git rebase <X>`). Detached `HEAD` refused.
  - Cherry-pick one or several commits of one repo, applied oldest first (`git cherry-pick <a> <b> …`).
  - Revert a commit (`git revert --no-edit`). A merge commit asks which parent is the mainline (default the first) and passes `-m`.
  - Abort: `git merge --abort`, `git rebase --abort`, `git cherry-pick --abort`, `git revert --abort`, chosen by the conflict state the core read (OPS-1), never by what the app started.
  - Continue: `git merge --continue`, `git rebase --continue`, `git cherry-pick --continue`, `git revert --continue`, with git's prepared message (`GIT_EDITOR=true`, M2-01). A rebase or a series that stops again ends the job *conflict* again.
  - *Nothing to do*: merging a ref already contained in `HEAD`; rebasing onto a ref `HEAD` already contains.
- Conflict banner (OPS-2) above the history list while the shown repo is in conflict state: names the operation and, where git has them, what is being applied and onto what (for a rebase: step `n` of `m`). **Abort** always; **Continue** enabled only when the core reports no unmerged paths. It reacts to conflicts started in a terminal the same way (OPS-1). Stash apply conflicts get their banner in M2-11.
- History context menu (UI-13) on a commit row: checkout (detached; M2-06), create branch here (M2-06's dialog with the commit as start point), create tag here (disabled until M2-12), cherry-pick, revert, reset current branch to here (disabled until M2-12), copy hash. The history gets multi-selection of commit rows (Ctrl/Cmd and Shift) for cherry-pick only; the other entries are disabled with more than one row selected.
- Toolbar (UI-20): **Merge** opens a dialog to pick X (branches, remote branches and tags of the repo, filtered as the user types, filtering in the core) and the fast-forward option, with a "rebase onto X instead" choice.
- Unmerged files still show M1's note in the changes pane; resolving them arrives with M2-09 and M2-10. Until then a conflict is resolved in a terminal and continued in the app.
- Tests (git integration, each with success, failure and conflict):
  - Merge: fast-forward; no-ff makes a merge commit; ff-only refused on diverged history; *nothing to do*; a conflicting merge ends *conflict* with `MERGE_HEAD`; abort restores the branch; continue after the conflict is resolved and staged in the fixture commits the merge.
  - Rebase: clean rebase; conflict at step 2 of 3, continue to the end with a further conflict and abort from there; detached `HEAD` refused.
  - Cherry-pick: one commit; three commits applied oldest first; a conflict in the second, then continue; abort.
  - Revert: a commit; a merge commit with mainline 1; a conflicting revert, then abort.
  - `grp_12_merge_never_stashes`: local changes in the way make it *failed* with git's message, files untouched.
  - `ops_2_banner_names_the_operation_with_abort_and_continue` (Vitest): one case per operation of OPS-1; Continue disabled while unmerged paths remain.
  - `ops_2_abort_follows_the_repo_state` (Rust): a cherry-pick started in a terminal is aborted with `cherry-pick --abort`.
  - `ui_13_commit_context_menu` (Vitest): the entries, cherry-pick enabled for a multi-selection and the others not.

## Out of scope
- The conflict editor and Mark resolved (M2-09, M2-10).
- Group merge and rebase, and conflict prediction with `git merge-tree` (GRP-13; M3).
- Interactive rebase (section 2, LATER).

## Acceptance
- `just dev`: merge a branch that conflicts; the banner names the merge; resolve the file in a terminal and `git add` it; Continue becomes enabled and finishes the merge.
- All tests listed above exist and pass on all three platforms, and `just check` passes.
