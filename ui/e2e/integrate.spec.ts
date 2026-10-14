import { expect, type Page, test } from "@playwright/test";

const tree = (page: Page) => page.getByRole("tree", { name: "Repositories" });
const toolbar = (page: Page) => page.getByRole("toolbar", { name: "Main" });

test("UI-20 merge from the toolbar and UI-13 cherry-pick from the history", async ({ page }) => {
  await page.goto("/?fake=tree");
  const status = page.getByRole("status");
  const merge = toolbar(page).getByRole("button", { name: "Merge" });
  await expect(tree(page).getByRole("treeitem", { name: /^api/ })).toBeVisible();
  await expect(merge).toBeDisabled();

  await tree(page).getByRole("treeitem", { name: /^api/ }).click();
  await merge.click();
  const dialog = page.getByRole("dialog", { name: "Merge into main in api" });
  await dialog.getByRole("textbox", { name: "Branch, remote branch or tag" }).fill("login");
  await expect(dialog.getByRole("option")).toHaveText(["feature/login", "origin/feature/login"]);
  await dialog.getByRole("option", { name: "feature/login", exact: true }).click();
  await dialog.getByRole("button", { name: "Merge" }).click();
  await expect(dialog).toBeHidden();
  await expect(status).toContainText("Merge feature/login into main: Succeeded");

  const history = page.getByRole("listbox", { name: "History" });
  const rows = history.getByRole("option");
  await rows.nth(1).click();
  await rows.nth(2).click({ modifiers: ["ControlOrMeta"] });
  await rows.nth(2).click({ button: "right" });
  const menu = page.getByRole("menu", { name: "Commit actions" });
  await expect(menu.getByRole("menuitem", { name: "Revert" })).toBeDisabled();
  await menu.getByRole("menuitem", { name: "Cherry-pick 2 commits" }).click();
  await expect(menu).toBeHidden();
  await expect(status).toContainText("Cherry-pick 2 commits: Succeeded");
});
