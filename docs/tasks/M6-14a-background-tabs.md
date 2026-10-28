# M6-14a Background tabs let go of their history and diffs

## Goal
A tab in the background holds no history rows, file lists or diffs, so
memory grows with the active tab, not with the number of tabs (UI-31). When
it becomes active again, it reads them again and looks exactly as it was
left (UI-29), as rc.1's tabs do.

This is the first of two parts of M6-14; M6-14b is `just bench-ui`.

## Depends on
- M6-13b: the highlights and images a diff holds.
- M6-13c: the per-tab layout that must survive the switch.

## Spec
Implements: UI-31
Respects: UI-29, UI-30, ARCH-6, ARCH-7, ARCH-8, ARCH-9, PERF-1, PERF-2,
OPS-7, QA-3, QA-4

## In scope
- **The core is unchanged.** `TabSwitch::background` already ends the
  tab's history walk and changes view, so their handles give `None`, and
  keeps its summaries, tree and loaded refs (rc.1's M3-03).
- **When a tab goes to the background**, the window tells that tab's panes,
  as rc.1's `App.tsx` calls `history.release` and `changes.release`:
  - **History:** its rows and walk handle go. Where the user was is kept
    under the source's key, as when the tree selection changes: the number
    of rows read, the scroll position, the selected row and the search
    text.
  - **Changes pane:** its file lists go. The selection it shows, the
    selected file by list and path (and repo, in a merged view), the
    collapsed folders and the list's scroll position stay.
  - **Diff:** its content, highlights and images go; a highlighting task
    still running is dropped, and the images are released from the GPU as
    when the diff changes. The scroll position, wrap, the hunk in focus and
    a large diff's **Load diff** choice stay.
  - A read still running for the tab is dropped when it answers, as a
    superseded read is.
- **When the tab becomes active again:**
  - the history reads as many rows as it had and scrolls back, with the
    same row selected, as rc.1's `history.ts` does;
  - the changes pane reads its view and selects the same file, and the
    diff is read and scrolled back.
- **Kept as they are:** the tree, its filter and expanded nodes, the
  summaries, the history columns, the pane layout and dock, and every
  other position of UI-29. Nothing here changes what the tree pane holds.

## Tests (QA-3)
- `ui_31_a_background_tab_releases_history_and_diffs`: after a switch to
  another tab, the background tab's history holds no rows, its changes
  pane no view and its diff no content, highlights or images; the core's
  history handle for it gives `None`.
- `ui_31_returning_reads_the_same_rows_and_scrolls_back`: with the
  history scrolled into its second page, a commit selected, a file
  selected and its diff scrolled, a switch away and back shows the same
  row selected at the same scroll position, the same file and the diff at
  the same offset.
- `ui_31_a_search_is_kept`: a history filtered by a search text reads the
  same filtered rows on return.

## Out of scope
- **`just bench-ui`** and its `memory-two-tabs` scenario: M6-14b.
- **Tab marks for jobs** (UI-32).

## Acceptance
- **On the reference machine,** `just dev-next` with two tabs on
  `target-project`: scroll deep into the history and open a large diff,
  switch tabs and back; the history, file and diff come back where they
  were.
- `just coverage M6` no longer lists UI-31.
- `just check` passes.
