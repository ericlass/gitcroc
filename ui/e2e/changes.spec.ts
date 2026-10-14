import { readFileSync } from "node:fs";
import { expect, type Page, test } from "@playwright/test";

const tree = (page: Page) => page.getByRole("tree", { name: "Repositories" });
const history = (page: Page) => page.getByRole("listbox", { name: "History" });
const files = (page: Page) => page.getByRole("listbox", { name: "Changed files" });
const diff = (page: Page) => page.getByRole("region", { name: "Diff" });

/** Opens api's history and selects its commit with `subject`. */
async function selectCommit(page: Page, subject: string) {
  await tree(page).getByRole("treeitem", { name: /^api/ }).click();
  await history(page).getByRole("option", { name: new RegExp(subject) }).click();
}

test("UI-14 selecting a commit shows its files and the first diff; side by side shows two columns", async ({
  page,
}) => {
  await page.setViewportSize({ width: 1600, height: 900 });
  await page.goto("/?fake=tree");
  await selectCommit(page, "api: change 40");

  await expect(files(page).getByRole("option")).toHaveCount(3);
  await expect(files(page).getByRole("option", { selected: true })).toContainText("README.md");
  await expect(diff(page).locator(".cm-content")).toContainText("line five");
  // Inline: one editor with both line numbers.
  await expect(diff(page).locator(".cm-editor")).toHaveCount(1);
  await expect(diff(page).locator(".cm-diff-removed")).toHaveCount(1);

  await page.getByRole("button", { name: "Side by side" }).click();
  const editors = diff(page).locator(".cm-mergeView .cm-editor");
  await expect(editors).toHaveCount(2);
  await expect(editors.nth(0)).toContainText("line 5");
  await expect(editors.nth(1)).toContainText("line five");
  const [left, right] = [await editors.nth(0).boundingBox(), await editors.nth(1).boundingBox()];
  expect(left && right && right.x >= left.x + left.width - 1).toBe(true);
});

test("UI-16 a merge commit can switch parent", async ({ page }) => {
  await page.goto("/?fake=tree");
  await selectCommit(page, "api: change 36");
  await expect(files(page).getByRole("option")).toHaveText([/side\.txt/]);
  await page.getByRole("combobox", { name: "Compare with parent" }).selectOption("1");
  await expect(files(page).getByRole("option")).toHaveText([/main\.txt/]);
  await expect(diff(page).locator(".cm-content")).toContainText("on main");
});

test("UI-15 uncommitted changes show both lists", async ({ page }) => {
  await page.goto("/?fake=tree");
  await tree(page).getByRole("treeitem", { name: /^api/ }).click();
  await history(page).getByRole("option", { name: /Uncommitted changes/ }).click();
  await expect(page.getByRole("heading", { name: "Uncommitted changes" })).toBeVisible();
  await expect(files(page).locator(".file-section")).toHaveText(["Staged (1)", "Unstaged (4)"]);
  await expect(files(page).getByRole("option", { selected: true })).toContainText("src/app.ts");
  await expect(diff(page).locator(".cm-content")).toContainText("staged line");
});

test("UI-15 stage a hunk, then discard a file after confirming", async ({ page }) => {
  await page.goto("/?fake=tree");
  await tree(page).getByRole("treeitem", { name: /^api/ }).click();
  await history(page).getByRole("option", { name: /Uncommitted changes/ }).click();
  await files(page).getByRole("option", { name: /main\.rs/ }).click();
  await diff(page).getByRole("button", { name: "Stage hunk 2" }).click();
  // The fake stages it; a file with a hunk left is in both lists.
  await expect(files(page).locator(".file-section")).toHaveText(["Staged (2)", "Unstaged (4)"]);

  await files(page).getByRole("option", { name: /notes\.txt/ }).click();
  await page.getByRole("button", { name: "Discard notes.txt" }).click();
  const dialog = page.getByRole("alertdialog");
  await expect(dialog).toContainText("Discard changes in api?");
  await expect(dialog.getByRole("button", { name: "Cancel" })).toBeFocused();
  await dialog.getByRole("button", { name: "Discard" }).click();
  await expect(dialog).toBeHidden();
  await expect(files(page).locator(".file-section")).toHaveText(["Staged (2)", "Unstaged (3)"]);
});

test("UI-15 commit the staged changes; the box empties", async ({ page }) => {
  await page.goto("/?fake=tree");
  await tree(page).getByRole("treeitem", { name: /^api/ }).click();
  await history(page).getByRole("option", { name: /Uncommitted changes/ }).click();
  const box = page.getByRole("region", { name: "Commit message" });
  const commit = box.getByRole("button", { name: "Commit" });
  await expect(commit).toBeDisabled();
  await box.getByRole("textbox", { name: "Subject" }).fill("Stage the app");
  await box.getByRole("textbox", { name: "Body" }).fill("Why it changed.");
  await expect(box).toBeInViewport();
  await commit.click();
  await expect(box.getByRole("textbox", { name: "Subject" })).toHaveValue("");
  await expect(box.getByRole("textbox", { name: "Body" })).toHaveValue("");
  await expect(files(page).locator(".file-section")).toHaveText(["Staged (0)", "Unstaged (4)"]);
});

