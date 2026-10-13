# M2-06 Branches

## Goal
From the tree and the toolbar the user checks out, creates, renames and deletes branches, sets upstreams, checks out remote branches and single commits, and double-clicks a local branch to check it out.

## Depends on
M2-01, M2-03 (deleting a remote branch)

## Spec
Implements: SEC-4, UI-7
Respects: ARCH-1, ARCH-3, ARCH-6, ARCH-7, GRP-8, GRP-9, GRP-12, GRP-14, OPS-7, UI-1, UI-20, section 5 "Checkout branch X", "Create branch X" and "Delete branch X" rows, section 6 "Branches" row

## In scope
- SEC-4 in the core: every branch or tag name typed by the user is checked against git's ref format rules (`git check-ref-format` rules, implemented in Rust) before any command runs, with the rule that failed as the message. Also used by tags (M2-12).
- `gitcroc-git::ops`, single repo:
  - Checkout of a local branch: `git switch <branch>`. Local changes that git refuses to carry are *failed* with git's message, never stashed (GRP-12).
  - Checkout of a remote branch `<remote>/<x>`: when a local `x` exists, switch to it; otherwise `git switch --track <remote>/<x>`.
  - Checkout of a commit (detached): `git switch --detach <hash>`.
  - Create: name, start point (current `HEAD` by default, the default branch, or any ref or commit picked from a list), switch after create (on by default). `git switch -c` or `git branch`. An existing name is refused, never reset.
  - Rename: `git branch -m <old> <new>`, single repo only.
  - Delete: `git branch -d`; force (`-D`) and delete on the remote (`git push <remote> --delete <x>`) are destructive (GRP-9) and ask first. The current branch cannot be deleted. Deleting a remote branch is offered on a remote branch row too, and runs with askpass (M2-03).
  - Set upstream: `git branch --set-upstream-to=<remote>/<x>`, or unset it.
- *Nothing to do*: checkout of the branch already checked out.
- Tree (UI-7): double-click on a local branch checks it out. Context menus on local branch, remote branch, tag and repo rows with the operations valid for that row; an invalid one is disabled. The repo menu keeps "Remove from workspace". Dragging repos between groups is M3.
- Toolbar (UI-20): **Branch** opens the create dialog for the selected repo.
- Dialogs: create (name, start point, switch after), rename (new name), delete (force, also on remote, with the GRP-9 confirmation), set upstream (remote branch list). Names are checked by the core as the user types (SEC-4) and the error is shown under the field.
- Operations are disabled for a repo whose summary is busy, missing or in conflict state (GRP-8); the core refuses them too.
- Tests (git integration, each with success and failure):
  - `sec_4_ref_names_follow_git_rules`: a table of valid and invalid names, compared against `git check-ref-format --branch` on the same names.
  - Checkout: success; *nothing to do*; failure with conflicting local changes, which are left untouched (`grp_12_checkout_never_stashes`).
  - Checkout of a remote branch with and without an existing local branch; detached checkout of a commit.
  - Create: from `HEAD`, from the default branch, from a commit, with and without switching; an existing name refused.
  - Rename; rename to an existing name fails.
  - Delete: `-d` of a merged branch; `-d` of an unmerged branch fails with git's message; force and remote delete need confirmation (`grp_9_force_delete_needs_confirmation`).
  - Set and unset upstream.
  - `ui_7_double_click_checks_out_a_local_branch` (Vitest), and the context menu of each row kind lists its operations (Vitest).

## Out of scope
- Group checkout, create and delete (section 5; M3), and the group Branches node (UI-5).
- The commit context menu in the history (UI-13; M2-08).

## Acceptance
- `just dev`: create a branch from a commit, check it out by double-click, rename it, delete it.
- All tests listed above exist and pass on all three platforms, and `just check` passes.
