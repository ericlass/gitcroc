# M3-11 Group checkout, create and delete branch, and the mixed-branch scenario

## Goal
The user checks out, creates and deletes a branch across a group in one action, double-clicks a name in the group's Branches node to check it out, and the mixed-branch scenario of section 5 passes as a test.

## Depends on
M3-10

## Spec
Implements: GRP-14, UI-5
Respects: GRP-4, GRP-6, GRP-7, GRP-8, GRP-9, GRP-12, SEC-4, OPS-7, UI-7, section 3 "Default branch", section 5 "Checkout branch X", "Create branch X" and "Delete branch X" rows and "The mixed-branch scenario"

## In scope
- GRP-14: "X" is typed or picked once, validated once against git's ref format (SEC-4), then resolved separately in each repo. Repos where X points at different commits are normal and not flagged.
- Checkout branch X, run with the M2-06 single-repo code:
  - Local X: switch to it.
  - No local X and exactly one remote has X: create a tracking branch and switch.
  - Otherwise the missing-branch policy decides:
    - skip (default): *will skip* "no branch X";
    - switch to the default branch: *needs attention* "no branch X; switches to <default>". The default branch follows section 3: `origin/HEAD`, then `main`, then `master`. With none of them it is *will skip* "no default branch";
    - create X from `HEAD`: *needs attention* "creates X".
  - X on several remotes and not local is *will skip* "X is on several remotes".
  - A repo already on X is *nothing to do* at run, decided from `HEAD` before and after.
  - Local changes in the way end *failed* with git's message (GRP-12).
- Create branch X:
  - *will skip* "X already exists". An existing branch is never reset.
  - Start point: current `HEAD` (default), the default branch, or a named ref. A ref that does not resolve in a repo is *will skip* "<ref> not found".
  - Switch after create (default on).
- Delete branch X (`git branch -d`):
  - *will skip* "no branch X" and "X is the current branch".
  - Force (`-D`) is destructive (GRP-9).
  - "Also delete on the remote" is destructive. It deletes X on the remote of X's upstream, else on `origin`. A repo with neither does the local delete only, and its result says so.
- Group Branches node (UI-5): double-click on a name starts a group checkout of that name on the group's members, through preflight. The context menu on a name offers Checkout and Delete.
- The toolbar's **Branch** and the group row's context menu open group create. Checkout and Delete are in the group row's menu with a name picker that lists the group's branch names (UI-5's list).
- Mixed-branch scenario (section 5), `grp_mixed_branch_scenario` through `gitcroc-cli` on a group of 5 fixture repos with local bare remotes, asserted on each step's report and on GRP-4:
  1. Group checkout `main`: 5 *succeeded* or *nothing to do*; the group shows `main`.
  2. Checkout `feature/x` in one repo: only that repo switches.
  3. The group shows "mixed (2)", and each repo's summary has its own branch.
  4. Group pull: each repo pulls its own branch, 5 outcomes.
  5. Group checkout `main`: everything is back on `main`; the group shows `main`.
- Tests:
  - `grp_14_x_resolves_separately_in_each_repo`: X at different commits in two repos is checked out in both, unflagged.
  - Each missing-branch policy, with one repo lacking X.
  - A tracking branch from the one remote that has X, and X on two remotes skipped.
  - Create when X exists in one repo: skipped and that branch unchanged, asserted on its commit.
  - Delete: missing and current skipped; force and remote delete refused without confirmation; remote delete with and without an upstream.
  - `grp_12_group_checkout_never_stashes`.
  - `ui_5_double_click_on_a_group_branch_starts_a_group_checkout` (Vitest).
  - `grp_mixed_branch_scenario` as above.

## Out of scope
- Group rename: rename is single-repo only (section 6).

## Acceptance
- `grp_mixed_branch_scenario` passes on all three platforms.
- All tests listed above exist and pass, and `just check` passes.
