# 2. Goals, non-goals and v1 scope

v1 is done when one person can run a project of the D5 first-target size (about 20 repos) day to day from this client without opening a terminal for the operations in section 6.

## Goals

1. Stay responsive at the D5 scale. Section 9 gives the measurable budgets.
2. Treat a set of repos as one unit: pull, switch branch, create branch and similar in one action, with a clear per-repo outcome.
3. Keep every repo individually controllable inside a group, so one repo can sit on a different branch than the rest.
4. Offer AI conflict resolution wherever a conflict can occur, using the user's own AI account.
5. Behave exactly like command-line git for anything that changes a repo (hooks, config, credentials, LFS filters all apply).
6. Run on Windows, macOS and Linux from one codebase with a modern, themeable UI.

## Non-goals for v1 (LATER)

- Pull request, issue or CI integration with GitHub, GitLab, Bitbucket or others.
- Interactive rebase UI (reorder, squash, edit). Plain rebase is in v1.
- Submodule management. Submodules are shown as read-only entries.
- Worktree management, sparse checkout UI, partial clone UI.
- Git LFS UI (locking, storage view). LFS files work because git runs the filters.
- Blame view, file history view, bisect, reflog browser, patch import/export.
- Line-level staging. v1 stages whole files and whole hunks.
- Nested groups, cross-repo atomic operations, cross-repo commit linking.
- AI features other than conflict resolution (commit messages, review, explain).
- Multiple windows, two tabs on the same workspace, tabs for anything other than a workspace, built-in terminal, plugin system, custom theme import, translations.
- Mobile, web, ARM Linux, Windows on ARM builds.

## Out of scope permanently

- Hosting the AI. The user always brings their own account (D4).
- Any paid tier, account system or telemetry (D12).
