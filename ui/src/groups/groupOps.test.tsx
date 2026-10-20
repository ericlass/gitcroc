import { cleanup, fireEvent, render, screen, within } from "@solidjs/testing-library";
import { Show } from "solid-js";
import { afterEach, beforeEach, expect, test, vi } from "vitest";
import App from "../App";
import { type GroupOp, groupPreflight, groupStart, type RepoId, type TargetSet } from "../ipc";
import { fakeState, reset } from "../ipc/fake";
import { fakeGroups, finishGroup } from "../ipc/fakeGroups";
import { refNodes, summary } from "../ipc/fakeTree";
import { createGroupOpStore } from "../stores/groupOps";
import PreflightDialog from "./PreflightDialog";

/** The status bar: its button shows the jobs (UI-21). */
const statusBar = () => screen.getByRole("button", { description: "Show the job list" });

// jsdom lays nothing out; give the tree and the job list a height so their rows are rendered.
beforeEach(() => {
  const real = HTMLElement.prototype.getBoundingClientRect;
  vi.spyOn(HTMLElement.prototype, "getBoundingClientRect").mockImplementation(function (this: HTMLElement) {
    if (this.classList.contains("tree-scroller") || this.classList.contains("job-scroller")) {
      return new DOMRect(0, 0, 300, 800);
    }
    return real.call(this);
  });
});

afterEach(() => {
  cleanup();
  reset();
  vi.restoreAllMocks();
});

const settle = () => new Promise((resolve) => setTimeout(resolve, 0));

/** Lets several rounds of fake calls and events finish. */
const settleAll = async () => {
  for (let round = 0; round < 8; round++) {
    await settle();
  }
};

const id = () => fakeState.workspace.id;

const repo = (repoId: string, name: string) => ({ id: repoId, name, path: `/src/${name}`, remote: null });

/** The group All holds api, web, cli and docs, in that order. */
const ALL: TargetSet = { kind: "group", group_id: "g1" };

const FETCH: GroupOp = { kind: "fetch", prune: "config" };

function workspace() {
  fakeState.workspace = {
    ...fakeState.workspace,
    repos: [repo("r1", "api"), repo("r2", "web"), repo("r3", "cli"), repo("r4", "docs")],
    groups: [{ id: "g1", name: "All", repos: ["r1", "r2", "r3", "r4"] }],
  };
  fakeState.refs = Object.fromEntries(
    ["r1", "r2", "r3", "r4"].map((repoId) => [repoId, refNodes(repoId, { local: ["main"], current: "main" })]),
  );
  fakeState.summaries = ["r1", "r2", "r3", "r4"].map((repoId) => summary(id(), repoId, "main"));
}

/** The preflight dialog with its store, as App puts them together. */
async function dialogSetup() {
  workspace();
  const groupOps = createGroupOpStore();
  await groupOps.subscribe();
  render(() => (
    <Show when={groupOps.state.pending}>{(pending) => <PreflightDialog pending={pending()} groupOps={groupOps} />}</Show>
  ));
  return groupOps;
}

/** The whole app on the workspace. */
async function appSetup() {
  workspace();
  render(() => <App />);
  await screen.findAllByRole("treeitem");
  await settleAll();
}

/** Runs `op` on All as another part of the app would, confirmed. */
async function runOnAll(op: GroupOp = FETCH) {
  const preflight = await groupPreflight(id(), ALL, op);
  await settleAll();
  const job = await groupStart(preflight, true);
  await settleAll();
  return job;
}

const dialog = () => screen.queryByRole("dialog") ?? screen.queryByRole("alertdialog");
const repoRows = () =>
  within(screen.getByRole("list", { name: "Repos" }))
    .getAllByRole("listitem")
    .map((row) => [row.querySelector(".preflight-name")?.textContent, row.querySelector(".preflight-class")?.textContent]);

