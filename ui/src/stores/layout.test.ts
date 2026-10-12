import { expect, test } from "vitest";
import { clampPaneWidth, createLayoutStore, DEFAULT_PANE_SIZES, MIN_PANE_WIDTH } from "./layout";

test("ARCH-9 pane sizes are stored per workspace", () => {
  const layout = createLayoutStore();
  expect(layout.paneSizes("a")).toEqual(DEFAULT_PANE_SIZES);

  layout.setPaneWidth("a", "tree", 350);
  layout.setPaneWidth("b", "history", 600);
  expect(layout.paneSizes("a")).toEqual({ tree: 350, history: DEFAULT_PANE_SIZES.history });
  expect(layout.paneSizes("b")).toEqual({ tree: DEFAULT_PANE_SIZES.tree, history: 600 });

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
