import { cleanup, fireEvent, render, screen, within } from "@solidjs/testing-library";
import { afterEach, beforeEach, expect, test, vi } from "vitest";
import App from "../App";
import { fakeState, reset } from "../ipc/fake";
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
  local: ["main"],
  current: "main",
  remotes: { origin: ["main"] },
  tags: ["v1"],
};

const HISTORY = { length: 5, prefix: "api" };

/** The app with repo `api` on `refs` selected, its history shown and its Tags node expanded. */
async function setup(refs: FakeRefs = API) {
  fakeState.workspace = {
    ...fakeState.workspace,
    repos: [{ id: "r1", name: "api", path: "/src/api", remote: null }],
  };
  fakeState.refModels = { r1: refs };
  fakeState.refs = { r1: refNodes("r1", refs) };
  fakeState.summaries = [summary(id(), "r1", "main")];
  fakeState.histories = { r1: HISTORY };
  render(() => <App />);
  const repo = await screen.findByRole("treeitem", { name: /^api/ });
  fireEvent.click(repo);
  fireEvent.dblClick(repo);
  await settleAll();
  fireEvent.dblClick(screen.getByRole("treeitem", { name: /^Tags/ }));
  await settleAll();
}

const commit = (index: number) => commitAt(HISTORY, index);

/** The history row of commit `index`. */
const row = (index: number) =>
  within(screen.getByRole("listbox", { name: "History" })).getByRole("option", {
    name: new RegExp(commit(index).subject),
  });

const tagRow = (name: string) => screen.queryByRole("treeitem", { name });

/** Picks `entry` from the menu `menu` opened on `target`. */
const pick = (target: HTMLElement, menu: string, entry: string) => {
  fireEvent.contextMenu(target, { clientX: 10, clientY: 10 });
  fireEvent.click(within(screen.getByRole("menu", { name: menu })).getByRole("menuitem", { name: entry }));
};

const create = () => screen.getByRole<HTMLButtonElement>("button", { name: "Create" });

test("UI-13 create tag here: annotated with a message by default, the name checked as typed", async () => {
  await setup();
  pick(row(2), "Commit actions", "Create tag here…");
  const dialog = screen.getByRole("dialog", {
    name: `New tag at ${commit(2).short_hash} ${commit(2).subject} in api`,
  });
  const name = within(dialog).getByRole("textbox", { name: "Name" });

  // SEC-4: the core's reason shows as the user types.
  fireEvent.input(name, { target: { value: "a b" } });
  await settleAll();
  expect(within(dialog).getByText("the name cannot contain spaces")).toBeDefined();
  expect(create().disabled).toBe(true);

  // Annotated is the default: it needs a message.
  fireEvent.input(name, { target: { value: "v2" } });
  await settleAll();
  expect(within(dialog).getByRole<HTMLInputElement>("radio", { name: /Annotated/ }).checked).toBe(true);
  expect(create().disabled).toBe(true);
  fireEvent.input(within(dialog).getByRole("textbox", { name: "Message" }), { target: { value: "Release 2" } });
  // Pushing is off by default.
  expect(within(dialog).getByRole<HTMLInputElement>("checkbox", { name: /Push the tag/ }).checked).toBe(false);
  fireEvent.click(create());
  await settleAll();
  expect(fakeState.tagRequests).toEqual([
    {
      repo_id: "r1",
      op: {
        kind: "create",
        name: "v2",
        commit: commit(2).hash,
        annotated: true,
        message: "Release 2",
        push: false,
        remote: null,
      },
      confirmed: false,
    },
  ]);
  expect(screen.queryByRole("dialog")).toBeNull();
  expect(tagRow("v2")).not.toBeNull();

  // An existing name: the core's refusal shows in the dialog, which stays.
  pick(row(0), "Commit actions", "Create tag here…");
  fireEvent.input(screen.getByRole("textbox", { name: "Name" }), { target: { value: "v1" } });
  fireEvent.click(screen.getByRole("radio", { name: "Lightweight" }));
  await settleAll();
  fireEvent.click(create());
  await settleAll();
  expect(within(screen.getByRole("dialog")).getByRole("alert").textContent).toBe("a tag named v1 exists already");
});

