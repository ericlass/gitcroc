import { afterEach, expect, test, vi } from "vitest";
import { debugEmitProgress } from "../ipc";
import { emitJobProgress, reset } from "../ipc/fake";
import { createJobStore } from "./jobs";

afterEach(() => {
  vi.useRealTimers();
  reset();
});

test("arch_7_progress_events_reach_the_store", async () => {
  vi.useFakeTimers();
  const store = createJobStore();
  const unlisten = await store.subscribe();

  // The fake's debug job sends three events 100 ms apart, like the core's.
  const jobId = await debugEmitProgress("ws-a");
  for (const done of [1, 2, 3]) {
    await vi.advanceTimersByTimeAsync(100);
    expect(store.jobs["ws-a"]?.[jobId]).toEqual({ job_id: jobId, workspace_id: "ws-a", label: "Debug job", done, total: 3 });
  }

  // Another workspace's events stay under its own id (ARCH-9).
  emitJobProgress({ job_id: 99, workspace_id: "ws-b", label: "Pull", done: 1, total: 2 });
  expect(Object.keys(store.jobs)).toEqual(["ws-a", "ws-b"]);
  expect(Object.keys(store.jobs["ws-a"] ?? {})).toEqual([String(jobId)]);

  unlisten();
  emitJobProgress({ job_id: 100, workspace_id: "ws-a", label: "Late", done: 1, total: 1 });
  expect(store.jobs["ws-a"]?.[100]).toBeUndefined();
});
