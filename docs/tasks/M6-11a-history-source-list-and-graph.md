# M6-11a History source, list and graph

## Goal
`gitcroc-next`'s middle pane shows the history of what the tree selects, as
rc.1 does: a repo's whole history, or the history reachable from a branch,
remote branch, tag or stash. The list is virtualized and paged, with the
spike's graph painter. Each tab keeps its own history.

This is the first of four parts of M6-11. The columns are M6-11b; search,
the uncommitted row and the commit-graph offer are M6-11c; the merged list,
summary strip and the merged uncommitted row are M6-11d.

## Depends on
- M6-10: the tree whose selection the history follows.
- M6-07: the tokens the rows draw with.
- M6-09: the tabs, whose active workspace the history shows.

## Spec
Implements: UI-9, UI-29
Respects: ARCH-6, ARCH-7, ARCH-8, ARCH-9, PERF-1, PERF-2, OPS-7, UI-22, UI-23, SEC-3, D15, QA-3, QA-4

## In scope
- **One history per tab** (ARCH-9, UI-29). The window keeps a history pane
  per open workspace, made when its tab is first shown and dropped when the
  tab closes. Switching tabs shows that tab's history as it was left: the
  source, the loaded rows, the selected row and the scroll position.
- **The source follows the tree** (section 8 "History pane" table):
  - a repo, or one of its four ref nodes, shows the whole repo;
  - a local branch, remote branch or tag shows the history reachable from
    that ref;
  - a stash shows the stash's commit row;
  - Ungrouped and nothing selected show the empty state.

  A repo's history has one source wherever the repo appears (UI-2), so both
  places show the same loaded rows. The tree pane reports the selected item
  to the window in addition to its target set (GRP-5).
- **Rows come from the core** (ARCH-6): `history_get` starts a walk and
  `history_more` continues it, 500 commits a page (ARCH-8, PERF-2). A newer
  request supersedes an older one, whose answer is dropped. The core builds
  the graph lanes; the UI only draws them.
- **Rows are prepared off the UI thread** (ARCH-7): the page's text becomes
  `SharedString`s and dates are formatted before it reaches the pane.
- **The list** (`crates/gitcroc-ui/src/history.rs` and
  `crates/gitcroc-ui/src/history_pane.rs`, a port of the read-only parts of
  `ui/src/history/HistoryPane.tsx` and `ui/src/stores/history.ts`, plus the
  spike's list):
  - a `uniform_list` of fixed-height rows (PERF-1), with role listbox,
    named "History";
  - the default columns: graph, subject with ref labels, author, date and
    short hash (UI-9);
  - the graph from the core's edges, with the spike's fast path for
    straight lanes and colours from the tokens (UI-23);
  - ref labels for `HEAD`, the current branch, local branches, remote
    branches and tags, each named for screen readers;
  - the author date as `YYYY-MM-DD HH:MM` in the commit's recorded time
    zone, with the full author and committer dates as its description;
  - placeholder rows below the loaded ones while a page follows;
  - the empty states for no source, no commits and an unreadable history.
- **Reading on**:
  - nearing the end of the loaded rows asks for the next page;
  - a `RepoSummary` whose `refs_version` rose rereads the shown history,
    keeping the position and the selected commit while it still exists
    (OPS-7);
  - events that were missed reread the shown history.
- **Selection:**
  - a click selects a row;
  - Up and Down move the selection, Page Up and Page Down move a page,
    Home and End go to the first and last loaded row;
  - the selected row scrolls into view;
  - the pane reports the selected row to the changes pane (M6-12), as the
    uncommitted row or the commit of its repo.
- **Regions (UI-22):** F6's history region is the active tab's history,
  with the focus on the list.
- **Names are text** (SEC-3). Every colour comes from the tokens (UI-23).
- **Strings (D15):** `history` moves from `ui/src/strings/en.ts` to
  `strings.rs`, without the entries the later parts use.
- **Test support:** a fixture can make repos with `repo-gen`, add them
  through the app, and select a tree row.

## Tests (QA-3)
- `ui_9_history_shows_graph_columns_and_refs`:
  - a repo's rows show the graph, the subject with ref labels, the author,
    the author date and the short hash;
  - the graph draws the core's edges and the commit's dot;
  - the ref labels name the current branch, a remote branch and a tag.
- `ui_9_branch_tag_and_stash_show_their_history`:
  - a branch row shows only commits reachable from that branch;
  - a tag and a remote branch do the same;
  - a stash shows its commit row;
  - Ungrouped shows the empty state.
- `perf_1_history_lays_out_only_the_rows_on_screen`.
- `arch_8_history_loads_pages_as_the_list_scrolls`: the first page arrives
  before the list asks for the next one, and scrolling to its end asks for
  exactly the next page.
- `ui_29_each_tab_keeps_its_history`: source, loaded rows, selected row and
  scroll position come back after a tab switch.
- `sec_3_history_subjects_and_ref_names_render_as_text`: a subject and a
  ref name with markup show as written.
- `ui_23_no_hardcoded_colors`, `d15_ui_text_comes_from_the_strings_module`
  and the keymap docs test keep passing over the new files.

## Out of scope
- **Columns (UI-36):** M6-11b. Here the default order and widths are drawn.
- **Search (UI-12), the uncommitted row (UI-10) and the commit-graph
  offer:** M6-11c.
- **A group's or several repos' merged history (UI-9), the summary strip
  (UI-11) and the merged uncommitted row (UI-34):** M6-11d.
- **The changes pane (UI-14, UI-16) and the commit context menu (UI-13):**
  M6-12 and M6-17.
- **The conflict banner (OPS-2):** M6-19.
- **Background tabs (UI-30, UI-31):** the history is kept while its tab is
  open; releasing it is M6-14's memory work.

## Acceptance
- **On the reference machine,** `just dev-next` on `target-project`:
  - selecting a repo, branch, tag, stash or remote branch shows its history;
  - scrolling loads pages without a blank row;
  - switching tabs restores each history.
- `just coverage M6` no longer lists UI-9 or UI-29 for the history.
- `just check` passes.
