import { afterEach, expect, test } from "vitest";
import { emitJobProgress, reset } from "../ipc/fake";
import { createJobStore } from "./jobs";

afterEach(reset);

test("arch_7_progress_events_reach_the_store", async () => {
  const store = createJobStore();
  const unlisten = await store.subscribe();

  for (const done of [1, 2, 3]) {
    emitJobProgress({ job_id: 7, workspace_id: "ws-a", label: "Fetch", done, total: 3 });
    expect(store.jobs["ws-a"]?.[7]).toEqual({ job_id: 7, workspace_id: "ws-a", label: "Fetch", done, total: 3 });
  }

  // Another workspace's events stay under its own id (ARCH-9).
  emitJobProgress({ job_id: 99, workspace_id: "ws-b", label: "Pull", done: 1, total: 2 });
  expect(Object.keys(store.jobs)).toEqual(["ws-a", "ws-b"]);
  expect(Object.keys(store.jobs["ws-a"] ?? {})).toEqual(["7"]);

  unlisten();
  emitJobProgress({ job_id: 100, workspace_id: "ws-a", label: "Late", done: 1, total: 1 });
  expect(store.jobs["ws-a"]?.[100]).toBeUndefined();
});
