import AxeBuilder from "@axe-core/playwright";
import { expect, type Locator, type Page, test } from "@playwright/test";

// WCAG 2.2 A and AA as axe-core checks them (README "Accessibility target"), in each view of the window and both app
// themes, against the fake core. A rule is turned off only for one element, with the reason next to it.

const TAGS = ["wcag2a", "wcag2aa", "wcag21a", "wcag21aa", "wcag22aa"];

/** A rule axe may not apply to one element, and why. */
type Exception = { rule: string; selector: string; reason: string };

const tree = (page: Page) => page.getByRole("tree", { name: "Repositories" });
const history = (page: Page) => page.getByRole("listbox", { name: "History" });
const toolbar = (page: Page) => page.getByRole("toolbar", { name: "Main" });
const item = (page: Page, name: RegExp) => tree(page).getByRole("treeitem", { name }).first();

/** Selects api's uncommitted changes. */
async function uncommitted(page: Page) {
  await item(page, /^api/).click();
  await history(page).getByRole("option", { name: /Uncommitted changes/ }).click();
  await expect(page.getByRole("heading", { name: "Uncommitted changes" })).toBeVisible();
}

/** Opens the palette and waits for its first match. */
async function openPalette(page: Page) {
  await expect(tree(page).getByRole("treeitem").first()).toBeVisible();
  await page.keyboard.press("Control+k");
  const palette = page.getByRole("dialog", { name: "Command palette" });
  await expect(palette.getByRole("option").first()).toBeVisible();
}

/** The views axe checks: where each starts and how it gets there. */
const VIEWS: { name: string; fake: string; open: (page: Page) => Promise<void>; except?: Exception[] }[] = [
  {
    name: "a repo selected",
    fake: "tree",
    open: async (page) => {
      await item(page, /^api/).click();
      await expect(history(page).getByRole("option").nth(1)).toBeVisible();
    },
  },
  {
    name: "a group selected",
    fake: "groups",
    open: async (page) => {
      await item(page, /^Backend/).click();
      await expect(page.locator(".history-columns")).toContainText("Repository");
    },
  },
  {
    name: "the uncommitted changes selected",
    fake: "tree",
    open: async (page) => {
      await uncommitted(page);
      await expect(page.locator(".diff-editor")).toContainText("staged line");
    },
  },
  {
    name: "the conflict editor with a proposal",
    fake: "ai-conflict",
    open: async (page) => {
      await uncommitted(page);
      const conflict = page.getByRole("region", { name: "Conflict" });
      await conflict.getByRole("button", { name: "Resolve with AI" }).click();
      await expect(conflict.getByRole("group", { name: "Proposal for conflict 1" })).toBeVisible();
    },
  },
  {
    name: "Settings with every section",
    fake: "ai-conflict",
    open: async (page) => {
      await toolbar(page).getByRole("button", { name: "Settings" }).click();
      const section = page.getByRole("group", { name: "AI" });
      await expect(section.getByRole("list", { name: "AI providers" })).toBeVisible();
    },
  },
  {
    name: "Settings with the provider form",
    fake: "ai",
    open: async (page) => {
      await toolbar(page).getByRole("button", { name: "Settings" }).click();
      const section = page.getByRole("group", { name: "AI" });
      await section.getByRole("button", { name: "Add provider…" }).click();
      await expect(section.getByRole("form", { name: "New AI provider" })).toBeVisible();
    },
  },
  {
    name: "the job list with a job's output",
    fake: "jobs",
    open: async (page) => {
      await page.getByRole("button", { description: "Show the job list" }).click();
      const jobs = page.getByRole("dialog", { name: "Jobs" });
      await jobs.getByRole("option", { name: /Push/ }).click();
      await expect(jobs.locator(".job-output")).toContainText("rejected");
    },
  },
  {
    name: "the job list with a result report",
    fake: "groups",
    open: async (page) => {
      await item(page, /^Backend/).click();
      await toolbar(page).getByRole("button", { name: "Fetch", exact: true }).click();
      await expect(page.locator("[aria-modal=true]")).toHaveCount(0);
      await page.getByRole("button", { description: "Show the job list" }).click();
      const jobs = page.getByRole("dialog", { name: "Jobs" });
      await jobs.getByRole("option", { name: /Fetch/ }).first().click();
      await expect(jobs.getByRole("list", { name: "Result report" })).toBeVisible();
    },
  },
  {
    name: "the preflight dialog",
    fake: "groups",
    open: async (page) => {
      await toolbar(page).getByRole("button", { name: "Settings" }).click();
      await page.getByRole("checkbox", { name: "Always show the preflight dialog" }).check();
      await page.keyboard.press("Escape");
      await item(page, /^Backend/).click();
      await toolbar(page).getByRole("button", { name: "Fetch", exact: true }).click();
      await expect(page.getByRole("dialog", { name: "Fetch · Backend (2 repos)" })).toBeVisible();
    },
  },
  {
    name: "a destructive confirmation",
    fake: "tree",
    open: async (page) => {
      await uncommitted(page);
      await page.getByRole("treegrid", { name: "Changed files" }).getByRole("row", { name: /notes\.txt/ }).click();
      await page.getByRole("button", { name: "Discard notes.txt" }).click();
      await expect(page.getByRole("alertdialog")).toContainText("Discard changes in api?");
    },
  },
  {
    name: "the command palette",
    fake: "tree",
    open: openPalette,
  },
  {
    name: "the git setup screen",
    fake: "no-git",
    open: async (page) => {
      await expect(page.getByRole("button", { name: /Check again/ })).toBeVisible();
    },
  },
  {
    name: "the update dialog of a package that installs",
    fake: "update",
    open: async (page) => {
      await page.getByRole("button", { name: "gitcroc 0.2.0 is available" }).click();
      await expect(page.getByRole("dialog", { name: "Update gitcroc" })).toContainText("Release notes");
    },
  },
  {
    name: "the update dialog of a package that only notifies",
    fake: "update-deb",
    open: async (page) => {
      await page.getByRole("button", { name: "gitcroc 0.2.0 is available" }).click();
      await expect(page.getByRole("button", { name: "Open download page" })).toBeVisible();
    },
  },
  {
    name: "Settings' Updates section",
    fake: "update",
    open: async (page) => {
      await toolbar(page).getByRole("button", { name: "Settings" }).click();
      const section = page.getByRole("group", { name: "Updates" });
      await section.scrollIntoViewIfNeeded();
      await expect(section).toContainText("gitcroc 0.2.0 is available (checked");
    },
  },
  {
    name: "the empty state with no tab open",
    fake: "no-tabs",
    open: async (page) => {
      await expect(page.locator(".empty-state")).toBeVisible();
    },
  },
];

