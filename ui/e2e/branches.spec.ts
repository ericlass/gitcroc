import { expect, type Page, test } from "@playwright/test";

const tree = (page: Page) => page.getByRole("tree", { name: "Repositories" });
const item = (page: Page, name: string | RegExp) => tree(page).getByRole("treeitem", { name, exact: true });

test("UI-7 create a branch from a commit, check it out by double-click, rename it, delete it", async ({ page }) => {
  await page.goto("/?fake=tree");
  const branch = page.getByRole("toolbar", { name: "Main" }).getByRole("button", { name: "Branch" });
  await expect(item(page, /^web/)).toBeVisible();
  // UI-20: nothing selected, nothing to create a branch in.
  await expect(branch).toBeDisabled();

  await item(page, /^web/).click();
  await branch.click();
  const create = page.getByRole("dialog", { name: "New branch in web" });
  await create.getByRole("textbox", { name: "Name" }).fill("hot fix");
  await expect(create.getByText("the name cannot contain spaces")).toBeVisible();
  await expect(create.getByRole("button", { name: "Create" })).toBeDisabled();
  await create.getByRole("textbox", { name: "Name" }).fill("hotfix");
  await create.getByRole("combobox", { name: "Branch, tag or commit" }).fill("0123abc");
  await create.getByRole("checkbox", { name: "Check out the new branch" }).uncheck();
  await create.getByRole("button", { name: "Create" }).click();
  await expect(create).toBeHidden();

  await item(page, /^web/).locator(".tree-chevron").click();
  await item(page, "Local").locator(".tree-chevron").click();
  await expect(item(page, "main Current branch")).toBeVisible();
  await item(page, "hotfix").dblclick();
  await expect(item(page, "hotfix Current branch")).toBeVisible();
  await expect(item(page, "main")).toBeVisible();

  await item(page, "main").dblclick();
  await expect(item(page, "main Current branch")).toBeVisible();
  await item(page, "hotfix").click({ button: "right" });
  const menu = page.getByRole("menu", { name: "Branch" });
  await menu.getByRole("menuitem", { name: "Rename…" }).click();
  const rename = page.getByRole("dialog", { name: "Rename hotfix in web" });
  await rename.getByRole("textbox", { name: "New name" }).fill("hotfix-2");
  await rename.getByRole("button", { name: "Rename" }).click();
  await expect(item(page, "hotfix-2")).toBeVisible();
  await expect(item(page, "hotfix")).toHaveCount(0);

  await item(page, "hotfix-2").click({ button: "right" });
  await menu.getByRole("menuitem", { name: "Delete…" }).click();
  await page.getByRole("dialog", { name: "Delete branch hotfix-2 in web?" }).getByRole("button", { name: "Delete" }).click();
  await expect(item(page, "hotfix-2")).toHaveCount(0);
  await expect(item(page, "release")).toBeVisible();
});
