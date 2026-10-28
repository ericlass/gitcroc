# M6-11c History search, uncommitted row and commit-graph offer

## Goal
The history pane gains rc.1's search box, its "Uncommitted changes (n)"
first row and the commit-graph offer.

This is the third of four parts of M6-11.

## Depends on
- M6-11a: the history source and list.

## Spec
Implements: UI-12, UI-10
Respects: PERF-2, ARCH-6, ARCH-7, ARCH-8, ARCH-9, PERF-3, UI-22, UI-23, D15, QA-3, QA-4

## In scope
- **Search** (UI-12), a port of the search parts of
  `ui/src/history/HistoryPane.tsx`:
  - gpui-component's input above the list, named "Search history", with
    the placeholder "Search subject, author or hash";
  - each change sends the text to `history_get`; the core filters, the UI
    filters nothing;
  - a change supersedes the previous request, whose answer is dropped;
  - filtered rows have no graph lines;
  - the search text is kept per workspace and per tree selection, and comes
    back with the rest of the history.
- **The uncommitted row** (UI-10):
  - a repo or its current branch shows "Uncommitted changes (n)" first
    when its working copy is dirty and no search is shown;
  - `n` is the repo summary's changed-path count;
  - a branch that is not checked out, a tag, a stash and a clean repo do
    not show it;
  - selecting it reports the repo's uncommitted target to the changes pane
    (M6-12).
- **The commit-graph offer** (PERF-2):
  - the first page's `offer_commit_graph` shows an offer under the search
    box, once per repo: write the file or don't ask again;
  - "Don't ask again" calls `commit_graph_decline` and keeps the answer in
    the workspace;
  - writing calls `commit_graph_write`, which runs as a job; its progress
    and end arrive as events;
  - a failed write shows why inline and is dismissible; history keeps
    working without the file;
  - an offer answered this session is not shown again.
- **Strings (D15):** the search, uncommitted and offer text moves to
  `strings.rs`.
- **Test support:** a fixture can make a repo dirty, and can force or refuse
  a commit-graph write.

## Tests (QA-3)
- `ui_12_search_filters_in_the_core`:
  - typing sends the text to the app and shows the core's answer as it is;
  - an older answer that arrives late never replaces a newer one;
  - filtered rows show no graph;
  - the text comes back after leaving and returning to the selection.
- `ui_10_uncommitted_row_leads_a_dirty_repo`:
  - a dirty repo shows the row first with its count;
  - selecting it reports the uncommitted target;
  - a branch that is not current has no row;
  - the row goes when the working copy is clean.
- `perf_2_commit_graph_offer_once_per_repo`:
  - a slow first page without the file shows the offer once;
  - declining keeps the answer and hides the offer, including after
    another selection in the same repo;
  - writing starts the job and a failure shows inline.
- `d15_ui_text_comes_from_the_strings_module` keeps passing over the new
  files.

## Out of scope
- **The merged uncommitted row (UI-34):** M6-11d.
- **The changes pane's Staged and Unstaged lists and the commit box
  (UI-15, UI-34):** M6-12.
- **The conflict banner (OPS-2):** M6-19.

## Acceptance
- **On the reference machine,** `just dev-next`:
  - searching narrows a repo's history as it types;
  - a dirty repo leads with the uncommitted row;
  - the commit-graph offer appears once and its answer sticks.
- `just coverage M6` no longer lists UI-10 or UI-12.
- `just check` passes.
