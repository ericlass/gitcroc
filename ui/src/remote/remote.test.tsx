import { cleanup, fireEvent, render, screen, within } from "@solidjs/testing-library";
import { afterEach, beforeEach, expect, test, vi } from "vitest";
import App from "../App";
import type { SummaryView } from "../ipc";
import { fakeState, reset } from "../ipc/fake";
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

/** `main` tracks `origin/main`. */
const API: FakeRefs = {
  local: ["main"],
  current: "main",
  counts: { main: [0, 0] },
  remotes: { origin: ["main"] },
};

/** The app with repo `api` on `refs`, its summary with `change` made, selected and its Remotes node expanded. */
async function setup(refs: FakeRefs = API, change: Partial<SummaryView> = {}) {
  fakeState.workspace = {
    ...fakeState.workspace,
    repos: [{ id: "r1", name: "api", path: "/src/api", remote: null }],
  };
  fakeState.refModels = { r1: refs };
  fakeState.refs = { r1: refNodes("r1", refs) };
  fakeState.summaries = [summary(id(), "r1", "main", change)];
  render(() => <App />);
  const repo = await screen.findByRole("treeitem", { name: /^api/ });
  fireEvent.click(repo);
  fireEvent.dblClick(repo);
  await settleAll();
  fireEvent.dblClick(screen.getByRole("treeitem", { name: /^Remotes/ }));
  await settleAll();
}

const remoteRow = (name: string) => screen.queryByRole("treeitem", { name });

/** Picks `entry` from the menu `menu` opened on `target`. */
const pick = (target: HTMLElement | null, menu: string, entry: string) => {
  fireEvent.contextMenu(target ?? document.body, { clientX: 10, clientY: 10 });
  fireEvent.click(within(screen.getByRole("menu", { name: menu })).getByRole("menuitem", { name: entry }));
};

const button = (name: string) => screen.getByRole<HTMLButtonElement>("button", { name });

test("UI-7 add a remote from the Remotes node: the name checked as typed, no fetch by default", async () => {
  await setup();
  pick(screen.getByRole("treeitem", { name: /^Remotes/ }), "Remotes", "Add remote…");
  const dialog = screen.getByRole("dialog", { name: "Add a remote to api" });
  const name = within(dialog).getByRole("textbox", { name: "Name" });

  // SEC-4: the core's reason shows as the user types.
  fireEvent.input(name, { target: { value: "up/stream" } });
  await settleAll();
  expect(within(dialog).getByText("a remote name cannot contain '/'")).toBeDefined();
  expect(button("Add").disabled).toBe(true);

  fireEvent.input(name, { target: { value: "mirror" } });
  await settleAll();
  expect(button("Add").disabled).toBe(true);
  fireEvent.input(within(dialog).getByRole("textbox", { name: "URL" }), {
    target: { value: "https://example.com/api.git" },
  });
  expect(within(dialog).getByRole<HTMLInputElement>("checkbox", { name: /Fetch it/ }).checked).toBe(false);
  fireEvent.click(button("Add"));
  await settleAll();
  expect(fakeState.remoteRequests).toEqual([
    {
      repo_id: "r1",
      op: { kind: "add", name: "mirror", url: "https://example.com/api.git", fetch: false },
      confirmed: false,
    },
  ]);
  expect(screen.queryByRole("dialog")).toBeNull();
  expect(remoteRow("mirror")).not.toBeNull();

  // A taken name: the core's refusal shows in the dialog, which stays.
  pick(screen.getByRole("treeitem", { name: /^Remotes/ }), "Remotes", "Add remote…");
  fireEvent.input(screen.getByRole("textbox", { name: "Name" }), { target: { value: "origin" } });
  fireEvent.input(screen.getByRole("textbox", { name: "URL" }), { target: { value: "https://example.com/x.git" } });
  fireEvent.click(screen.getByRole("checkbox", { name: /Fetch it/ }));
  await settleAll();
  fireEvent.click(button("Add"));
  await settleAll();
  expect(fakeState.remoteRequests.at(-1)?.op).toEqual({
    kind: "add",
    name: "origin",
    url: "https://example.com/x.git",
    fetch: true,
  });
  const refusal = within(screen.getByRole("dialog")).getByRole("alert");
  expect(refusal.textContent).toBe("a remote named origin exists already");
});

