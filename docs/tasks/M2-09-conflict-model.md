# M2-09 Conflict model in the core

## Goal
For each conflicted file the core builds the conflict hunks from the index stages, labels both sides with their real branch or commit, resolves the file on request, and refuses to mark it resolved while markers remain.

## Depends on
M2-08

## Spec
Implements: OPS-4, OPS-5, OPS-6, OPS-8
Respects: OPS-1, OPS-7, OPS-9, ARCH-1, ARCH-2, ARCH-3, ARCH-5, ARCH-7, ARCH-8, PLAT-2, DATA-3, SEC-3

## In scope
- `gitcroc-git` read API, `conflict(repo, path) -> Conflict`:
  - Reads stages 1, 2 and 3 of the path from the index (OPS-8). The file on disk is never parsed for markers.
  - A text conflict (stages 2 and 3 both regular files of the same mode, neither binary by git's rule: a NUL in the first 8,000 bytes, or the `binary` and `-diff` attributes of M1-03): merged in memory in diff3 style with `gix`'s blob merge, giving the result text with every conflict hunk as base, ours and theirs. A missing stage 1 (both added) merges against an empty base.
  - Anything else is a choice conflict (OPS-6), with its kind: binary, deleted by us, deleted by them, both added with different modes, submodule, or a type change. It lists what each side holds (size, mode, submodule commit).
  - The text is decoded lossily for display (M1's rule); a file that is not UTF-8 is a choice conflict, so a resolution never rewrites its bytes.
- Labels (OPS-4), from the conflict state the core read (OPS-1), each with the branch name where one points at the commit, else the short hash and subject:
  - merge: ours = the current branch; theirs = `MERGE_HEAD`.
  - rebase: ours = the commit being rebased onto (`onto`); theirs = the commit being replayed, with the branch being rebased (`head-name`).
  - cherry-pick: ours = the current branch; theirs = `CHERRY_PICK_HEAD`.
  - revert: ours = the current branch; theirs = "revert of" `REVERT_HEAD`.
  - stash apply: ours = the current branch; theirs = "stash".
- Edited outside the app (OPS-8): the core renders the stages with the repo's `merge.conflictStyle` (merge, diff3 or zdiff3) as git does, and compares with the file on disk, ignoring the label text after each marker. When they differ, `Conflict.edited_outside` is set, and the UI asks (M2-10).
- Actions, each a job (ARCH-7), each followed by a re-read (OPS-7):
  - Mark resolved (OPS-5) with the editor's text: refused while the text has a conflict marker line (a line starting with exactly seven `<`, `=`, `|` or `>` followed by a space or the line end) unless the request carries the override. Otherwise the core writes the text to the working tree file, the one direct write ARCH-1 allows, keeping the file's line endings (CRLF when the file on disk used CRLF) and its mode, then runs `git add -- <path>`.
  - Choose a side (OPS-6): ours or theirs with `git checkout --ours|--theirs -- <path>` and `git add`; a deleted side with `git rm -- <path>`; a submodule side with `git update-index --cacheinfo`.
  - A file not in conflict any more (resolved in a terminal) is refused with "already resolved" and the lists are read again.
- IPC: `conflict_get` takes the handle of the changes view (M1-09) and the file id of an unmerged file (PLAT-2) and returns the `Conflict`; `conflict_resolve` and `conflict_choose` return a job id. The result text crosses IPC once per file, at most 1 MB per side; a larger text file is a choice conflict (ARCH-8).
- Tests (git integration; conflicts made with `git` in fixtures, as a terminal user would):
  - `ops_8_hunks_come_from_the_index_stages`: the hunks equal the stages, also after the file on disk was overwritten; the merged text with markers equals `git merge-file --diff3 -p` on the same three blobs (test only).
  - `ops_8_detects_a_file_edited_outside`: unedited after merge, rebase, cherry-pick with each `merge.conflictStyle` → not set; one line changed → set.
  - `ops_4_labels_name_the_real_sides`: one case per operation, including that ours and theirs swap in a rebase.
  - `ops_5_mark_resolved_refuses_markers`: refused with a marker line; accepted with the override; accepted with marker-like text that is not a marker line (`<<<<<<<<` eight, or indented).
  - `ops_5_mark_resolved_writes_and_stages`: the file has the text, the path is staged, its mode and CRLF line endings are kept, and nothing else in the repo or `.git` was created (DATA-3).
  - `ops_6_choice_conflicts`: binary, deleted by us, deleted by them, mode conflict and submodule are classified, and each choice leaves the index resolved with the chosen side.
  - A conflict resolved in a terminal meanwhile is refused as already resolved.

## Out of scope
- The editor itself, its buttons and its unsaved state (M2-10).
- AI proposals (section 7; M4).

## Acceptance
- `gitcroc-cli conflict <repo-path> <path>` prints a file's conflict as JSON, labels included.
- All tests listed above exist and pass on all three platforms, and `just check` passes.
