import { expect, type Locator, type Page, test } from "@playwright/test";

// Must match ROW_HEIGHT and OVERSCAN in src/tree/TreePane.tsx.
const ROW_HEIGHT = 22;
const OVERSCAN = 8;

const tree = (page: Page) => page.getByRole("tree", { name: "Repositories" });
const item = (page: Page, name: string | RegExp) => tree(page).getByRole("treeitem", { name, exact: true });

/** The rendered rows top to bottom, as labels indented by level. */
async function rows(scroller: Locator): Promise<string[]> {
  return scroller.getByRole("treeitem").evaluateAll((elements) =>
    elements
      .map((element) => ({
        y: element.getBoundingClientRect().y,
        text:
          "  ".repeat(Number(element.getAttribute("aria-level")) - 1) +
          (element.querySelector(".tree-label")?.textContent ?? ""),
      }))
      .sort((a, b) => a.y - b.y)
      .map((row) => row.text),
  );
}

test("PERF-1 tree is virtualized", async ({ page }) => {
  await page.setViewportSize({ width: 1200, height: 700 });
  await page.goto("/?fake=big-tree");
  await item(page, /^big/).locator(".tree-chevron").click();
  await item(page, "Local").locator(".tree-chevron").click();
  await expect(item(page, "branch-0000 Current branch")).toBeVisible();

  const scroller = tree(page);
  const height = await scroller.evaluate((element) => element.clientHeight);
  const fit = Math.ceil(height / ROW_HEIGHT) + 1;
  const limit = fit + 2 * OVERSCAN;
  // 5,000 branches expanded, yet the DOM holds only what fits plus the overscan.
  expect(await scroller.getByRole("treeitem").count()).toBeLessThanOrEqual(limit);
  expect(await scroller.evaluate((element) => element.scrollHeight)).toBeGreaterThan(5000 * ROW_HEIGHT);

  await scroller.evaluate((element) => element.scrollTo({ top: element.scrollHeight }));
  await expect(item(page, "branch-4999")).toBeVisible();
  expect(await scroller.getByRole("treeitem").count()).toBeLessThanOrEqual(limit);
  expect((await rows(scroller)).slice(-4)).toEqual(["      branch-4999", "    Remotes", "    Tags", "    Stashes"]);
});

test("UI-8 filter", async ({ page }) => {
  await page.goto("/?fake=tree");
  const filter = page.getByRole("searchbox", { name: "Filter repos and branches" });
  await expect(item(page, /^api/)).toBeVisible();
  const all = await rows(tree(page));
  expect(all).toEqual(["Ungrouped", "  api", "  web", "  login-service"]);

  await filter.fill("LOGIN");
  await expect(item(page, "login")).toHaveCount(2);
  expect(await rows(tree(page))).toEqual([
    "Ungrouped",
    "  api",
    "    Local",
    "      feature",
    "        login",
    "    Remotes",
    "      origin",
    "        feature",
    "          login",
    "  login-service",
    "    Local",
    "    Remotes",
    "    Tags",
    "    Stashes",
  ]);

  await filter.fill("");
  await expect(item(page, "login")).toHaveCount(0);
  expect(await rows(tree(page))).toEqual(all);
});

test("UI-1 keyboard, add and remove", async ({ page }) => {
  await page.goto("/?fake=tree");
  await item(page, /^api/).click();
  const scroller = tree(page);
  await expect(item(page, /^api/)).toHaveAttribute("aria-selected", "true");

  // Right expands, right again enters, down moves, left goes back up.
  const local = item(page, "Local").first();
  await scroller.press("ArrowRight");
  await expect(local).toHaveAttribute("aria-expanded", "false");
  await scroller.press("ArrowRight");
  await expect(local).toHaveAttribute("aria-selected", "true");
  await scroller.press("ArrowRight");
  await expect(local).toHaveAttribute("aria-expanded", "true");
  const main = item(page, "main Current branch 1 ahead 2 behind");
  await expect(main).toContainText("↑1↓2");
  await scroller.press("ArrowDown");
  await expect(item(page, "bugfix")).toHaveAttribute("aria-selected", "true");
  await scroller.press("ArrowLeft");
  await expect(local).toHaveAttribute("aria-selected", "true");
  await scroller.press("ArrowLeft");
  await expect(local).toHaveAttribute("aria-expanded", "false");
  await expect(main).toHaveCount(0);

  // The add menu's Add folder uses the folder picker and shows what happened.
  await page.getByRole("button", { name: "Add repository" }).click();
  await page.getByRole("menu", { name: "Add repository" }).getByRole("menuitem", { name: "Add folder…" }).click();
  await expect(page.getByRole("list", { name: "Add result" })).toHaveText("Added: new-repo");
  await expect(item(page, /^new-repo/)).toBeVisible();

  // A repo row's context menu removes it from the workspace.
  await item(page, /^new-repo/).click({ button: "right" });
  await page.getByRole("menuitem", { name: "Remove from workspace" }).click();
  await expect(item(page, /^new-repo/)).toHaveCount(0);
});

