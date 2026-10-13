import { afterEach, expect, test } from "vitest";
import { emitJobEnded, emitJobProgress, fakeState, reset } from "../ipc/fake";
import { fakeJob, OTHER_WORKSPACE } from "../ipc/fakeJobs";
import { createJobStore } from "./jobs";

afterEach(reset);

const settle = () => new Promise((resolve) => setTimeout(resolve, 0));

const workspace = { id: "ws-a", name: "A" };
const running = (done: number) =>
  fakeJob(workspace, { job_id: 7, label: "Fetch", state: "running", outcome: null, done, total: 3 });

test("arch_7_progress_events_reach_the_store", async () => {
  const store = createJobStore();
  const unlisten = await store.subscribe();

  for (const done of [1, 2, 3]) {
    emitJobProgress(running(done));
    expect(store.state.byId[7]).toEqual(running(done));
  }

  // Every workspace's jobs are kept, each under its own workspace id (ARCH-9).
  emitJobProgress(fakeJob(OTHER_WORKSPACE, { job_id: 99, label: "Pull", state: "running", outcome: null }));
  expect(store.jobs().map((job) => [job.job_id, job.workspace_id])).toEqual([
    [99, OTHER_WORKSPACE.id],
    [7, "ws-a"],
  ]);

  unlisten();
  emitJobProgress(fakeJob(workspace, { job_id: 100, label: "Late", state: "running", outcome: null }));
  expect(store.state.byId[100]).toBeUndefined();
});

test("UI-21 the store reads this run's jobs and keeps what events said since", async () => {
  fakeState.jobs = [running(1), fakeJob(workspace, { job_id: 3, label: "Push", outcome: { kind: "failed" } })];
  const store = createJobStore();
  const unlisten = await store.subscribe();
  // An event newer than the list that `jobs_get` returns.
  const done = { ...running(3), state: "ended" as const, outcome: { kind: "succeeded" as const } };
  emitJobEnded(done);
  await settle();

  expect(store.jobs().map((job) => job.job_id)).toEqual([7, 3]);
  expect(store.state.byId[7]).toEqual(done);
  expect(store.state.lastEnded).toEqual({ "ws-a": 7 });

  store.markSeen();
  expect(store.state.seen).toEqual({ 3: true, 7: true });
  unlisten();
});