for (const theme of ["light", "dark"] as const) {
  for (const view of VIEWS) {
    test(`UI-23 WCAG 2.2 AA in the ${theme} theme: ${view.name}`, async ({ page }) => {
      await page.emulateMedia({ colorScheme: theme });
      await page.goto(`/?fake=${view.fake}`);
      await expect(page.locator(":root")).toHaveAttribute("data-theme", theme);
      await view.open(page);
      const { violations } = await new AxeBuilder({ page }).withTags(TAGS).analyze();
      const found = violations.flatMap((violation) => {
        const targets = violation.nodes
          .map((node) => node.target.join(" "))
          .filter((target) => !(view.except ?? []).some((e) => e.rule === violation.id && e.selector === target));
        return targets.length === 0 ? [] : [{ rule: violation.id, help: violation.help, targets }];
      });
      expect(found).toEqual([]);
    });
  }
}

/** Opens a hard reset of api's main to its third commit, from the palette. */
async function openReset(page: Page) {
  await item(page, /^api/).click();
  await history(page).getByRole("option").nth(2).click();
  await page.keyboard.press("Control+k");
  const palette = page.getByRole("dialog", { name: "Command palette" });
  await palette.getByRole("combobox").fill("reset hard");
  await expect(palette.getByRole("option").first()).toContainText("Reset current branch: hard…");
  await page.keyboard.press("Enter");
  await expect(page.getByRole("dialog", { name: /^Reset main in api to / })).toBeVisible();
}

/** Whether the focus is inside `dialog`. */
const focusInside = (dialog: Locator) => dialog.evaluate((element) => element.contains(document.activeElement));

/**
 * The dialog parts' focus (UI-22): the dialog has the focus once open; Tab and Shift+Tab go around inside it;
 * Escape closes it and the focus is back on `opener`.
 */
async function checkFocus(page: Page, dialog: Locator, opener: Locator) {
  await expect(dialog).toBeVisible();
  await expect.poll(() => focusInside(dialog)).toBe(true);
  const stops = await dialog.evaluate(
    (element) =>
      [...element.querySelectorAll<HTMLElement>("button, input, select, textarea, [tabindex]")].filter(
        (stop) => stop.tabIndex >= 0 && !(stop as HTMLButtonElement).disabled && stop.getClientRects().length > 0,
      ).length,
  );
  for (const key of ["Tab", "Shift+Tab"]) {
    for (let presses = 0; presses <= stops + 1; presses++) {
      await page.keyboard.press(key);
      expect(await focusInside(dialog), `${key} ${presses + 1} stays in the dialog`).toBe(true);
    }
  }
  await page.keyboard.press("Escape");
  await expect(dialog).toBeHidden();
  await expect(opener).toBeFocused();
}

