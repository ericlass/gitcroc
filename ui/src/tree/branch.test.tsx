import { cleanup, fireEvent, render, screen, within } from "@solidjs/testing-library";
import { afterEach, beforeEach, expect, test, vi } from "vitest";
import { fakeState, reset } from "../ipc/fake";
import { type FakeRefs, refNodes, summary } from "../ipc/fakeTree";
import { createBranchStore } from "../stores/branch";
import { createCloneStore } from "../stores/clone";
import { createShareStore } from "../stores/share";
import { createGroupOpStore } from "../stores/groupOps";
import { createRemoteStore } from "../stores/remote";
import { createStashStore } from "../stores/stash";
import { createSummaryStore } from "../stores/summaries";
import { createTagStore } from "../stores/tag";
import { createTreeStore, loadKey } from "../stores/tree";
import { createWorkspaceStore } from "../stores/workspaces";
import BranchDialogs from "./BranchDialogs";
import TreePane from "./TreePane";
import { createCommandStore } from "../palette/commands";

// jsdom lays nothing out; give the tree a height so rows are rendered.
beforeEach(() => {
  const real = HTMLElement.prototype.getBoundingClientRect;
  vi.spyOn(HTMLElement.prototype, "getBoundingClientRect").mockImplementation(function (this: HTMLElement) {
    return this.classList.contains("tree-scroller") ? new DOMRect(0, 0, 300, 800) : real.call(this);
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

const REFS: FakeRefs = {
  local: ["main", "topic", "old"],
  current: "main",
  counts: { main: [0, 0] },
  upstreams: { topic: "origin/topic" },
  remotes: { origin: ["main", "topic"] },
  tags: ["v1.0"],
  stashes: ["On main: work"],
};

/** The tree of one repo, `api`, with every ref node expanded. */
async function setup(change: { busy?: boolean } = {}) {
  fakeState.workspace = { ...fakeState.workspace, repos: [{ id: "r1", name: "api", path: "/src/api", remote: null }] };
  fakeState.refModels = { r1: REFS };
  fakeState.refs = { r1: refNodes("r1", REFS) };
  fakeState.summaries = [summary(id(), "r1", "main", { busy: change.busy ?? false })];
  const tree = createTreeStore();
  const summaries = createSummaryStore();
  const branch = createBranchStore();
  render(() => (
    <>
      <TreePane
        workspace={id()}
        commands={createCommandStore()}
        tree={tree}
        summaries={summaries}
        workspaces={createWorkspaceStore()}
        branch={branch}
        stash={createStashStore()}
        tag={createTagStore()}
        remote={createRemoteStore()}
        clone={createCloneStore(() => {})}
        share={createShareStore()}
        groupOps={createGroupOpStore()}
      />
      <BranchDialogs workspace={id()} branch={branch} repoName={() => "api"} />
    </>
  ));
  await Promise.all([tree.subscribe(id()), summaries.subscribe(id())]);
  const repo = tree.tree(id()).roots?.[0]?.children?.[0];
  if (repo === undefined) {
    throw new Error("no repo node");
  }
  tree.setExpanded(id(), repo, true);
  for (const child of repo.children ?? []) {
    tree.setExpanded(id(), child, true);
  }
  await settleAll();
  const remotes = repo.children?.[1];
  const loaded = remotes === undefined ? null : loadKey(remotes.item);
  for (const remote of (loaded === null ? undefined : tree.tree(id()).refs[loaded]?.[1]?.children) ?? []) {
    tree.setExpanded(id(), remote, true);
  }
  await settleAll();
  return { tree, branch };
}

/** The row labelled `label` at tree level `level`: 4 for local branches and tags, 5 for remote branches. */
function row(label: string, level: number): HTMLElement {
  const found = screen
    .getAllByRole("treeitem")
    .find((item) => item.getAttribute("aria-level") === String(level) && item.querySelector(".tree-label")?.textContent === label);
  if (found === undefined) {
    throw new Error(`no row ${label} at level ${level}`);
  }
  return found;
}

/** Opens the context menu of `target`: its entries, `!` marking a disabled one. */
function menuOf(target: HTMLElement): string[] {
  fireEvent.contextMenu(target, { clientX: 10, clientY: 10 });
  const items = within(screen.getByRole("menu")).getAllByRole<HTMLButtonElement>("menuitem");
  const entries = items.map((item) => `${item.disabled ? "!" : ""}${item.textContent}`);
  fireEvent.keyDown(document, { key: "Escape" });
  return entries;
}

const pick = (target: HTMLElement, entry: string) => {
  fireEvent.contextMenu(target, { clientX: 10, clientY: 10 });
  fireEvent.click(within(screen.getByRole("menu")).getByRole("menuitem", { name: entry }));
};

test("UI-7 double-click checks out a local branch", async () => {
  await setup();
  fireEvent.dblClick(row("topic", 4));
  await settleAll();
  expect(fakeState.branchRequests).toEqual([{ repo_id: "r1", op: { kind: "checkout", name: "topic" }, confirmed: false }]);
  // The job's end re-read the refs: topic is current now.
  expect(within(row("topic", 4)).getByRole("img", { name: "Current branch" })).toBeDefined();
  expect(within(row("main", 4)).queryByRole("img", { name: "Current branch" })).toBeNull();

  // A remote branch row opens and closes; it checks nothing out.
  fireEvent.dblClick(row("topic", 5));
  await settleAll();
  expect(fakeState.branchRequests).toHaveLength(1);
});

test("UI-7 the context menu of each row kind lists its operations", async () => {
  await setup();
  expect(menuOf(row("api", 2))).toEqual([
    "New branch…",
    "New tag…",
    "Add to group ▸",
    "Open in file manager",
    "Open in terminal",
    "Remove from workspace",
  ]);
  expect(menuOf(row("main", 4))).toEqual([
    "!Check out",
    "New branch from here…",
    "Rename…",
    "!Delete…",
    "Set upstream…",
    "Unset upstream",
  ]);
  expect(menuOf(row("old", 4))).toEqual([
    "Check out",
    "New branch from here…",
    "Rename…",
    "Delete…",
    "Set upstream…",
    "!Unset upstream",
  ]);
  expect(menuOf(row("topic", 5))).toEqual(["Check out", "New branch from here…", "Delete on the remote…"]);
  expect(menuOf(row("v1.0", 4))).toEqual(["Check out (detached)", "New branch from here…", "Push", "Delete…"]);

  expect(menuOf(row("On main: work", 4))).toEqual(["Apply", "Pop", "Drop…"]);

  // Rows that only group others have no menu.
  fireEvent.contextMenu(row("Local", 3));
  expect(screen.queryByRole("menu")).toBeNull();
});

test("GRP-8 a busy repo's branch operations are disabled", async () => {
  await setup({ busy: true });
  // Opening it is no write.
  expect(menuOf(row("api", 2))).toEqual([
    "!New branch…",
    "!New tag…",
    // Groups are not repo writes (GRP-3).
    "Add to group ▸",
    "Open in file manager",
    "Open in terminal",
    "Remove from workspace",
  ]);
  expect(menuOf(row("old", 4)).every((entry) => entry.startsWith("!"))).toBe(true);
  fireEvent.dblClick(row("topic", 4));
  await settleAll();
  expect(fakeState.branchRequests).toEqual([]);
});

test("UI-7 a remote branch is checked out from its menu", async () => {
  await setup();
  pick(row("main", 5), "Check out");
  await settleAll();
  expect(fakeState.branchRequests.map((request) => request.op)).toEqual([
    { kind: "checkout_remote", branch: { remote: "origin", branch: "main" } },
  ]);
});

test("SEC-4 the create dialog shows the core's reason under the name", async () => {
  await setup();
  pick(row("v1.0", 4), "New branch from here…");
  const dialog = await screen.findByRole("dialog", { name: "New branch in api" });
  const name = within(dialog).getByRole<HTMLInputElement>("textbox", { name: "Name" });
  const create = within(dialog).getByRole<HTMLButtonElement>("button", { name: "Create" });
  // The start point is the row's tag.
  expect(within(dialog).getByRole<HTMLInputElement>("combobox", { name: "Branch, tag or commit" }).value).toBe("v1.0");

  fireEvent.input(name, { target: { value: "my branch" } });
  await settleAll();
  expect(within(dialog).getByText("the name cannot contain spaces")).toBeDefined();
  expect(name.getAttribute("aria-invalid")).toBe("true");
  expect(create.disabled).toBe(true);

  fireEvent.input(name, { target: { value: "release/1.0" } });
  await settleAll();
  expect(within(dialog).queryByText("the name cannot contain spaces")).toBeNull();
  fireEvent.click(within(dialog).getByRole("checkbox", { name: "Check out the new branch" }));
  fireEvent.click(create);
  await settleAll();
  expect(fakeState.branchRequests.map((request) => request.op)).toEqual([
    { kind: "create", name: "release/1.0", start: { kind: "revision", revision: "v1.0" }, switch: false },
  ]);
  expect(screen.queryByRole("dialog")).toBeNull();
});

test("UI-7 an existing name is refused by the core and the dialog stays", async () => {
  await setup();
  pick(row("api", 2), "New branch…");
  const dialog = await screen.findByRole("dialog", { name: "New branch in api" });
  // The default branch is offered by name.
  expect(await within(dialog).findByRole("radio", { name: "Default branch (origin/main)" })).toBeDefined();
  fireEvent.input(within(dialog).getByRole("textbox", { name: "Name" }), { target: { value: "topic" } });
  await settleAll();
  fireEvent.click(within(dialog).getByRole("button", { name: "Create" }));
  await settleAll();
  expect(within(dialog).getByRole("alert").textContent).toBe("a branch named topic exists already");
  expect(fakeState.branchRequests.map((request) => request.op)).toEqual([
    { kind: "create", name: "topic", start: { kind: "head" }, switch: true },
  ]);
});

test("GRP-9 force delete asks for confirmation before it runs", async () => {
  await setup();
  pick(row("topic", 4), "Delete…");
  const dialog = await screen.findByRole("dialog", { name: "Delete branch topic in api?" });
  fireEvent.click(within(dialog).getByRole("checkbox", { name: "Delete it even if it is not merged" }));
  fireEvent.click(within(dialog).getByRole("checkbox", { name: "Also delete the branch it tracks on the remote" }));
  fireEvent.click(within(dialog).getByRole("button", { name: "Delete" }));
  await settleAll();

  const confirm = screen.getByRole("alertdialog", { name: "Cannot be undone in api" });
  expect(within(confirm).getAllByRole("listitem").map((item) => item.textContent)).toEqual([
    "Delete branch topic even if it is not merged; its commits may be lost",
    "Delete topic on the remote origin",
  ]);
  // Nothing ran yet; Cancel has the focus.
  expect(fakeState.jobs).toEqual([]);
  expect(document.activeElement?.textContent).toBe("Cancel");

  fireEvent.click(within(confirm).getByRole("button", { name: "Delete" }));
  await settleAll();
  const op = { kind: "delete", name: "topic", force: true, on_remote: true };
  expect(fakeState.branchRequests).toEqual([
    { repo_id: "r1", op, confirmed: false },
    { repo_id: "r1", op, confirmed: true },
  ]);
  expect(screen.queryByRole("alertdialog")).toBeNull();
  expect(screen.queryByText("topic")).toBeNull();
});

test("GRP-9 a plain delete runs at once; Escape cancels a confirmation", async () => {
  await setup();
  pick(row("old", 4), "Delete…");
  fireEvent.click(within(await screen.findByRole("dialog")).getByRole("button", { name: "Delete" }));
  await settleAll();
  expect(fakeState.branchRequests.map((request) => request.confirmed)).toEqual([false]);
  expect(fakeState.jobs.map((job) => job.label)).toEqual(["Delete branch old"]);

  pick(row("topic", 5), "Delete on the remote…");
  await settleAll();
  fireEvent.keyDown(screen.getByRole("alertdialog"), { key: "Escape" });
  expect(screen.queryByRole("alertdialog")).toBeNull();
  expect(fakeState.jobs).toHaveLength(1);
});

test("UI-7 set and unset the upstream", async () => {
  await setup();
  pick(row("old", 4), "Set upstream…");
  const dialog = await screen.findByRole("dialog", { name: "Upstream of old in api" });
  await settleAll();
  fireEvent.change(within(dialog).getByRole("combobox", { name: "Remote branch" }), { target: { value: "origin/topic" } });
  fireEvent.click(within(dialog).getByRole("button", { name: "Set upstream" }));
  await settleAll();
  pick(row("topic", 4), "Unset upstream");
  await settleAll();
  expect(fakeState.branchRequests.map((request) => request.op)).toEqual([
    { kind: "set_upstream", name: "old", upstream: { remote: "origin", branch: "topic" } },
    { kind: "set_upstream", name: "topic", upstream: null },
  ]);
});

test("UI-7 a refused checkout shows the reason in the tree", async () => {
  await setup();
  fakeState.summaries = [summary(id(), "r1", "main", { conflict: true })];
  fireEvent.dblClick(row("topic", 4));
  await settleAll();
  expect(screen.getByRole("alert").textContent).toContain("the repo is in conflict state; continue or abort first");
  fireEvent.click(screen.getByRole("button", { name: "Dismiss" }));
  expect(screen.queryByRole("alert")).toBeNull();
});
