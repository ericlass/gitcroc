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

  // The add button uses the folder picker and shows what happened.
  await page.getByRole("button", { name: "Add repository" }).click();
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