test("UI-22 dialog focus, Frame: New branch takes the focus, keeps Tab in and gives it back", async ({ page }) => {
  await page.goto("/?fake=tree");
  await item(page, /^api/).click();
  const opener = toolbar(page).getByRole("button", { name: "Branch", exact: true });
  await opener.click();
  await checkFocus(page, page.getByRole("dialog", { name: "New branch in api" }), opener);
});

test("UI-22 dialog focus, NameDialog: New workspace gives the focus back to +", async ({ page }) => {
  await page.goto("/?fake=tree");
  const opener = page.getByRole("button", { name: "Open workspace" });
  await opener.focus();
  await page.keyboard.press("Enter");
  await page.getByRole("menuitem", { name: "New workspace…" }).click();
  await checkFocus(page, page.getByRole("dialog", { name: "New workspace" }), opener);
});

test("UI-22 dialog focus, ConfirmDialog: a hard reset's confirmation returns it to the history", async ({ page }) => {
  await page.goto("/?fake=tree");
  await openReset(page);
  await page.getByRole("button", { name: "Reset hard…" }).click();
  await checkFocus(page, page.getByRole("alertdialog", { name: "Cannot be undone in api" }), history(page));
});

test("UI-22 dialog focus, Question: Delete group gives the focus back to the tree", async ({ page }) => {
  await page.goto("/?fake=groups");
  await item(page, /^Backend/).click({ button: "right" });
  await page.getByRole("menuitem", { name: "Delete…" }).click();
  await checkFocus(page, page.getByRole("alertdialog", { name: /^Delete the group Backend/ }), tree(page));
});

test("UI-22 dialog focus, Modal: Settings, the job list, the preflight and a discard do the same", async ({
  page,
}) => {
  await page.goto("/?fake=groups");
  const settings = toolbar(page).getByRole("button", { name: "Settings" });
  await settings.click();
  await checkFocus(page, page.getByRole("dialog", { name: "Settings" }), settings);

  const jobs = page.getByRole("button", { description: "Show the job list" });
  await jobs.click();
  await checkFocus(page, page.getByRole("dialog", { name: "Jobs" }), jobs);

  await settings.click();
  await page.getByRole("checkbox", { name: "Always show the preflight dialog" }).check();
  await page.keyboard.press("Escape");
  await item(page, /^Backend/).click();
  const fetch = toolbar(page).getByRole("button", { name: "Fetch", exact: true });
  await fetch.click();
  await checkFocus(page, page.getByRole("dialog", { name: "Fetch · Backend (2 repos)" }), fetch);

  await page.goto("/?fake=tree");
  await uncommitted(page);
  const list = page.getByRole("treegrid", { name: "Changed files" });
  await list.getByRole("row", { name: /notes\.txt/ }).click();
  await page.keyboard.press("Delete");
  await checkFocus(page, page.getByRole("alertdialog", { name: "Discard changes in api?" }), list);
});

test("UI-21 a failed job is announced in the one polite live region; progress is not", async ({ page }) => {
  await page.goto("/?fake=jobs");
  const live = page.locator("[aria-live=polite]");
  await expect(live).toHaveCount(1);
  await expect(live).toHaveAttribute("role", "status");
  // A job runs with progress: nothing is said.
  await expect(page.getByRole("button", { description: "Show the job list" })).toContainText("Receiving objects 3/10");
  await expect(live).toHaveText("");

  await page.goto("/?fake=tree");
  await item(page, /^api/).click();
  const fetch = toolbar(page).getByRole("button", { name: "Fetch", exact: true });
  await fetch.click();
  await expect(page.getByRole("button", { description: "Show the job list" })).toContainText("Nothing to do");
  await expect(live).toHaveText("");
  // api's main is ahead and behind, so the default pull fails, twice: each is said.
  const pull = toolbar(page).getByRole("button", { name: "Pull", exact: true });
  await pull.click();
  await expect(live).toHaveText("Pull origin/main into main in My workspace: Failed");
  const first = await live.textContent();
  await pull.click();
  await expect.poll(() => live.textContent()).not.toBe(first);
  await expect(live).toHaveText(/^Pull origin\/main into main in My workspace: Failed\s?$/);
});

