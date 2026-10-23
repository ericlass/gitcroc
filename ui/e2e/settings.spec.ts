import { expect, type Page, test } from "@playwright/test";

const toolbar = (page: Page) => page.getByRole("toolbar", { name: "Main" });
const settings = (page: Page) => page.getByRole("dialog", { name: "Settings" });
const tabs = (page: Page) => settings(page).getByRole("tablist", { name: "Settings sections" });
const tab = (page: Page, name: string) => tabs(page).getByRole("tab", { name, exact: true });

/** The dialog's size, rounded. */
async function size(page: Page): Promise<{ width: number; height: number }> {
  const box = await settings(page).boundingBox();
  if (!box) {
    throw new Error("Settings is not visible");
  }
  return { width: Math.round(box.width), height: Math.round(box.height) };
}

test("UI-20 Settings shows one group of settings at a time in tabs, each without scrolling, at one size", async ({
  page,
}) => {
  await page.setViewportSize({ width: 1280, height: 720 });
  await page.goto("/?fake=ai-conflict");
  await toolbar(page).getByRole("button", { name: "Settings" }).click();

  expect(await tabs(page).getByRole("tab").allTextContents()).toEqual(["Appearance", "Git", "Updates", "AI"]);
  await expect(tab(page, "Appearance")).toHaveAttribute("aria-selected", "true");
  await expect(settings(page).getByRole("radio", { name: "Follow the system" })).toBeFocused();
  await expect(settings(page).getByRole("spinbutton", { name: "Repos at once" })).toBeHidden();
  const first = await size(page);

  const shown: Record<string, ["spinbutton" | "checkbox", string]> = {
    Appearance: ["spinbutton", "UI font size"],
    Git: ["spinbutton", "Repos at once"],
    Updates: ["checkbox", "Check for updates automatically"],
    AI: ["checkbox", "AI features"],
  };
  for (const [name, [role, control]] of Object.entries(shown)) {
    await tab(page, name).click();
    await expect(tab(page, name)).toHaveAttribute("aria-selected", "true");
    await expect(settings(page).getByRole("tabpanel", { name })).toBeVisible();
    await expect(settings(page).getByRole(role, { name: control })).toBeVisible();
    expect(await size(page)).toEqual(first);
    expect(await settings(page).evaluate((dialog) => dialog.scrollHeight <= dialog.clientHeight)).toBe(true);
  }
  // The version line stays below every tab.
  await expect(settings(page)).toContainText(/version .* on /);
});

test("UI-20 the arrow keys, Home and End move between the Settings tabs and show each one", async ({ page }) => {
  await page.goto("/?fake=tree");
  await toolbar(page).getByRole("button", { name: "Settings" }).click();
  await tab(page, "Appearance").focus();

  await page.keyboard.press("ArrowRight");
  await expect(tab(page, "Git")).toBeFocused();
  await expect(tab(page, "Git")).toHaveAttribute("aria-selected", "true");
  await expect(settings(page).getByRole("spinbutton", { name: "Repos at once" })).toBeVisible();
  await page.keyboard.press("End");
  await expect(tab(page, "AI")).toBeFocused();
  await page.keyboard.press("ArrowRight");
  await expect(tab(page, "Appearance")).toBeFocused();
  await page.keyboard.press("ArrowLeft");
  await expect(tab(page, "AI")).toBeFocused();
  await page.keyboard.press("Home");
  await expect(tab(page, "Appearance")).toHaveAttribute("aria-selected", "true");

  // Only the selected tab is a Tab stop; Tab goes on into its panel.
  await expect(tabs(page).locator("[tabindex='0']")).toHaveCount(1);
  await page.keyboard.press("Tab");
  await expect(settings(page).getByRole("radio", { name: "Follow the system" })).toBeFocused();
});
