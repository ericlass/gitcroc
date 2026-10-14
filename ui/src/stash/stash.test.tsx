import { cleanup, fireEvent, render, screen, within } from "@solidjs/testing-library";
import { afterEach, beforeEach, expect, test, vi } from "vitest";
import App from "../App";
import type { ConflictDetails } from "../ipc";
import { fakeState, reset, setConflict } from "../ipc/fake";
import { type FakeRefs, refNodes, stashHash, summary } from "../ipc/fakeTree";

// jsdom lays nothing out; give the tree and the history a height so rows
// are rendered.
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

const API: FakeRefs = {
  local: ["main"],
  current: "main",
  stashes: ["On main: second", "On main: first"],
};

/** The app with repo `api` selected, its Stashes node expanded; `dirty` when it has changes to stash. */
async function setup(dirty = false) {
  fakeState.workspace = {
    ...fakeState.workspace,
    repos: [{ id: "r1", name: "api", path: "/src/api", remote: null }],
  };
  fakeState.refModels = { r1: API };
  fakeState.refs = { r1: refNodes("r1", API) };
  const view = summary(id(), "r1", "main");
  fakeState.summaries = [
    view.summary === null ? view : { ...view, summary: { ...view.summary, dirty, changes: dirty ? 2 : 0 } },
  ];
  fakeState.histories = { r1: { length: 3, prefix: "api" } };
  render(() => <App />);
  const repo = await screen.findByRole("treeitem", { name: /^api/ });
  fireEvent.click(repo);
  fireEvent.dblClick(repo);
  await settleAll();
  fireEvent.dblClick(screen.getByRole("treeitem", { name: /^Stashes/ }));
  await settleAll();
}

const stashButton = () =>
  within(screen.getByRole("toolbar", { name: "Main" })).getByRole<HTMLButtonElement>("button", { name: "Stash" });

const stashRow = (message: string) => screen.getByRole("treeitem", { name: message });

const pick = (target: HTMLElement, entry: string) => {
  fireEvent.contextMenu(target, { clientX: 10, clientY: 10 });
  fireEvent.click(within(screen.getByRole("menu", { name: "Stash" })).getByRole("menuitem", { name: entry }));
};

const ref = (index: number, message: string) => ({ index, hash: stashHash(message) });

const offer = () => screen.queryByRole("status", { name: "Stash kept" });

const STASH_CONFLICT: ConflictDetails = {
  operation: "stash_apply",
  applying: null,
  onto: "main",
  step: null,
  unmerged_paths: 1,
};

test("UI-20 Stash opens the stash dialog with a message and untracked files", async () => {
  await setup();
  // Nothing to stash in a clean repo.
  expect(stashButton().disabled).toBe(true);
  cleanup();
  reset();

  await setup(true);
  fireEvent.click(stashButton());
  const dialog = screen.getByRole("dialog", { name: "Stash changes in api" });
  fireEvent.input(within(dialog).getByRole("textbox", { name: "Message (optional)" }), { target: { value: "work" } });
  fireEvent.click(within(dialog).getByRole("checkbox", { name: "Include untracked files" }));
  fireEvent.click(within(dialog).getByRole("button", { name: "Stash" }));
  await settleAll();
  expect(fakeState.stashRequests).toEqual([
    { repo_id: "r1", op: { kind: "push", message: "work", include_untracked: true }, confirmed: false },
  ]);
  expect(screen.queryByRole("dialog")).toBeNull();
  // The job's end re-read the refs: the new stash is listed first.
  expect(stashRow("On main: work")).toBeDefined();
  expect(stashButton().disabled).toBe(true);

  // Blocked in conflict state (GRP-8).
  fakeState.summaries = fakeState.summaries.map((known) =>
    known.summary === null ? known : { ...known, summary: { ...known.summary, dirty: true } },
  );
  setConflict("r1", STASH_CONFLICT);
  await settleAll();
  expect(stashButton().disabled).toBe(true);
});

