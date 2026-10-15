# M3-13 Group commit

## Goal
For a group or several repos with uncommitted changes, the user stages in each repo from one view and commits them all with one shared message, one independent commit per repo.

## Depends on
M3-07, M3-09

## Spec
Implements: GRP-15, UI-34
Respects: GRP-6, GRP-7, GRP-8, GRP-12, UI-10, UI-15, SEC-6, ARCH-6, ARCH-8, PLAT-2, section 5 "Commit" row

## In scope
- Merged list (UI-34): when at least one repo of the target set is dirty, the first row is "Uncommitted changes (n repos)", above the merged commits. In conflict state the single-repo banner rules of UI-10 do not apply here. A repo in conflict state is listed but blocked (GRP-8).
- Selecting that row shows UI-15 sectioned by repo:
  - The Staged and Unstaged lists each have one section per dirty repo, in target-set order, with the repo name as its header and the M2-04 file controls.
  - Stage, unstage and discard run as single-repo jobs of that repo (M2-04), and discard keeps its confirmation (GRP-9).
  - Selecting a file shows its diff with the hunk controls, as for one repo.
- Core: `changes_get` for a merged target returns one view per dirty repo in one response. Each list keeps the core-side paths and the paging rule of M1-09, so no path crosses back (PLAT-2) and nothing unbounded is sent (ARCH-8).
- One commit message box: subject and body, kept per workspace and target set while the app runs (as M2-05 keeps it per repo).
  - The Commit button reads "Commit 3 repos", counting repos with staged changes. It is disabled at 0.
  - Amend is not offered (GRP-15).
- Group Commit (section 5 row), through the M3-08 engine:
  - *will skip* "nothing staged". With "Stage all tracked changes first" (default off), a repo with tracked changes counts as staged, and the sub-job runs `git add -u` before committing.
  - The confirmation always appears, even when every repo will run. It lists each repo with its number of staged files before anything is committed (GRP-15).
  - Each repo gets its own commit with the same message, through `git commit -F -` as in M2-05. Hooks and signing follow git config (SEC-6).
  - A commit rejected in one repo, for example by a hook, is *failed* with git's output, and the others still commit (GRP-6). The commits are not linked in any way.
  - "Push after commit" (default off) pushes each repo that committed to its upstream in the same sub-job, with the M3-10 push rules. A push that fails after the commit succeeded makes the repo *failed*, and the message says the commit was made and the push failed.
  - The message box is cleared only when every repo that ran committed.
- Tests:
  - `grp_15_one_commit_per_repo_with_the_same_message`: three repos with staged changes and one without. Three commits with equal messages, unrelated parents, and the fourth *skipped (nothing staged)*.
  - `grp_15_a_hook_rejection_fails_one_repo_only`: a `pre-commit` hook that exits 1 in one repo.
  - `grp_15_confirmation_lists_staged_file_counts`: the preflight carries the counts, and `group_start` refuses without confirmation even when every repo will run.
  - Stage all tracked first, and push after commit with one rejected push.
  - `ui_34_uncommitted_row_sections_lists_by_repo_and_labels_commit_with_the_repo_count` (Vitest).
  - No Amend in the group view (Vitest).

## Out of scope
- Cross-repo commit linking (section 2, LATER).

## Acceptance
- With `just dev`, a group with changes in three repos shows "Uncommitted changes (3 repos)". After staging in two of them, "Commit 2 repos" commits both with one message.
- All tests listed above exist and pass on all three platforms, and `just check` passes.
