import { expect, test } from "@playwright/test";

test("PLAT-3 a git chosen through the file dialog reaches the app after a git_status event", async ({ page }) => {
  await page.goto("/?fake=no-git");
  await expect(page.getByRole("heading", { name: "gitcroc needs git" })).toBeVisible();
  await expect(page.getByRole("button", { name: "Use automatic detection" })).toHaveCount(0);

  await page.getByRole("button", { name: "Choose git…" }).click();
  await expect(page.getByRole("toolbar", { name: "Main" })).toBeVisible();
  await expect(page.getByRole("heading", { name: "gitcroc needs git" })).toHaveCount(0);

  await page.getByRole("button", { name: "Settings" }).click();
  const section = page.getByRole("group", { name: "Git" });
  await expect(section).toContainText("git version 2.51.0 at /opt/git/bin/git");

  // Back to automatic detection, which finds no git: the setup screen again.
  await section.getByRole("button", { name: "Use automatic detection" }).click();
  await expect(page.getByRole("heading", { name: "gitcroc needs git" })).toBeVisible();
  await expect(page.getByRole("button", { name: "Use automatic detection" })).toHaveCount(0);
});