test("the preflight dialog appears only when the core says so, and Run starts with confirmed", async () => {
  const groupOps = await dialogSetup();

  // Every repo will run and nothing is destructive: the job starts at once.
  await groupOps.run(id(), ALL, FETCH);
  await settleAll();
  expect(dialog()).toBeNull();
  expect(fakeGroups.starts).toEqual([{ preflightId: 1, confirmed: false }]);
  expect(fakeState.jobs[0]?.label).toBe("Fetch · All (4 repos)");

  // A repo that is not *will run* shows the dialog; a progress line first.
  fakeGroups.classes = {
    r3: { kind: "blocked", reason: "in conflict state" },
    r2: { kind: "needs_attention", reason: "no upstream; will set origin/main" },
  };
  void groupOps.run(id(), ALL, FETCH);
  await settle();
  expect(within(dialog() as HTMLElement).getByRole("status").textContent).toContain("Checking the repos");
  await settleAll();
  expect(repoRows()).toEqual([
    ["web", "Needs attention"],
    ["cli", "Blocked"],
    ["api", "Will run"],
    ["docs", "Will run"],
  ]);
  expect(screen.getByText("2 will run, 1 needs attention, 1 blocked")).toBeTruthy();
  expect(screen.getByText("in conflict state")).toBeTruthy();
  expect(screen.getByText(/change the selection in the tree/)).toBeTruthy();
  const run = screen.getByRole("button", { name: "Run on 3 repos" });
  expect(document.activeElement).toBe(run);
  fireEvent.click(run);
  await settleAll();
  expect(dialog()).toBeNull();
  expect(fakeGroups.starts.at(-1)).toEqual({ preflightId: 2, confirmed: true });

  // Escape cancels: the core drops the preflight and nothing starts.
  void groupOps.run(id(), ALL, FETCH);
  await settleAll();
  fireEvent.keyDown(dialog() as HTMLElement, { key: "Escape" });
  await settleAll();
  expect(dialog()).toBeNull();
  expect(fakeGroups.drops).toEqual([3]);
  expect(fakeGroups.starts).toHaveLength(2);

  // The setting shows it for every operation.
  fakeGroups.classes = {};
  fakeState.settings = {
    ...fakeState.settings,
    settings: { ...fakeState.settings.settings, always_show_preflight: true },
  };
  void groupOps.run(id(), ALL, FETCH);
  await settleAll();
  expect(screen.getByRole("button", { name: "Run on 4 repos" })).toBeTruthy();
  fireEvent.click(screen.getByRole("button", { name: "Cancel" }));
  await settleAll();
  expect(fakeGroups.drops).toEqual([3, 4]);
});

test("grp_9_destructive_group_operation_names_every_affected_repo", async () => {
  const groupOps = await dialogSetup();
  fakeGroups.classes = { r4: { kind: "will_skip", reason: "old does not exist" } };
  void groupOps.run(id(), ALL, { kind: "delete_branch", name: "old", force: true, on_remote: false });
  await settleAll();

  const confirm = screen.getByRole("alertdialog");
  expect(within(confirm).getByText("Cannot be undone in 3 repos:")).toBeTruthy();
  expect(within(confirm).getByText(/Force-delete branch old even if it is not merged/)).toBeTruthy();
  const affected = confirm.querySelector(".preflight-affected") as HTMLElement;
  expect(within(affected).getAllByRole("listitem").map((item) => item.textContent)).toEqual(["api", "web", "cli"]);
  // The run button names the action; Cancel has the focus.
  const run = within(confirm).getByRole("button", { name: "Delete branch old in 3 repos" });
  expect(document.activeElement).toBe(within(confirm).getByRole("button", { name: "Cancel" }));
  fireEvent.click(run);
  await settleAll();
  expect(fakeGroups.starts).toEqual([{ preflightId: 1, confirmed: true }]);

  // Even when every repo will run, a destructive operation asks.
  fakeGroups.classes = {};
  void groupOps.run(id(), ALL, { kind: "push", set_upstream: true, force_with_lease: true });
  await settleAll();
  expect(screen.getByRole("button", { name: "Force push in 4 repos" })).toBeTruthy();
});

/** The lines of the open report, as (repo, outcome). */
const reportLines = () =>
  within(screen.getByRole("list", { name: "Result report" }))
    .getAllByRole("listitem")
    .map((line) => [line.querySelector(".report-name")?.textContent, line.querySelector(".outcome")?.textContent]);

