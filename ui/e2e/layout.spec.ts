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
  await expect(page.getByRole("status")).toHaveText("No jobs running");

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
