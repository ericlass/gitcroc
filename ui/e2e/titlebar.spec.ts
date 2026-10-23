import { expect, type Page, test } from "@playwright/test";

/** What reached the fake core's window, oldest first. */
const actions = (page: Page) =>
  page.evaluate(() => (window as unknown as { fakeCore: { window: { actions: string[] } } }).fakeCore.window.actions);

test("UI-35 the tab bar is the title bar: the empty space moves the window and the buttons work", async ({ page }) => {
  await page.setViewportSize({ width: 1280, height: 800 });
  await page.goto("/?fake=tree");
  const tab = page.getByRole("tab", { name: "My workspace" });
  await expect(tab).toBeVisible();
  const buttons = page.getByRole("group", { name: "Window controls" });

  // The buttons end the tab bar, at the window's right edge.
  const close = buttons.getByRole("button", { name: "Close window" });
  const box = await close.boundingBox();
  expect(Math.round((box?.x ?? 0) + (box?.width ?? 0))).toBe(1280);
  const bar = await page.locator(".tab-bar").boundingBox();
  expect(box?.y).toBe(bar?.y);

  // A press on the empty space moves the window, a double-click maximizes it; a press on a tab does neither.
  const space = page.locator(".title-drag");
  await space.click();
  await tab.click();
  await space.dblclick();
  expect(await actions(page)).toEqual(["start_dragging", "start_dragging", "toggle_maximize"]);
  await expect(buttons.getByRole("button", { name: "Restore" })).toBeVisible();

  await buttons.getByRole("button", { name: "Restore" }).click();
  await buttons.getByRole("button", { name: "Minimize" }).click();
  await close.click();
  expect((await actions(page)).slice(3)).toEqual(["toggle_maximize", "minimize", "close"]);
});

test("UI-35 the system title bar setting leaves only the tabs", async ({ page }) => {
  await page.goto("/?fake=tree");
  const buttons = page.getByRole("group", { name: "Window controls" });
  await expect(buttons).toBeVisible();
  await page.getByRole("button", { name: "Settings" }).click();
  await page.getByRole("checkbox", { name: "Use the system title bar" }).check();
  await expect(buttons).toBeHidden();
  await page.keyboard.press("Escape");
  await expect(page.getByRole("dialog", { name: "Settings" })).toBeHidden();
  await page.locator(".title-drag").click();
  expect(await actions(page)).toEqual([]);
});

test("UI-35 the git setup screen has a title bar with the app's name", async ({ page }) => {
  await page.goto("/?fake=no-git");
  await expect(page.getByRole("heading", { level: 1 })).toBeVisible();
  await expect(page.locator(".title-bar-name")).toHaveText("gitcroc");
  await page.locator(".title-bar").click({ position: { x: 300, y: 10 } });
  await page.getByRole("button", { name: "Minimize" }).click();
  expect(await actions(page)).toEqual(["start_dragging", "minimize"]);
});
