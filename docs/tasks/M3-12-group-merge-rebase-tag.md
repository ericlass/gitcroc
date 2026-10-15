# M3-12 Group merge, rebase and create tag

## Goal
The user merges a ref into, or rebases onto a ref, every repo of a group, with predicted conflicts flagged before anything runs, and creates a tag across a group.

## Depends on
M3-09

## Spec
Implements: GRP-13
Respects: GRP-6, GRP-7, GRP-8, GRP-12, GRP-14, SEC-4, OPS-1, OPS-2, OPS-7, UI-20, section 5 "Merge X into current", "Rebase current onto X" and "Create tag X" rows

## In scope
- Merge X into current, run with the M2-08 single-repo code:
  - *will skip* "X not found" and "detached HEAD".
  - Fast-forward option: git config (default), ff-only, no-ff.
  - Up to date is *nothing to do*. A stopped merge is *conflict* (OPS-1). The repo then shows the OPS-2 banner, and its report line says to resolve it in that repo.
- Rebase current onto X: the same skips. No options. A stopped rebase is *conflict*.
- GRP-13, conflict prediction in preflight:
  - For each repo that would run, `git merge-tree --write-tree HEAD X`, read for the conflicted paths. Use only options git 2.40 has (D13), checked against its documentation.
  - Exit status 1 means *needs attention* "conflicts predicted in N files", with the file names in the dialog row. Exit status 0 stays *will run*.
  - A fast-forward or up-to-date case is not merged in memory.
  - For rebase the same check is made between `HEAD` and X as an approximation, and the reason says "may conflict". The run decides either way, since the prediction is advisory.
  - `merge-tree` is added to the runner's read-only subcommands, since it writes objects only to the object store, like the commit-graph exception of M1. The `arch_1` test names it.
- Create tag X:
  - *will skip* "X already exists".
  - Annotated with a message (default) or lightweight. Push the tag (default off) uses the M2-12 push target rule, as one sub-job.
  - X is validated once (SEC-4) and created at each repo's `HEAD` (GRP-14).
- Toolbar (UI-20): **Merge** opens group merge and rebase for a group or a multi-selection. The ref picker lists names from the group's branches (UI-5's list) and accepts any typed ref. The group row's menu adds Merge, Rebase and Create tag.
- Tests (git integration through `gitcroc-cli`, asserted on preflight and report):
  - `grp_13_merge_tree_predicts_conflicts_as_needs_attention`: of three repos, one clean merge, one conflicting and one fast-forward; the classes are *will run*, *needs attention* with the file, *will run*.
  - `grp_13_prediction_is_advisory`: a predicted conflict that a configured merge driver resolves still runs, and ends as git decides.
  - Merge with ff-only on diverged history: *failed*. Up to date: *nothing to do*. Conflict: *conflict*, and the repo is in conflict state.
  - Rebase with one conflicting repo among three: the others succeed (GRP-6).
  - Tag: an existing name skipped, annotated and lightweight, and push to a file remote.
  - `grp_12_group_merge_never_stashes`.
  - The preflight writes no ref and changes no working file: refs and status are equal before and after.

## Out of scope
- Group cherry-pick, revert and reset: single-repo only (section 5 lists the group operations).

## Acceptance
- With `just dev`, group merge of a branch that conflicts in one repo shows *needs attention* for that repo before running, and *conflict* in the report after.
- All tests listed above exist and pass on all three platforms, and `just check` passes.
