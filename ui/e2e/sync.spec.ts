import { expect, type Page, test } from "@playwright/test";

const tree = (page: Page) => page.getByRole("tree", { name: "Repositories" });
const toolbar = (page: Page) => page.getByRole("toolbar", { name: "Main" });

test("UI-20 fetch, pull and push the selected repo from the toolbar", async ({ page }) => {
  await page.goto("/?fake=tree");
  const status = page.getByRole("button", { description: "Show the job list" });
  const fetch = toolbar(page).getByRole("button", { name: "Fetch", exact: true });
  await expect(tree(page).getByRole("treeitem", { name: /^api/ })).toBeVisible();
  await expect(fetch).toBeDisabled();

  await tree(page).getByRole("treeitem", { name: /^api/ }).click();
  await fetch.click();
  await expect(status).toContainText("Fetch all remotes: Nothing to do");

  // api's main is ahead and behind: the default pull fast-forwards only.
  await toolbar(page).getByRole("button", { name: "Pull", exact: true }).click();
  await expect(status).toContainText("Pull origin/main into main: Failed");
  await toolbar(page).getByRole("button", { name: "Pull options" }).click();
  const pull = page.getByRole("dialog", { name: "Pull api" });
  await pull.getByRole("radio", { name: "Merge" }).check();
  await pull.getByRole("button", { name: "Pull" }).click();
  await expect(pull).toBeHidden();
  await expect(status).toContainText("Pull origin/main into main: Succeeded");

  await toolbar(page).getByRole("button", { name: "Push", exact: true }).click();
  await expect(status).toContainText("Push main to origin/main: Succeeded");
  await toolbar(page).getByRole("button", { name: "Push", exact: true }).click();
  await expect(status).toContainText("Push main to origin/main: Nothing to do");
});
