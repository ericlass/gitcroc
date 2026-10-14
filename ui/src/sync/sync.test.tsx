import { cleanup, fireEvent, render, screen, within } from "@solidjs/testing-library";
import { afterEach, beforeEach, expect, test, vi } from "vitest";
import App from "../App";
import type { SummaryView } from "../ipc";
import { emitRepoSummary, fakeState, finishSync, reset } from "../ipc/fake";
import { type FakeRefs, refNodes, summary } from "../ipc/fakeTree";

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

/** `main` one ahead of and two behind `origin/main`. */
const API: FakeRefs = {
  local: ["main", "topic"],
  current: "main",
  counts: { main: [1, 2] },
  remotes: { origin: ["main"] },
};

/** The app with repo `api` on `refs`, its row selected unless told otherwise. */
async function setup(refs: FakeRefs = API, change: Partial<SummaryView> = {}, select = true) {
  fakeState.workspace = {
    ...fakeState.workspace,
    repos: [{ id: "r1", name: "api", path: "/src/api", remote: null }],
  };
  fakeState.refModels = { r1: refs };
  fakeState.refs = { r1: refNodes("r1", refs) };
  fakeState.summaries = [summary(id(), "r1", refs.current ?? "main", change)];
  render(() => <App />);
  const row = await screen.findByRole("treeitem", { name: /^api/ });
  await settleAll();
  if (select) {
    fireEvent.click(row);
    await settleAll();
  }
}

const toolbar = () => within(screen.getByRole("toolbar", { name: "Main" }));
const button = (name: string) => toolbar().getByRole<HTMLButtonElement>("button", { name });
const SYNC = ["Fetch", "Pull", "Pull options", "Push", "Push options"];

/** The sync buttons that are enabled. */
const enabled = () => SYNC.filter((name) => !button(name).disabled);

/** Sends the summary of `api` again with `change`, as the core would. */
async function becomes(change: Partial<SummaryView>) {
  emitRepoSummary(summary(id(), "r1", "main", change));
  await settleAll();
}

test("ui_20_toolbar_acts_on_the_selected_repo", async () => {
  await setup(API, {}, false);
  // Nothing selected: nothing to act on.
  expect(enabled()).toEqual([]);

  fireEvent.click(screen.getByRole("treeitem", { name: /^api/ }));
  await settleAll();
  expect(enabled()).toEqual(SYNC);

  // A click runs with the defaults.
  fireEvent.click(button("Fetch"));
  await settleAll();
  fireEvent.click(button("Pull"));
  await settleAll();
  expect(fakeState.syncRequests).toEqual([
    { repo_id: "r1", op: { kind: "fetch", prune: "config" }, confirmed: false },
    {
      repo_id: "r1",
      op: { kind: "pull", strategy: "config" },
      confirmed: false,
    },
  ]);
  // The fake's pull failed: main is ahead and behind, and ff-only is the default.
  expect(screen.getByRole("status").textContent).toContain("Pull origin/main into main: Failed");

  // The arrow opens the options.
  fireEvent.click(button("Pull options"));
  const pull = await screen.findByRole("dialog", { name: "Pull api" });
  fireEvent.click(within(pull).getByRole("radio", { name: "Rebase" }));
  fireEvent.click(within(pull).getByRole("button", { name: "Pull" }));
  await settleAll();
  expect(fakeState.syncRequests.at(-1)?.op).toEqual({
    kind: "pull",
    strategy: "rebase",
  });
  expect(screen.queryByRole("dialog")).toBeNull();
  // OPS-7: ahead and behind read again after the job.
  expect(screen.getByRole("status").textContent).toContain("Pull origin/main into main: Succeeded");

  // GRP-8: busy or missing, nothing; in conflict state or on a detached HEAD, only Fetch.
  await becomes({ busy: true });
  expect(enabled()).toEqual([]);
  await becomes({ missing: true });
  expect(enabled()).toEqual([]);
  await becomes({ conflict: true });
  expect(enabled()).toEqual(["Fetch"]);
  const detached = summary(id(), "r1", "main");
  await becomes({
    summary: detached.summary && {
      ...detached.summary,
      head: { kind: "detached", hash: "0123abc" },
      upstream: null,
    },
  });
  expect(enabled()).toEqual(["Fetch"]);
  await becomes({});
  expect(enabled()).toEqual(SYNC);
});

test("UI-21 a fetch shows git's phase in the status bar while it runs", async () => {
  await setup();
  fakeState.holdSync = true;
  fireEvent.click(button("Fetch"));
  await settleAll();
  expect(screen.getByRole("status").textContent).toBe("Fetch all remotes Receiving objects 1/2");
  finishSync();
  await settleAll();
  expect(screen.getByRole("status").textContent).toContain("Fetch all remotes: Nothing to do");
});

test("UI-20 a push without a remote to go to asks which", async () => {
  const refs: FakeRefs = {
    local: ["topic"],
    current: "topic",
    remotes: { a: [], b: [] },
  };
  const topic = summary(id(), "r1", "topic").summary;
  await setup(refs, { summary: topic && { ...topic, upstream: null } });
  fireEvent.click(button("Push"));
  const push = await screen.findByRole("dialog", { name: "Push api" });
  expect(fakeState.syncRequests).toHaveLength(1);
  const send = within(push).getByRole<HTMLButtonElement>("button", {
    name: "Push",
  });
  await settleAll();
  expect(send.disabled).toBe(true);
  fireEvent.change(within(push).getByRole("combobox", { name: "Remote" }), {
    target: { value: "b" },
  });
  expect(send.disabled).toBe(false);
  fireEvent.click(send);
  await settleAll();
  expect(fakeState.syncRequests.at(-1)?.op).toEqual({
    kind: "push",
    set_upstream: true,
    remote: "b",
    force_with_lease: false,
  });
  expect(screen.getByRole("status").textContent).toContain("Push topic to b/topic: Succeeded");
});

test("GRP-9 a force push needs the user's confirmation", async () => {
  await setup();
  fireEvent.click(button("Push options"));
  const push = await screen.findByRole("dialog", { name: "Push api" });
  await settleAll();
  expect(within(push).getByText("Pushes main to origin/main.")).toBeDefined();
  fireEvent.click(within(push).getByRole("checkbox", { name: /Force with lease/ }));
  fireEvent.click(within(push).getByRole("button", { name: "Push" }));
  await settleAll();
  const confirm = screen.getByRole("alertdialog", {
    name: "Cannot be undone in api",
  });
  expect(confirm.textContent).toContain("Replace main on the remote origin with main");
  expect(fakeState.syncRequests.map((request) => request.confirmed)).toEqual([false]);
  fireEvent.click(within(confirm).getByRole("button", { name: "Force push" }));
  await settleAll();
  expect(fakeState.syncRequests.map((request) => request.confirmed)).toEqual([false, true]);
  expect(screen.queryByRole("alertdialog")).toBeNull();
  expect(screen.getByRole("status").textContent).toContain("Force push main to origin/main: Succeeded");
});

test("UI-20 a refused click shows the core's reason until dismissed", async () => {
  await setup({ local: ["main"], current: "main" });
  fireEvent.click(button("Fetch"));
  await settleAll();
  const alert = screen.getByRole("alert");
  expect(alert.textContent).toContain("the repo has no remote");
  fireEvent.click(within(alert).getByRole("button", { name: "Dismiss" }));
  expect(screen.queryByRole("alert")).toBeNull();
});