test("the report lists every repo with one outcome, opens on failure and survives closing the job list", async () => {
  await appSetup();
  // A group job shows its repos done while it runs, and its sub-jobs.
  fakeGroups.hold = true;
  fakeGroups.outcomes = { r2: { kind: "failed" }, r4: { kind: "conflict" } };
  fakeGroups.outputs = { r2: "fatal: could not read from remote repository\n" };
  fakeGroups.classes = { r3: { kind: "blocked", reason: "in conflict state" } };
  await runOnAll();
  expect(statusBar().textContent).toBe("Fetch · All (4 repos) 0/3 repos");
  expect(dialog()).toBeNull();
  fireEvent.click(screen.getByRole("button", { description: "Show the job list" }));
  const subJobs = within(screen.getByRole("list", { name: "Repos" })).getAllByRole("listitem");
  expect(subJobs.map((sub) => sub.textContent)).toEqual(["api", "webWaiting", "docsWaiting"]);
  fireEvent.keyDown(dialog() as HTMLElement, { key: "Escape" });

  // It ends with a failure while its tab is active: the report opens by itself.
  finishGroup();
  await settleAll();
  expect(within(dialog() as HTMLElement).getByRole("heading").textContent).toBe("Jobs");
  expect(screen.getByText("1 failed, 1 conflict, 1 succeeded, 1 skipped")).toBeTruthy();
  expect(reportLines()).toEqual([
    ["api", "Succeeded"],
    ["web", "Failed"],
    ["cli", "Skipped (in conflict state)"],
    ["docs", "Conflict"],
  ]);

  // A line expands to git's output; the arrow keys move between lines.
  const toggle = screen.getByRole("button", { name: "Git's output in web" });
  fireEvent.click(toggle);
  expect(toggle.getAttribute("aria-expanded")).toBe("true");
  expect(screen.getByText(/could not read from remote repository/)).toBeTruthy();
  toggle.focus();
  fireEvent.keyDown(toggle, { key: "ArrowDown" });
  expect(document.activeElement).toBe(screen.getByRole("button", { name: "Git's output in cli" }));

  // Closed and opened again, the report is still there.
  fireEvent.keyDown(dialog() as HTMLElement, { key: "Escape" });
  expect(dialog()).toBeNull();
  fireEvent.click(screen.getByRole("button", { description: "Show the job list" }));
  expect(reportLines()).toHaveLength(4);

  // The repo's name selects it in the tree and closes the list.
  fireEvent.click(within(screen.getByRole("list", { name: "Result report" })).getByRole("button", { name: "docs" }));
  await settleAll();
  expect(dialog()).toBeNull();
  const selected = screen.getAllByRole("treeitem").filter((item) => item.getAttribute("aria-selected") === "true");
  expect(selected.map((item) => item.querySelector(".tree-label")?.textContent)).toEqual(["docs"]);
});

test("a report without a failure or conflict does not open by itself", async () => {
  await appSetup();
  await runOnAll();
  expect(dialog()).toBeNull();
  expect(statusBar().textContent).toContain("Succeeded");
});

test("grp_11_retry_failed_targets_only_the_failed_repos", async () => {
  await appSetup();
  fakeGroups.outcomes = { r2: { kind: "failed" }, r3: { kind: "conflict" }, r4: { kind: "failed" } };
  const job = await runOnAll();
  const retry = screen.getByRole("button", { name: "Retry failed" }) as HTMLButtonElement;
  expect(retry.disabled).toBe(false);

  fakeGroups.outcomes = {};
  fireEvent.click(retry);
  await settleAll();
  // The UI sends only the job; the core builds the target set from the report (GRP-5).
  expect(fakeGroups.retries).toEqual([job]);
  expect(fakeGroups.preflights.at(-1)?.target).toEqual({ kind: "repos", repo_ids: ["r2", "r4"] as RepoId[] });
  expect(fakeGroups.preflights.at(-1)?.op).toEqual(FETCH);
  // Every repo will run, so it starts without the dialog.
  expect(dialog()).toBeNull();
  expect(fakeState.jobs[0]?.label).toBe("Fetch · 2 repos");
  expect(fakeState.jobs[0]?.report?.map((line) => line.repo_name)).toEqual(["web", "docs"]);

  // Without a failed repo, Retry failed is disabled.
  fireEvent.click(screen.getByRole("button", { description: "Show the job list" }));
  expect((screen.getByRole("button", { name: "Retry failed" }) as HTMLButtonElement).disabled).toBe(true);
});

