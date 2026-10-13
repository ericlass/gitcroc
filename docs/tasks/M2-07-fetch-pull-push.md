# M2-07 Fetch, pull and push

## Goal
The user fetches, pulls and pushes the selected repo from the toolbar, sees the progress of the transfer, and can cancel a fetch.

## Depends on
M2-02, M2-03, M2-06

## Spec
Implements: UI-20, GRP-12
Respects: D10, ARCH-1, ARCH-3, ARCH-7, GRP-7, GRP-8, GRP-9, GRP-10, OPS-1, OPS-7, section 5 "Fetch", "Pull" and "Push" rows, section 6 "Sync" row

## In scope
- `gitcroc-git::ops`, single repo, each with `--progress` and askpass (M2-03):
  - Fetch: `git fetch --all`, prune as git config says unless the user picks prune or no prune. Allowed in conflict state (GRP-8). *Nothing to do* when no ref changed.
  - Pull: updates the current branch from its upstream. Strategy: git config (default), ff-only, merge or rebase. With `pull.rebase` and `pull.ff` both unset in git config, ff-only (section 5). Detached `HEAD` or no upstream: refused with the reason. Local changes git refuses to overwrite are *failed* with git's message (GRP-12). A merge or rebase that stops with conflicts ends *conflict* (OPS-1).
  - Push: the current branch to its upstream. Without an upstream, "set upstream" (default on) pushes with `-u` to `origin`, or to the only remote when there is no `origin`; with several remotes and no `origin` the dialog asks which. Force-with-lease is destructive (GRP-9). A rejected non-fast-forward push is *failed* with git's message.
- Progress: git's `--progress` lines (counting, compressing, receiving, resolving, writing objects) become `JobProgress` steps; a phase without numbers reports `total` 0.
- Cancel kills a running fetch (GRP-10), and a running pull while it fetches. A push is not cancelled once started.
- Toolbar (UI-20): **Fetch**, **Pull** and **Push** act on the selected repo. A click runs with the defaults; a small arrow beside Pull and Push opens their options. Buttons are disabled when no repo is selected, the repo is busy or missing, and (except Fetch) in conflict state or on a detached `HEAD`.
- Ahead and behind in the tree update after each job (OPS-7).
- Tests (git integration against local bare remotes over `file://`, each with success and failure):
  - Fetch: new commits arrive; *nothing to do* the second time; prune follows git config; failure on an unreachable remote URL; fetch in conflict state is allowed.
  - Pull: fast-forward; ff-only refused on diverged branches (*failed*); merge and rebase strategies on diverged branches; the git-config default followed; the ff-only default when unset; a conflicting merge pull and a conflicting rebase pull end *conflict*; `grp_12_pull_never_stashes` with local changes in the way.
  - Push: to an existing upstream; with set upstream; *nothing to do* when up to date; rejected non-fast-forward; force-with-lease needs confirmation and replaces the remote branch.
  - `grp_10_cancel_kills_a_running_fetch` against a remote that never answers (a fake `ssh` command in `core.sshCommand` that sleeps until killed; the test waits for the kill, not for a time).
  - `ui_20_toolbar_acts_on_the_selected_repo` (Vitest): enabled and disabled states for no selection, busy, conflict, detached `HEAD`.

## Out of scope
- Group fetch, pull and push, and the *needs attention* classification of a missing upstream (preflight; M3).
- Tag push (M2-12).

## Acceptance
- `just dev` with a repo that has a real remote: fetch shows progress in the status bar, pull and push succeed, ahead and behind update.
- All tests listed above exist and pass on all three platforms, and `just check` passes.