test("a lightweight tag at HEAD, pushed after it is created, asks which remote when there is no origin", async () => {
  await setup({ ...API, remotes: { mirror: ["main"], upstream: ["main"] } });
  pick(screen.getByRole("treeitem", { name: /^api/ }), "Repository", "New tag…");
  const dialog = screen.getByRole("dialog", { name: "New tag at HEAD in api" });
  fireEvent.input(within(dialog).getByRole("textbox", { name: "Name" }), { target: { value: "v2" } });
  fireEvent.click(within(dialog).getByRole("radio", { name: "Lightweight" }));
  expect(within(dialog).getByRole<HTMLTextAreaElement>("textbox", { name: "Message" }).disabled).toBe(true);
  fireEvent.click(within(dialog).getByRole("checkbox", { name: /Push the tag/ }));
  await settleAll();
  fireEvent.click(create());
  await settleAll();
  expect(tagRow("v2")).toBeNull();

  // The core asked: the same request goes again with the remote picked.
  const remote = screen.getByRole("dialog", { name: "Push tag v2 to which remote?" });
  fireEvent.change(within(remote).getByRole("combobox", { name: "Remote" }), { target: { value: "upstream" } });
  fireEvent.click(within(remote).getByRole("button", { name: "Create" }));
  await settleAll();
  expect(fakeState.tagRequests.map((request) => request.op)).toEqual([
    { kind: "create", name: "v2", commit: null, annotated: false, message: "", push: true, remote: null },
    { kind: "create", name: "v2", commit: null, annotated: false, message: "", push: true, remote: "upstream" },
  ]);
  expect(screen.queryByRole("dialog")).toBeNull();
  expect(tagRow("v2")).not.toBeNull();
});

test("UI-7 a tag is pushed and deleted from the tree, the delete on the remote after a confirmation", async () => {
  await setup({ ...API, tags: ["v1", "v2"] });
  pick(tagRow("v1") ?? document.body, "Tag", "Push");
  await settleAll();
  expect(fakeState.tagRequests.at(-1)).toEqual({
    repo_id: "r1",
    op: { kind: "push", name: "v1", remote: null },
    confirmed: false,
  });

  // Here only: no confirmation.
  pick(tagRow("v2") ?? document.body, "Tag", "Delete…");
  fireEvent.click(within(screen.getByRole("dialog", { name: "Delete tag v2 in api?" })).getByRole("button", { name: "Delete" }));
  await settleAll();
  expect(fakeState.tagRequests.at(-1)?.op).toEqual({ kind: "delete", name: "v2", on_remote: false, remote: null });
  expect(tagRow("v2")).toBeNull();

  // Also on the remote: the core asks first.
  pick(tagRow("v1") ?? document.body, "Tag", "Delete…");
  const dialog = screen.getByRole("dialog", { name: "Delete tag v1 in api?" });
  fireEvent.click(within(dialog).getByRole("checkbox", { name: "Also delete it on the remote" }));
  fireEvent.click(within(dialog).getByRole("button", { name: "Delete" }));
  await settleAll();
  const confirm = screen.getByRole("alertdialog");
  expect(confirm.textContent).toContain("Cannot be undone in api");
  expect(confirm.textContent).toContain("Delete tag v1 on the remote origin, for everyone who fetches from it");
  expect(tagRow("v1")).not.toBeNull();
  fireEvent.click(within(confirm).getByRole("button", { name: "Delete" }));
  await settleAll();
  expect(fakeState.tagRequests.at(-1)).toEqual({
    repo_id: "r1",
    op: { kind: "delete", name: "v1", on_remote: true, remote: null },
    confirmed: true,
  });
  expect(screen.queryByRole("alertdialog")).toBeNull();
  expect(tagRow("v1")).toBeNull();
});

test("a push refused by the core shows in the tree until dismissed", async () => {
  await setup({ ...API, remotes: {} });
  pick(tagRow("v1") ?? document.body, "Tag", "Push");
  await settleAll();
  const refusal = within(screen.getByRole("region", { name: "Tree" })).getByRole("alert");
  expect(refusal.textContent).toContain("the repo has no remote");
  fireEvent.click(within(refusal).getByRole("button", { name: "Dismiss" }));
  expect(within(screen.getByRole("region", { name: "Tree" })).queryByRole("alert")).toBeNull();
});