test("the group settings round-trip through the core, outside 1 to 32 refused", async () => {
  await appSetup();
  fireEvent.click(screen.getByRole("button", { name: "Settings" }));
  const settings = await screen.findByRole("group", { name: "Group operations" });
  const parallelism = within(settings).getByRole("spinbutton", { name: "Repos at once" }) as HTMLInputElement;
  expect(parallelism.value).toBe("8");

  fireEvent.change(parallelism, { target: { value: "12" } });
  await settleAll();
  expect(fakeState.settings.settings.group_parallelism).toBe(12);
  expect(parallelism.value).toBe("12");

  fireEvent.change(parallelism, { target: { value: "40" } });
  await settleAll();
  expect(screen.getByRole("alert").textContent).toBe("group operations run 1 to 32 repos at once");
  expect(fakeState.settings.settings.group_parallelism).toBe(12);
  expect(parallelism.value).toBe("12");

  const always = within(settings).getByRole("checkbox", { name: "Always show the preflight dialog" });
  fireEvent.click(always);
  await settleAll();
  expect(fakeState.settings.settings.always_show_preflight).toBe(true);
});

/** The tree row labelled `label` at `level`. */
function row(label: string, level: number): HTMLElement {
  const found = screen
    .getAllByRole("treeitem")
    .find(
      (item) =>
        item.getAttribute("aria-level") === String(level) && item.querySelector(".tree-label")?.textContent === label,
    );
  if (found === undefined) {
    throw new Error(`no row ${label} at level ${level}`);
  }
  return found;
}

test("grp_12_the_toolbar_runs_fetch_pull_and_push_on_a_group_through_the_preflight", async () => {
  await appSetup();
  fireEvent.click(row("All", 1));
  await settleAll();
  const toolbar = within(screen.getByRole("toolbar", { name: "Main" }));
  const button = (name: string) => toolbar.getByRole<HTMLButtonElement>("button", { name });
  for (const name of ["Fetch", "Fetch options", "Pull", "Pull options", "Push", "Push options"]) {
    expect(button(name).disabled).toBe(false);
  }

  // A click runs with the defaults of section 5; every repo will run, so it starts at once.
  fireEvent.click(button("Fetch"));
  await settleAll();
  fireEvent.click(button("Pull"));
  await settleAll();
  fireEvent.click(button("Push"));
  await settleAll();
  expect(fakeGroups.preflights.map((preflight) => [preflight.target, preflight.op])).toEqual([
    [ALL, { kind: "fetch", prune: "config" }],
    [ALL, { kind: "pull", strategy: "config" }],
    [ALL, { kind: "push", set_upstream: true, force_with_lease: false }],
  ]);
  expect(fakeGroups.starts.map((start) => start.confirmed)).toEqual([false, false, false]);

  // The arrow opens the options; the run still goes through the preflight.
  fireEvent.click(button("Pull options"));
  const options = screen.getByRole("dialog", { name: "Pull All" });
  fireEvent.click(within(options).getByRole("radio", { name: "Rebase" }));
  fireEvent.click(within(options).getByRole("button", { name: "Pull" }));
  await settleAll();
  expect(screen.queryByRole("dialog", { name: "Pull All" })).toBeNull();
  expect(fakeGroups.preflights.at(-1)?.op).toEqual({ kind: "pull", strategy: "rebase" });

  // Force with lease is destructive: the preflight dialog names every repo (GRP-9).
  fireEvent.click(button("Push options"));
  const push = screen.getByRole("dialog", { name: "Push All" });
  fireEvent.click(within(push).getByRole("checkbox", { name: /Force with lease/ }));
  fireEvent.click(within(push).getByRole("button", { name: "Push" }));
  await settleAll();
  expect(fakeGroups.preflights.at(-1)?.op).toEqual({ kind: "push", set_upstream: true, force_with_lease: true });
  const confirm = screen.getByRole("alertdialog");
  expect(within(confirm).getByRole("button", { name: "Force push in 4 repos" })).toBeTruthy();
  fireEvent.click(within(confirm).getByRole("button", { name: "Cancel" }));
  await settleAll();

  // The group row's menu offers the same three.
  fireEvent.contextMenu(row("All", 1), { clientX: 10, clientY: 10 });
  fireEvent.click(within(screen.getByRole("menu")).getByRole("menuitem", { name: "Push" }));
  await settleAll();
  expect(fakeGroups.preflights.at(-1)).toEqual({
    workspaceId: id(),
    target: ALL,
    op: { kind: "push", set_upstream: true, force_with_lease: false },
  });
});

