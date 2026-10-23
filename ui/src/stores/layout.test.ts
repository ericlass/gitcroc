import { afterEach, expect, test } from "vitest";
import { emptyLayout, fakeState, reset } from "../ipc/fake";
import {
  clampPaneWidth,
  createLayoutStore,
  DEFAULT_COLUMN_ORDER,
  DEFAULT_HISTORY_COLUMNS,
  DEFAULT_PANE_SIZES,
  MIN_COLUMN_WIDTH,
  MIN_PANE_HEIGHT,
  MIN_PANE_WIDTH,
} from "./layout";

afterEach(reset);

test("ARCH-9 pane sizes are stored per workspace", () => {
  const layout = createLayoutStore();
  expect(layout.paneSizes("a")).toEqual(DEFAULT_PANE_SIZES);

  layout.setPaneWidth("a", "tree", 350);
  layout.setPaneWidth("b", "history", 600);
  expect(layout.paneSizes("a")).toEqual({ ...DEFAULT_PANE_SIZES, tree: 350 });
  expect(layout.paneSizes("b")).toEqual({ ...DEFAULT_PANE_SIZES, history: 600 });

  layout.resetPane("a", "tree");
  expect(layout.paneSizes("a")).toEqual(DEFAULT_PANE_SIZES);
  expect(layout.paneSizes("b").history).toBe(600);
});

test("pane widths keep the minimum, which wins over the maximum", () => {
  expect(clampPaneWidth(50)).toBe(MIN_PANE_WIDTH);
  expect(clampPaneWidth(900, 500)).toBe(500);
  expect(clampPaneWidth(900, 120)).toBe(MIN_PANE_WIDTH);
  expect(clampPaneWidth(300.4)).toBe(300);
});

test("UI-9 history column widths are stored per workspace", () => {
  const layout = createLayoutStore();
  expect(layout.historyColumns("a")).toEqual(DEFAULT_HISTORY_COLUMNS);

  layout.setColumnWidth("a", "author", 200.4);
  layout.setColumnWidth("b", "graph", 5);
  expect(layout.historyColumns("a")).toEqual({ ...DEFAULT_HISTORY_COLUMNS, author: 200 });
  expect(layout.historyColumns("b")).toEqual({ ...DEFAULT_HISTORY_COLUMNS, graph: MIN_COLUMN_WIDTH });

  layout.resetColumn("a", "author");
  expect(layout.historyColumns("a")).toEqual(DEFAULT_HISTORY_COLUMNS);

  // A column grows only as far as the subject can give up width (UI-36).
  layout.setColumnWidth("a", "date", 400, 300);
  expect(layout.historyColumns("a").date).toBe(300);
});

test("UI-36 column order and widths are kept with the workspace's layout", async () => {
  const id = fakeState.workspace.id;
  const layout = createLayoutStore();
  expect(layout.columnOrder(id)).toEqual(DEFAULT_COLUMN_ORDER);
  layout.load(id, {
    ...emptyLayout,
    columns: { order: ["subject", "graph", "author", "date", "hash"], graph: 96, author: null, date: 4, hash: null },
  });
  expect(layout.columnOrder(id)).toEqual(["subject", "graph", "author", "date", "hash"]);
  // A width missing from the file is the default; one too small for a column is its minimum.
  expect(layout.historyColumns(id)).toEqual({ ...DEFAULT_HISTORY_COLUMNS, graph: 96, date: MIN_COLUMN_WIDTH });
  expect(layout.columnOrder("other")).toEqual(DEFAULT_COLUMN_ORDER);

  // Moving saves at once, with the widths.
  layout.moveColumn(id, "hash", 0);
  expect(layout.columnOrder(id)).toEqual(["hash", "subject", "graph", "author", "date"]);
  layout.moveColumn(id, "subject", 4);
  expect(layout.columnOrder(id)).toEqual(["hash", "graph", "author", "date", "subject"]);
  await Promise.resolve();
  expect(fakeState.layouts.at(-1)?.columns).toEqual({
    order: ["hash", "graph", "author", "date", "subject"],
    ...DEFAULT_HISTORY_COLUMNS,
    graph: 96,
    date: MIN_COLUMN_WIDTH,
  });

  // Reset restores the defaults and leaves the columns out of the file.
  layout.resetColumns(id);
  expect(layout.columnOrder(id)).toEqual(DEFAULT_COLUMN_ORDER);
  expect(layout.historyColumns(id)).toEqual(DEFAULT_HISTORY_COLUMNS);
  await Promise.resolve();
  expect(fakeState.layouts.at(-1)?.columns).toBeNull();
});

test("UI-19 dock position is per workspace", async () => {
  const id = fakeState.workspace.id;
  const layout = createLayoutStore();
  expect(layout.dock(id)).toBe("right");
  layout.load(id, emptyLayout);
  layout.load("other", { dock: "bottom", tree: 300, history: null, changes: 200, columns: null });
  expect(layout.dock(id)).toBe("right");
  expect(layout.dock("other")).toBe("bottom");
  expect(layout.paneSizes("other")).toEqual({ ...DEFAULT_PANE_SIZES, tree: 300, changes: 200 });

  // Docking saves the workspace's whole layout in its file, through the core.
  layout.setDock(id, "bottom");
  expect(layout.dock(id)).toBe("bottom");
  expect(layout.dock("other")).toBe("bottom");
  layout.setDock("other", "right");
  expect(layout.dock(id)).toBe("bottom");
  await Promise.resolve();
  expect(fakeState.layouts).toEqual([{ dock: "bottom", ...DEFAULT_PANE_SIZES, columns: null }]);

  // Resizing saves when it ends; the changes pane's height has its own minimum.
  layout.setPaneWidth(id, "changes", 10);
  expect(layout.paneSizes(id).changes).toBe(MIN_PANE_HEIGHT);
  layout.save(id);
  await Promise.resolve();
  expect(fakeState.layouts.at(-1)).toEqual({
    dock: "bottom",
    ...DEFAULT_PANE_SIZES,
    changes: MIN_PANE_HEIGHT,
    columns: null,
  });

  // The next start reads it back from the workspace; a later load changes nothing.
  const next = createLayoutStore();
  next.load(id, fakeState.workspace.layout);
  expect(next.dock(id)).toBe("bottom");
  expect(next.paneSizes(id).changes).toBe(MIN_PANE_HEIGHT);
  next.load(id, emptyLayout);
  expect(next.dock(id)).toBe("bottom");
});
