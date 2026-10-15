import { cleanup, fireEvent, render, screen, within } from "@solidjs/testing-library";
import { afterAll, afterEach, beforeAll, beforeEach, expect, test, vi } from "vitest";
import type { SummaryView } from "../ipc";
import { emitRepoSummary, fakeState, reset } from "../ipc/fake";
import { fakeGroups } from "../ipc/fakeGroups";
import { refNodes, summary } from "../ipc/fakeTree";
import { createBranchStore } from "../stores/branch";
import { createCloneStore } from "../stores/clone";
import { createGroupOpStore } from "../stores/groupOps";
import { sourceOf } from "../stores/history";
import { createRemoteStore } from "../stores/remote";
import { createStashStore } from "../stores/stash";
import { createSummaryStore } from "../stores/summaries";
import { createTagStore } from "../stores/tag";
import { createTreeStore } from "../stores/tree";
import { createWorkspaceStore } from "../stores/workspaces";
import TreePane from "./TreePane";

/** Row height in the mocked layout; a drop in its upper half lands above the row. */
const ROW = 22;

// jsdom has no DragEvent; a MouseEvent carries the pointer and the modifier keys.
const realDragEvent = (window as { DragEvent?: unknown }).DragEvent;
beforeAll(() => {
  Object.defineProperty(window, "DragEvent", { value: class extends MouseEvent {}, configurable: true });
});
afterAll(() => {
  Object.defineProperty(window, "DragEvent", { value: realDragEvent, configurable: true });
});

