import { cleanup, fireEvent, render, screen, within } from "@solidjs/testing-library";
import { afterEach, beforeEach, expect, test, vi } from "vitest";
import App from "../App";
import type { ConflictDetails, Operation, SummaryView } from "../ipc";
import { fakeState, reset, setConflict } from "../ipc/fake";
import { defaultDetails, defaultFiles } from "../ipc/fakeChanges";
import { commitAt } from "../ipc/fakeHistory";
import { type FakeRefs, refNodes, summary } from "../ipc/fakeTree";

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
  local: ["main", "feature", "fix/feat-two"],
  current: "main",
  remotes: { origin: ["main"] },
  tags: ["v1"],
};

const HISTORY = { length: 5, prefix: "api" };

/** The app with repo `api` on `refs` selected, its history of five commits shown. */
async function setup(refs: FakeRefs = API, change: Partial<SummaryView> = {}) {
  fakeState.workspace = {
    ...fakeState.workspace,
    repos: [{ id: "r1", name: "api", path: "/src/api", remote: null }],
  };
  fakeState.refModels = { r1: refs };
  fakeState.refs = { r1: refNodes("r1", refs) };
  fakeState.summaries = [summary(id(), "r1", refs.current ?? "main", change)];
  fakeState.histories = { r1: HISTORY };
  render(() => <App />);
  fireEvent.click(await screen.findByRole("treeitem", { name: /^api/ }));
  await settleAll();
}

const hash = (index: number) => commitAt(HISTORY, index).hash;

/** The history row of commit `index`. */
const row = (index: number) =>
  within(screen.getByRole("listbox", { name: "History" })).getByRole("option", {
    name: new RegExp(commitAt(HISTORY, index).subject),
  });

const banner = () => screen.queryByRole("region", { name: "Conflict state" });

const bannerButton = (name: string) =>
  within(screen.getByRole("region", { name: "Conflict state" })).getByRole<HTMLButtonElement>("button", { name });

const conflict = (operation: Operation, change: Partial<ConflictDetails> = {}): ConflictDetails => ({
  operation,
  applying: null,
  onto: null,
  step: null,
  unmerged_paths: 2,
  ...change,
});

/** The open commit menu's entries, as label and whether each is enabled. */
const menuEntries = () =>
  within(screen.getByRole("menu", { name: "Commit actions" }))
    .getAllByRole<HTMLButtonElement>("menuitem")
    .map((item) => [item.textContent, !item.disabled]);

test.each([
  {
    details: conflict("merge", { applying: "feature", onto: "main" }),
    text: "Merge in progress: merging feature into main",
  },
  {
    details: conflict("rebase", { applying: "feature", onto: "main", step: { current: 2, total: 3 } }),
    text: "Rebase in progress: rebasing feature onto main, step 2 of 3",
  },
  {
    details: conflict("cherry_pick", { applying: "1a2b3c4 Fix the parser", onto: "main" }),
    text: "Cherry-pick in progress: applying 1a2b3c4 Fix the parser onto main",
  },
  {
    details: conflict("revert", { applying: "1a2b3c4 Fix the parser", onto: "main" }),
    text: "Revert in progress: reverting 1a2b3c4 Fix the parser on main",
  },
  {
    details: conflict("apply_mailbox", { step: { current: 1, total: 4 } }),
    text: "Patch application in progress: patch 1 of 4",
  },
  {
    // git records no stash for an apply, only where it was applied (OPS-1).
    details: conflict("stash_apply", { onto: "main" }),
    text: "Stash apply stopped with conflicts on main",
  },
])("ops_2_banner_names_the_operation_with_abort_and_continue: $details.operation", async ({ details, text }) => {
  await setup();
  expect(banner()).toBeNull();

  // Started in a terminal or by the app, the repo's state is what counts (OPS-1).
  setConflict("r1", details);
  await settleAll();
  expect(banner()?.textContent).toContain(text);
  expect(banner()?.textContent).toContain("2 paths are unmerged");
  expect(bannerButton("Abort").disabled).toBe(false);
  // OPS-2: Continue waits until no path is unmerged.
  expect(bannerButton("Continue").disabled).toBe(true);

  if (details.operation === "stash_apply") {
    // Nothing to continue: the last path marked resolved ends the state.
    expect(banner()?.textContent).toContain("mark them resolved, and the stash apply ends");
    setConflict("r1", null);
    await settleAll();
    expect(banner()).toBeNull();
    expect(fakeState.integrateRequests).toEqual([]);
    return;
  }
  setConflict("r1", { ...details, unmerged_paths: 0 });
  await settleAll();
  expect(banner()?.textContent).toContain("All conflicts are resolved");
  expect(bannerButton("Continue").disabled).toBe(false);

  fireEvent.click(bannerButton("Continue"));
  await settleAll();
  expect(fakeState.integrateRequests.at(-1)).toEqual({ repo_id: "r1", op: { kind: "continue" }, confirmed: false });
  expect(banner()).toBeNull();
});

