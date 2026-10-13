import { afterEach, expect, test } from "vitest";
import { emitRepoSummary, fakeState, reset } from "../ipc/fake";
import { repoSelect, type SummaryView } from "../ipc";
import { createSummaryStore } from "./summaries";

afterEach(reset);

const view = (repo: string, hash: string, stale: boolean): SummaryView => ({
  workspace_id: fakeState.workspace.id,
  repo_id: repo,
  summary: {
    head: { kind: "branch", name: "main", hash },
    upstream: null,
    dirty: false,
    changes: 0,
    state: { operation: null, unmerged_paths: 0 },
  },
  stale,
  missing: false,
  error: null,
  refreshing: false,
  conflict: false,
  refs_version: 0,
  busy: false,
});

test("PERF-3 cached summaries show stale until a fresh one arrives", async () => {
  fakeState.summaries = [view("r1", "aaa", true), view("r2", "bbb", true)];
  const store = createSummaryStore();
  const id = fakeState.workspace.id;
  const stop = await store.subscribe(id);
  expect(store.state.byWorkspace[id]?.r1?.stale).toBe(true);

  emitRepoSummary(view("r1", "ccc", false));
  expect(store.state.byWorkspace[id]?.r1).toEqual(view("r1", "ccc", false));
  expect(store.state.byWorkspace[id]?.r2?.stale).toBe(true);

  // ARCH-9: another workspace's summary does not land here.
  emitRepoSummary({ ...view("r2", "ddd", false), workspace_id: "elsewhere" });
  expect(store.state.byWorkspace[id]?.r2?.stale).toBe(true);
  stop();
});

test("ARCH-9 summary calls name their workspace", async () => {
  const store = createSummaryStore();
  await store.subscribe("elsewhere");
  expect(store.state.errors.elsewhere).toMatch("no open workspace");
  await repoSelect(fakeState.workspace.id, "r2");
  expect(fakeState.selected[fakeState.workspace.id]).toBe("r2");
});
