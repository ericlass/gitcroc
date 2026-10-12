import { expect, test } from "@playwright/test";

test("built app shows what the core reports", async ({ page }) => {
  await page.goto("/");
  await expect(page).toHaveTitle("gitcroc");
  await page.getByRole("button", { name: "Settings" }).click();
  await expect(page.getByText("gitcroc version 0.0.0-fake on fake-os (fake-arch)")).toBeVisible();
});
