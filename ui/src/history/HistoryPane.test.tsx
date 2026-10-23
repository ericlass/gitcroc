import { cleanup, fireEvent, render, screen, within } from "@solidjs/testing-library";
import { afterEach, beforeEach, expect, test, vi } from "vitest";
import type { HistoryRow } from "../ipc";
import { emitRepoSummary, fake, fakeState, reset } from "../ipc/fake";
import { commitAt } from "../ipc/fakeHistory";
import { summary } from "../ipc/fakeTree";
import { createBranchStore } from "../stores/branch";
import { createHistoryStore, sourceOf } from "../stores/history";
import { createIntegrateStore } from "../stores/integrate";
import { createLayoutStore } from "../stores/layout";
import { createResetStore } from "../stores/reset";
import { createSelectionStore } from "../stores/selection";
import { createSummaryStore } from "../stores/summaries";
import { createTagStore } from "../stores/tag";
import HistoryPane, { ROW_HEIGHT } from "./HistoryPane";
import { formatDate } from "./rows";
import { createCommandStore } from "../palette/commands";

// jsdom lays nothing out; give the list a height so rows are rendered.
beforeEach(() => {
  const real = HTMLElement.prototype.getBoundingClientRect;
  vi.spyOn(HTMLElement.prototype, "getBoundingClientRect").mockImplementation(function (this: HTMLElement) {
    return this.classList.contains("history-scroller") ? new DOMRect(0, 0, 600, 480) : real.call(this);
  });
});

afterEach(() => {
  cleanup();
  reset();
  vi.restoreAllMocks();
});

const settle = () => new Promise((resolve) => setTimeout(resolve, 0));

const id = () => fakeState.workspace.id;

const repoItem = { kind: "repo" as const, repo_id: "r1", group_id: null };

async function setup(options: { changes?: number } = {}) {
  fakeState.workspace = {
    ...fakeState.workspace,
    repos: [{ id: "r1", name: "api", path: "/src/api", remote: null }],
  };
  const changes = options.changes ?? 0;
  const view = summary(id(), "r1", "main");
  fakeState.summaries = [{ ...view, summary: view.summary && { ...view.summary, dirty: changes > 0, changes } }];
  const selection = createSelectionStore();
  const history = createHistoryStore(selection);
  const summaries = createSummaryStore();
  const layout = createLayoutStore();
  await summaries.subscribe(id());
  await history.subscribe(id());
  render(() => (
    <HistoryPane
      workspace={id()}
      commands={createCommandStore()}
      history={history}
      summaries={summaries}
      name={(repo) => repo}
      layout={layout}
      branch={createBranchStore()}
      integrate={createIntegrateStore()}
      tag={createTagStore()}
      reset={createResetStore()}
    />
  ));
  return { history, selection, summaries, layout };
}

/** The rendered rows, top to bottom, as their text. */
const rowTexts = () =>
  screen
    .queryAllByRole("option")
    .sort((a, b) => Number(a.dataset.index) - Number(b.dataset.index))
    .map((row) => row.textContent ?? "");

test("UI-9 history columns", async () => {
  fakeState.histories = {
    r1: {
      length: 30,
      prefix: "api",
      refs: {
        0: [
          { kind: "current_branch", name: "main" },
          { kind: "remote_branch", name: "origin/main" },
          { kind: "tag", name: "v2.0" },
        ],
      },
    },
  };
  const { history } = await setup();
  history.show(id(), sourceOf("repo:r1", repoItem));
  await settle();

  const first = screen.getAllByRole("option").find((row) => row.dataset.index === "0");
  if (!first) {
    throw new Error("no first row");
  }
  const commit = commitAt(fakeState.histories.r1 ?? { length: 0, prefix: "" }, 0);
  const row = within(first);
  // Graph: a dot on the commit's lane and its lines, drawn from the core's edges.
  expect(first.querySelector(".history-graph svg circle")).not.toBeNull();
  expect(first.querySelectorAll(".history-graph svg line")).toHaveLength(commit.graph?.down.length ?? -1);
  // Subject with labels, which tell branches, remote branches and tags apart.
  expect(row.getByText("api: change 30")).toBeDefined();
  expect(row.getByLabelText("Current branch main").textContent).toBe("✓ main");
  expect(row.getByLabelText("Remote branch origin/main")).toBeDefined();
  expect(row.getByLabelText("Tag v2.0")).toBeDefined();
  // Author, the author date in local time, and the short hash.
  expect(row.getByText("Ada Lovelace")).toBeDefined();
  const date = row.getByText(formatDate(commit.author_time.seconds));
  expect(date.textContent).toMatch(/^\d{4}-\d{2}-\d{2} \d{2}:\d{2}$/);
  expect(date.getAttribute("title")).toContain("Committed");
  expect(row.getByText(commit.short_hash)).toBeDefined();
  // The column header can be resized.
  expect(screen.getByRole("separator", { name: "Resize Author" })).toBeDefined();
});

