# M2-11 Stash

## Goal
The user stashes the working copy with an optional message and optionally its untracked files, and applies, pops and drops stashes from the tree; a stash that applies with conflicts is resolved like any other conflict.

## Depends on
M2-10

## Spec
Respects: OPS-1, OPS-2, OPS-3, OPS-7, ARCH-1, ARCH-3, ARCH-7, GRP-7, GRP-8, GRP-12, UI-1, UI-20, SEC-3, section 6 "Stash" row

## In scope
- `gitcroc-git::ops`, single repo only (section 6):
  - Stash: `git stash push [-m <message>] [--include-untracked]`. A clean working copy is *nothing to do*.
  - Apply and pop: `git stash apply|pop <stash@{n}>`, the stash named by its commit hash as read with the tree, so a stash list that changed since is refused instead of applying the wrong one. Changes git refuses to overwrite are *failed* (GRP-12). Unmerged paths afterwards end the job *conflict*.
  - Drop: `git stash drop <stash@{n}>`, checked against the hash the same way. It asks for confirmation first, naming the stash (see the README: not a GRP-9 operation, but it cannot be undone from the app).
- Stash conflicts: git leaves no state file for a stash apply, so the conflict state is the unmerged index entries alone (OPS-1), named "stash apply". The banner (OPS-2) shows it, with:
  - **Abort**: `git reset --merge`, which puts the files the apply touched back to `HEAD` and keeps other local changes; the stash is still there, since git keeps a stash whose apply conflicted. Destructive for what the apply brought in, so it asks first.
  - **Continue**: disabled while unmerged paths remain (OPS-2). The conflict state ends when the last file is marked resolved (M2-10), and the banner goes. After a pop, the app then offers to drop the stash git kept.
- Toolbar (UI-20): **Stash** opens the stash dialog (message, include untracked) for the selected repo.
- Tree: the Stashes node's rows get a context menu with Apply, Pop and Drop; double-click shows the stash in the history as before (M1).
- Tests (git integration, each with success and failure):
  - Stash with and without a message and untracked files; *nothing to do* on a clean repo.
  - Apply and pop: clean apply; pop drops the stash; apply over conflicting local changes is *failed* with files untouched; a conflicting apply ends *conflict*, is resolved with M2-09's Mark resolved and leaves no conflict state; after a conflicting pop the stash is still there.
  - Drop needs confirmation; a stash list changed since it was read refuses the request.
  - Abort of a conflicting stash apply restores the touched files and keeps unrelated local changes.
  - `ops_2_banner_names_the_operation_with_abort_and_continue` (Vitest) gains the stash apply case.

## Out of scope
- Group stash: stash is single-repo only (section 6).
- Stash of selected files only, and stash show as a diff beyond the history's stash row (not in section 6).

## Acceptance
- `just dev`: stash with a message, see it under Stashes, pop it back; stash again, change the same line, apply, resolve in the conflict editor.
- All tests listed above exist and pass on all three platforms, and `just check` passes.
