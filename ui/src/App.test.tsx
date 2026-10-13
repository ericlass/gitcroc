import { cleanup, fireEvent, render, screen } from "@solidjs/testing-library";
import { afterEach, expect, test } from "vitest";
import App from "./App";
import { emitJobProgress, emitWatchWarning, fakeState, reset } from "./ipc/fake";

afterEach(() => {
  cleanup();
  reset();
});

test("arch_6_ui_state_comes_only_from_core", async () => {
  fakeState.appInfo = { name: "gitcroc", version: "9.9.9", os: "plan9", arch: "mips", ui_font: null };
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

  expect((await screen.findByRole("tab", { name: "My workspace" })).getAttribute("aria-selected")).toBe("true");
  expect((screen.getByRole("button", { name: "Open workspace" }) as HTMLButtonElement).disabled).toBe(true);
  for (const pane of ["Tree", "History", "Changes"]) {
    expect(await screen.findByRole("region", { name: pane })).toBeDefined();
  }
  expect(await screen.findAllByRole("separator")).toHaveLength(2);
  expect(screen.getByRole("status").textContent).toBe("No jobs running");
});

test("UI-21 status bar shows running jobs of the workspace", async () => {
  render(() => <App />);
  const status = await screen.findByRole("status");
  emitJobProgress({ job_id: 1, workspace_id: "fake-workspace", label: "Fetch", done: 1, total: 4 });
  emitJobProgress({ job_id: 2, workspace_id: "elsewhere", label: "Push", done: 1, total: 2 });
  expect(status.textContent).toBe("Fetch 1/4");
  emitJobProgress({ job_id: 1, workspace_id: "fake-workspace", label: "Fetch", done: 4, total: 4 });
  expect(status.textContent).toBe("No jobs running");
});

test("ARCH-9 the workspace id comes from the core's workspace file", async () => {
  fakeState.workspace = { ...fakeState.workspace, id: "0b7c6f2e-uuid", name: "Platform" };
  render(() => <App />);
  const status = await screen.findByRole("status");
  expect(screen.getByRole("tab", { name: "Platform" })).toBeDefined();
  emitJobProgress({ job_id: 1, workspace_id: "fake-workspace", label: "Old id", done: 1, total: 2 });
  emitJobProgress({ job_id: 2, workspace_id: "0b7c6f2e-uuid", label: "Looking for repos", done: 5, total: 0 });
  expect(status.textContent).toBe("Looking for repos 5");
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

test("OPS-7 a file watching failure shows a banner until dismissed", async () => {
  render(() => <App />);
  await screen.findByRole("toolbar", { name: "Main" });
  // Let the workspace's subscriptions start.
  await new Promise((resolve) => setTimeout(resolve, 0));
  emitWatchWarning({ workspace_id: fakeState.workspace.id, message: "OS file watch limit reached." });
  const banner = await screen.findByRole("alert");
  expect(banner.textContent).toContain("OS file watch limit reached.");
  expect(banner.textContent).toContain("when the window regains focus");
  fireEvent.click(screen.getByRole("button", { name: "Dismiss" }));
  expect(screen.queryByRole("alert")).toBeNull();
});