test("ui_5_double_click_on_a_group_branch_starts_a_group_checkout", async () => {
  workspace();
  fakeState.refs = {
    ...fakeState.refs,
    r1: refNodes("r1", { local: ["main", "topic"], current: "main" }),
    r2: refNodes("r2", { local: ["main", "topic"], current: "topic" }),
  };
  render(() => <App />);
  await screen.findAllByRole("treeitem");
  await settleAll();
  fireEvent.click(within(row("Branches", 2)).getByText("▸"));
  await settleAll();

  // A double-click checks the name out on the group's members, through the preflight.
  fireEvent.dblClick(row("topic", 3));
  await settleAll();
  expect(fakeGroups.preflights).toEqual([
    { workspaceId: id(), target: ALL, op: { kind: "checkout", name: "topic", missing: "skip" } },
  ]);
  expect(fakeState.branchRequests).toEqual([]);

  // The name's menu offers Check out and Delete; Delete opens the dialog on that name.
  fireEvent.contextMenu(row("topic", 3), { clientX: 10, clientY: 10 });
  const menu = within(screen.getByRole("menu"));
  expect(menu.getAllByRole("menuitem").map((item) => item.textContent)).toEqual(["Check out", "Delete…"]);
  fireEvent.click(menu.getByRole("menuitem", { name: "Delete…" }));
  await settleAll();
  const remove = screen.getByRole("dialog", { name: "Delete a branch in All" });
  expect(within(remove).getByRole<HTMLInputElement>("combobox", { name: "Branch" }).value).toBe("topic");
  fireEvent.click(within(remove).getByRole("checkbox", { name: /Force/ }));
  fireEvent.click(within(remove).getByRole("button", { name: "Delete" }));
  await settleAll();
  expect(fakeGroups.preflights.at(-1)?.op).toEqual({
    kind: "delete_branch",
    name: "topic",
    force: true,
    on_remote: false,
  });
  // Destructive: the preflight dialog asks (GRP-9).
  fireEvent.click(within(screen.getByRole("alertdialog")).getByRole("button", { name: "Cancel" }));
  await settleAll();

  // The group row's Check out branch… picks from the group's branch names.
  fireEvent.contextMenu(row("All", 1), { clientX: 10, clientY: 10 });
  fireEvent.click(within(screen.getByRole("menu")).getByRole("menuitem", { name: "Check out branch…" }));
  await settleAll();
  const checkout = screen.getByRole("dialog", { name: "Check out a branch in All" });
  const options = [...checkout.querySelectorAll("#group-branch-names option")];
  const names = options.map((option) => option.getAttribute("value"));
  expect(names).toEqual(["main", "topic"]);
  fireEvent.input(within(checkout).getByRole("combobox", { name: "Branch" }), { target: { value: "topic" } });
  fireEvent.click(within(checkout).getByRole("radio", { name: "Switch to its default branch" }));
  await settleAll();
  fireEvent.click(within(checkout).getByRole("button", { name: "Check out" }));
  await settleAll();
  expect(fakeGroups.preflights.at(-1)?.op).toEqual({ kind: "checkout", name: "topic", missing: "default_branch" });

  // The toolbar's Branch opens the group create dialog; the core checks the name (SEC-4).
  fireEvent.click(row("All", 1));
  await settleAll();
  const branch = within(screen.getByRole("toolbar", { name: "Main" })).getByRole<HTMLButtonElement>("button", {
    name: "Branch",
  });
  expect(branch.disabled).toBe(false);
  fireEvent.click(branch);
  const create = screen.getByRole("dialog", { name: "New branch in All" });
  const field = within(create).getByRole("textbox", { name: "Branch" });
  fireEvent.input(field, { target: { value: "bad..name" } });
  await settleAll();
  expect(within(create).getByRole<HTMLButtonElement>("button", { name: "Create" }).disabled).toBe(true);
  fireEvent.input(field, { target: { value: "feature/y" } });
  await settleAll();
  fireEvent.click(within(create).getByRole("button", { name: "Create" }));
  await settleAll();
  expect(fakeGroups.preflights.at(-1)?.op).toEqual({
    kind: "create_branch",
    name: "feature/y",
    start: { kind: "head" },
    switch: true,
  });
  expect(screen.queryByRole("dialog", { name: "New branch in All" })).toBeNull();
});

