# M2-04 Stage, unstage and discard

## Goal
In "Uncommitted changes" the user stages, unstages and discards whole files and single hunks, with a confirmation before anything is discarded.

## Depends on
M2-01

## Spec
Implements: UI-15, GRP-9
Respects: ARCH-1, ARCH-3, ARCH-4, ARCH-6, ARCH-7, ARCH-9, OPS-7, PLAT-2, SEC-3, DATA-3, section 2 "Line-level staging" (LATER)

## In scope
- `gitcroc-git::ops`:
  - By file: stage (`git add -A -- <path>`, which also stages a deletion), unstage (`git restore --staged -- <path>`; on an unborn branch `git rm --cached -- <path>`), discard (`git restore --worktree -- <path>` for a tracked file; `git clean -f -- <path>` for an untracked file or folder).
  - By hunk: the core builds a patch for one hunk of the diff it showed (M1-04) and applies it with `git apply --cached` (stage), `git apply --cached -R` (unstage) or `git apply -R` (discard), patch on stdin, `--recount`. Hunks come from the core's own diff, so the UI sends only the file id and the hunk index (PLAT-2).
  - A hunk whose file changed since the diff was read is refused with "the file changed; showing it again", and the diff is read again. The check compares the blob or file content the hunk came from.
- GRP-9: discard by file or by hunk is destructive. The core reports it as needing confirmation (see the README's "Destructive operations" choice); the UI shows a dialog naming the repo and the files, and the core refuses a discard sent without confirmation.
- Several files at once: the file lists get multi-selection (Ctrl/Cmd and Shift) and "Stage all" / "Unstage all" per list; one job per action.
- UI (UI-15): stage and unstage buttons per file row, per list and per hunk in the diff; discard per file and per hunk; double-click a row moves it to the other list. Unmerged files have no stage or discard controls until the conflict editor (M2-10).
- Each action is a job (ARCH-7). After it the lists and the diff are re-read (OPS-7), and the selection stays on the same file when it still exists.
- Fake core: staging moves rows between lists.
- Tests (git integration, real `git` on fixture repos):
  - `ui_15_stage_and_unstage_a_file` for modified, added, deleted, renamed, mode-changed and untracked files, and unstage on an unborn branch.
  - `ui_15_stage_and_unstage_a_hunk`: on a file with three hunks, staging the second leaves the index with exactly that change; unstaging it restores the index.
  - `ui_15_discard_a_file_and_a_hunk`, including an untracked file and an untracked folder.
  - `grp_9_discard_needs_confirmation`: refused without it, run with it.
  - `ui_15_stale_hunk_is_refused`: the file is edited between reading the diff and staging the hunk.
  - Failure: staging in a repo whose index is locked (`index.lock` present) ends *failed* with git's message.
  - Vitest: controls per file, per list and per hunk send the right request; discard shows the confirmation first.

## Out of scope
- The commit message box, Commit and Amend (M2-05).
- Line-level staging (section 2, LATER).

## Acceptance
- `just dev`: stage one hunk of a file, commit it from a terminal, and see only that hunk committed.
- All tests listed above exist and pass on all three platforms, and `just check` passes.
