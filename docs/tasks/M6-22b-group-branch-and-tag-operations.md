# M6-22b Group branch, merge, rebase and tag operations

## Goal
The group operations on a name X typed or picked once run in the GPUI app as
in rc.1: Check out, Create and Delete branch, Merge, Rebase and Create tag,
from the toolbar, the group row's menu and the Branches node, and a group
branch's double-click checks it out on every member. Each goes through
M6-22a's preflight.

## Depends on
- M6-22a: the preflight flow, its dialog and the report.
- M6-21: the group row's menu and the Branches node with its counts.
- M6-17: the branch dialogs' name field and the core's name checks.

## Spec
Implements: UI-5 (the double-click), UI-7 (the group entries), UI-20 (Branch and Merge on a group), GRP-13, GRP-14
Respects: GRP-4, GRP-5, GRP-9, UI-22, UI-23, SEC-3, SEC-4, ARCH-6, ARCH-9, D15, QA-2, QA-3, QA-4

Also implements the rows of section 5's "Group-capable operations" from Checkout to Create tag.

The dialog only picks the name and the options. The core checks a new
branch or tag name as the user types (SEC-4), resolves X in each repo
(GRP-14), predicts conflicts (GRP-13) and decides what is destructive
(GRP-9), as in rc.1's `ui/src/groups/GroupNameDialog.tsx` (ARCH-6).

## In scope
- **The group name dialog**, a port of `GroupNameDialog.tsx`, on the
  shared dialog parts, titled with the target, such as "Check out a branch
  in Backend":
  - **Check out:** X from the group's branch names (`group_branches`) or
    typed; where X is missing: skip (default), switch to its default
    branch, or create it from HEAD;
  - **Create branch:** X, checked by the core as typed; the start point:
    current HEAD (default), the default branch, or a named ref; switch
    after create (default on); the note that a repo where X exists, or the
    start point does not, is skipped;
  - **Delete branch:** X from the names or typed; Force and Also delete on
    the remote, both destructive (GRP-9);
  - **Merge:** any ref, picked from the names or typed; fast-forward as git
    config says (default), fast-forward only, or always a merge commit; a
    switch to Rebase the current branch onto it, which changes the title
    and the button; the note that the preflight predicts conflicts;
  - **Rebase:** any ref;
  - **Create tag:** X, checked by the core as typed; annotated with a
    message (default) or lightweight; push the tag (default off); the note
    that each repo is tagged at its own HEAD;
  - a ref that looks like an option (leading `-`) disables the button,
    and the core refuses it too;
  - a group whose branches cannot be read says so, and X can still be
    typed. Names render as text (SEC-3).
- **The entries** (UI-7, UI-20):
  - the group row's menu enables Check out branch…, New branch…, Delete
    branch…, Merge…, Rebase… and Create tag…;
  - a group branch's menu: Check out, which runs at once with the skip
    policy, and Delete…, which opens the dialog on that name;
  - **double-click** on a group branch (UI-5) starts a group checkout of
    that name with the skip policy, through the preflight; it never checks
    out in one repo only;
  - the toolbar's Branch opens Create branch and Merge opens Merge for a
    group or a multi-selection (GRP-5). As in rc.1, a multi-selection's
    repo menu keeps its single-repo entries; the palette (M6-24) reaches
    the other operations on it.
- **The conflict prediction** (GRP-13): a *needs attention* repo shows the
  core's reason, such as the predicted files, in the preflight row; a repo
  where git stops in conflict is *conflict* in the report, and selecting it
  from the report shows M6-19c's conflict banner.
- **Strings (D15):** the group name dialog's text moves from
  `ui/src/strings/en.ts` to `strings.rs`.

## Tests (QA-3)
Real git on `repo-gen` fixtures (QA-2), in a group of three repos on mixed
branches, one where `topic` is missing.
- `ui_5_double_click_on_a_group_branch_starts_a_group_checkout`: through
  the preflight, with the skip policy, and the group then shows that branch
  or "mixed" (GRP-4).
- `grp_14_checkout_resolves_the_name_in_each_repo`: each missing-branch
  policy.
- `grp_14_create_branch_skips_where_it_exists_and_never_resets`.
- `grp_9_force_delete_and_delete_on_remote_ask_first`.
- `grp_13_merge_predicts_conflicts_and_marks_needs_attention`: and a
  repo that conflicts is *conflict*, not *failed*.
- `grp_13_rebase_onto_a_ref_from_the_toolbar_and_the_menu`.
- `ui_7_the_group_menu_and_the_branches_node_offer_the_operations`.
- `sec_4_branch_and_tag_names_are_checked_as_typed`: and an option-like
  ref is never sent.
- `ui_20_branch_and_merge_open_the_group_dialogs`.
- `sec_3_branch_names_render_as_text`.
- `ui_23_no_hardcoded_colors`, `d15_ui_text_comes_from_the_strings_module`
  and the keymap docs test keep passing over the new files.

## Out of scope
- **Group commit** (UI-34, GRP-15): M6-22c.
- **The palette's group commands** (UI-22): M6-24.

## Acceptance
- **On the reference machine,** `just dev-next` on `target-project`: the
  mixed-branch scenario of section 5 step by step; a group merge with a
  predicted conflict; a group tag pushed; all of it with the keyboard only
  too.
- `just coverage M6` no longer lists UI-5, GRP-13 or GRP-14.
- `just check` passes.