/** The column headers, left to right, as their text. */
const headerTexts = () =>
  within(screen.getByRole("toolbar", { name: "History columns" }))
    .getAllByRole("button")
    .map((header) => header.textContent ?? "");

/** The first row's cells, left to right, by their column class. */
const firstRowColumns = () =>
  [...(document.querySelector("#history-row-0")?.children ?? [])].map(
    (cell) => [...cell.classList].find((name) => name !== "history-cell") ?? "",
  );

async function showColumns() {
  fakeState.histories = { r1: { length: 5, prefix: "api" } };
  const shown = await setup();
  shown.history.show(id(), sourceOf("repo:r1", repoItem));
  await settle();
  return shown;
}

test("UI-36 the header and the rows follow the column order", async () => {
  const { layout } = await showColumns();
  expect(headerTexts()).toEqual(["Graph", "Subject", "Author", "Date", "Hash"]);
  expect(firstRowColumns()).toEqual(["history-graph", "history-subject", "history-author", "history-date", "history-hash"]);

  layout.moveColumn(id(), "hash", 0);
  layout.moveColumn(id(), "subject", 4);
  await settle();
  expect(headerTexts()).toEqual(["Hash", "Graph", "Author", "Date", "Subject"]);
  expect(firstRowColumns()).toEqual(["history-hash", "history-graph", "history-author", "history-date", "history-subject"]);
});

test("UI-36 an edge right of the subject resizes the column to its right, and the subject keeps its minimum", async () => {
  const real = HTMLElement.prototype.getBoundingClientRect;
  let author = () => 140;
  vi.mocked(HTMLElement.prototype.getBoundingClientRect).mockImplementation(function (this: HTMLElement) {
    if (this.classList.contains("history-scroller")) {
      return new DOMRect(0, 0, 600, 480);
    }
    // The subject's header has 40 px more than its minimum, and gives up what the author column takes.
    return this.matches(".history-columns .history-subject")
      ? new DOMRect(0, 0, 120 - (author() - 140), 20)
      : real.call(this);
  });
  const { layout } = await showColumns();
  author = () => layout.historyColumns(id()).author;
  const graphEdge = screen.getByRole("separator", { name: "Resize Graph" });
  const authorEdge = screen.getByRole("separator", { name: "Resize Author" });
  // Left of the subject, the edge is the column's right one; right of it, its left one.
  expect(graphEdge.classList.contains("splitter-before")).toBe(false);
  expect(authorEdge.classList.contains("splitter-before")).toBe(true);

  // Moving the edge left makes the column right of it wider, up to what the subject can give up.
  fireEvent.keyDown(authorEdge, { key: "ArrowLeft" });
  expect(layout.historyColumns(id()).author).toBe(140 + 16);
  for (let step = 0; step < 4; step++) {
    fireEvent.keyDown(authorEdge, { key: "ArrowLeft" });
  }
  expect(layout.historyColumns(id()).author).toBe(140 + 40);
  fireEvent.keyDown(authorEdge, { key: "ArrowRight" });
  expect(layout.historyColumns(id()).author).toBe(140 + 40 - 16);

  // Moved right of the subject, the graph's edge is on its left too.
  layout.moveColumn(id(), "graph", 4);
  await settle();
  expect(screen.getByRole("separator", { name: "Resize Graph" }).classList.contains("splitter-before")).toBe(true);
  // Each edge between two columns has one splitter; the subject has none.
  expect(screen.getAllByRole("separator")).toHaveLength(4);
});

