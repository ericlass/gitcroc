import { expect, test } from "@playwright/test";

// UI-21 with the fake core's jobs (src/ipc/fakeJobs.ts): one per outcome,
// a running and a queued job, from two workspaces.
test("UI-21 job list opens from the status bar with the keyboard", async ({ page }) => {
  await page.goto("/?fake=jobs");
  const opener = page.getByRole("button", { description: "Show the job list" });
  await expect(opener).toContainText("Fetch Receiving objects 3/10");

  await opener.focus();
  await page.keyboard.press("Enter");
  const dialog = page.getByRole("dialog", { name: "Jobs" });
  await expect(dialog).toBeVisible();
  const list = dialog.getByRole("listbox", { name: "Jobs" });
  await expect(list).toBeFocused();
  await expect(list.getByRole("option")).toHaveCount(7);

  // Down to the failed push: queued fetch, running fetch, conflict, failed.
  await page.keyboard.press("ArrowDown");
  await page.keyboard.press("ArrowDown");
  await page.keyboard.press("ArrowDown");
  await expect(list.getByRole("option", { selected: true })).toContainText("Push");
  const output = dialog.getByRole("region", { name: "Job output" });
  await expect(output.locator("pre")).toHaveText(
    " ! [rejected]        main -> main (fetch first)\nerror: failed to push some refs\n",
    { useInnerText: false },
  );

  await page.keyboard.press("Escape");
  await expect(dialog).toBeHidden();
  await expect(opener).toBeFocused();
  // The failure was seen; the running fetch still shows.
  await expect(opener).toHaveText("Fetch Receiving objects 3/10");
});
