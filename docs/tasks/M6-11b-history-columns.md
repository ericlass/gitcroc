# M6-11b History columns

## Goal
The history's columns have rc.1's behaviour (UI-36): they can be resized and
moved, reset from a header menu, and the order and widths are kept per
workspace.

This is the second of four parts of M6-11.

## Depends on
- M6-11a: the history list and its default columns.
- M6-09: the workspace layout the columns are saved with.

## Spec
Implements: UI-36
Respects: UI-9, UI-22, UI-23, ARCH-6, ARCH-9, DATA-1, D15, QA-3, QA-4

## In scope
- **The header bar**, a port of `ui/src/history/ColumnHeaders.tsx` and
  `ColumnMenu.tsx`:
  - one cell per column in the workspace's order, with the subject taking
    the width that is left;
  - the merged list's repo column takes the graph column's place and width
    (M6-11d);
  - the header leaves room for the list's scrollbar.
- **Resizing:**
  - the edge between two headers resizes the column on the side away from
    the subject, so the edge follows the pointer;
  - the subject keeps its minimum width;
  - a double-click on an edge resets that column's width;
  - a focused edge resizes with the arrow keys.
- **Moving:** a header can be dragged onto another to move its column, the
  subject included, with a drop marker.
- **The header menu**, through the shared menu (`menu.rs`): Move left, Move
  right and Reset columns. Move left is disabled on the first column and
  Move right on the last, not hidden. Reset columns restores the default
  order and widths.
- **The keyboard (UI-22):**
  - the headers are one tab stop;
  - Left, Right, Home and End move between them;
  - Shift+F10 and the context-menu key open the focused header's menu;
  - the focus goes back to the header when the menu closes.
- **Saving:** the order and widths are read from and written to the
  workspace's `PaneLayout::columns` with `workspace_set_layout` (UI-36).
  A workspace without them uses the defaults; Reset columns removes them
  from the file. Each workspace has its own.
- **Strings (D15):** the column headers and menu entries move to
  `strings.rs`.
- **Test support:** a fixture can read and set a workspace's saved layout
  through the app.

## Tests (QA-3)
- `ui_36_columns_resize_move_and_reset`:
  - dragging an edge right of the subject widens the column to its right
    and keeps the subject's minimum;
  - a double-click resets a width;
  - dragging a header moves the column in the header and in the rows;
  - the header menu moves and resets columns by mouse and keyboard.
- `ui_36_columns_are_kept_per_workspace`: the order and widths survive a
  tab switch and a restart, and Reset columns removes them from the
  workspace file.
- `ui_22_history_headers_are_one_tab_stop` (or the same checks inside the
  test above): the arrow keys and Home and End move between headers, and
  Shift+F10 opens the menu.
- `ui_23_no_hardcoded_colors` and
  `d15_ui_text_comes_from_the_strings_module` keep passing over the new
  files.

## Out of scope
- Hiding columns, sorting by a column, and a width for the subject.
- Column settings shared across workspaces.
- The merged list itself (M6-11d), beyond reading the graph column's width.

## Acceptance
- **On the reference machine,** `just dev-next`:
  - resizes and moves every column, including the subject;
  - resets a width and the whole order;
  - keeps the result per workspace across a restart.
- `just coverage M6` no longer lists UI-36.
- `just check` passes.
