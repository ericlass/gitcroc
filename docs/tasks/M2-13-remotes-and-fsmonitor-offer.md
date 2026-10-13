# M2-13 Remotes and the fsmonitor offer

## Goal
The user adds, renames and removes remotes and edits their URLs, and for a repo over 50,000 files the app offers once to turn on `core.fsmonitor` and `core.untrackedCache`.

## Depends on
M2-07

## Spec
Implements: PERF-7
Respects: ARCH-1, ARCH-3, ARCH-7, GRP-8, OPS-7, SEC-3, SEC-6, DATA-1, section 6 "Remotes" row, section 9 "Working copy status" budget

## In scope
- `gitcroc-git::ops`, single repo:
  - Add: `git remote add <name> <url>`; the name is checked against git's remote name rules (a valid ref component, SEC-4's checker) before git runs. Fetching it afterwards is an option, off by default.
  - Rename: `git remote rename <old> <new>`; git moves its remote branches and upstreams.
  - Remove: `git remote remove <name>`; asks first, naming the upstreams that will be unset.
  - Edit URL: `git remote set-url <name> <url>` (fetch URL; a separate push URL is shown and edited the same way when set).
- Tree: the Remotes node gets a context menu with Add remote; each remote row gets Rename, Edit URL and Remove.
- PERF-7: when a repo's status read finds more than 50,000 tracked files, and the repo has neither setting on, the app offers once per repo, in the changes pane: turn on `core.fsmonitor` and `core.untrackedCache`, or don't ask again. On Linux, where git has no built-in monitor, only `core.untrackedCache` is offered (M1-04). Accepting runs `git config core.fsmonitor true` and `git config core.untrackedCache true` in the repo's local config, as one job. "Don't ask again" is kept as an optional `fsmonitor = "declined"` on the `[[repo]]` of the workspace file, schema unchanged, like `commit_graph` (M1-08). The app never changes git config without this answer.
- Tests (git integration, each with success and failure):
  - Add, rename, remove a remote and edit its URL; adding an existing name and an invalid name fail; rename keeps upstreams pointing at the renamed remote; remove asks first.
  - `perf_7_offers_fsmonitor_once_for_large_repos`: a fixture over the threshold (made small for the test) gets the offer; below it, with the settings already on, or after "don't ask again", it does not.
  - `perf_7_never_changes_git_config_without_asking`: the status read and the refresh write no config; accepting writes exactly the two keys (one on Linux) to the local config.
  - Vitest: the remote dialogs and the offer.

## Out of scope
- Any other git config editing (not in section 6).

## Acceptance
- `just dev`: add a remote, fetch it, rename it, edit its URL, remove it. On `big-tree`, the offer appears once; accepting it turns on the untracked cache (Linux).
- All tests listed above exist and pass on all three platforms, and `just check` passes.
