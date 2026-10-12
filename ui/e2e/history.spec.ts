import { expect, type Page, test } from "@playwright/test";

// Must match ROW_HEIGHT and OVERSCAN in src/history/HistoryPane.tsx.
const ROW_HEIGHT = 24;
const OVERSCAN = 8;

const tree = (page: Page) => page.getByRole("tree", { name: "Repositories" });
const item = (page: Page, name: string | RegExp) => tree(page).getByRole("treeitem", { name, exact: true });
const list = (page: Page) => page.getByRole("listbox", { name: "History" });

/** The subjects of the rendered rows, top to bottom. */
async function subjects(page: Page): Promise<string[]> {
  return list(page)
    .getByRole("option")
    .evaluateAll((rows) =>
      rows
        .map((row) => ({
          y: row.getBoundingClientRect().y,
          text: row.querySelector(".history-subject-text, .history-uncommitted")?.textContent ?? "",
        }))
        .sort((a, b) => a.y - b.y)
        .map((row) => row.text),
    );
}

test("PERF-2 history loads pages on demand while scrolling, never blank for long", async ({ page }) => {
  await page.setViewportSize({ width: 1400, height: 800 });
  await page.goto("/?fake=big-history");
  await item(page, /^huge/).click();
  await expect(list(page).getByRole("option").first()).toContainText("huge: change 100000");
  // One page, and room for the next one's placeholders: nothing counted up front.
  expect(await list(page).evaluate((scroller) => scroller.scrollHeight)).toBe(1000 * ROW_HEIGHT);

  // Jump to the end of what the scrollbar offers, again and again, until the
  // last commit shows. Each jump lands on placeholders for the next page;
  // the time until none is visible is how long rows stayed blank.
  const result = await list(page).evaluate(async (scroller) => {
    const frame = () => new Promise((resolve) => requestAnimationFrame(resolve));
    const blankVisible = () => {
      const view = scroller.getBoundingClientRect();
      return [...scroller.querySelectorAll(".history-row.placeholder")].some((row) => {
        const box = row.getBoundingClientRect();
        return box.bottom > view.top && box.top < view.bottom;
      });
    };
    let longestBlank = 0;
    let mostRows = 0;
    let jumps = 0;
    let previous = -1;
    // The scrollbar grows with each page and stops once the last one is in.
    while (scroller.scrollHeight !== previous && jumps < 1000) {
      previous = scroller.scrollHeight;
      scroller.scrollTop = scroller.scrollHeight;
      jumps++;
      const began = performance.now();
      await frame();
      while (blankVisible()) {
        mostRows = Math.max(mostRows, scroller.querySelectorAll(".history-row").length);
        await frame();
        if (performance.now() - began > 5000) {
          break;
        }
      }
      longestBlank = Math.max(longestBlank, performance.now() - began);
      mostRows = Math.max(mostRows, scroller.querySelectorAll(".history-row").length);
    }
    return { longestBlank, mostRows, jumps, height: scroller.clientHeight, total: scroller.scrollHeight };
  });

  test.info().annotations.push({
    type: "scroll",
    description: `${result.jumps} jumps, longest blank ${Math.round(result.longestBlank)} ms, at most ${result.mostRows} rows`,
  });
  // 100,000 commits in pages of 500, each requested as the user neared it.
  expect(result.jumps).toBeGreaterThan(100);
  expect(result.total).toBe(100_000 * ROW_HEIGHT);
  expect(result.longestBlank).toBeLessThan(100);
  // The DOM never held more than fits plus the overscan.
  expect(result.mostRows).toBeLessThanOrEqual(Math.ceil(result.height / ROW_HEIGHT) + 1 + 2 * OVERSCAN);
  expect((await subjects(page)).at(-1)).toBe("huge: change 1");
});

test("UI-9 selecting a branch shows that branch's history", async ({ page }) => {
  await page.goto("/?fake=tree");
  await item(page, /^api/).click();
  // The repo is dirty: its uncommitted changes lead (UI-10).
  await expect.poll(() => subjects(page).then((rows) => rows.slice(0, 2))).toEqual([
    "Uncommitted changes (3)",
    "api: change 40",
  ]);

  await item(page, /^api/).locator(".tree-chevron").click();
  await item(page, "Local").first().locator(".tree-chevron").click();
  await item(page, "feature").first().locator(".tree-chevron").click();
  await item(page, "login").first().click();
  await expect.poll(() => subjects(page).then((rows) => rows[0])).toBe("feature/login: change 20");

  // Keyboard: down selects, End goes to the last row.
  const rows = list(page);
  await rows.focus();
  await rows.press("ArrowDown");
  await expect(rows.getByRole("option", { selected: true })).toContainText("feature/login: change 20");
  await rows.press("End");
  await expect(rows.getByRole("option", { selected: true })).toContainText("feature/login: change 1");

  // Back to the repo: its history again, where it was.
  await item(page, /^api/).click();
  await expect.poll(() => subjects(page).then((rows) => rows[1])).toBe("api: change 40");
});

test("UI-12 search", async ({ page }) => {
  await page.goto("/?fake=tree");
  await item(page, /^web/).click();
  await expect.poll(() => subjects(page).then((rows) => rows[0])).toBe("web: change 40");
  await page.getByRole("searchbox", { name: "Search history" }).fill("change 3");
  await expect
    .poll(() => subjects(page))
    .toEqual([39, 38, 37, 36, 35, 34, 33, 32, 31, 30, 3].map((n) => `web: change ${n}`));
  await expect(list(page).locator("svg")).toHaveCount(0);
});
