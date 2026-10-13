# M2-12 Tags and reset

## Goal
The user creates, deletes and pushes tags, and resets the current branch to a commit soft, mixed or hard, with a confirmation before a hard reset.

## Depends on
M2-07, M2-08

## Spec
Implements: GRP-9
Respects: SEC-4, ARCH-1, ARCH-3, ARCH-7, GRP-8, GRP-12, OPS-7, UI-13, D10, section 5 "Create tag X" row, section 6 "Tags" and "Reset" rows

## In scope
- `gitcroc-git::ops`, single repo:
  - Create tag at a commit (`HEAD` by default, or the commit of "create tag here", UI-13): annotated with a message (default) or lightweight. The name is checked by SEC-4 (M2-06). An existing name is refused. "Push tag" (default off) pushes it after creating, as one job.
  - Delete tag: `git tag -d`; "also delete on the remote" (`git push <remote> --delete refs/tags/<x>`) asks first.
  - Push tag: `git push <remote> refs/tags/<x>`, with askpass (M2-03), to `origin` or the only remote, otherwise the dialog asks which.
  - Reset the current branch to a commit: `git reset --soft|--mixed|--hard <hash>`, from the history's context menu (UI-13). Hard is destructive (GRP-9): the confirmation names the repo, the branch, the commit and the number of changed files that will be lost. A detached `HEAD` resets `HEAD` and says so.
- Tree: tag rows get a context menu with Push and Delete. History (UI-13): "create tag here" and "reset current branch to here" are enabled.
- Tests (git integration, each with success and failure):
  - Create annotated and lightweight tags; the push-after-create option; an existing name refused; an invalid name refused before git runs (SEC-4).
  - Delete a tag, also on the remote with confirmation.
  - Push a tag to a `file://` remote; push to a remote that already has a different tag of that name fails with git's message.
  - Reset soft, mixed and hard: the index and working tree as git defines each; `grp_9_hard_reset_needs_confirmation`.
  - Vitest: tag and reset dialogs, and the hard reset confirmation names what will be lost.

## Out of scope
- Group tag creation (section 5; M3).

## Acceptance
- `just dev`: tag a commit from the history and push the tag; reset hard to an earlier commit after the confirmation.
- All tests listed above exist and pass on all three platforms, and `just check` passes.
