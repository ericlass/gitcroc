# 6. Git operations in v1

This table is the complete list of git operations in v1; anything not listed is LATER. The ten operations in section 5 also run on a single repo with the same options.

| Area | Operations | Notes |
| --- | --- | --- |
| Repos | Add existing folder, clone, remove from workspace, open in file manager, open in terminal | Clone shows progress and can add the repo to a group on completion. "Add folder" can scan a parent folder and add every repo found, to a depth of 4 |
| Working copy | Stage and unstage by file and by hunk, discard by file and by hunk, commit, amend last commit | Commit message box with subject and body. Sign-off and GPG signing follow git config |
| Branches | Checkout, create, rename, delete, set upstream, checkout of a remote branch, checkout of a commit (detached) | Rename is single-repo only |
| Sync | Fetch, pull, push | Ahead and behind counts shown per branch |
| Integrate | Merge, rebase (non-interactive), cherry-pick one or several commits, revert a commit | Each can abort or continue from conflict state |
| Stash | Stash (with optional message, optional untracked files), apply, pop, drop | Single-repo only |
| Tags | Create, delete, push |  |
| Reset | Reset current branch to a commit: soft, mixed, hard | Hard is destructive (GRP-9) |
| Remotes | Add, rename, remove, edit URL |  |

## Conflict state

- **OPS-1** The app detects conflict state from the repo itself (`MERGE_HEAD`, `rebase-merge`, `rebase-apply`, `CHERRY_PICK_HEAD`, `REVERT_HEAD`, unmerged index entries), not from its own memory. A conflict started in a terminal looks the same as one started in the app.
- **OPS-2** While a repo is in conflict state, the history pane shows a banner naming the operation, with **Abort** and **Continue**. Continue is enabled only when no unmerged paths remain.
- **OPS-3** The changes pane lists conflicted files first. Each has a conflict editor showing the result with every conflict hunk highlighted and three buttons per hunk: take ours, take theirs, take both. The result is freely editable.
- **OPS-4** "Ours" and "theirs" are always labelled with the real branch or commit they come from, because their meaning flips between merge and rebase.
- **OPS-5** **Mark resolved** stages the file. It is refused while conflict markers remain, with an override for files that legitimately contain marker-like text.
- **OPS-6** Conflicts the editor cannot represent as text hunks (binary file, deleted on one side, both added with different modes, submodule) show a choice between the two versions instead of an editor.

* **OPS-8** The conflict editor builds its hunks from index stages 1 to 3 (base, ours, theirs), merged in memory in diff3 style. It does not parse the markers in the file on disk. If the file on disk was already edited outside the app, the editor asks: keep the file as it is, or start over from the conflict.
* **OPS-9** Nothing is written to the conflicted file until **Mark resolved**. Unsaved editor state survives switching files and repos, and is dropped on Abort.

## Refresh

- **OPS-7** After every job the affected repos are re-read. Outside jobs, a repo refreshes when its `.git` directory changes on disk, and all repos refresh when the window regains focus.
