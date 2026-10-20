import { expect, type Page, test } from "@playwright/test";

const tree = (page: Page) => page.getByRole("tree", { name: "Repositories" });

test("PLAT-4 from the notice to Install and restart, with the download in the status bar and the job list", async ({
  page,
}) => {
  await page.goto("/?fake=update");
  await expect(tree(page).getByRole("treeitem").first()).toBeVisible();
  const notice = page.getByRole("button", { name: "gitcroc 0.2.0 is available" });
  await expect(page.locator("[aria-live=polite]")).toHaveText("gitcroc 0.2.0 is available");

  await notice.click();
  const dialog = page.getByRole("dialog", { name: "Update gitcroc" });
  await expect(dialog).toContainText("You have gitcroc 0.1.0; gitcroc 0.2.0 is available.");
  // The notes are text, markup and all (SEC-3).
  await expect(dialog.locator(".update-notes")).toHaveText("Faster history.\n<b>Shown as text</b>");
  await dialog.getByRole("button", { name: "Install and restart" }).click();
  await expect(dialog).toBeHidden();

  const status = page.getByRole("button", { description: "Show the job list" });
  await expect(status).toContainText("Update to 0.2.0: Succeeded");
  await expect(notice).toBeHidden();
  await status.click();
  const jobs = page.getByRole("dialog", { name: "Jobs" });
  await expect(jobs.getByRole("option", { name: /Update to 0\.2\.0/ })).toBeVisible();
  const installs = await page.evaluate(
    () => (window as unknown as { fakeCore: { update: { installs: number } } }).fakeCore.update.installs,
  );
  expect(installs).toBe(1);
});

test("PLAT-4 a .deb opens the download page and Later hides the notice", async ({ page }) => {
  await page.goto("/?fake=update-deb");
  const notice = page.getByRole("button", { name: "gitcroc 0.2.0 is available" });
  await notice.click();
  const dialog = page.getByRole("dialog", { name: "Update gitcroc" });
  await expect(dialog.getByRole("button", { name: "Install and restart" })).toHaveCount(0);
  await dialog.getByRole("button", { name: "Open download page" }).click();
  await expect(dialog).toBeHidden();

  await notice.click();
  await dialog.getByRole("button", { name: "Later" }).click();
  await expect(notice).toBeHidden();
});
