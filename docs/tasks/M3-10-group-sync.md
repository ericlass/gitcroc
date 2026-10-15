# M3-10 Group fetch, pull and push

## Goal
The user fetches, pulls and pushes a group or several repos in one action, from the toolbar and the group menu, and gets a per-repo report.

## Depends on
M3-09

## Spec
Implements: GRP-12 (group pull and push)
Respects: GRP-6, GRP-7, GRP-8, GRP-9, GRP-10, D10, ARCH-1, ARCH-3, OPS-1, OPS-7, UI-20, section 5 "Fetch", "Pull" and "Push" rows

## In scope
- Preflight and run for each operation, reusing the M2-07 single-repo code for the run.
- Fetch:
  - *will skip* "no remote" for a repo without remotes.
  - Prune option as in M2-07 (default: follow git config).
  - Allowed in conflict state (GRP-8).
  - Cancel kills running fetches (GRP-10).
- Pull:
  - *will skip* "detached HEAD" and "no upstream".
  - Strategy option: git config (default), ff-only, merge, rebase. With git config unset, ff-only.
  - Each repo pulls its own current branch, whatever branch that is.
  - Local changes git refuses to overwrite end *failed* with git's message (GRP-12). A stopped merge or rebase ends *conflict* (OPS-1).
- Push:
  - *will skip* "detached HEAD".
  - No upstream is *needs attention* "no upstream; will set origin/<branch>" while "set upstream" is on (the default). With it off, it is *will skip* "no upstream".
  - Without a remote named `origin`, a repo with no upstream is *will skip* "no upstream and no origin". Section 5 names `origin/<branch>`, so the M2 rule of the only remote does not apply to groups.
  - Force-with-lease is destructive (GRP-9) and always confirms, naming the repos.
- Askpass (D10): prompts from parallel sub-jobs queue in the app's dialog one at a time. Each prompt names its repo. Cancelling one prompt fails only that sub-job.
- Toolbar (UI-20): Fetch, Pull and Push work for a group or a multi-selection. A click runs with the defaults, through preflight, and the arrow opens the options. The group row's context menu offers the same three.
- Tests (git integration with local bare remotes over `file://`, driven through `gitcroc-cli` per section 12 "Group scenarios", asserted on the report):
  - Fetch of a group with one repo without a remote: *skipped (no remote)*, others *succeeded*, then *nothing to do* the second time.
  - Pull of a group with a detached repo, a repo without upstream, a fast-forward, a conflicting merge pull and a repo with local changes in the way: *skipped*, *skipped*, *succeeded*, *conflict*, *failed*. `grp_12_group_pull_never_stashes`: the dirty repo's files are unchanged.
  - Push with a new branch in one repo (*needs attention*, then *succeeded* with the upstream set), a rejected push (*failed*), and an up-to-date repo (*nothing to do*).
  - Force-with-lease refused without confirmation.
  - Cancel during a group fetch against a remote that never answers (the fake `ssh` of M2-07): *skipped (cancelled)* for it and the queued ones.
  - The toolbar enables the three for a group (Vitest).

## Out of scope
- Group branch operations (M3-11), merge, rebase and tag (M3-12), commit (M3-13).

## Acceptance
- With `just dev` on `target-project` with local bare remotes, a group Fetch reports per repo and the tree's ahead and behind counts update.
- All tests listed above exist and pass on all three platforms, and `just check` passes.