test("UI-3 UI-23 reduced motion: nothing animates; a spinner is still, with its text alternative", async ({ page }) => {
  await page.emulateMedia({ reducedMotion: "reduce" });
  await page.goto("/?fake=jobs");
  await page.getByRole("button", { description: "Show the job list" }).click();
  await expect(page.getByRole("dialog", { name: "Jobs" })).toBeVisible();
  // A spinner as a job without a known total shows it (src/jobs/outcome.tsx); the fake's running job has one.
  await page.evaluate(() =>
    document
      .querySelector(".job-list")
      ?.insertAdjacentHTML("beforeend", '<span class="spinner" role="img" aria-label="Running"></span>'),
  );
  const spinner = page.getByRole("img", { name: "Running" });
  await expect(spinner).toBeVisible();
  const moving = await page.evaluate(() =>
    [...document.querySelectorAll("*")].flatMap((element) =>
      ["", "::before", "::after"].flatMap((pseudo) => {
        const style = getComputedStyle(element, pseudo);
        const animated = style.animationName !== "none";
        const transition = style.transitionDuration.split(",").some((time) => Number.parseFloat(time) > 0);
        return animated || transition ? [`${element.tagName.toLowerCase()}.${element.className}${pseudo}`] : [];
      }),
    ),
  );
  expect(moving).toEqual([]);

  // Without the preference the spinner turns.
  await page.emulateMedia({ reducedMotion: "no-preference" });
  await expect.poll(() => spinner.evaluate((element) => getComputedStyle(element).animationName)).toBe("spin");
});

test("UI-6 UI-23 forced colors: the selected row and the focus keep an outline", async ({ page }) => {
  await page.emulateMedia({ forcedColors: "active" });
  await page.goto("/?fake=tree");
  await item(page, /^api/).click();
  const outline = (locator: Locator) =>
    locator.evaluate((element) => {
      const style = getComputedStyle(element);
      return style.outlineStyle !== "none" && Number.parseFloat(style.outlineWidth) >= 1;
    });
  const selectedRow = page.locator(".tree-row.selected").first();
  await expect.poll(() => outline(selectedRow)).toBe(true);
  await history(page).getByRole("option").nth(1).click();
  await expect.poll(() => outline(page.locator(".history-row.selected"))).toBe(true);
  // The focus ring of the list that has the focus.
  await page.keyboard.press("ArrowDown");
  await expect.poll(() => outline(history(page))).toBe(true);
  await expect.poll(() => outline(page.locator(".tab-item").first())).toBe(true);
});

test("UI-26 at the largest UI font size no label is cut off in the toolbar, the tab bar and dialogs", async ({
  page,
}) => {
  await page.setViewportSize({ width: 1600, height: 1000 });
  await page.goto("/?fake=tree");
  await item(page, /^api/).click();
  const settings = toolbar(page).getByRole("button", { name: "Settings" });
  await settings.click();
  const size = page.getByRole("spinbutton", { name: "UI font size" });
  await size.fill("32");
  await size.press("Enter");
  const uiSize = () =>
    page.evaluate(() => getComputedStyle(document.documentElement).getPropertyValue("--font-size-ui").trim());
  await expect.poll(uiSize).toBe("32px");

  /** The controls in `scope` whose text is wider than their box. */
  const cut = (scope: Locator) =>
    scope.evaluate((root) =>
      [...root.querySelectorAll<HTMLElement>("button, label, legend, [role=tab], h2, .tab-name")]
        .filter((element) => element.getClientRects().length > 0 && element.scrollWidth > element.clientWidth + 1)
        .map((element) => element.textContent?.trim() ?? ""),
    );
  expect(await cut(page.getByRole("dialog", { name: "Settings" }))).toEqual([]);
  await page.keyboard.press("Escape");
  expect(await cut(toolbar(page))).toEqual([]);
  expect(await cut(page.locator(".tab-bar"))).toEqual([]);

  const opener = toolbar(page).getByRole("button", { name: "Branch", exact: true });
  await opener.click();
  expect(await cut(page.getByRole("dialog", { name: "New branch in api" }))).toEqual([]);
  await page.keyboard.press("Escape");
  await page.getByRole("button", { description: "Show the job list" }).click();
  expect(await cut(page.getByRole("dialog", { name: "Jobs" }))).toEqual([]);
  await page.keyboard.press("Escape");
  await openReset(page);
  expect(await cut(page.getByRole("dialog", { name: /^Reset main in api to / }))).toEqual([]);
  await page.getByRole("button", { name: "Reset hard…" }).click();
  expect(await cut(page.getByRole("alertdialog", { name: "Cannot be undone in api" }))).toEqual([]);
});
