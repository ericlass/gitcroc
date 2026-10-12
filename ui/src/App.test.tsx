import { cleanup, fireEvent, render, screen } from "@solidjs/testing-library";
import { afterEach, expect, test } from "vitest";
import App from "./App";
import { emitJobProgress, fakeState, reset } from "./ipc/fake";

afterEach(() => {
  cleanup();
  reset();
});

test("arch_6_ui_state_comes_only_from_core", async () => {
  fakeState.appInfo = { name: "gitcroc", version: "9.9.9", os: "plan9", arch: "mips" };
  render(() => <App />);
  fireEvent.click(await screen.findByRole("button", { name: "Settings" }));
  expect(await screen.findByText("gitcroc version 9.9.9 on plan9 (mips)")).toBeDefined();
});

test("PLAT-3: Settings shows the git that detection found", async () => {
  fakeState.gitStatus = { kind: "usable", path: "/opt/git/bin/git", version: "2.51.1" };
  render(() => <App />);
  fireEvent.click(await screen.findByRole("button", { name: "Settings" }));
  expect(await screen.findByText("git version 2.51.1 at /opt/git/bin/git")).toBeDefined();
});

test("UI-20 layout: tab bar, toolbar, three panes and status bar", async () => {
  render(() => <App />);
  const toolbar = await screen.findByRole("toolbar", { name: "Main" });
  const buttons = [...toolbar.querySelectorAll("button")];
  expect(buttons.map((b) => b.textContent)).toEqual([
    "Fetch",
    "Pull",
    "Push",
    "Branch",
    "Merge",
    "Stash",
    "Command palette",
    "Settings",
  ]);
  expect(buttons.filter((b) => !b.disabled).map((b) => b.textContent)).toEqual(["Settings"]);

  expect(screen.getByRole("tab", { name: "No workspace" }).getAttribute("aria-selected")).toBe("true");
  expect((screen.getByRole("button", { name: "Open workspace" }) as HTMLButtonElement).disabled).toBe(true);
  for (const pane of ["Tree", "History", "Changes"]) {
    expect(screen.getByRole("region", { name: pane })).toBeDefined();
  }
  expect(screen.getAllByRole("separator")).toHaveLength(2);
  expect(screen.getByRole("status").textContent).toBe("No jobs running");
});

test("UI-21 status bar shows running jobs of the workspace", async () => {
  render(() => <App />);
  const status = await screen.findByRole("status");
  emitJobProgress({ job_id: 1, workspace_id: "placeholder", label: "Fetch", done: 1, total: 4 });
  emitJobProgress({ job_id: 2, workspace_id: "elsewhere", label: "Push", done: 1, total: 2 });
  expect(status.textContent).toBe("Fetch 1/4");
  emitJobProgress({ job_id: 1, workspace_id: "placeholder", label: "Fetch", done: 4, total: 4 });
  expect(status.textContent).toBe("No jobs running");
});

test("settings dialog closes with Escape and the Close button", async () => {
  render(() => <App />);
  fireEvent.click(await screen.findByRole("button", { name: "Settings" }));
  fireEvent.keyDown(await screen.findByRole("dialog"), { key: "Escape" });
  expect(screen.queryByRole("dialog")).toBeNull();
  fireEvent.click(screen.getByRole("button", { name: "Settings" }));
  fireEvent.click(await screen.findByRole("button", { name: "Close" }));
  expect(screen.queryByRole("dialog")).toBeNull();
});
