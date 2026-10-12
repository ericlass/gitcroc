import { expect, test } from "@playwright/test";

test("built app shows what the core reports", async ({ page }) => {
  await page.goto("/");
  await expect(page.getByRole("heading", { name: "gitcroc" })).toBeVisible();
  await expect(page.getByText("Version 0.0.0-fake on fake-os (fake-arch)")).toBeVisible();
  await expect(page).toHaveTitle("gitcroc");
});