test("grp_13_the_toolbar_merge_and_the_group_menu_merge_rebase_and_create_tag", async () => {
  workspace();
  fakeState.refs = {
    ...fakeState.refs,
    r1: refNodes("r1", { local: ["main", "topic"], current: "main" }),
  };
  fakeGroups.classes = { r2: { kind: "needs_attention", reason: "conflicts predicted in 1 file: a.txt" } };
  render(() => <App />);
  await screen.findAllByRole("treeitem");
  await settleAll();
  fireEvent.click(row("All", 1));
  await settleAll();

  // The toolbar's Merge picks from the group's branch names and takes any typed ref.
  const merge = within(screen.getByRole("toolbar", { name: "Main" })).getByRole<HTMLButtonElement>("button", {
    name: "Merge",
  });
  expect(merge.disabled).toBe(false);
  fireEvent.click(merge);
  await settleAll();
  const options = screen.getByRole("dialog", { name: "Merge into the current branch in All" });
  const names = [...options.querySelectorAll("#group-branch-names option")].map((option) =>
    option.getAttribute("value"),
  );
  expect(names).toEqual(["main", "topic"]);
  const field = within(options).getByRole("combobox", { name: "Branch, tag or commit" });
  fireEvent.input(field, { target: { value: "origin/topic" } });
  fireEvent.click(within(options).getByRole("radio", { name: "Fast-forward only" }));
  await settleAll();
  fireEvent.click(within(options).getByRole("button", { name: "Merge" }));
  await settleAll();
  expect(fakeGroups.preflights.at(-1)).toEqual({
    workspaceId: id(),
    target: ALL,
    op: { kind: "merge", name: "origin/topic", fast_forward: "ff_only" },
  });

  // A predicted conflict asks first, with the files in the repo's row (GRP-13).
  const confirm = dialog() as HTMLElement;
  expect(within(confirm).getByText("conflicts predicted in 1 file: a.txt")).toBeTruthy();
  fireEvent.click(within(confirm).getByRole("button", { name: "Cancel" }));
  await settleAll();

  // The same dialog rebases instead.
  fireEvent.click(merge);
  const again = screen.getByRole("dialog", { name: "Merge into the current branch in All" });
  fireEvent.click(within(again).getByRole("radio", { name: "Rebase the current branch onto it" }));
  fireEvent.input(within(again).getByRole("combobox", { name: "Branch, tag or commit" }), {
    target: { value: "topic" },
  });
  await settleAll();
  fireEvent.click(within(again).getByRole("button", { name: "Rebase" }));
  await settleAll();
  expect(fakeGroups.preflights.at(-1)?.op).toEqual({ kind: "rebase", onto: "topic" });
  fireEvent.click(within(dialog() as HTMLElement).getByRole("button", { name: "Cancel" }));
  await settleAll();

  // The group row's Rebase… opens it on rebase; an option-like ref is never sent.
  fireEvent.contextMenu(row("All", 1), { clientX: 10, clientY: 10 });
  fireEvent.click(within(screen.getByRole("menu")).getByRole("menuitem", { name: "Rebase…" }));
  const rebase = screen.getByRole("dialog", { name: "Rebase the current branch in All" });
  fireEvent.input(within(rebase).getByRole("combobox", { name: "Branch, tag or commit" }), {
    target: { value: "--root" },
  });
  await settleAll();
  expect(within(rebase).getByRole<HTMLButtonElement>("button", { name: "Rebase" }).disabled).toBe(true);
  fireEvent.click(within(rebase).getByRole("button", { name: "Cancel" }));

  // Create tag…: annotated needs a message; pushing is off unless asked.
  fakeGroups.classes = {};
  fireEvent.contextMenu(row("All", 1), { clientX: 10, clientY: 10 });
  fireEvent.click(within(screen.getByRole("menu")).getByRole("menuitem", { name: "Create tag…" }));
  const tag = screen.getByRole("dialog", { name: "New tag in All" });
  fireEvent.input(within(tag).getByRole("textbox", { name: "Tag" }), { target: { value: "v1.0" } });
  await settleAll();
  const create = within(tag).getByRole<HTMLButtonElement>("button", { name: "Create" });
  expect(create.disabled).toBe(true);
  fireEvent.input(within(tag).getByRole("textbox", { name: "Message" }), { target: { value: "Release 1.0" } });
  fireEvent.click(within(tag).getByRole("checkbox", { name: /Push the tag/ }));
  await settleAll();
  fireEvent.click(create);
  await settleAll();
  expect(fakeGroups.preflights.at(-1)?.op).toEqual({
    kind: "create_tag",
    name: "v1.0",
    annotated: true,
    message: "Release 1.0",
    push: true,
  });
  expect(screen.queryByRole("dialog", { name: "New tag in All" })).toBeNull();
});
