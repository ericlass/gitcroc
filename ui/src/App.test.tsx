import { cleanup, fireEvent, render, screen, within } from "@solidjs/testing-library";
import { afterEach, expect, test } from "vitest";
import App from "./App";
import { emitJobProgress, emitWatchWarning, fakeState, reset } from "./ipc/fake";
import { fakeJob } from "./ipc/fakeJobs";

afterEach(() => {
  cleanup();
  reset();
});

test("arch_6_ui_state_comes_only_from_core", async () => {
  fakeState.appInfo = { name: "gitcroc", version: "9.9.9", os: "plan9", arch: "mips", ui_font: null, code_font: null };
  render(() => <App />);
  fireEvent.click(await screen.findByRole("button", { name: "Settings" }));
  expect(await screen.findByText("gitcroc version 9.9.9 on plan9 (mips)")).toBeDefined();
});

test("PLAT-3: Settings shows the git that detection found", async () => {
  fakeState.gitStatus = {
    kind: "usable",
    path: "/opt/git/bin/git",
    version: "2.51.1",
    chosen: null,
    chosen_missing: false,
  };
  render(() => <App />);
  fireEvent.click(await screen.findByRole("button", { name: "Settings" }));
  expect(await screen.findByText("git version 2.51.1 at /opt/git/bin/git")).toBeDefined();
});

test("UI-20 layout: tab bar, toolbar, three panes and status bar", async () => {
  render(() => <App />);
  const toolbar = await screen.findByRole("toolbar", { name: "Main" });
  const buttons = [...toolbar.querySelectorAll("button")];
  expect(buttons.map((b) => b.getAttribute("aria-label") ?? b.textContent)).toEqual([
    "Fetch",
    "Fetch options",
    "Pull",
    "Pull options",
    "Push",
    "Push options",
    "Branch",
    "Merge",
    "Stash",
    "Command palette",
    "Settings",
  ]);
  expect(buttons.filter((b) => !b.disabled).map((b) => b.textContent)).toEqual(["Settings"]);

  expect((await screen.findByRole("tab", { name: "My workspace" })).getAttribute("aria-selected")).toBe("true");
  expect((screen.getByRole("button", { name: "Open workspace" }) as HTMLButtonElement).disabled).toBe(false);
  for (const pane of ["Tree", "History", "Changes"]) {
    expect(await screen.findByRole("region", { name: pane })).toBeDefined();
  }
  expect(await screen.findAllByRole("separator")).toHaveLength(2);
  expect(screen.getByRole("button", { description: "Show the job list" }).textContent).toBe("No jobs running");
});

test("ARCH-9 the workspace id comes from the core's workspace file", async () => {
  fakeState.workspace = { ...fakeState.workspace, id: "0b7c6f2e-uuid", name: "Platform" };
  fakeState.tabs = ["0b7c6f2e-uuid"];
  fakeState.active = "0b7c6f2e-uuid";
  render(() => <App />);
  const status = await screen.findByRole("button", { description: "Show the job list" });
  expect(screen.getByRole("tab", { name: "Platform" })).toBeDefined();
  const running = { state: "running" as const, outcome: null, done: 1, total: 2 };
  emitJobProgress(fakeJob({ id: "fake-workspace", name: "Old" }, { job_id: 1, label: "Old id", ...running }));
  emitJobProgress(fakeJob({ id: "0b7c6f2e-uuid", name: "Platform" }, { job_id: 2, label: "Add repos", ...running }));
  expect(status.textContent).toBe("Add repos 1/2");
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

test("UI-27 with every tab closed the window shows that no workspace is open", async () => {
  fakeState.tabs = [];
  fakeState.active = null;
  render(() => <App />);
  expect(await screen.findByText(/No workspace is open/)).toBeDefined();
  expect(screen.queryByRole("region", { name: "Tree" })).toBeNull();
  // The "+" menu's actions, in place (UI-28).
  const start = screen.getByRole("group", { name: "Workspaces" });
  expect(await within(start).findByRole("button", { name: /^My workspace/ })).toBeDefined();
  expect(within(start).getByRole("button", { name: "New workspace…" })).toBeDefined();
  expect((within(start).getByRole("button", { name: "Import workspace…" }) as HTMLButtonElement).disabled).toBe(false);
  fireEvent.click(within(start).getByRole("button", { name: /^My workspace/ }));
  expect(await screen.findByRole("region", { name: "Tree" })).toBeDefined();
});
