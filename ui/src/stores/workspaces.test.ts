import { afterEach, expect, test } from "vitest";
import { fakeState, reset } from "../ipc/fake";
import { workspaceAddPath, workspacePickAndAdd, workspaceRemoveRepo } from "../ipc";
import { createWorkspaceStore } from "./workspaces";

afterEach(reset);

const settle = () => new Promise((resolve) => setTimeout(resolve, 30));

test("ARCH-6 the workspace store mirrors the core's events", async () => {
  const store = createWorkspaceStore();
  const stop = await store.subscribe();
  const id = fakeState.workspace.id;
  expect(store.state.views?.map((view) => view.name)).toEqual(["My workspace"]);

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
