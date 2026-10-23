import { expect, type Locator, type Page, test } from "@playwright/test";

const MIN_PANE_WIDTH = 200;

async function width(locator: Locator): Promise<number> {
  const box = await locator.boundingBox();
  if (!box) {
    throw new Error("element is not visible");
  }
  return box.width;
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

test("splitters resize panes, keep the minimum width and reset on double-click", async ({ page }) => {
  await page.setViewportSize({ width: 1400, height: 800 });
  await page.goto("/");
  const tree = page.getByRole("region", { name: "Tree" });
  const history = page.getByRole("region", { name: "History" });
  const changes = page.getByRole("region", { name: "Changes" });
  const treeSplitter = page.getByRole("separator", { name: "Resize Tree" });
  const historySplitter = page.getByRole("separator", { name: "Resize History" });
  await expect(page.getByRole("button", { description: "Show the job list" })).toHaveText("No jobs running");

  const start = await width(tree);
  await drag(page, treeSplitter, 100);
  expect(await width(tree)).toBeCloseTo(start + 100, 0);

  await drag(page, treeSplitter, -1000);
  expect(await width(tree)).toBe(MIN_PANE_WIDTH);

  // Growing a pane stops where the changes pane reaches its minimum.
  await drag(page, historySplitter, 2000);
  expect(await width(changes)).toBe(MIN_PANE_WIDTH);
  expect(await width(history)).toBeGreaterThan(MIN_PANE_WIDTH);

  await treeSplitter.dblclick();
  expect(await width(tree)).toBe(start);
});

/** The changes pane lies inside the viewport, at least `min` wide and high. */
async function expectInWindow(page: Page, pane: Locator, min: { width: number; height: number }): Promise<void> {
  const box = await pane.boundingBox();
  const viewport = page.viewportSize();
  if (!box || !viewport) {
    throw new Error("changes pane is not visible");
  }
  expect(box.x + box.width).toBeLessThanOrEqual(viewport.width);
  expect(box.y + box.height).toBeLessThanOrEqual(viewport.height);
  expect(Math.round(box.width)).toBeGreaterThanOrEqual(min.width);
  expect(Math.round(box.height)).toBeGreaterThanOrEqual(min.height);
}

test("UI-19 the changes pane docked right stays in the window when the window narrows", async ({ page }) => {
  await page.setViewportSize({ width: 1800, height: 800 });
  await page.goto("/");
  const tree = page.getByRole("region", { name: "Tree" });
  const history = page.getByRole("region", { name: "History" });
  const changes = page.getByRole("region", { name: "Changes" });
  await expect(page.getByRole("button", { description: "Show the job list" })).toHaveText("No jobs running");

  await drag(page, page.getByRole("separator", { name: "Resize History" }), 2000);
  expect(await width(changes)).toBe(MIN_PANE_WIDTH);
  const wide = await width(history);

  await page.setViewportSize({ width: 960, height: 800 });
  await expectInWindow(page, changes, { width: MIN_PANE_WIDTH, height: 0 });
  expect(await width(history)).toBeGreaterThanOrEqual(MIN_PANE_WIDTH);
  expect(await width(tree)).toBeGreaterThanOrEqual(MIN_PANE_WIDTH);

  // The saved sizes are kept: a wide window shows them again.
  await page.setViewportSize({ width: 1800, height: 800 });
  expect(await width(history)).toBeCloseTo(wide, 0);

  // A drag starts from the width shown, not the larger saved one.
  await page.setViewportSize({ width: 960, height: 800 });
  const shown = await width(history);
  await drag(page, page.getByRole("separator", { name: "Resize History" }), -20);
  expect(await width(history)).toBeCloseTo(shown - 20, 0);
  await expectInWindow(page, changes, { width: MIN_PANE_WIDTH, height: 0 });
});

test("UI-19 the changes pane docked at the bottom stays in the window when the window gets shorter", async ({
  page,
}) => {
  await page.setViewportSize({ width: 1400, height: 1200 });
  await page.goto("/");
  const changes = page.getByRole("region", { name: "Changes" });
  await expect(page.getByRole("button", { description: "Show the job list" })).toHaveText("No jobs running");
  await page.getByRole("button", { name: "Dock at bottom" }).click();
  const splitter = page.getByRole("separator", { name: "Resize Changes" });
  await splitter.focus();
  for (let step = 0; step < 30; step++) {
    await page.keyboard.press("ArrowUp");
  }
  const tall = (await changes.boundingBox())?.height ?? 0;
  expect(tall).toBeGreaterThan(600);

  await page.setViewportSize({ width: 1400, height: 600 });
  await expectInWindow(page, changes, { width: 0, height: 120 });
  const history = await page.getByRole("region", { name: "History" }).boundingBox();
  expect(Math.round(history?.height ?? 0)).toBeGreaterThanOrEqual(120);

  // A wide tree leaves room for the history and changes when the window narrows.
  await page.setViewportSize({ width: 1400, height: 800 });
  await drag(page, page.getByRole("separator", { name: "Resize Tree" }), 2000);
  await page.setViewportSize({ width: 960, height: 800 });
  await expectInWindow(page, changes, { width: MIN_PANE_WIDTH, height: 120 });
});

const background = (page: Page) => page.evaluate(() => getComputedStyle(document.body).backgroundColor);

test("UI-23 theme toggle changes the background token", async ({ page }) => {
  await page.emulateMedia({ colorScheme: "light" });
  await page.goto("/");
  await expect(page.getByRole("toolbar", { name: "Main" })).toBeVisible();
  const light = await background(page);

  await page.getByRole("button", { name: "Settings" }).click();
  await page.getByRole("radio", { name: "Dark" }).check();
  await expect(page.locator("html")).toHaveAttribute("data-theme", "dark");
  const dark = await background(page);
  expect(dark).not.toBe(light);

  // `system` follows the OS without a reload.
  await page.getByRole("radio", { name: "Follow the system" }).check();
  await expect(page.locator("html")).toHaveAttribute("data-theme", "light");
  expect(await background(page)).toBe(light);
  await page.emulateMedia({ colorScheme: "dark" });
  await expect(page.locator("html")).toHaveAttribute("data-theme", "dark");
  expect(await background(page)).toBe(dark);
});
