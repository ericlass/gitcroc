import { afterEach, expect, test } from "vitest";
import { fakeState, reset } from "../ipc/fake";
import {
  workspaceAddPath,
  workspaceClose,
  workspaceOpen,
  workspacePickAndAdd,
  workspaceRemoveRepo,
  workspaceRename,
} from "../ipc";
import { createWorkspaceStore } from "./workspaces";

afterEach(reset);

const settle = () => new Promise((resolve) => setTimeout(resolve, 30));

test("ARCH-6 the workspace store mirrors the core's events", async () => {
  const store = createWorkspaceStore();
  const stop = await store.subscribe();
  const id = fakeState.workspace.id;
  expect(store.state.views?.map((view) => view.name)).toEqual(["My workspace"]);
  expect(store.state.active).toBe(id);

  await workspaceAddPath(id, "/src/api");
  await settle();
  expect(store.state.views?.[0]?.repos.map((repo) => repo.name)).toEqual(["api"]);
  expect(store.state.reports[id]?.added.map((repo) => repo.id)).toEqual(["r1"]);

  // GRP-1: the same path again is reported as present.
  await workspaceAddPath(id, "/src/api");
  await settle();
  expect(store.state.reports[id]?.added).toEqual([]);
  expect(store.state.reports[id]?.already_present.map((repo) => repo.id)).toEqual(["r1"]);

  // A cancelled dialog adds nothing; a picked folder is added.
  await workspacePickAndAdd(id);
  await settle();
  expect(store.state.views?.[0]?.repos).toHaveLength(1);
  fakeState.pickedFolder = "/src/web";
  await workspacePickAndAdd(id);
  await settle();
  expect(store.state.views?.[0]?.repos.map((repo) => repo.name)).toEqual(["api", "web"]);

  await workspaceRemoveRepo(id, "r1");
  expect(store.state.views?.[0]?.repos.map((repo) => repo.name)).toEqual(["web"]);
  stop();
});

test("ARCH-9 workspace calls name their workspace", async () => {
  await expect(workspaceAddPath("elsewhere", "/src/api")).rejects.toThrow("no open workspace");
  await expect(workspaceRemoveRepo(fakeState.workspace.id, "r9")).rejects.toThrow("no repo");
});

test("UI-27 the store follows the open tabs and the active one", async () => {
  const store = createWorkspaceStore();
  const stop = await store.subscribe();
  const id = fakeState.workspace.id;
  expect(store.active()?.id).toBe(id);

  await workspaceRename(id, "  Java  ");
  expect(store.active()?.name).toBe("Java");

  // Closing the last tab leaves none open; the file stays (UI-28).
  await workspaceClose(id);
  expect(store.state.views).toEqual([]);
  expect(store.state.active).toBeNull();
  expect(store.active()).toBeUndefined();

  await workspaceOpen(id);
  expect(store.active()?.name).toBe("Java");
  stop();
});

test("UI-27 workspace names are refused as the core refuses them", async () => {
  const id = fakeState.workspace.id;
  await expect(workspaceRename(id, "   ")).rejects.toThrow("cannot be blank");
  await expect(workspaceRename(id, "x".repeat(65))).rejects.toThrow("at most 64");
});
