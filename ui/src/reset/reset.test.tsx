import { cleanup, fireEvent, render, screen, within } from "@solidjs/testing-library";
import { afterEach, beforeEach, expect, test, vi } from "vitest";
import App from "../App";
import type { RepoSummary, SummaryView } from "../ipc";
import { fakeState, reset } from "../ipc/fake";
import { keep, textFile } from "../ipc/fakeChanges";
import { commitAt } from "../ipc/fakeHistory";
import { type FakeRefs, refNodes, summary } from "../ipc/fakeTree";

// jsdom lays nothing out; give the history a height so rows are rendered.
beforeEach(() => {
  const real = HTMLElement.prototype.getBoundingClientRect;
  vi.spyOn(HTMLElement.prototype, "getBoundingClientRect").mockImplementation(function (this: HTMLElement) {
    if (this.classList.contains("tree-scroller")) {
      return new DOMRect(0, 0, 300, 800);
    }
    return this.classList.contains("history-scroller") ? new DOMRect(0, 0, 600, 480) : real.call(this);
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

const API: FakeRefs = { local: ["main"], current: "main" };

const HISTORY = { length: 5, prefix: "api" };

/** The app with repo `api` selected and its history shown. */
async function setup(change: Partial<SummaryView> = {}) {
  fakeState.workspace = {
    ...fakeState.workspace,
    repos: [{ id: "r1", name: "api", path: "/src/api", remote: null }],
  };
  fakeState.refModels = { r1: API };
  fakeState.refs = { r1: refNodes("r1", API) };
  fakeState.summaries = [summary(id(), "r1", "main", change)];
  fakeState.histories = { r1: HISTORY };
  render(() => <App />);
  fireEvent.click(await screen.findByRole("treeitem", { name: /^api/ }));
  await settleAll();
}

/** `api`'s summary with `change` made to it. */
function changed(change: Partial<RepoSummary>): Partial<SummaryView> {
  const view = summary(id(), "r1", "main");
  return { summary: view.summary === null ? null : { ...view.summary, ...change } };
}

const commit = (index: number) => commitAt(HISTORY, index);

const text = (index: number) => `${commit(index).short_hash} ${commit(index).subject}`;

/** Opens "Reset current branch to here…" on commit `index`. */
const resetTo = (index: number) => {
  const row = within(screen.getByRole("listbox", { name: "History" })).getByRole("option", {
    name: new RegExp(commit(index).subject),
  });
  fireEvent.contextMenu(row, { clientX: 10, clientY: 10 });
  fireEvent.click(
    within(screen.getByRole("menu", { name: "Commit actions" })).getByRole("menuitem", {
      name: "Reset current branch to here…",
    }),
  );
};

const mode = (name: RegExp) => screen.getByRole<HTMLInputElement>("radio", { name });

test("UI-13 reset current branch to here: mixed by default, soft and mixed without a confirmation", async () => {
  await setup();
  resetTo(2);
  const dialog = screen.getByRole("dialog", { name: `Reset main in api to ${text(2)}` });
  expect(mode(/^Mixed/).checked).toBe(true);
  fireEvent.click(within(dialog).getByRole("button", { name: "Reset" }));
  await settleAll();
  expect(fakeState.resetRequests).toEqual([{ repo_id: "r1", commit: commit(2).hash, mode: "mixed", confirmed: false }]);
  expect(screen.queryByRole("dialog")).toBeNull();
  expect(screen.queryByRole("alertdialog")).toBeNull();

  resetTo(3);
  fireEvent.click(mode(/^Soft/));
  fireEvent.click(screen.getByRole("button", { name: "Reset" }));
  await settleAll();
  expect(fakeState.resetRequests.at(-1)).toEqual({ repo_id: "r1", commit: commit(3).hash, mode: "soft", confirmed: false });
  expect(screen.queryByRole("dialog")).toBeNull();
});

test("grp_9 the hard reset confirmation names the repo, the branch, the commit and the changed files lost", async () => {
  fakeState.workingCopies = {
    r1: {
      staged: [textFile("new.txt", [{ add: "new" }], "added")],
      unstaged: [textFile("a.txt", [...keep(1, 2), { add: "edit" }]), textFile("scratch.txt", [{ add: "x" }], "untracked")],
      unmerged: [],
    },
  };
  await setup(changed({ dirty: true, changes: 3 }));
  resetTo(1);
  fireEvent.click(mode(/^Hard/));
  fireEvent.click(screen.getByRole("button", { name: "Reset hard…" }));
  await settleAll();
  const confirm = screen.getByRole("alertdialog");
  expect(confirm.textContent).toContain("Cannot be undone in api");
  const actions = within(confirm)
    .getAllByRole("listitem")
    .map((item) => item.textContent);
  expect(actions).toEqual([
    `Reset branch main to ${text(1)}`,
    "Discard the uncommitted changes to 2 files; untracked files stay",
  ]);
  // Not confirmed: nothing ran.
  expect(fakeState.jobs).toEqual([]);

  // Cancel sends nothing more.
  fireEvent.click(within(confirm).getByRole("button", { name: "Cancel" }));
  expect(screen.queryByRole("alertdialog")).toBeNull();
  expect(fakeState.resetRequests).toHaveLength(1);

  resetTo(1);
  fireEvent.click(mode(/^Hard/));
  fireEvent.click(screen.getByRole("button", { name: "Reset hard…" }));
  await settleAll();
  fireEvent.click(within(screen.getByRole("alertdialog")).getByRole("button", { name: "Reset hard" }));
  await settleAll();
  expect(fakeState.resetRequests.at(-1)).toEqual({ repo_id: "r1", commit: commit(1).hash, mode: "hard", confirmed: true });
  expect(screen.queryByRole("alertdialog")).toBeNull();
});

test("a detached HEAD is reset itself, and the dialog and the confirmation say so", async () => {
  await setup(changed({ head: { kind: "detached", hash: commit(0).hash }, upstream: null }));
  resetTo(2);
  const dialog = screen.getByRole("dialog", { name: `Reset HEAD in api to ${text(2)}` });
  expect(dialog.textContent).toContain("HEAD is detached: HEAD is reset, not a branch.");
  fireEvent.click(mode(/^Hard/));
  fireEvent.click(within(dialog).getByRole("button", { name: "Reset hard…" }));
  await settleAll();
  expect(screen.getByRole("alertdialog").textContent).toContain(
    `HEAD is detached: reset HEAD, not a branch, to ${text(2)}`,
  );
  expect(screen.getByRole("alertdialog").textContent).toContain("No uncommitted changes are lost");
});

test("GRP-8 reset is disabled for a busy repo", async () => {
  await setup({ busy: true });
  const row = within(screen.getByRole("listbox", { name: "History" })).getByRole("option", {
    name: new RegExp(commit(1).subject),
  });
  fireEvent.contextMenu(row, { clientX: 10, clientY: 10 });
  const menu = within(screen.getByRole("menu", { name: "Commit actions" }));
  expect(menu.getByRole<HTMLButtonElement>("menuitem", { name: "Reset current branch to here…" }).disabled).toBe(true);
  expect(menu.getByRole<HTMLButtonElement>("menuitem", { name: "Create tag here…" }).disabled).toBe(true);
});