test("rename a remote and edit its URLs from its row; a push URL is shown only when set", async () => {
  await setup({ ...API, urls: { origin: { url: "https://a.example/api.git", push_url: "ssh://b.example/api.git" } } });
  pick(remoteRow("origin"), "Remote", "Rename…");
  const rename = screen.getByRole("dialog", { name: "Rename remote origin in api" });
  const name = within(rename).getByRole<HTMLInputElement>("textbox", { name: "New name" });
  expect(name.value).toBe("origin");
  await settleAll();
  expect(button("Rename").disabled).toBe(true);
  fireEvent.input(name, { target: { value: "upstream" } });
  await settleAll();
  fireEvent.click(button("Rename"));
  await settleAll();
  expect(fakeState.remoteRequests.at(-1)?.op).toEqual({ kind: "rename", name: "origin", new_name: "upstream" });
  expect(remoteRow("origin")).toBeNull();
  expect(remoteRow("upstream")).not.toBeNull();

  pick(remoteRow("upstream"), "Remote", "Edit URL…");
  await settleAll();
  const edit = screen.getByRole("dialog", { name: "URL of remote upstream in api" });
  const fetchUrl = within(edit).getByRole<HTMLInputElement>("textbox", { name: "Fetch URL" });
  expect(fetchUrl.value).toBe("https://a.example/api.git");
  expect(within(edit).getByRole<HTMLInputElement>("textbox", { name: "Push URL" }).value).toBe(
    "ssh://b.example/api.git",
  );
  fireEvent.input(fetchUrl, { target: { value: "https://c.example/api.git" } });
  fireEvent.click(button("Save"));
  await settleAll();
  expect(fakeState.remoteRequests.at(-1)?.op).toEqual({
    kind: "set_url",
    name: "upstream",
    url: "https://c.example/api.git",
    push_url: "ssh://b.example/api.git",
  });
  expect(screen.queryByRole("dialog")).toBeNull();

  // No push URL set: one URL field only, and none sent.
  cleanup();
  await setup({ ...API, urls: { origin: { url: "https://a.example/api.git", push_url: null } } });
  pick(remoteRow("origin"), "Remote", "Edit URL…");
  await settleAll();
  const single = screen.getByRole("dialog", { name: "URL of remote origin in api" });
  expect(within(single).getAllByRole("textbox")).toHaveLength(1);
  fireEvent.input(within(single).getByRole("textbox", { name: "URL" }), { target: { value: "https://d.example" } });
  fireEvent.click(button("Save"));
  await settleAll();
  expect(fakeState.remoteRequests.at(-1)?.op).toEqual({
    kind: "set_url",
    name: "origin",
    url: "https://d.example",
    push_url: null,
  });
});

test("remove asks first, naming the upstreams it unsets", async () => {
  await setup();
  pick(remoteRow("origin"), "Remote", "Remove…");
  await settleAll();
  const confirm = screen.getByRole("alertdialog");
  expect(confirm.textContent).toContain("Cannot be undone in api");
  const actions = within(confirm)
    .getAllByRole("listitem")
    .map((item) => item.textContent);
  expect(actions).toEqual([
    "Remove remote origin and its remote-tracking branches",
    "Unset the upstream of main (origin/main)",
  ]);
  // Not confirmed: nothing ran.
  expect(remoteRow("origin")).not.toBeNull();
  fireEvent.click(within(confirm).getByRole("button", { name: "Remove" }));
  await settleAll();
  expect(fakeState.remoteRequests).toEqual([
    { repo_id: "r1", op: { kind: "remove", name: "origin" }, confirmed: false },
    { repo_id: "r1", op: { kind: "remove", name: "origin" }, confirmed: true },
  ]);
  expect(screen.queryByRole("alertdialog")).toBeNull();
  expect(remoteRow("origin")).toBeNull();
});

test("GRP-8 a busy repo has the remote entries disabled", async () => {
  await setup(API, { busy: true });
  fireEvent.contextMenu(remoteRow("origin") ?? document.body, { clientX: 10, clientY: 10 });
  const menu = within(screen.getByRole("menu", { name: "Remote" }));
  for (const entry of ["Rename…", "Edit URL…", "Remove…"]) {
    expect(menu.getByRole<HTMLButtonElement>("menuitem", { name: entry }).disabled).toBe(true);
  }
});