// jsdom lays nothing out; give the tree a height so rows are rendered, and each row one row's height.
beforeEach(() => {
  const real = HTMLElement.prototype.getBoundingClientRect;
  vi.spyOn(HTMLElement.prototype, "getBoundingClientRect").mockImplementation(function (this: HTMLElement) {
    if (this.classList.contains("tree-scroller")) {
      return new DOMRect(0, 0, 300, 800);
    }
    return this.classList.contains("tree-row") ? new DOMRect(0, 0, 300, ROW) : real.call(this);
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

/** `view` with its summary changed by `change`, and itself by `extra`. */
const changed = (
  view: SummaryView,
  change: Partial<NonNullable<SummaryView["summary"]>>,
  extra: Partial<SummaryView> = {},
): SummaryView => (view.summary === null ? view : { ...view, ...extra, summary: { ...view.summary, ...change } });

const repo = (repoId: string, name: string) => ({ id: repoId, name, path: `/src/${name}`, remote: null });

/**
 * Backend holds api and web, Frontend web and cli; docs is in no group.
 * api and web are on main, web is dirty, cli is on develop in conflict state.
 */
async function setup() {
  fakeState.workspace = {
    ...fakeState.workspace,
    repos: [repo("r1", "api"), repo("r2", "web"), repo("r3", "cli"), repo("r4", "docs")],
    groups: [
      { id: "g1", name: "Backend", repos: ["r1", "r2"] },
      { id: "g2", name: "Frontend", repos: ["r2", "r3"] },
    ],
  };
  fakeState.refs = {
    r1: refNodes("r1", { local: ["main", "topic"], current: "main" }),
    r2: refNodes("r2", { local: ["main"], current: "main" }),
    r3: refNodes("r3", { local: ["develop"], current: "develop" }),
    r4: refNodes("r4", { local: ["main"], current: "main" }),
  };
  const state = { operation: "merge" as const, unmerged_paths: 1 };
  fakeState.summaries = [
    summary(id(), "r1", "main"),
    changed(summary(id(), "r2", "main"), { dirty: true }),
    changed(summary(id(), "r3", "develop"), { state }, { conflict: true }),
    summary(id(), "r4", "main"),
  ];
  const tree = createTreeStore();
  const summaries = createSummaryStore();
  render(() => (
    <TreePane
      workspace={id()}
      tree={tree}
      summaries={summaries}
      workspaces={createWorkspaceStore()}
      branch={createBranchStore()}
      stash={createStashStore()}
      tag={createTagStore()}
      remote={createRemoteStore()}
      clone={createCloneStore(() => {})}
      groupOps={createGroupOpStore()}
    />
  ));
  await Promise.all([tree.subscribe(id()), summaries.subscribe(id())]);
  await settleAll();
  return { tree };
}

/** The `nth` row labelled `label` at tree level `level`: 1 for groups and Ungrouped, 2 for repos. */
function row(label: string, level: number, nth = 0): HTMLElement {
  const found = screen
    .getAllByRole("treeitem")
    .filter((item) => item.getAttribute("aria-level") === String(level))
    .filter((item) => item.querySelector(".tree-label")?.textContent === label)[nth];
  if (found === undefined) {
    throw new Error(`no row ${label} at level ${level}`);
  }
  return found;
}

/** The labels of the visible rows at levels 1 and 2, indented. */
const outline = () =>
  screen
    .getAllByRole("treeitem")
    .filter((item) => Number(item.getAttribute("aria-level")) <= 2)
    .map((item) => `${item.getAttribute("aria-level") === "2" ? "  " : ""}${item.querySelector(".tree-label")?.textContent}`);

const groups = () => fakeState.workspace.groups.map((group) => `${group.name}: ${group.repos.join(" ")}`);

/** Opens the context menu of `target`: its entries, `!` marking a disabled one. */
function menuOf(target: HTMLElement): string[] {
  fireEvent.contextMenu(target, { clientX: 10, clientY: 10 });
  const items = within(screen.getAllByRole("menu")[0] as HTMLElement).getAllByRole<HTMLButtonElement>("menuitem");
  const entries = items.map((item) => `${item.disabled ? "!" : ""}${item.textContent}`);
  fireEvent.keyDown(document, { key: "Escape" });
  return entries;
}

const pick = async (target: HTMLElement, entry: string) => {
  fireEvent.contextMenu(target, { clientX: 10, clientY: 10 });
  fireEvent.click(within(screen.getByRole("menu")).getByRole("menuitem", { name: entry }));
  await settleAll();
};

/** Drags `from` onto `to`, `y` pixels down its row, with `keys` held at the drop. */
async function drag(from: HTMLElement, to: HTMLElement, y = ROW / 2, keys: { ctrlKey?: boolean; altKey?: boolean } = {}) {
  fireEvent.dragStart(from);
  fireEvent.dragOver(to, { clientY: y, ...keys });
  fireEvent.drop(to, { clientY: y, ...keys });
  fireEvent.dragEnd(from);
  await settleAll();
}

test("ui_4_group_row_shows_count_branch_and_totals", async () => {
  await setup();
  expect(outline()).toEqual(["Backend", "  Branches", "  api", "  web", "Frontend", "  Branches", "  web", "  cli", "Ungrouped", "  docs"]);

  // A common branch; one member dirty, none in conflict state.
  const backend = row("Backend", 1);
  expect(within(backend).getByLabelText("2 members").textContent).toBe("2");
  expect(backend.querySelector(".tree-head")?.textContent).toBe("main");
  expect(within(backend).getAllByRole("img").map((icon) => icon.getAttribute("aria-label"))).toEqual([
    "1 member with changes",
  ]);
  // Two branches; one member dirty and one in conflict state.
  const frontend = row("Frontend", 1);
  expect(frontend.querySelector(".tree-head")?.textContent).toBe("mixed (2)");
  expect(within(frontend).getAllByRole("img").map((icon) => icon.getAttribute("aria-label"))).toEqual([
    "1 member with changes",
    "1 member in conflict state",
  ]);
  expect(frontend.textContent).toBe("▾Frontend2mixed (2)●1⚔1");

  // cli is back on main and out of conflict: the row follows the core's event.
  const clean = summary(id(), "r3", "main");
  fakeState.summaries = fakeState.summaries.map((view) => (view.repo_id === "r3" ? clean : view));
  emitRepoSummary(clean);
  await settleAll();
  expect(row("Frontend", 1).textContent).toBe("▾Frontend2main●1");

  // The Branches node counts the members that have each name (UI-5).
  fireEvent.click(within(row("Branches", 2)).getByText("▸"));
  await settleAll();
  const counts = screen
    .getAllByRole("treeitem")
    .filter((item) => item.getAttribute("aria-level") === "3")
    .map((item) => item.textContent);
  expect(counts).toEqual(["main(2/2)", "topic(1/2)"]);
  // Double-click on a group branch starts a group checkout, never a single repo's.
  fireEvent.dblClick(screen.getByLabelText("in 1 of 2 members"));
  await settleAll();
  expect(fakeState.branchRequests).toEqual([]);
  expect(fakeGroups.preflights.map((preflight) => preflight.op)).toEqual([
    { kind: "checkout", name: "topic", missing: "skip" },
  ]);
});

test("ui_7_drag_moves_between_groups_and_ctrl_or_alt_adds", async () => {
  await setup();

  // Dragged onto another group, a repo moves from the group it came from.
  fireEvent.dragStart(row("api", 2));
  fireEvent.dragOver(row("Frontend", 1), { clientY: 5 });
  // The drop target is highlighted while dragging.
  expect(row("Frontend", 1).classList.contains("drop-into")).toBe(true);
  expect(row("api", 2).classList.contains("dragged")).toBe(true);
  fireEvent.drop(row("Frontend", 1), { clientY: 5 });
  fireEvent.dragEnd(row("api", 2));
  await settleAll();
  expect(groups()).toEqual(["Backend: r2", "Frontend: r2 r3 r1"]);
  expect(document.querySelector(".drop-into, .dragged")).toBeNull();

  // Ctrl adds instead, and so does Alt (macOS).
  await drag(row("cli", 2), row("Backend", 1), ROW / 2, { ctrlKey: true });
  expect(groups()).toEqual(["Backend: r2 r3", "Frontend: r2 r3 r1"]);
  await drag(row("api", 2), row("Backend", 1), ROW / 2, { altKey: true });
  expect(groups()).toEqual(["Backend: r2 r3 r1", "Frontend: r2 r3 r1"]);

  // A repo in no group dropped on Ungrouped changes nothing.
  await drag(row("docs", 2), row("Ungrouped", 1));
  expect(groups()).toEqual(["Backend: r2 r3 r1", "Frontend: r2 r3 r1"]);

  // From Ungrouped a repo is always added; dropped above a member, it goes there.
  await drag(row("docs", 2), row("cli", 2, 0), 5);
  expect(groups()).toEqual(["Backend: r2 r4 r3 r1", "Frontend: r2 r3 r1"]);

  // Dropped on Ungrouped, a repo leaves the group it came from only.
  await drag(row("web", 2, 1), row("Ungrouped", 1));
  expect(groups()).toEqual(["Backend: r2 r4 r3 r1", "Frontend: r3 r1"]);

  // Within a group, dropping between members reorders: api below web.
  await drag(row("api", 2, 0), row("web", 2), ROW - 2);
  expect(groups()).toEqual(["Backend: r2 r1 r4 r3", "Frontend: r3 r1"]);

  // A group dropped above another moves there.
  await drag(row("Frontend", 1), row("Backend", 1), 5);
  expect(groups()).toEqual(["Frontend: r3 r1", "Backend: r2 r1 r4 r3"]);
  // Nothing else is a drop target for a group.
  fireEvent.dragStart(row("Backend", 1));
  fireEvent.dragOver(row("cli", 2), { clientY: 5 });
  expect(document.querySelector(".drop-into, .drop-before, .drop-after")).toBeNull();
  fireEvent.dragEnd(row("Backend", 1));

  // The workspace's repos stay throughout (GRP-3).
  expect(fakeState.workspace.repos.map((known) => known.id)).toEqual(["r1", "r2", "r3", "r4"]);
});

test("ui_7_group_and_repo_context_menus_offer_the_group_operations", async () => {
  const { tree } = await setup();
  const group = [
    "Fetch",
    "Pull",
    "Push",
    "Check out branch…",
    "New branch…",
    "Delete branch…",
    "Merge…",
    "Rebase…",
    "Create tag…",
    "Rename…",
    "Delete…",
    "New group…",
  ];
  expect(menuOf(row("Backend", 1))).toEqual([...group, "!Move up", "Move down"]);
  expect(menuOf(row("Frontend", 1))).toEqual([...group, "Move up", "!Move down"]);
  expect(menuOf(row("Ungrouped", 1))).toEqual(["New group…"]);
  expect(menuOf(row("api", 2))).toEqual([
    "New branch…",
    "New tag…",
    "Add to group ▸",
    "Remove from group",
    "!Move up",
    "Move down",
    "Open in file manager",
    "Open in terminal",
    "Remove from workspace",
  ]);
  expect(menuOf(row("docs", 2))).toContain("Add to group ▸");
  expect(menuOf(row("docs", 2))).not.toContain("Remove from group");

  // Add to group lists the groups the repo is not in, then New group.
  const submenu = (target: HTMLElement) => {
    fireEvent.contextMenu(target, { clientX: 10, clientY: 10 });
    fireEvent.click(within(screen.getByRole("menu")).getByRole("menuitem", { name: "Add to group ▸" }));
    const entries = within(screen.getByRole("menu", { name: "Add to group" }))
      .getAllByRole("menuitem")
      .map((item) => item.textContent);
    return entries;
  };
  expect(submenu(row("api", 2))).toEqual(["Frontend", "New group…"]);
  // Escape closes the submenu first, then the menu.
  fireEvent.keyDown(document, { key: "Escape" });
  expect(screen.getAllByRole("menu")).toHaveLength(1);
  fireEvent.keyDown(document, { key: "Escape" });
  expect(screen.queryByRole("menu")).toBeNull();
  expect(submenu(row("web", 2))).toEqual(["New group…"]);
  fireEvent.click(screen.getByRole("menuitem", { name: "New group…" }));
  expect(screen.getByRole("dialog", { name: "New group" })).toBeDefined();
  fireEvent.input(screen.getByLabelText("Name"), { target: { value: "Shared" } });
  fireEvent.submit(screen.getByLabelText("Name").closest("form") as HTMLFormElement);
  await settleAll();
  expect(groups()).toEqual(["Backend: r1 r2", "Frontend: r2 r3", "Shared: r2"]);

  submenu(row("docs", 2));
  fireEvent.click(within(screen.getByRole("menu", { name: "Add to group" })).getByRole("menuitem", { name: "Frontend" }));
  await settleAll();
  expect(groups()).toEqual(["Backend: r1 r2", "Frontend: r2 r3 r4", "Shared: r2"]);

  // Every drag is in a menu too (UI-22): reorder members and groups, remove.
  await pick(row("api", 2), "Move down");
  expect(groups()).toEqual(["Backend: r2 r1", "Frontend: r2 r3 r4", "Shared: r2"]);
  await pick(row("Shared", 1), "Move up");
  expect(groups()).toEqual(["Backend: r2 r1", "Shared: r2", "Frontend: r2 r3 r4"]);
  await pick(row("cli", 2), "Remove from group");
  expect(groups()).toEqual(["Backend: r2 r1", "Shared: r2", "Frontend: r2 r4"]);

  // The context menu key opens the selected row's menu.
  tree.select(id(), tree.tree(id()).roots?.[0] ?? null);
  fireEvent.keyDown(screen.getByRole("tree"), { key: "ContextMenu" });
  expect(screen.getByRole("menu", { name: "Group" })).toBeDefined();
  fireEvent.keyDown(document, { key: "Escape" });

  // Deleting a group says the repos are not touched (GRP-3).
  await pick(row("Shared", 1), "Delete…");
  const question = screen.getByRole("alertdialog", { name: "Delete the group Shared?" });
  expect(question.textContent).toContain("Nothing on disk is touched.");
  fireEvent.click(within(question).getByRole("button", { name: "Delete" }));
  await settleAll();
  expect(groups()).toEqual(["Backend: r2 r1", "Frontend: r2 r4"]);
  expect(fakeState.workspace.repos).toHaveLength(4);
});

test("ui_2_selecting_one_place_of_a_repo_in_two_groups", async () => {
  const { tree } = await setup();
  fireEvent.click(row("web", 2, 1));
  expect(row("web", 2, 1).getAttribute("aria-selected")).toBe("true");
  expect(row("web", 2, 0).getAttribute("aria-selected")).toBe("false");
  const second = sourceOf(tree.tree(id()).selected, tree.tree(id()).selectedItem);
  await settleAll();
  expect(fakeState.selected[id()]).toBe("r2");

  fireEvent.click(row("web", 2, 0));
  expect(row("web", 2, 0).getAttribute("aria-selected")).toBe("true");
  expect(row("web", 2, 1).getAttribute("aria-selected")).toBe("false");
  // Both places show the same history.
  const first = sourceOf(tree.tree(id()).selected, tree.tree(id()).selectedItem);
  expect(first).toEqual(second);
  expect(first?.repo).toBe("r2");
});

test("ui_8_the_filter_matches_group_names", async () => {
  const { tree } = await setup();
  await tree.setFilter(id(), "front");
  await settleAll();
  expect(outline()).toEqual(["Frontend", "  Branches", "  web", "  cli", "Ungrouped"]);
});

test("grp_2_rename_and_new_group_show_the_refusal_inline", async () => {
  await setup();
  const name = () => within(screen.getByRole("dialog")).getByRole("textbox");
  const submit = async (value: string) => {
    fireEvent.input(name(), { target: { value } });
    fireEvent.submit(name().closest("form") as HTMLFormElement);
    await settleAll();
  };

  await pick(row("Ungrouped", 1), "New group…");
  await submit("backend");
  expect(screen.getByRole("dialog", { name: "New group" })).toBeDefined();
  expect(name().getAttribute("aria-invalid")).toBe("true");
  expect(screen.getByText('a group named "Backend" exists already')).toBeDefined();
  await submit("Ungrouped");
  expect(screen.getByText('"Ungrouped" is the name of the repos in no group')).toBeDefined();
  // Typing clears the refusal.
  fireEvent.input(name(), { target: { value: "Docs" } });
  expect(name().getAttribute("aria-invalid")).toBe("false");
  fireEvent.keyDown(name(), { key: "Escape" });
  expect(screen.queryByRole("dialog")).toBeNull();

  await pick(row("Frontend", 1), "Rename…");
  expect((name() as HTMLInputElement).value).toBe("Frontend");
  await submit(" BACKEND ");
  expect(screen.getByText('a group named "Backend" exists already')).toBeDefined();
  await submit("Web");
  expect(screen.queryByRole("dialog")).toBeNull();
  expect(groups()).toEqual(["Backend: r1 r2", "Web: r2 r3"]);
  expect(row("Web", 1)).toBeDefined();
});