test("UI-1 an empty workspace offers to add repositories", async ({ page }) => {
  await page.goto("/");
  await expect(page.getByText("This workspace has no repositories yet.")).toBeVisible();
  await expect(page.getByRole("button", { name: "Add repository" })).toHaveCount(2);
});

test("UI-7 repos are dragged between groups and groups reordered", async ({ page }) => {
  await page.goto("/?fake=groups");
  const scroller = tree(page);
  await expect(item(page, /^Backend/)).toBeVisible();
  expect(await rows(scroller)).toEqual([
    "Backend",
    "  Branches",
    "  api",
    "  web",
    "Services",
    "  Branches",
    "  web",
    "  login-service",
    "Ungrouped",
  ]);

  // api moves from Backend to Services.
  await item(page, /^api/).dragTo(item(page, /^Services/));
  await expect(item(page, /^Backend/).locator(".tree-count")).toHaveText("1");
  expect(await rows(scroller)).toEqual([
    "Backend",
    "  Branches",
    "  web",
    "Services",
    "  Branches",
    "  web",
    "  login-service",
    "  api",
    "Ungrouped",
  ]);

  // Dropped on Ungrouped, web leaves Backend only.
  await item(page, /^web/).first().dragTo(item(page, /^Ungrouped/));
  await expect(item(page, /^Backend/).locator(".tree-count")).toHaveText("0");

  // Services dropped above Backend comes first.
  await item(page, /^Services/).dragTo(item(page, /^Backend/), { targetPosition: { x: 20, y: 3 } });
  await expect.poll(() => rows(scroller)).toEqual([
    "Services",
    "  Branches",
    "  web",
    "  login-service",
    "  api",
    "Backend",
    "  Branches",
    "Ungrouped",
  ]);
});

test("UI-6 UI-7 PLAT-1 a group dialog and its menu stay above the history", async ({ page }) => {
  // Windows' 125% scaling leaves about 1024 CSS pixels in the 1280px app window.
  await page.setViewportSize({ width: 1024, height: 768 });
  await page.goto("/?fake=groups");
  // The merged history of a selection draws its rows under the menu's place.
  await item(page, /^api/).click();
  await item(page, /^login-service/).click({ modifiers: ["ControlOrMeta"] });
  await expect(page.locator(".history-repo").first()).toBeVisible();

  // Playwright clicks only what is on top: the menu must not be behind the panes.
  await item(page, /^login-service/).click({ button: "right" });
  await page.getByRole("menu", { name: "Repository" }).getByRole("menuitem", { name: "Add to group ▸" }).click();
  await page.getByRole("menu", { name: "Add to group" }).getByRole("menuitem", { name: "New group…" }).click();
  const dialog = page.getByRole("dialog", { name: "New group" });
  await dialog.getByRole("textbox", { name: /^Name/ }).fill("Pair");
  const create = dialog.getByRole("button", { name: "Create" });
  expect(await create.evaluate((button) => {
    const rect = button.getBoundingClientRect();
    const hit = document.elementFromPoint(rect.x + rect.width / 2, rect.y + rect.height / 2);
    return hit !== null && button.contains(hit);
  })).toBe(true);
  await create.click();
  await expect(item(page, /^Pair/).locator(".tree-count")).toHaveText("2");
});
