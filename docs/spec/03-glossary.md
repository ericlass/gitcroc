# 3. Glossary

These terms are used with exactly these meanings in the spec, the code and the UI.

| Term | Meaning |
| --- | --- |
| Repo | One local Git repository with a working tree, identified by its absolute path. Bare repos are not supported in v1 |
| Workspace | A named set of repos and groups. Each open workspace is one tab in the window (UI-27). A user can have many workspaces |
| Group | A named, ordered set of repos inside one workspace (D7) |
| Ungrouped | The built-in tree node listing workspace repos that belong to no group. It is not a group and has no group operations |
| Target set | The list of repos an operation will run on: one repo, a group's members, or a multi-selection of repos |
| Operation | One git action the user starts, for example pull or checkout |
| Group operation | An operation whose target set has more than one repo |
| Job | The running instance of an operation. A group operation is one job with one sub-job per repo |
| Preflight | The read-only check that runs before a group operation and predicts the outcome per repo |
| Result report | The per-repo outcome list shown when a group operation ends |
| Working copy | The uncommitted state of a repo: staged changes, unstaged changes and untracked files |
| Conflict state | A repo that git has stopped mid-operation with unmerged paths (merge, rebase, cherry-pick, revert, stash apply) |
| Hunk | In a diff: one contiguous block of changed lines. In a conflict: one block between conflict markers |
| Provider | A configured way to reach an AI model: an API endpoint with a key, or an agent CLI |
| Proposal | The AI's suggested resolution for a conflicted file. It exists only in memory until the user accepts it (D9) |
| Default branch | The branch `origin/HEAD` points to; if unset, `main`, then `master`, whichever exists |