test("UI-36 the header menu moves and resets columns", async () => {
  const { layout } = await showColumns();
  const header = (name: string) =>
    within(screen.getByRole("toolbar", { name: "History columns" })).getByRole("button", { name });

  fireEvent.contextMenu(header("Graph"));
  let menu = screen.getByRole("menu", { name: "Graph column" });
  expect(within(menu).getByRole("menuitem", { name: "Move left" }).hasAttribute("disabled")).toBe(true);
  fireEvent.click(within(menu).getByRole("menuitem", { name: "Move right" }));
  expect(screen.queryByRole("menu")).toBeNull();
  expect(headerTexts()).toEqual(["Subject", "Graph", "Author", "Date", "Hash"]);
  expect(document.activeElement).toBe(header("Graph"));

  fireEvent.contextMenu(header("Hash"));
  menu = screen.getByRole("menu", { name: "Hash column" });
  expect(within(menu).getByRole("menuitem", { name: "Move right" }).hasAttribute("disabled")).toBe(true);
  fireEvent.click(within(menu).getByRole("menuitem", { name: "Move left" }));
  expect(headerTexts()).toEqual(["Subject", "Graph", "Author", "Hash", "Date"]);

  layout.setColumnWidth(id(), "author", 300);
  fireEvent.contextMenu(header("Author"));
  fireEvent.click(screen.getByRole("menuitem", { name: "Reset columns" }));
  expect(headerTexts()).toEqual(["Graph", "Subject", "Author", "Date", "Hash"]);
  expect(layout.historyColumns(id()).author).toBe(140);
});

test("UI-36 the headers are one tab stop and the keyboard opens their menu", async () => {
  await showColumns();
  const toolbar = screen.getByRole("toolbar", { name: "History columns" });
  const headers = () => within(toolbar).getAllByRole("button");
  expect(headers().map((header) => header.tabIndex)).toEqual([0, -1, -1, -1, -1]);

  headers()[0]?.focus();
  fireEvent.keyDown(document.activeElement ?? toolbar, { key: "ArrowRight" });
  expect(document.activeElement?.textContent).toBe("Subject");
  fireEvent.keyDown(document.activeElement ?? toolbar, { key: "End" });
  expect(document.activeElement?.textContent).toBe("Hash");
  expect(headers().map((header) => header.tabIndex)).toEqual([-1, -1, -1, -1, 0]);

  fireEvent.keyDown(document.activeElement ?? toolbar, { key: "F10", shiftKey: true });
  await settle();
  expect(document.activeElement?.textContent).toBe("Move left");
  fireEvent.click(document.activeElement ?? toolbar);
  expect(headerTexts()).toEqual(["Graph", "Subject", "Author", "Hash", "Date"]);
  expect(document.activeElement?.textContent).toBe("Hash");

  fireEvent.keyDown(document.activeElement ?? toolbar, { key: "ContextMenu" });
  expect(screen.getByRole("menu", { name: "Hash column" })).toBeDefined();
  fireEvent.keyDown(document, { key: "Escape" });
  expect(screen.queryByRole("menu")).toBeNull();
  expect(document.activeElement?.textContent).toBe("Hash");
});

test("PERF-1 the rows on screen are reused as the list scrolls", async () => {
  fakeState.histories = { r1: { length: 300, prefix: "api" } };
  const { history } = await setup();
  history.show(id(), sourceOf("repo:r1", repoItem));
  await settle();
  const scroller = document.querySelector<HTMLElement>(".history-scroller");
  if (!scroller) {
    throw new Error("no list");
  }
  let top = 0;
  Object.defineProperty(scroller, "scrollTop", { get: () => top, set: (value: number) => (top = value) });
  const scrollTo = async (row: number) => {
    top = row * ROW_HEIGHT;
    fireEvent.scroll(scroller);
    await settle();
  };

  // Mid-list, as many rows show as ever will at this height.
  await scrollTo(96);
  await scrollTo(100);
  const before = new Set(screen.getAllByRole("option"));
  const last = Math.max(...[...before].map((row) => Number(row.dataset.index)));
  await scrollTo(104);
  const after = screen.getAllByRole("option");
  // Rows came into view and others left it, in the same elements: new
  // ones would leave the web process the memory of every row shown (M1-14).
  const indexes = after.map((row) => Number(row.dataset.index));
  expect(Math.max(...indexes)).toBeGreaterThan(last);
  expect(after.every((row) => before.has(row))).toBe(true);
  // Each shows its own commit, and says where it is.
  const row = after.find((element) => element.dataset.index === "110");
  expect(row?.textContent).toContain(commitAt({ length: 300, prefix: "api" }, 110).subject);
  expect(row?.getAttribute("aria-posinset")).toBe("111");
});

test("UI-9 dates are the author date as local YYYY-MM-DD HH:MM", () => {
  const seconds = new Date(2024, 0, 2, 3, 4, 59).getTime() / 1000;
  expect(formatDate(seconds)).toBe("2024-01-02 03:04");
});

