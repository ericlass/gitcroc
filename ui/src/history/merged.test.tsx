import { cleanup, fireEvent, render, screen, within } from "@solidjs/testing-library";
import { afterEach, beforeEach, expect, test, vi } from "vitest";
import App from "../App";
import { emitRepoSummary, fakeState, reset } from "../ipc/fake";
import { textFile } from "../ipc/fakeChanges";
import { fakeGroups } from "../ipc/fakeGroups";
import { hashOf } from "../ipc/fakeHistory";
import { GROUP_COMMIT_FAILED } from "../stores/commit";
import { refNodes, summary } from "../ipc/fakeTree";
import { LINE_HEIGHT, STRIP_LINES } from "./TargetView";

// jsdom lays nothing out; give the tree, the strip and the history a height so their rows are rendered.
beforeEach(() => {
  const real = HTMLElement.prototype.getBoundingClientRect;
  vi.spyOn(HTMLElement.prototype, "getBoundingClientRect").mockImplementation(function (this: HTMLElement) {
    if (this.classList.contains("tree-scroller")) {
      return new DOMRect(0, 0, 300, 800);
    }
    if (this.classList.contains("summary-strip")) {
      return new DOMRect(0, 0, 300, STRIP_LINES * LINE_HEIGHT);
    }
    if (this.classList.contains("history-scroller")) {
      return new DOMRect(0, 0, 600, 480);
    }
    if (this.classList.contains("file-list")) {
      return new DOMRect(0, 0, 400, 400);
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

/**
 * The app with Backend holding web and api, in that order, and docs in no
 * group. Every fake history has the same dates, so the merged list takes
 * one commit of each repo in turn, web's first (PERF-5).
 */
async function setup() {
  fakeState.workspace = {
    ...fakeState.workspace,
    repos: [repo("r1", "api"), repo("r2", "web"), repo("r3", "docs")],
    groups: [{ id: "g1", name: "Backend", repos: ["r2", "r1"] }],
  };
  fakeState.refs = {
    r1: refNodes("r1", { local: ["main"], current: "main" }),
    r2: refNodes("r2", { local: ["main"], current: "main" }),
    r3: refNodes("r3", { local: ["main"], current: "main" }),
  };
  fakeState.summaries = ["r1", "r2", "r3"].map((repoId) => summary(id(), repoId, "main"));
  fakeState.histories = { r1: { length: 30, prefix: "api" }, r2: { length: 20, prefix: "web" } };
  render(() => <App />);
  await screen.findAllByRole("treeitem");
  await settleAll();
}

/** The `nth` row labelled `label` at tree level `level`: 1 for groups and Ungrouped, 2 for repos. */
function treeRow(label: string, level: number, nth = 0): HTMLElement {
  const found = screen
    .getAllByRole("treeitem")
    .filter((item) => item.getAttribute("aria-level") === String(level))
    .filter((item) => item.querySelector(".tree-label")?.textContent === label)[nth];
  if (found === undefined) {
    throw new Error(`no row ${label} at level ${level}`);
  }
  return found;
}

const list = () => screen.getByRole("listbox", { name: "History" });

/** The history rows on screen, top to bottom. */
const historyRows = () =>
  within(list())
    .queryAllByRole("option")
    .sort((a, b) => Number(a.dataset.index) - Number(b.dataset.index));

/** Each row's first two cells: the repo (or nothing for a graph) and the subject. */
const cells = () =>
  historyRows().map((row) => {
    const repoCell = row.querySelector(".history-repo")?.textContent ?? null;
    return [repoCell, row.querySelector(".history-subject-text")?.textContent ?? ""];
  });

const click = async (target: HTMLElement, keys: { ctrlKey?: boolean; shiftKey?: boolean } = {}) => {
  fireEvent.click(target, keys);
  await settleAll();
};

test("ui_9_merged_list_shows_the_repo_column_instead_of_the_graph", async () => {
  await setup();
  await click(treeRow("Backend", 1));

  // The strip above, then the merged list: newest first, ties in group order.
  expect(screen.getByRole("list", { name: "Repositories of the selection" })).toBeTruthy();
  const header = document.querySelector(".history-columns");
  expect(header?.textContent).toContain("Repository");
  expect(header?.textContent).not.toContain("Graph");
  expect(cells().slice(0, 4)).toEqual([
    ["web", "web: change 20"],
    ["api", "api: change 30"],
    ["web", "web: change 19"],
    ["api", "api: change 29"],
  ]);
  expect(list().querySelector(".history-graph svg")).toBeNull();
  const last = fakeState.historyCalls.at(-1);
  expect(last?.command).toBe("get");

  // A row shows that commit of that repo in the changes pane (UI-14); the
  // same commit of the other repo is another row.
  await click(historyRows()[1] as HTMLElement);
  expect(fakeState.changesCalls.filter((call) => call.command === "changes").at(-1)).toMatchObject({
    repo: "r1",
    target: { kind: "commit", hash: hashOf(0) },
  });
  expect(historyRows().filter((row) => row.getAttribute("aria-selected") === "true")).toHaveLength(1);

  // Search runs in the core and keeps the repo column (UI-12).
  const search = screen.getByRole("searchbox", { name: "Search history" });
  fireEvent.input(search, { target: { value: "web: change 1" } });
  await settleAll();
  expect(fakeState.historyCalls.at(-1)).toEqual({ command: "get", text: "web: change 1" });
  expect(cells().map(([repoCell]) => repoCell)).toEqual(Array(11).fill("web"));

  // The position is kept per tree selection: a single repo shows its graph,
  // and back in the group the search and the selected row return.
  await click(treeRow("docs", 2));
  expect(screen.queryByRole("list", { name: "Repositories of the selection" })).toBeNull();
  expect(document.querySelector(".history-columns")?.textContent).toContain("Graph");
  expect(cells()[0]).toEqual([null, "docs: change 40"]);
  await click(treeRow("Backend", 1));
  expect((screen.getByRole("searchbox", { name: "Search history" }) as HTMLInputElement).value).toBe("web: change 1");

  // A change to the refs of any repo of the set reads the list again (OPS-7).
  const calls = fakeState.historyCalls.length;
  emitRepoSummary({ ...summary(id(), "r3", "main"), refs_version: 5 });
  await settleAll();
  expect(fakeState.historyCalls.length).toBe(calls);
  emitRepoSummary({ ...summary(id(), "r1", "main"), refs_version: 5 });
  await settleAll();
  expect(fakeState.historyCalls.slice(calls)[0]).toEqual({ command: "get", text: "web: change 1" });
});

test("ui_13_cherry_pick_needs_rows_of_one_repo", async () => {
  await setup();
  await click(treeRow("Backend", 1));
  const menu = async (row: HTMLElement) => {
    fireEvent.contextMenu(row);
    await settleAll();
    const opened = within(screen.getByRole("menu", { name: "Commit actions" }));
    return (label: RegExp) => opened.getByRole<HTMLButtonElement>("menuitem", { name: label });
  };

  // Rows of web and api: no cherry-pick, and the tooltip says why.
  const [web20, api30, web19] = historyRows();
  await click(web20 as HTMLElement);
  await click(api30 as HTMLElement, { ctrlKey: true });
  let entry = await menu(api30 as HTMLElement);
  expect(entry(/^Cherry-pick/).disabled).toBe(true);
  expect(entry(/^Cherry-pick/).title).toBe("Cherry-pick takes commits of one repository only");
  fireEvent.keyDown(document, { key: "Escape" });
  await settleAll();

  // Two rows of web: cherry-pick applies.
  await click(web20 as HTMLElement);
  await click(web19 as HTMLElement, { ctrlKey: true });
  entry = await menu(web19 as HTMLElement);
  expect(entry(/^Cherry-pick 2 commits$/).disabled).toBe(false);
  expect(entry(/^Revert$/).disabled).toBe(true);
  fireEvent.keyDown(document, { key: "Escape" });
  await settleAll();

  // One row acts on its own repo, as for a single repo.
  entry = await menu(api30 as HTMLElement);
  expect(entry(/^Cherry-pick$/).disabled).toBe(false);
  expect(entry(/^Revert$/).disabled).toBe(false);
});

/**
 * web has a staged and an unstaged file, api an unstaged one, docs (in no
 * group) a staged one; each summary says so.
 */
async function dirty() {
  fakeState.workingCopies = {
    r2: {
      staged: [textFile("b.txt", [{ add: "b" }], "added")],
      unstaged: [textFile("a.txt", [{ add: "a" }])],
      unmerged: [],
    },
    r1: { staged: [], unstaged: [textFile("c.txt", [{ add: "c" }])], unmerged: [] },
    r3: { staged: [textFile("d.txt", [{ add: "d" }], "added")], unstaged: [], unmerged: [] },
  };
  for (const [repoId, changes] of [
    ["r1", 1],
    ["r2", 2],
    ["r3", 1],
  ] as const) {
    const view = summary(id(), repoId, "main");
    emitRepoSummary({ ...view, summary: view.summary && { ...view.summary, dirty: true, changes } });
  }
  await settleAll();
}

/** The file list's rows on screen, top to bottom, as text. */
const fileRowTexts = () =>
  [...document.querySelectorAll<HTMLElement>(".file-list .file-row")]
    .sort((a, b) => Number(a.dataset.index) - Number(b.dataset.index))
    .map((row) => (row.querySelector(".file-section, .file-repo, .file-name")?.textContent ?? "").trim());

test("ui_34_uncommitted_row_sections_lists_by_repo_and_labels_commit_with_the_repo_count", async () => {
  await setup();
  await dirty();
  await click(treeRow("Backend", 1));

  // The first row counts the dirty repos of the group, not docs.
  expect(historyRows()[0]?.textContent).toBe("Uncommitted changes (2 repos)");
  await click(historyRows()[0] as HTMLElement);
  expect(fakeState.changesCalls.at(-2)).toEqual({ command: "merged", target: { kind: "group", group_id: "g1" } });

  // Staged and Unstaged each have a section per repo, in group order.
  expect(fileRowTexts()).toEqual([
    "Staged (1)",
    "web (1)",
    "b.txt",
    "Unstaged (2)",
    "web (1)",
    "a.txt",
    "api (1)",
    "c.txt",
  ]);
  // The first file is selected and diffed as web's.
  expect(fakeState.changesCalls.at(-1)).toMatchObject({ command: "diff", file: { list: "staged", id: 0 } });

  // A repo's Stage all stages that repo's files as a job of that repo.
  fireEvent.click(screen.getByRole("button", { name: "Stage all in api" }));
  await settleAll();
  expect(fakeState.changeRequests.at(-1)).toMatchObject({
    action: "stage",
    target: { kind: "files", files: [{ list: "unstaged", id: 0 }] },
  });
  expect(fakeState.workingCopies.r1?.staged.map((file) => file.file.path)).toEqual(["c.txt"]);
  expect(fakeState.workingCopies.r2?.unstaged.map((file) => file.file.path)).toEqual(["a.txt"]);

  // Commit counts the repos with staged changes; staging all tracked
  // changes first counts the others too.
  fakeState.workingCopies.r1 = { staged: [], unstaged: [textFile("c.txt", [{ add: "c" }])], unmerged: [] };
  emitRepoSummary({ ...summary(id(), "r1", "main"), refs_version: 1 });
  await settleAll();
  const commit = () => screen.getByRole<HTMLButtonElement>("button", { name: /^Commit \d+ repos?$/ });
  expect(commit().textContent).toBe("Commit 1 repo");
  expect(commit().disabled).toBe(true);
  fireEvent.click(screen.getByRole("checkbox", { name: "Stage all tracked changes first" }));
  expect(commit().textContent).toBe("Commit 2 repos");
  const subject = screen.getByRole<HTMLInputElement>("textbox", { name: "Subject" });
  fireEvent.input(subject, { target: { value: "Ship it" } });
  expect(commit().disabled).toBe(false);

  // The group Commit asks first, listing each repo's files (GRP-15).
  fireEvent.click(commit());
  await settleAll();
  expect(fakeGroups.preflights.at(-1)).toEqual({
    workspaceId: id(),
    target: { kind: "group", group_id: "g1" },
    op: { kind: "commit", message: { subject: "Ship it", body: "" }, stage_all: true, push: false },
  });
  const preflight = within(await screen.findByRole("dialog"));
  expect(preflight.getAllByText(/staged files?$/).map((files) => files.textContent)).toEqual([
    "2 staged files",
    "1 staged file",
  ]);
  fireEvent.click(preflight.getByRole("button", { name: "Run on 2 repos" }));
  await settleAll();

  // Every repo that ran committed: the box is cleared.
  expect(fakeGroups.starts.at(-1)).toMatchObject({ confirmed: true });
  expect(screen.getByRole<HTMLInputElement>("textbox", { name: "Subject" }).value).toBe("");
});

test("ui_34_no_amend_in_the_group_view", async () => {
  await setup();
  await dirty();
  await click(treeRow("Backend", 1));
  await click(historyRows()[0] as HTMLElement);
  expect(screen.getByRole("region", { name: "Commit message" })).toBeTruthy();
  expect(screen.queryByRole("button", { name: "Amend" })).toBeNull();

  // A repo whose commit failed keeps the message, and the box says so.
  fakeGroups.outcomes = { r2: { kind: "failed" } };
  fireEvent.input(screen.getByRole("textbox", { name: "Subject" }), { target: { value: "Keep me" } });
  fireEvent.click(screen.getByRole("button", { name: "Commit 1 repo" }));
  await settleAll();
  const preflight = within(await screen.findByRole("dialog"));
  fireEvent.click(preflight.getByRole("button", { name: /^Run on/ }));
  await settleAll();
  expect(screen.getByRole<HTMLInputElement>("textbox", { name: "Subject" }).value).toBe("Keep me");
  const box = within(screen.getByRole("region", { name: "Commit message" }));
  expect(box.getByRole("alert").textContent).toBe(GROUP_COMMIT_FAILED);

  // One repo alone has Amend, as before (UI-15).
  await click(treeRow("docs", 2));
  await click(historyRows()[0] as HTMLElement);
  expect(screen.getByRole("button", { name: "Amend" })).toBeTruthy();
});