test("ops_2_abort_of_a_stash_apply_asks_first", async () => {
  await setup();
  setConflict("r1", conflict("stash_apply", { onto: "main", unmerged_paths: 1 }));
  await settleAll();

  // It cannot be undone from the app: the core asks, the dialog names what is lost.
  fireEvent.click(bannerButton("Abort"));
  await settleAll();
  const dialog = screen.getByRole("alertdialog");
  expect(dialog.textContent).toContain("Cannot be undone in api");
  expect(dialog.textContent).toContain("back to HEAD");
  expect(banner()).not.toBeNull();
  fireEvent.click(within(dialog).getByRole("button", { name: "Cancel" }));
  expect(screen.queryByRole("alertdialog")).toBeNull();
  expect(banner()).not.toBeNull();

  fireEvent.click(bannerButton("Abort"));
  await settleAll();
  fireEvent.click(within(screen.getByRole("alertdialog")).getByRole("button", { name: "Abort" }));
  await settleAll();
  expect(fakeState.integrateRequests.map((request) => request.confirmed)).toEqual([false, false, true]);
  expect(screen.queryByRole("alertdialog")).toBeNull();
  expect(banner()).toBeNull();
});

test("ops_2_abort_from_the_banner_and_a_continue_that_stops_again", async () => {
  await setup();
  // Merge, Branch and the history's writes are blocked in conflict state (GRP-8).
  setConflict("r1", conflict("rebase", { applying: "feature", onto: "main", step: { current: 1, total: 2 } }));
  await settleAll();
  const toolbar = within(screen.getByRole("toolbar", { name: "Main" }));
  expect(toolbar.getByRole<HTMLButtonElement>("button", { name: "Merge" }).disabled).toBe(true);

  // A continue that stops on the next commit: the banner stays, at the next step.
  setConflict("r1", conflict("rebase", { applying: "feature", onto: "main", step: { current: 1, total: 2 }, unmerged_paths: 0 }));
  fakeState.integrateConflict = conflict("rebase", { applying: "feature", onto: "main", step: { current: 2, total: 2 } });
  await settleAll();
  fireEvent.click(bannerButton("Continue"));
  await settleAll();
  expect(banner()?.textContent).toContain("step 2 of 2");
  expect(bannerButton("Continue").disabled).toBe(true);

  fireEvent.click(bannerButton("Abort"));
  await settleAll();
  expect(fakeState.integrateRequests.at(-1)).toEqual({ repo_id: "r1", op: { kind: "abort" }, confirmed: false });
  expect(banner()).toBeNull();
  expect(toolbar.getByRole<HTMLButtonElement>("button", { name: "Merge" }).disabled).toBe(false);
});

test("ui_13_commit_context_menu", async () => {
  await setup();

  fireEvent.contextMenu(row(1));
  expect(menuEntries()).toEqual([
    ["Check out (detached)", true],
    ["Create branch here…", true],
    ["Create tag here…", true],
    ["Cherry-pick", true],
    ["Revert", true],
    ["Reset current branch to here…", true],
    ["Copy hash", true],
  ]);
  fireEvent.click(screen.getByRole("menuitem", { name: "Cherry-pick" }));
  await settleAll();
  expect(fakeState.integrateRequests.at(-1)).toEqual({
    repo_id: "r1",
    op: { kind: "cherry_pick", commits: [hash(1)] },
    confirmed: false,
  });

  // Ctrl/Cmd adds to the selection: only Cherry-pick applies to several.
  fireEvent.click(row(0));
  fireEvent.click(row(3), { ctrlKey: true });
  fireEvent.contextMenu(row(3));
  expect(menuEntries()).toEqual([
    ["Check out (detached)", false],
    ["Create branch here…", false],
    ["Create tag here…", false],
    ["Cherry-pick 2 commits", true],
    ["Revert", false],
    ["Reset current branch to here…", false],
    ["Copy hash", false],
  ]);
  fireEvent.click(screen.getByRole("menuitem", { name: "Cherry-pick 2 commits" }));
  await settleAll();
  expect(fakeState.integrateRequests.at(-1)).toEqual({
    repo_id: "r1",
    op: { kind: "cherry_pick", commits: [hash(0), hash(3)] },
    confirmed: false,
  });

  // Shift selects the range from the last row clicked.
  fireEvent.click(row(1));
  fireEvent.click(row(3), { shiftKey: true });
  fireEvent.contextMenu(row(2));
  expect(screen.getByRole("menuitem", { name: "Cherry-pick 3 commits" })).toBeTruthy();
  fireEvent.keyDown(document, { key: "Escape" });
  expect(screen.queryByRole("menu")).toBeNull();

  // A row outside the selection gets a menu of its own.
  fireEvent.contextMenu(row(4));
  expect(screen.getByRole("menuitem", { name: "Cherry-pick" })).toBeTruthy();
  fireEvent.click(screen.getByRole("menuitem", { name: "Check out (detached)" }));
  await settleAll();
  expect(fakeState.branchRequests.at(-1)).toEqual({
    repo_id: "r1",
    op: { kind: "checkout_detached", revision: hash(4) },
    confirmed: false,
  });
});