test("UI-10 uncommitted changes row", async () => {
  const { history, selection } = await setup({ changes: 3 });
  history.show(id(), sourceOf("repo:r1", repoItem));
  await settle();
  expect(rowTexts()[0]).toBe("Uncommitted changes (3)");
  fireEvent.click(screen.getByText("Uncommitted changes (3)"));
  expect(selection.selected(id())).toEqual({ kind: "uncommitted", repo_id: "r1" });

  // A branch that is not checked out has no uncommitted row.
  const other = { kind: "branch" as const, repo_id: "r1", name: "topic", current: false, upstream: null };
  history.show(id(), sourceOf("repo:r1:local:ref:topic", { ...other, gone: false, ahead: null, behind: null }));
  await settle();
  expect(rowTexts()[0]).toMatch(/^topic: change 20/);

  // Once the working copy is clean, it is gone.
  history.show(id(), sourceOf("repo:r1", repoItem));
  await settle();
  expect(rowTexts()[0]).toBe("Uncommitted changes (3)");
  emitRepoSummary(summary(id(), "r1", "main"));
  await settle();
  expect(rowTexts()[0]).not.toMatch(/Uncommitted/);
});

test("UI-12 search filters in the core", async () => {
  const { history } = await setup();
  history.show(id(), sourceOf("repo:r1", repoItem));
  await settle();
  expect(rowTexts()).not.toHaveLength(0);

  // Whatever the core answers is shown as it is: none of these rows
  // contains the text, and none is dropped.
  const answer: HistoryRow[] = [0, 1].map((index) => ({
    ...commitAt({ length: 2, prefix: "unrelated" }, index),
    graph: null,
    repo_id: "r1",
  }));
  const spy = vi
    .spyOn(fake.commands, "historyGet")
    .mockImplementation(() =>
      Promise.resolve({
        status: "ok",
        data: { handle: 99, rows: answer, repos: ["r1"], done: true, offer_commit_graph: false },
      }),
    );
  fireEvent.input(screen.getByRole("searchbox", { name: "Search history" }), { target: { value: "fix" } });
  await settle();
  expect(spy).toHaveBeenLastCalledWith(id(), "r1", { kind: "repo" }, "fix");
  expect(rowTexts().map((text) => text.replace(/Ada.*|Grace.*/, ""))).toEqual([
    "unrelated: change 2",
    "unrelated: change 1",
  ]);
  // Filtered rows are not contiguous: no graph lines.
  expect(document.querySelector(".history-graph svg")).toBeNull();
});

test("Commit-graph offer", async () => {
  fakeState.slowRepos = { r1: true };
  const { history } = await setup();
  history.show(id(), sourceOf("repo:r1", repoItem));
  await settle();
  expect(screen.getAllByRole("group", { name: "Commit-graph offer" })).toHaveLength(1);

  // Another selection in the same repo offers again, yet it is shown once.
  const tag = { kind: "tag" as const, repo_id: "r1", name: "v1", annotated: false };
  history.show(id(), sourceOf("repo:r1:tags:ref:v1", tag));
  await settle();
  expect(screen.getAllByRole("group", { name: "Commit-graph offer" })).toHaveLength(1);

  fireEvent.click(screen.getByRole("button", { name: "Don't ask again" }));
  await settle();
  expect(fakeState.declined.r1).toBe(true);
  expect(fakeState.commitGraphWrites).toEqual([]);
  expect(screen.queryByRole("group", { name: "Commit-graph offer" })).toBeNull();

  history.show(id(), sourceOf("repo:r1", repoItem));
  await settle();
  expect(screen.queryByRole("group", { name: "Commit-graph offer" })).toBeNull();
});

test("Commit-graph offer: writing runs as a job, and a failure is reported", async () => {
  fakeState.slowRepos = { r1: true };
  fakeState.commitGraphError = "git could not write the commit-graph file: disk full";
  const { history } = await setup();
  history.show(id(), sourceOf("repo:r1", repoItem));
  await settle();
  fireEvent.click(screen.getByRole("button", { name: "Write commit-graph file" }));
  await settle();
  expect(fakeState.commitGraphWrites).toEqual(["r1"]);
  expect(screen.queryByRole("group", { name: "Commit-graph offer" })).toBeNull();
  await new Promise((resolve) => setTimeout(resolve, 20));
  expect(screen.getByRole("alert").textContent).toContain("disk full");
  // History keeps working without the file.
  expect(rowTexts().length).toBeGreaterThan(0);
});

test("UI-9 nothing selected shows an empty state", async () => {
  const { history } = await setup();
  history.show(id(), sourceOf(null, null));
  expect(screen.getByText(/Select a repo, branch, tag or stash/)).toBeDefined();
  history.show(id(), sourceOf("ungrouped", { kind: "ungrouped" }));
  expect(screen.queryByRole("listbox")).toBeNull();
});
