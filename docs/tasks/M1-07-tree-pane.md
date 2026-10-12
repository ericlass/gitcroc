# M1-07 Tree pane

## Goal
The left pane shows the workspace's repos with their branches, remotes, tags and stashes, each repo row with its live summary, and the user can add repos to it.

## Depends on
M1-06

## Spec
Implements: UI-1, UI-3, UI-8, PERF-1
Respects: UI-2, UI-4, UI-5, UI-6, UI-7, GRP-5, ARCH-6, ARCH-8, ARCH-9, PERF-3, SEC-3, UI-23, section 6 "Sync" note (ahead and behind per branch)

## In scope
- `gitcroc-git` read API `refs(repo) -> Refs`: local branches (with upstream and ahead and behind against it), remote branches grouped by remote, tags, and stash entries (index, message, date). Ahead and behind per branch are computed when a repo's **Local** node is first expanded, not at start.
- `gitcroc-core`: the tree model for one workspace. Until groups exist (M3) every repo sits under **Ungrouped** (UI-1). A repo node contains **Local**, **Remotes**, **Tags** and **Stashes**. Branch names containing `/` fold into folders, with folders before branches and both sorted by name.
- Filter (UI-8): the core filters the tree by case-insensitive substring of repo names and ref names and returns the matching nodes with their ancestors, so the UI keeps no filtering logic (ARCH-6). Results arrive while the user types; each keystroke supersedes the previous request.
- IPC: `tree_get`, `tree_filter`, `refs_get`, plus the `repo_summary` event from M1-06.
- UI (`ui/src/tree/`):
  - Virtualized with TanStack Virtual (PERF-1): the tree is flattened into rows by the expanded state, and the DOM holds only the visible rows plus a small overscan.
  - Repo row (UI-3): name, current branch or detached hash, ahead and behind, a dot when dirty, a conflict icon in conflict state, a spinner while its summary is being refreshed, a warning icon when the path is missing, and a stale style while the summary comes from the cache (PERF-3).
  - Branch, remote branch, tag and stash rows show their names; local branches show ahead and behind. The current branch is marked.
  - Single selection of any row with mouse and keyboard (up, down, left to collapse, right to expand). Multi-selection is M3 (UI-6), double-click checkout and operation menus are M2 (UI-7).
  - Selection and expanded nodes live in a store keyed by workspace id (ARCH-9).
  - An "Add repository" button above the tree, and a placeholder with the same button when the workspace is empty. Both use the M1-05 folder picker and show the add result: added, already present, refused with reason.
  - A repo row's context menu has "Remove from workspace", the only repo action that exists in M1.
  - All repo text is rendered as text, never as HTML (SEC-3). Colors only from theme tokens (UI-23).
- Fake core: tree, refs and summaries for front-end tests, including a large tree for the virtualization test.
- Tests:
  - `ui_1_tree_structure` (Rust): repos under Ungrouped, the four ref nodes per repo, `feature/a/b` folded into `feature` and `a`.
  - `ui_8_filter_matches_repos_and_branches` (Rust): a substring match keeps the node and its ancestors and drops the rest; case-insensitive.
  - `UI-3 repo row` (Vitest): each state of the row, from fake summaries: clean, dirty, ahead and behind, conflict, refreshing, missing, stale.
  - `PERF-1 tree is virtualized` (Vitest or Playwright): with 5,000 rows expanded, the DOM holds no more rows than fit plus the overscan, and scrolling to the end shows the last row.
  - `UI-8 filter` (Playwright, fake core): typing filters the tree; clearing restores it.
  - Refs table test: local branch with and without upstream, remote branches of two remotes, lightweight and annotated tags, two stashes.

## Out of scope
- Groups, group rows, a group's Branches node, multi-selection, drag between groups (M3: UI-2, UI-4, UI-5, UI-6).
- Double-click checkout and operation context menus (M2: UI-7).
- What selecting a row shows in the history pane (M1-08).

## Acceptance
- `just dev`: adding a folder of repos shows them in the tree; expanding a repo shows its branches, remotes, tags and stashes; on the next start the tree appears from the cache before the summaries refresh.
- All tests listed above exist and pass on all three platforms.
