# M2-05 Commit and amend

## Goal
The user writes a subject and body and commits the staged changes, or amends the last commit, with hooks, sign-off and signing as git config says.

## Depends on
M2-04

## Spec
Implements: UI-15, SEC-6
Respects: ARCH-1, ARCH-3, ARCH-6, ARCH-7, ARCH-9, OPS-7, GRP-12, D15, section 6 "Working copy" notes

## In scope
- Commit message box under the file lists: a subject line and a body. The message is the subject, a blank line and the body; trailing whitespace is trimmed and an empty body adds nothing. Commit is disabled while the subject is empty or nothing is staged; Amend is disabled on an unborn branch.
- `gitcroc-git::ops`:
  - Commit: `git commit -F -`, message on stdin. No `--no-verify`, no `-S`, no `--signoff`: hooks, `commit.gpgSign` and `format.signOff` apply exactly as on the command line (section 6, SEC-6). git's own template and `commit.cleanup` apply as with `-F`.
  - Amend: `git commit --amend -F -` with the message in the box. Choosing Amend fills the box with the last commit's message once, if the box is empty; the staged changes are added to it.
- A hook that rejects the commit ends the job *failed* with the hook's output in the job's output, and the message stays in the box. A successful commit clears the box.
- The unsent message is kept per repo, per workspace (ARCH-9), while the app runs; switching repos does not lose it.
- In conflict state the box is hidden: the commit that ends a merge, cherry-pick or revert is made by Continue (M2-08).
- Tests (git integration):
  - `ui_15_commit_staged_changes`: subject and body arrive as the commit message; unstaged and untracked changes stay uncommitted.
  - `ui_15_amend_last_commit`: message replaced, staged changes added, parent unchanged.
  - `sec_6_hooks_run_as_on_the_command_line`: a `pre-commit` hook that exits 1 fails the job with its output; a `commit-msg` hook that edits the message is honoured.
  - `sec_6_sign_off_follows_git_config`: with `format.signOff` the commit has the trailer; without it, none.
  - Failure: commit with `user.email` unset and no global identity fails with git's message (fixture sets `HOME` and `XDG_CONFIG_HOME` to an empty folder).
  - Vitest: Commit disabled without subject or staged files; the message survives a repo switch; a failed commit keeps the message.

## Out of scope
- Group commit (GRP-15, UI-34; M3).
- GPG pinentry that needs a terminal (see M2-03).

## Acceptance
- `just dev`: stage, write a subject and body, commit; the history shows the commit at once (OPS-7) and the box is empty.
- All tests listed above exist and pass on all three platforms, and `just check` passes.
