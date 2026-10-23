import { expect, type Locator, type Page, test } from "@playwright/test";

const tree = (page: Page) => page.getByRole("tree", { name: "Repositories" });
const headers = (page: Page) => page.getByRole("toolbar", { name: "History columns" });
const header = (page: Page, name: string) => headers(page).getByRole("button", { name, exact: true });

/** The left and right edges of `locator`. */
async function edges(locator: Locator): Promise<{ left: number; right: number }> {
  const box = await locator.boundingBox();
  if (!box) {
    throw new Error("element is not visible");
  }
  return { left: Math.round(box.x), right: Math.round(box.x + box.width) };
}

/** Drags `handle` horizontally by `dx` pixels with the mouse. */
async function drag(page: Page, handle: Locator, dx: number): Promise<void> {
  const box = await handle.boundingBox();
  if (!box) {
    throw new Error("splitter is not visible");
  }
  const x = box.x + box.width / 2;
  const y = box.y + box.height / 2;
  await page.mouse.move(x, y);
  await page.mouse.down();
  await page.mouse.move(x + dx / 2, y);
  await page.mouse.move(x + dx, y);
  await page.mouse.up();
}

/** The column headers, left to right, as their text. */
const headerTexts = (page: Page) => headers(page).getByRole("button").allTextContents();

/** The first row's cells, left to right, by their column class. */
const firstRowColumns = (page: Page) =>
  page
    .locator("#history-row-1 > .history-cell")
    .evaluateAll((cells) => cells.map((cell) => [...cell.classList].find((name) => name !== "history-cell")));

/** The columns of the layout the core was last sent. */
const savedColumns = (page: Page) =>
  page.evaluate(
    () =>
      (
        window as unknown as { fakeCore: { state: { layouts: { columns: unknown }[] } } }
      ).fakeCore.state.layouts.at(-1)?.columns,
  );

async function showHistory(page: Page): Promise<void> {
  await page.setViewportSize({ width: 1400, height: 800 });
  await page.goto("/?fake=tree");
  await tree(page).getByRole("treeitem", { name: /^api/ }).click();
  await expect(page.locator("#history-row-1")).toBeVisible();
}

test("UI-36 a column edge follows the pointer on both sides of the subject", async ({ page }) => {
  await showHistory(page);
  // Room for the subject to give up.
  await drag(page, page.getByRole("separator", { name: "Resize History" }), 300);
  const graph = page.locator(".history-columns .history-graph");
  const author = page.locator(".history-columns .history-author");

  // Left of the subject: the graph's right edge moves with the pointer.
  const graphBefore = await edges(graph);
  await drag(page, page.getByRole("separator", { name: "Resize Graph" }), 40);
  expect(await edges(graph)).toEqual({ left: graphBefore.left, right: graphBefore.right + 40 });

  // Right of the subject: the author's left edge moves with the pointer, and its right edge stays.
  const authorBefore = await edges(author);
  await drag(page, page.getByRole("separator", { name: "Resize Author" }), -30);
  expect(await edges(author)).toEqual({ left: authorBefore.left - 30, right: authorBefore.right });
  await drag(page, page.getByRole("separator", { name: "Resize Author" }), 50);
  expect(await edges(author)).toEqual({ left: authorBefore.left + 20, right: authorBefore.right });

  // Rows follow the header, and the widths reach the core when a resize ends.
  expect(await edges(page.locator("#history-row-1 .history-author"))).toEqual(await edges(author));
  expect(await savedColumns(page)).toEqual({ order: null, graph: 120, author: 120, date: 128, hash: 72 });
});

test("UI-36 dragging a header moves its column, and the menu resets the columns", async ({ page }) => {
  await showHistory(page);

  // Dropped on the left half of the graph's header: before it.
  await header(page, "Hash").dragTo(header(page, "Graph"), { targetPosition: { x: 4, y: 4 } });
  expect(await headerTexts(page)).toEqual(["Hash", "Graph", "Subject", "Author", "Date"]);
  expect(await firstRowColumns(page)).toEqual([
    "history-hash",
    "history-graph",
    "history-subject",
    "history-author",
    "history-date",
  ]);
  expect(await savedColumns(page)).toEqual({
    order: ["hash", "graph", "subject", "author", "date"],
    graph: 80,
    author: 140,
    date: 128,
    hash: 72,
  });

  // On the right half of the date's header: after it, the subject too.
  await header(page, "Subject").dragTo(header(page, "Date"), { targetPosition: { x: 100, y: 4 } });
  expect(await headerTexts(page)).toEqual(["Hash", "Graph", "Author", "Date", "Subject"]);

  await header(page, "Author").click({ button: "right" });
  await page.getByRole("menuitem", { name: "Reset columns" }).click();
  expect(await headerTexts(page)).toEqual(["Graph", "Subject", "Author", "Date", "Hash"]);
  expect(await savedColumns(page)).toBeNull();
});