test("ui_13_create_branch_here_and_revert_a_merge_commit", async () => {
  await setup();
  fakeState.commits = {
    [hash(2)]: { details: defaultDetails(hash(2), [hash(3), "f".repeat(40)]), files: [defaultFiles(), defaultFiles()] },
  };

  fireEvent.contextMenu(row(1));
  fireEvent.click(screen.getByRole("menuitem", { name: "Create branch here…" }));
  await settleAll();
  const create = screen.getByRole("dialog");
  expect(within(create).getByLabelText<HTMLInputElement>("Branch, tag or commit").value).toBe(hash(1));
  fireEvent.click(within(create).getByRole("button", { name: "Cancel" }));

  // A merge commit's revert asks which parent's side to keep, the first by default.
  fireEvent.contextMenu(row(2));
  fireEvent.click(screen.getByRole("menuitem", { name: "Revert" }));
  await settleAll();
  const mainline = screen.getByRole("dialog", { name: /Revert a merge commit/ });
  expect(within(mainline).getByRole<HTMLInputElement>("radio", { name: new RegExp(hash(3).slice(0, 7)) }).checked).toBe(
    true,
  );
  fireEvent.click(within(mainline).getByRole("radio", { name: /^2: f{7}/ }));
  fireEvent.click(within(mainline).getByRole("button", { name: "Revert" }));
  await settleAll();
  expect(fakeState.integrateRequests.map((request) => request.op)).toEqual([
    { kind: "revert", commit: hash(2), mainline: null },
    { kind: "revert", commit: hash(2), mainline: 2 },
  ]);
  expect(screen.queryByRole("dialog")).toBeNull();
});

test("ui_20_merge_dialog_filters_in_the_core_and_merges_or_rebases", async () => {
  await setup();
  const merge = () => within(screen.getByRole("toolbar", { name: "Main" })).getByRole("button", { name: "Merge" });

  fireEvent.click(merge());
  await settleAll();
  const dialog = screen.getByRole("dialog", { name: "Merge into main in api" });
  const refs = () =>
    within(within(dialog).getByRole("listbox", { name: "Refs" }))
      .queryAllByRole("option")
      .map((option) => option.textContent);
  // Every ref but the current branch.
  expect(refs()).toEqual(["feature", "fix/feat-two", "origin/main", "v1"]);
  const field = within(dialog).getByRole("textbox", { name: "Branch, remote branch or tag" });
  fireEvent.input(field, { target: { value: "FEAT" } });
  await settleAll();
  expect(refs()).toEqual(["feature", "fix/feat-two"]);

  fireEvent.click(within(dialog).getByRole("option", { name: "feature" }));
  fireEvent.click(within(dialog).getByRole("radio", { name: "Always create a merge commit" }));
  fireEvent.click(within(dialog).getByRole("button", { name: "Merge" }));
  await settleAll();
  expect(fakeState.integrateRequests.at(-1)).toEqual({
    repo_id: "r1",
    op: { kind: "merge", revision: "feature", fast_forward: "no_ff" },
    confirmed: false,
  });
  expect(screen.queryByRole("dialog")).toBeNull();

  fireEvent.click(merge());
  await settleAll();
  const again = screen.getByRole("dialog");
  fireEvent.input(within(again).getByRole("textbox"), { target: { value: "origin/main" } });
  fireEvent.click(within(again).getByRole("checkbox", { name: "Rebase main onto it instead" }));
  fireEvent.click(within(again).getByRole("button", { name: "Rebase" }));
  await settleAll();
  expect(fakeState.integrateRequests.at(-1)).toEqual({
    repo_id: "r1",
    op: { kind: "rebase", onto: "origin/main" },
    confirmed: false,
  });

  // The core's refusal shows in the dialog.
  fireEvent.click(merge());
  await settleAll();
  const refused = screen.getByRole("dialog");
  fireEvent.input(within(refused).getByRole("textbox"), { target: { value: "nope" } });
  fireEvent.click(within(refused).getByRole("button", { name: "Merge" }));
  await settleAll();
  expect(within(refused).getByRole("alert").textContent).toBe("'nope' names no commit");
});

test("ui_20_merge_is_disabled_on_a_detached_head", async () => {
  const onBranch = summary("", "r1", "main").summary;
  await setup({ ...API, current: undefined }, {
    summary: onBranch && { ...onBranch, head: { kind: "detached", hash: hash(0) } },
  });
  const merge = within(screen.getByRole("toolbar", { name: "Main" })).getByRole<HTMLButtonElement>("button", {
    name: "Merge",
  });
  expect(merge.disabled).toBe(true);
});