test("UI-19 the changes pane docks at the bottom and back", async ({ page }) => {
  await page.setViewportSize({ width: 1400, height: 900 });
  await page.goto("/?fake=tree");
  await selectCommit(page, "api: change 40");
  const changes = page.getByRole("region", { name: "Changes" });
  const historyPane = page.getByRole("region", { name: "History" });

  await page.getByRole("button", { name: "Dock at bottom" }).click();
  const [top, below] = [await historyPane.boundingBox(), await changes.boundingBox()];
  expect(top && below && below.y >= top.y + top.height - 1).toBe(true);
  await expect(diff(page).locator(".cm-content")).toContainText("line five");

  await page.getByRole("button", { name: "Dock right" }).click();
  const [left, right] = [await historyPane.boundingBox(), await changes.boundingBox()];
  expect(left && right && right.x >= left.x + left.width - 1).toBe(true);
});

test("UI-24 UI-26 the open diff follows the code theme and the code font at once", async ({ page }) => {
  await page.setViewportSize({ width: 1600, height: 900 });
  await page.goto("/?fake=tree");
  await selectCommit(page, "api: change 40");
  const editor = diff(page).locator(".cm-editor");
  await expect(diff(page).locator(".cm-content")).toContainText("line five");
  const style = () =>
    editor.evaluate((element) => {
      const computed = getComputedStyle(element);
      const scroller = getComputedStyle(element.querySelector(".cm-scroller") as Element);
      return { background: computed.backgroundColor, font: scroller.fontFamily, size: computed.fontSize };
    });
  // GitHub Light, the default for light mode.
  expect((await style()).background).toBe("rgb(255, 255, 255)");

  await page.getByRole("button", { name: "Settings" }).click();
  await page.getByRole("combobox", { name: "Code theme in light mode" }).selectOption("solarized_light");
  await expect.poll(async () => (await style()).background).toBe("rgb(253, 246, 227)");

  await page.getByRole("textbox", { name: "Code font" }).fill("Courier New");
  await page.getByRole("spinbutton", { name: "Code font size" }).fill("18");
  await page.getByRole("spinbutton", { name: "Code font size" }).press("Tab");
  await expect.poll(async () => (await style()).size).toBe("18px");
  // The font and size settings arrive as two changes; the editor may still apply the second.
  await expect.poll(async () => (await style()).font).toMatch(/^"Courier New", ui-monospace/);
});

/** The app's content security policy from `tauri.conf.json`, as a header. */
function appCsp(styleNonce: string): string {
  const config = JSON.parse(readFileSync(new URL("../../crates/gitcroc-app/tauri.conf.json", import.meta.url), "utf8"));
  const directives = config.app.security.csp as Record<string, string>;
  return Object.entries(directives)
    .map(([name, sources]) => `${name} ${sources}${name === "style-src" ? ` 'nonce-${styleNonce}'` : ""}`)
    .join("; ");
}

test("SEC-2 diffs and images work under the app's CSP without 'unsafe-inline'", async ({ page }) => {
  const nonce = "e2e-nonce";
  const csp = appCsp(nonce);
  expect(csp).not.toContain("unsafe-inline");
  // Like Tauri: the policy as a header, and the nonce on the page's style tags.
  await page.route(/\/(\?.*)?$/, async (route) => {
    const response = await route.fetch();
    const body = (await response.text()).replaceAll("<style", `<style nonce="${nonce}"`);
    await route.fulfill({ response, body, headers: { ...response.headers(), "content-security-policy": csp } });
  });
  await page.addInitScript(() => {
    const violations: string[] = [];
    (window as unknown as { violations: string[] }).violations = violations;
    document.addEventListener("securitypolicyviolation", (event) => {
      violations.push(`${event.violatedDirective} ${event.blockedURI} ${event.sourceFile}:${event.lineNumber}:${event.columnNumber} ${event.sample}`);
    });
  });
  await page.setViewportSize({ width: 1600, height: 900 });
  await page.goto("/?fake=tree");
  await selectCommit(page, "api: change 40");
  await expect(diff(page).locator(".cm-content")).toContainText("line five");

  // CodeMirror's own styles applied: its editor is a flex column.
  const editor = diff(page).locator(".cm-editor");
  expect(await editor.evaluate((element) => getComputedStyle(element).display)).toBe("flex");
  // The theme's added and removed lines have their colors.
  const removed = diff(page).locator(".cm-diff-removed").first();
  const background = await removed.evaluate((element) => getComputedStyle(element).backgroundColor);
  expect(background).not.toBe("rgba(0, 0, 0, 0)");

  // Images arrive as bytes and show through blob: URLs.
  await files(page).getByRole("option", { name: /logo\.png/ }).click();
  const after = diff(page).getByRole("img", { name: "After image" });
  await expect(after).toHaveAttribute("src", /^blob:/);
  await expect.poll(() => after.evaluate((image: HTMLImageElement) => image.naturalWidth)).toBe(1);

  expect(await page.evaluate(() => (window as unknown as { violations: string[] }).violations)).toEqual([]);
});
