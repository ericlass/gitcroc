import { expect, test } from "@playwright/test";

test("built app renders the app name", async ({ page }) => {
  await page.goto("/");
  await expect(page.getByRole("heading", { name: "gitcroc" })).toBeVisible();
  await expect(page).toHaveTitle("gitcroc");
});
