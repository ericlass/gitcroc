import { afterEach, expect, test } from "vitest";
import { emitRepoRefsChanged, emitWorkspaceChanged, fakeState, reset } from "../ipc/fake";
import { refNodes } from "../ipc/fakeTree";
import type { TreeNode } from "../ipc";
import { flatten } from "../tree/rows";
import { createTreeStore } from "./tree";

afterEach(reset);

const settle = () => new Promise((resolve) => setTimeout(resolve, 0));

const id = () => fakeState.workspace.id;

function withRepos() {
  fakeState.workspace = {
    ...fakeState.workspace,
    repos: [
      { id: "r1", name: "api", path: "/src/api", remote: null },
      { id: "r2", name: "web", path: "/src/web", remote: null },
    ],
  };
  fakeState.refs = {
    r1: refNodes("r1", { local: ["main", "feature/a/b"], current: "main", tags: ["v1"] }),
    r2: refNodes("r2", { local: ["main"], current: "main" }),
  };
}

/** The visible rows as indented labels. */
const outline = (store: ReturnType<typeof createTreeStore>) =>
  flatten(store.tree(id())).map((row) => "  ".repeat(row.depth) + (row.kind === "node" ? row.node.label : row.text));

const node = (store: ReturnType<typeof createTreeStore>, label: string): TreeNode => {
  const found = flatten(store.tree(id())).find((row) => row.kind === "node" && row.node.label === label);
  if (found?.kind !== "node") {
    throw new Error(`no row ${label}`);
  }
  return found.node;
};

test("UI-1 refs load when a ref node is first expanded", async () => {
  withRepos();
  const store = createTreeStore();
  const stop = await store.subscribe(id());
  expect(outline(store)).toEqual(["Ungrouped", "  api", "  web"]);

  store.setExpanded(id(), node(store, "api"), true);
  expect(outline(store)).toEqual(["Ungrouped", "  api", "    Local", "    Remotes", "    Tags", "    Stashes", "  web"]);
  expect(fakeState.refsCalls.r1).toBeUndefined();

  store.setExpanded(id(), node(store, "Local"), true);
  expect(outline(store)).toContain("      Loading…");
  await settle();
  expect(outline(store).slice(2, 6)).toEqual(["    Local", "      feature", "      main", "    Remotes"]);
  store.setExpanded(id(), node(store, "Tags"), true);
  expect(outline(store)).toContain("      v1");
  expect(fakeState.refsCalls.r1).toBe(1);

  // The core dropped the refs after a refresh: they are read again.
  fakeState.refs.r1 = refNodes("r1", { local: ["main", "new"], current: "main" });
  emitRepoRefsChanged({ workspace_id: id(), repo_id: "r1" });
  await settle();
  expect(fakeState.refsCalls.r1).toBe(2);
  expect(outline(store).slice(2, 6)).toEqual(["    Local", "      main", "      new", "    Remotes"]);
  // Refs never loaded stay unloaded.
  emitRepoRefsChanged({ workspace_id: id(), repo_id: "r2" });
  await settle();
  expect(fakeState.refsCalls.r2).toBeUndefined();
  stop();
});

test("UI-1 unreadable refs show why", async () => {
  withRepos();
  delete fakeState.refs.r2;
  const store = createTreeStore();
  await store.subscribe(id());
  store.setExpanded(id(), node(store, "web"), true);
  store.setExpanded(id(), node(store, "Stashes"), true);
  await settle();
  const message = flatten(store.tree(id())).find((row) => row.kind === "message");
  expect(message).toMatchObject({ text: "cannot read the refs of r2", error: true });
});

test("UI-1 the tree follows workspace changes", async () => {
  withRepos();
  const store = createTreeStore();
  await store.subscribe(id());
  fakeState.workspace = { ...fakeState.workspace, repos: fakeState.workspace.repos.slice(1) };
  emitWorkspaceChanged(fakeState.workspace);
  await settle();
  expect(outline(store)).toEqual(["Ungrouped", "  web"]);
});

test("UI-8 the filter shows the core's answer, expanded, and clears", async () => {
  withRepos();
  const store = createTreeStore();
  await store.subscribe(id());
  await store.setFilter(id(), "FEAT");
  expect(outline(store)).toEqual(["Ungrouped", "  api", "    Local", "      feature", "        a", "          b"]);

  // An answer to older text never replaces a newer one.
  const older = store.setFilter(id(), "we");
  const newer = store.setFilter(id(), "web");
  await Promise.all([older, newer]);
  expect(store.tree(id()).filter).toBe("web");
  // Matched by its name alone, the repo keeps its ref nodes, not loaded.
  expect(outline(store)).toEqual(["Ungrouped", "  web", "    Local", "    Remotes", "    Tags", "    Stashes"]);

  await store.setFilter(id(), "");
  expect(outline(store)).toEqual(["Ungrouped", "  api", "  web"]);
});

test("ARCH-9 tree state is kept per workspace", async () => {
  withRepos();
  const store = createTreeStore();
  await store.subscribe(id());
  store.select(id(), "repo:r2", "r2");
  expect(fakeState.selected[id()]).toBe("r2");
  store.setExpanded(id(), node(store, "api"), true);

  await store.subscribe("elsewhere");
  expect(store.tree("elsewhere").error).toMatch("no open workspace");
  expect(store.tree("elsewhere").selected).toBeNull();
  expect(store.tree("elsewhere").expanded).toEqual({});
  expect(store.tree(id()).selected).toBe("repo:r2");
});
