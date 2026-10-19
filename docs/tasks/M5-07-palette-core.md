# M5-07 Command palette in the core

## Goal
The core answers a command palette query for a workspace: the operations and the repos, groups and branches whose text matches, ranked and bounded, so the UI only shows the list (UI-22, ARCH-6).

## Depends on
M4

## Spec
Implements: UI-22 (the palette's contents; M5-08 shows it, M5-09 completes keyboard reach)
Respects: ARCH-6, ARCH-7, ARCH-8, ARCH-9, AI-16, UI-8, UI-20, GRP-5, PERF-1, section 5 "Group-capable operations", section 6 table

## In scope
- `gitcroc-core::palette`, the catalog of operations: one entry per operation, each with a stable id (a generated enum, `PaletteOp`), a label, search keywords, and what it acts on: the target set, the selected tree row (a branch, tag, stash or remote), the selected commit, the selected file, the repo in conflict state, or the app. The labels are English text in this module, as the core's other text is (README "D15 in the core").
- The catalog lists every operation of sections 5 and 6, and the actions around them:
  - Repos: add folder, clone, remove from workspace, open in file manager, open in terminal.
  - Working copy: stage file, unstage file, discard file, stage all, unstage all, commit, amend. Hunks are reached in the diff (M5-09), not the palette.
  - Branches: checkout, create, rename, delete, set upstream, check out a remote branch, check out a commit (detached).
  - Sync: fetch, pull, push, each also "with options…".
  - Integrate: merge, rebase, cherry-pick, revert; abort and continue in conflict state; mark resolved; take ours, take theirs, take both for the hunk at the cursor; keep ours or keep theirs for a file the editor cannot show (OPS-6).
  - Stash: stash, apply, pop, drop. Tags: create, delete, push. Reset: soft, mixed, hard. Remotes: add, rename, remove, edit URL.
  - Section 5's group forms (fetch, pull, push, checkout X, create X, delete X, merge, rebase, create tag, commit) are the same entries: they act on the target set, as the toolbar does (UI-20).
  - AI: Resolve with AI, Resolve all with AI (AI-3), Accept all high-confidence (AI-11).
  - App: Settings, New workspace, Import workspace, Export workspace, open the job list, close the tab, next and previous tab, new group.
- `palette_query(workspace_id, text)` returns `PaletteResult | null`:
  - Items of four kinds: operation (id and label), repo (id, name, current branch), group (id, name, member count), branch (repo id and name, full ref name, local or remote).
  - Repos and groups are those of the workspace. Branches are every local and remote branch of its repos; refs not loaded yet are read for it without ahead and behind, as the tree filter does (M1-07). Tags and stashes are not listed.
  - While AI features are off (AI-16), the AI entries are not in the catalog's answer at all. Everything else is listed whether or not it can apply now; the UI disables it, as the toolbar does (UI-20).
  - Matching (README "Palette matching"): case-insensitive; every word of the query must occur in the item's text (an operation's label and keywords; a repo's or group's name; a branch's full name and its repo's name). An empty query returns the operations only, in catalog order.
  - Ranking: the whole name equals the query, then the name starts with it, then a word of the name starts with it, then anywhere. Ties: operations, repos, groups, branches; then catalog or tree order.
  - At most 100 items, with the total count (ARCH-8).
  - It works as `tree_filter` does (M1-07): an async command that reads refs off the IPC thread, and a newer query of the workspace supersedes the older one, which returns `null`.
- Tests (core, with fixture repos and workspaces):
  - `ui_22_palette_lists_every_operation_of_sections_5_and_6`: against a table in the test that names each operation of the two sections.
  - `ui_22_palette_finds_repos_groups_and_branches`, including a remote branch and a branch of a repo whose refs were not loaded.
  - `ui_22_palette_matches_every_word_in_any_order`, ranking order, ties, the empty query.
  - `ui_22_palette_returns_at_most_100_items_with_the_total`.
  - `ui_22_palette_newer_query_supersedes`.
  - `ai_16_palette_has_no_ai_entries_while_ai_is_off`.
  - `arch_9_palette_is_per_workspace`: two workspaces, each query sees only its own.

## Out of scope
- The palette's window and running an entry (M5-08). Deciding whether an entry can apply now (the UI's existing rules).

## Acceptance
- The tests above exist and pass on all three platforms, the IPC types are generated, and `just check` passes.