test("UI-7 a stash is applied, popped and dropped from the tree, the drop after a confirmation", async () => {
  await setup();
  pick(stashRow("On main: first"), "Apply");
  await settleAll();
  // Named by the hash the tree read, so a changed list is refused.
  expect(fakeState.stashRequests.at(-1)).toEqual({
    repo_id: "r1",
    op: { kind: "apply", stash: ref(1, "On main: first") },
    confirmed: false,
  });
  expect(stashRow("On main: first")).toBeDefined();

  pick(stashRow("On main: second"), "Pop");
  await settleAll();
  expect(fakeState.stashRequests.at(-1)?.op).toEqual({ kind: "pop", stash: ref(0, "On main: second") });
  expect(screen.queryByRole("treeitem", { name: "On main: second" })).toBeNull();

  // The list was read again: first is stash@{0} now.
  pick(stashRow("On main: first"), "Drop…");
  await settleAll();
  const confirm = screen.getByRole("alertdialog");
  expect(confirm.textContent).toContain("Cannot be undone in api");
  expect(confirm.textContent).toContain("Drop stash@{0} (On main: first)");
  expect(stashRow("On main: first")).toBeDefined();
  fireEvent.click(within(confirm).getByRole("button", { name: "Drop" }));
  await settleAll();
  expect(fakeState.stashRequests.at(-1)).toEqual({
    repo_id: "r1",
    op: { kind: "drop", stash: ref(0, "On main: first") },
    confirmed: true,
  });
  expect(screen.queryByRole("alertdialog")).toBeNull();
  expect(screen.queryByRole("treeitem", { name: "On main: first" })).toBeNull();
});

test("a stash list changed since the tree read it is refused", async () => {
  await setup();
  // Stashed in a terminal: the tree has not read the list again yet.
  fakeState.refModels.r1 = { ...API, stashes: ["On main: third", ...(API.stashes ?? [])] };
  pick(stashRow("On main: second"), "Apply");
  await settleAll();
  const refusal = within(screen.getByRole("region", { name: "Tree" })).getByRole("alert");
  expect(refusal.textContent).toContain("the stash list changed since it was read");
  fireEvent.click(within(refusal).getByRole("button", { name: "Dismiss" }));
  expect(within(screen.getByRole("region", { name: "Tree" })).queryByRole("alert")).toBeNull();
});

test("after a conflicting pop is resolved, the app offers to drop the stash git kept", async () => {
  await setup();
  fakeState.stashConflict = STASH_CONFLICT;
  pick(stashRow("On main: second"), "Pop");
  await settleAll();
  // git kept the stash; while the conflict lasts there is no offer.
  expect(stashRow("On main: second")).toBeDefined();
  expect(screen.getByRole("region", { name: "Conflict state" }).textContent).toContain("Stash apply stopped");
  expect(offer()).toBeNull();

  // The last Mark resolved ends the conflict state.
  setConflict("r1", null);
  await settleAll();
  expect(offer()?.textContent).toContain("The pop of stash@{0} (On main: second) in api stopped with conflicts");
  fireEvent.click(within(offer() ?? document.body).getByRole("button", { name: "Drop stash…" }));
  await settleAll();
  fireEvent.click(within(screen.getByRole("alertdialog")).getByRole("button", { name: "Drop" }));
  await settleAll();
  expect(fakeState.stashRequests.at(-1)).toEqual({
    repo_id: "r1",
    op: { kind: "drop", stash: ref(0, "On main: second") },
    confirmed: true,
  });
  expect(offer()).toBeNull();
  expect(screen.queryByRole("treeitem", { name: "On main: second" })).toBeNull();
});

test("no offer to drop the stash after the conflicting pop was aborted or kept", async () => {
  await setup();
  fakeState.stashConflict = STASH_CONFLICT;
  pick(stashRow("On main: second"), "Pop");
  await settleAll();
  const banner = within(screen.getByRole("region", { name: "Conflict state" }));
  fireEvent.click(banner.getByRole("button", { name: "Abort" }));
  await settleAll();
  fireEvent.click(within(screen.getByRole("alertdialog")).getByRole("button", { name: "Abort" }));
  await settleAll();
  expect(screen.queryByRole("region", { name: "Conflict state" })).toBeNull();
  expect(offer()).toBeNull();

  // Popped again and resolved: "Keep it" ends the offer and drops nothing.
  pick(stashRow("On main: second"), "Pop");
  await settleAll();
  setConflict("r1", null);
  await settleAll();
  fireEvent.click(within(offer() ?? document.body).getByRole("button", { name: "Keep it" }));
  expect(offer()).toBeNull();
  expect(fakeState.stashRequests.map((request) => request.op.kind)).toEqual(["pop", "pop"]);
});
