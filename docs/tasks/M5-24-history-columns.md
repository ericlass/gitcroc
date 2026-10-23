# M5-24 History columns resize and move

## Goal
Asked for in M5-20's verification of `1.0.0-rc.1` (Eric, 2026-10-23): the history's columns can be resized and reordered. M1-08 made the widths resizable, but in practice they were not: the column edges were 3 px wide and invisible until hovered, an edge right of the subject did not follow the pointer (the column grew to the left and the subject shrank instead), and the widths were lost on restart. Eric's decisions: every column can be moved, the subject and graph included; the order and widths are kept per workspace with the pane layout; and a header menu (Move left, Move right, Reset columns) moves columns without a mouse.

## Depends on
M5-20

## Spec
Implements: UI-36 (new, in section 8 "History pane", added by this task)
Respects: UI-9, UI-19, UI-22, ARCH-6, ARCH-9, DATA-1, DATA-2, D15, section 10 "Files"

## In scope
- Spec: UI-36 after UI-9 in `docs/spec/08-ui.md`. UI-9's columns are the default order. Section 10's row for workspace files names the history columns.
- Workspace file: `[layout.columns]` with `order`, a list of column names, and the widths `graph`, `author`, `date` and `hash`, each optional. The schema stays 1; a file without it loads unchanged, and older builds ignore it (DATA-1). The order is read leniently: names this build does not know are dropped, repeats are dropped, and missing columns follow in their default order, so a newer or hand-edited file never makes the workspace read-only. An export leaves the layout out, as before (DATA-2).
- Resizing: an edge between two headers resizes the column on the side away from the subject, so the edge follows the pointer. The subject keeps at least 80 px. The edges are 8 px wide and show a divider line. Double-click resets a width; arrow keys on a focused edge resize, as before. A resize is saved when it ends.
- Moving: a column's header can be dragged before or after another header, with a drop marker. Moving saves at once.
- Header menu: right-click on a header, or Shift+F10 or the Menu key on a focused header, opens Move left, Move right and Reset columns; Move left is disabled on the first column, Move right on the last. Reset columns restores the default order and widths and removes `[layout.columns]` from the file. The headers are one tab stop; the arrow keys, Home and End move between them, and the focus stays on a moved column.
- Rows show their cells in the column order. In the merged list the repo column has the graph column's place and width.
- User docs: `docs/user/getting-started.md` says how to resize, move and reset the columns, and `docs/user/keyboard.md` names the header keys.
- Text comes from the strings module (D15).
- Tests:
  - Rust: `ui_36_history_columns_round_trip_in_workspace_file` (the table is written and read back; a partial table, unknown and repeated names are read leniently and the workspace stays writable).
  - Vitest: `ui_36` the store keeps order and widths per workspace, saves them with the layout and resets them; a resize clamps to the subject's minimum; an edge right of the subject resizes the column to its right; rows follow the order; the header menu moves and resets columns, by mouse and keyboard.
  - Playwright: `UI-36` dragging an edge right of the subject follows the pointer; dragging a header moves the column in the header and the rows; the order and widths reach the core; Reset columns restores them.

## Out of scope
- Hiding columns, sorting by a column, and a width for the subject.
- Column settings shared across workspaces.

## Acceptance
- The tests above pass, and `just check` passes.
- A package change: it ships in the next release candidate (M5-20).
