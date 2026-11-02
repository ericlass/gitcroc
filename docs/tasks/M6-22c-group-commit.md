# M6-22c Group commit

## Goal
For a group or a multi-selection with dirty repos, the merged "Uncommitted
changes (n repos)" row leads to one commit message box under the lists
sectioned by repo. Its Commit runs section 5's group Commit through M6-22a's
preflight: one commit per repo with staged changes, all with the same
message.

## Depends on
- M6-22a: the preflight flow, its dialog and the report.
- M6-16: the commit box, and the Staged and Unstaged lists sectioned by
  repo for a group or several repos.
- M6-11d: the merged uncommitted row.

## Spec
Implements: UI-34, GRP-15
Respects: GRP-5, GRP-6, GRP-7, GRP-12, UI-15, UI-22, UI-29, SEC-3, ARCH-6, ARCH-9, D15, QA-2, QA-3, QA-4

Also implements the "Commit" row of section 5's "Group-capable operations".

The core counts each repo's staged files for the confirmation (GRP-15),
skips a repo with nothing staged and commits each repo on its own (GRP-6).
The box keeps the draft and sends the operation, as rc.1's
`ui/src/stores/commit.ts` does for a group (ARCH-6).

## In scope
- **The group commit box**, a port of the group parts of
  `ui/src/changes/CommitBox.tsx` and `ui/src/stores/commit.ts`:
  - a subject and a body, as UI-15's box, with no Amend (UI-34);
  - the options above it: Stage all tracked changes first and Push after
    commit, both off by default;
  - Commit is labelled with the number of repos that will commit, such as
    "Commit 3 repos", counting repos with staged files, or with tracked
    changes when Stage all is on; disabled with no subject or no such repo;
  - Ctrl or Cmd and Enter commits, as in UI-15's box.
- **The draft**: the message and the options are kept per target set
  (the group, or the set of selected repos) and per workspace (UI-29),
  across a selection change and back.
- **Running** it: Commit sends `GroupOp::Commit` through the preflight;
  the confirmation, which GRP-15 always asks for, lists each repo with
  its number of staged files. While the job runs the box is busy. When it
  ends, the box is cleared only if every repo that ran committed; else it
  keeps the message and says a repo did not commit, and the report says
  why (GRP-7). A commit rejected by a hook is *failed* there, and the
  others still commit (GRP-6).
- **Strings (D15):** the group commit box's text moves from
  `ui/src/strings/en.ts` to `strings.rs`.

## Tests (QA-3)
Real git on `repo-gen` fixtures (QA-2), with a group of four repos: two
with staged changes, one with only unstaged ones, and one with staged
changes and a `pre-commit` hook that fails.
- `ui_34_uncommitted_row_sections_lists_by_repo_and_labels_commit_with_the_repo_count`:
  and Stage all changes the count.
- `ui_34_no_amend_in_the_group_view`.
- `grp_15_the_confirmation_lists_each_repo_with_its_staged_files`.
- `grp_15_one_commit_per_repo_with_the_same_message`: a repo with nothing
  staged is skipped, and no commit is linked to another.
- `grp_6_a_hook_rejecting_one_commit_does_not_stop_the_others`: the box
  keeps the message and the report says which failed.
- `ui_34_the_draft_is_kept_per_target_set`.
- `grp_15_push_after_commit_pushes_the_repos_that_committed`.
- `sec_3_the_message_and_repo_names_render_as_text`.
- `ui_23_no_hardcoded_colors`, `d15_ui_text_comes_from_the_strings_module`
  and the keymap docs test keep passing over the new files.

## Out of scope
- **The palette's commit commands** (UI-22): M6-24.
- **AI commit messages:** out of v1 (section 2).

## Acceptance
- **On the reference machine,** `just dev-next` on `target-project`:
  stage changes in three repos of a group, commit them with one message,
  with Stage all once, and see one commit in each.
- `just coverage M6` no longer lists UI-34 or GRP-15.
- `just check` passes.
