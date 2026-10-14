import { expect, type Page, test } from "@playwright/test";

const tree = (page: Page) => page.getByRole("tree", { name: "Repositories" });
const history = (page: Page) => page.getByRole("listbox", { name: "History" });
const files = (page: Page) => page.getByRole("listbox", { name: "Changed files" });
const conflict = (page: Page) => page.getByRole("region", { name: "Conflict" });
const editor = (page: Page) => conflict(page).locator(".cm-content");

test("OPS-3 resolve two files of a merge with the keyboard only; Continue becomes enabled", async ({ page }) => {
  await page.setViewportSize({ width: 1400, height: 900 });
  await page.goto("/?fake=conflict");
  await tree(page).getByRole("treeitem", { name: /^api/ }).click();
  await history(page).getByRole("option", { name: /Uncommitted changes/ }).click();
  const banner = page.getByRole("region", { name: "Conflict state" });
  await expect(banner.getByRole("button", { name: "Continue" })).toBeDisabled();
  await expect(files(page).locator(".file-section")).toHaveText(["Unmerged (2)", "Staged (0)", "Unstaged (0)"]);
  await expect(files(page).getByRole("option", { selected: true })).toContainText("README.md");
  await expect(conflict(page).getByText("Theirs: feature/login")).toBeVisible();

  // From here on, the keyboard only: from the file list to Next conflict.
  await files(page).focus();
  await page.keyboard.press("Tab");
  await page.keyboard.press("Tab");
  await expect(conflict(page).getByRole("button", { name: "Next conflict" })).toBeFocused();
  await page.keyboard.press("Enter");
  await expect(editor(page)).toBeFocused();
  await page.keyboard.press("Alt+2");
  await expect(editor(page)).toHaveText("firsttheirslast");
  await expect(files(page).getByRole("option", { name: /README\.md/ }).getByRole("img", { name: "unsaved edits" })).toBeVisible();
  await page.keyboard.press("ControlOrMeta+Enter");

  // The file leaves the list; the next one opens, with the focus.
  await expect(files(page).locator(".file-section")).toHaveText(["Unmerged (1)", "Staged (0)", "Unstaged (0)"]);
  await expect(files(page).getByRole("option", { selected: true })).toContainText("src/app.ts");
  await expect(editor(page)).toBeFocused();
  await expect(banner.getByRole("button", { name: "Continue" })).toBeDisabled();
  await page.keyboard.press("Alt+ArrowDown");
  await page.keyboard.press("Alt+1");
  // A hand edit on top of it.
  await page.keyboard.press("End");
  await page.keyboard.type(" and more");
  await expect(editor(page)).toHaveText("firstours and morelast");
  await page.keyboard.press("ControlOrMeta+Enter");

  await expect(files(page).locator(".file-section")).toHaveText(["Staged (0)", "Unstaged (0)"]);
  await expect(banner).toContainText("All conflicts are resolved and staged.");
  await expect(banner.getByRole("button", { name: "Continue" })).toBeEnabled();
});
