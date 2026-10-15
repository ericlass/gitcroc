import { cleanup, fireEvent, render, screen, within } from "@solidjs/testing-library";
import { afterEach, beforeEach, expect, test, vi } from "vitest";
import App from "../App";
import { LINE_HEIGHT, STRIP_LINES } from "../history/TargetView";
import { groupDelete, type SummaryView } from "../ipc";
import { fakeState, reset } from "../ipc/fake";
import { refNodes, summary } from "../ipc/fakeTree";

// jsdom lays nothing out; give the tree and the strip a height so their rows are rendered.
beforeEach(() => {
  const real = HTMLElement.prototype.getBoundingClientRect;
  vi.spyOn(HTMLElement.prototype, "getBoundingClientRect").mockImplementation(function (this: HTMLElement) {
    if (this.classList.contains("tree-scroller")) {
      return new DOMRect(0, 0, 300, 800);
    }
    if (this.classList.contains("summary-strip")) {
      return new DOMRect(0, 0, 300, STRIP_LINES * LINE_HEIGHT);
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

/** `view` with its summary changed by `change`. */
const changed = (view: SummaryView, change: Partial<NonNullable<SummaryView["summary"]>>): SummaryView =>
  view.summary === null ? view : { ...view, summary: { ...view.summary, ...change } };

/**
 * The app with Backend holding api and web, Frontend web and cli, and docs
 * in no group. web has 3 changed files, cli is in conflict state, docs'
 * summary is stale and api is 2 ahead.
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
    r1: refNodes("r1", { local: ["main"], current: "main" }),
    r2: refNodes("r2", { local: ["main"], current: "main" }),
    r3: refNodes("r3", { local: ["develop"], current: "develop" }),
    r4: refNodes("r4", { local: ["main"], current: "main" }),
  };
  const api = summary(id(), "r1", "main");
  fakeState.summaries = [
    changed(api, { upstream: { name: "origin/main", ahead: 2, behind: 0, gone: false } }),
    changed(summary(id(), "r2", "main"), { dirty: true, changes: 3 }),
    summary(id(), "r3", "develop", { conflict: true }),
    summary(id(), "r4", "main", { stale: true }),
  ];
  render(() => <App />);
  await screen.findAllByRole("treeitem");
  await settleAll();
}

const tree = () => screen.getByRole("tree", { name: "Repositories" });

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

/** The selected rows, a repo row under its group as `Group/repo`. */
const selected = () => {
  let group = "";
  return screen.getAllByRole("treeitem").flatMap((item) => {
    const label = item.querySelector(".tree-label")?.textContent ?? "";
    const level = item.getAttribute("aria-level");
    if (level === "1") {
      group = label;
    }
    const name = level === "2" ? `${group}/${label}` : label;
    return item.getAttribute("aria-selected") === "true" ? [name] : [];
  });
};

/** The repo names of the summary strip's lines; `null` without a strip. */
const strip = () => {
  const list = screen.queryByRole("list", { name: "Repositories of the selection" });
  return list === null
    ? null
    : within(list)
        .getAllByRole("listitem")
        .map((line) => line.querySelector(".tree-label")?.textContent);
};

const click = async (target: HTMLElement, keys: { ctrlKey?: boolean; metaKey?: boolean; shiftKey?: boolean } = {}) => {
  fireEvent.click(target, keys);
  await settleAll();
};

const key = async (name: string, keys: { ctrlKey?: boolean; shiftKey?: boolean } = {}) => {
  fireEvent.keyDown(tree(), { key: name, ...keys });
  await settleAll();
};

test("ui_6_ctrl_and_shift_click_build_a_multi_selection_of_repo_rows_only", async () => {
  await setup();
  expect(tree().getAttribute("aria-multiselectable")).toBe("true");

  // Ctrl-click across two groups; Cmd works as Ctrl.
  await click(row("api", 2));
  await click(row("cli", 2), { ctrlKey: true });
  expect(selected()).toEqual(["Backend/api", "Frontend/cli"]);
  expect(strip()).toEqual(["api", "cli"]);
  await click(row("web", 2, 1), { metaKey: true });
  expect(selected()).toEqual(["Backend/api", "Frontend/web", "Frontend/cli"]);
  // The strip is in tree order, from the core (GRP-5).
  expect(strip()).toEqual(["api", "web", "cli"]);
  // A repo shown twice counts once: its other row toggles it off.
  await click(row("web", 2, 0), { ctrlKey: true });
  expect(selected()).toEqual(["Backend/api", "Frontend/cli"]);

  // Shift-click selects the repo rows between the anchor and the row, each repo once.
  await click(row("api", 2));
  await click(row("cli", 2), { shiftKey: true });
  expect(selected()).toEqual(["Backend/api", "Backend/web", "Frontend/cli"]);
  expect(strip()).toEqual(["api", "web", "cli"]);

  // Group and branch rows are never multi-selected: they replace the selection.
  await click(row("Frontend", 1), { ctrlKey: true });
  expect(selected()).toEqual(["Frontend"]);
  await click(row("api", 2), { ctrlKey: true });
  await click(row("Branches", 2), { shiftKey: true });
  expect(selected()).toEqual(["Backend/Branches"]);

  // Keyboard: Shift+Up extends over repo rows only, skipping Ungrouped.
  await click(row("docs", 2));
  await key("ArrowUp", { shiftKey: true });
  expect(selected()).toEqual(["Frontend/cli", "Ungrouped/docs"]);
  await key("ArrowUp", { shiftKey: true });
  expect(selected()).toEqual(["Frontend/web", "Frontend/cli", "Ungrouped/docs"]);
  // Ctrl+Up moves without changing the selection; Ctrl+Space toggles.
  for (let step = 0; step < 4; step++) {
    await key("ArrowUp", { ctrlKey: true });
  }
  expect(selected()).toEqual(["Frontend/web", "Frontend/cli", "Ungrouped/docs"]);
  await key(" ", { ctrlKey: true });
  expect(selected()).toEqual(["Backend/api", "Frontend/web", "Frontend/cli", "Ungrouped/docs"]);
  expect(strip()).toEqual(["api", "web", "cli", "docs"]);
  await key(" ", { ctrlKey: true });
  expect(strip()).toEqual(["web", "cli", "docs"]);
  // A plain arrow selects one row again.
  await key("ArrowDown");
  expect(selected()).toEqual(["Backend/web"]);
  expect(strip()).toBeNull();
});

test("ui_11_summary_strip_lists_each_repo_and_selects_on_click", async () => {
  await setup();
  await click(row("Backend", 1));
  expect(strip()).toEqual(["api", "web"]);
  expect(screen.getByText("Merged history arrives in M3-07")).toBeTruthy();
  const lines = () => within(screen.getByRole("list", { name: "Repositories of the selection" }));
  const line = (name: string) => lines().getByRole("button", { name: new RegExp(`^${name}`) });
  // Branch, ahead and behind, the dirty count.
  expect(line("api").textContent).toBe("apimain↑2");
  expect(within(line("web")).getByRole("img", { name: "3 changed files" })).toBeTruthy();

  // The conflict flag, and a stale summary marked as in the tree (PERF-3).
  await click(row("cli", 2));
  await click(row("docs", 2), { ctrlKey: true });
  expect(strip()).toEqual(["cli", "docs"]);
  expect(within(line("cli")).getByRole("img", { name: "Conflict state" })).toBeTruthy();
  expect(line("cli").dataset.stale).toBe("false");
  expect(line("docs").dataset.stale).toBe("true");

  // A click selects the repo under the group being viewed.
  await click(row("Frontend", 1));
  await click(line("web"));
  expect(selected()).toEqual(["Frontend/web"]);
  expect(strip()).toBeNull();
  expect(screen.queryByText("Merged history arrives in M3-07")).toBeNull();

  // A group that no longer exists is an error shown in its place.
  await click(row("Backend", 1));
  await groupDelete(id(), "g1");
  await settleAll();
  expect(screen.getByRole("alert").textContent).toContain("the workspace has no group with the id g1");
});

test("ui_11_summary_strip_scrolls_beyond_8_lines", async () => {
  const names = Array.from({ length: 30 }, (_, index) => `repo${index + 1}`);
  const ids = names.map((_, index) => `r${index + 1}`);
  fakeState.workspace = {
    ...fakeState.workspace,
    repos: names.map((name, index) => repo(ids[index] ?? "", name)),
    groups: [{ id: "g1", name: "All", repos: ids }],
  };
  fakeState.refs = Object.fromEntries(ids.map((repoId) => [repoId, refNodes(repoId, { local: ["main"] })]));
  fakeState.summaries = ids.map((repoId) => summary(id(), repoId, "main"));
  render(() => <App />);
  await screen.findAllByRole("treeitem");
  await settleAll();
  await click(row("All", 1));
  const list = screen.getByRole("list", { name: "Repositories of the selection" });
  expect(list.style.height).toBe(`${STRIP_LINES * LINE_HEIGHT}px`);
  // Only the lines in view and a few beyond are rendered.
  const shown = strip() ?? [];
  expect(shown[0]).toBe("repo1");
  expect(shown.length).toBeLessThan(names.length);
});

test("ui_20_toolbar_follows_the_target_set", async () => {
  await setup();
  const toolbar = within(screen.getByRole("toolbar", { name: "Main" }));
  const button = (name: string) => toolbar.getByRole<HTMLButtonElement>("button", { name });
  const BUTTONS = ["Fetch", "Pull", "Pull options", "Push", "Push options", "Branch", "Merge", "Stash"];
  const enabled = () => BUTTONS.filter((name) => !button(name).disabled);

  // A single repo as in M2-07: web is dirty, so it can be stashed.
  await click(row("web", 2));
  expect(enabled()).toEqual(BUTTONS);
  expect(button("Fetch").title).toBe("Fetch all remotes of the selected repository");

  // A group: everything is disabled, and says why.
  await click(row("Backend", 1));
  expect(enabled()).toEqual([]);
  const later = "Not available yet for a group or several repositories";
  for (const name of ["Fetch", "Pull", "Pull options", "Push", "Push options", "Branch", "Merge"]) {
    expect(button(name).title).toBe(later);
  }
  expect(button("Stash").title).toBe("Stash works on one repository at a time");

  // Several repos: the same.
  await click(row("api", 2));
  await click(row("docs", 2), { ctrlKey: true });
  expect(enabled()).toEqual([]);
  expect(button("Merge").title).toBe(later);
});
